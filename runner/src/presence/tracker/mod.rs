// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The per-device state and every live transition: seen, gone, retire,
//! expire. The read-back lives beside it in `readback.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use apkaudio_contracts::device_publish::{Publication, Sighting};
use apkaudio_contracts::discovery_presence::DiscoveryPresence;

use super::document::{empty, publication, set};
use super::ADOPT_GRACE;

mod readback;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod readback_tests;
#[cfg(test)]
mod tests;

/// One device as this process last said it, or will say it.
#[derive(Debug, Clone)]
struct Entry {
    topic: String,
    /// The tracker's own state. Never published.
    online: bool,
    /// The document as it stands — what has been said, or is about to be.
    doc: DiscoveryPresence,
    /// The real last sighting in THIS process. `None` for a device adopted off
    /// the broker and not heard from since.
    last_seen_ms: Option<i64>,
    /// What the expiry clock counts from: the last sighting, or the adoption.
    clock_ms: i64,
    /// Sighted during a boot read-back and not yet reconciled with what the
    /// broker holds, so its stamps are provisional.
    settled: bool,
}

impl Entry {
    fn adopted(&self) -> bool {
        self.last_seen_ms.is_none()
    }
}

/// The state machine, with the clock and the client left outside it so the
/// tests can drive both. [`seen`](super::seen), [`gone`](super::gone) and the tick are this plus a mutex.
#[derive(Debug)]
pub struct Tracker {
    /// Keyed by the D2 device id, which is what a departure carries.
    entries: BTreeMap<String, Entry>,
    /// `None`: departures only — the protocol says when a device leaves.
    inactive_after_ms: Option<i64>,
    /// `Some` while a read-back window is open: topic → the retained document.
    readback: Option<BTreeMap<String, DiscoveryPresence>>,
    /// Retained bodies under this plugin's protocols that are NOT presence — a
    /// v1 `DeviceRecord` or the state/since draft, each carrying identity the
    /// owner's rule keeps out of this tree. Retracted when the window closes.
    legacy: BTreeSet<String>,
    /// Topics [`Tracker::retire`]d while a window was open: retracted when it
    /// closes, never adopted off the broker's replay.
    retired: BTreeSet<String>,
}

impl Tracker {
    pub fn new(inactive_after: Option<Duration>) -> Self {
        Self {
            entries: BTreeMap::new(),
            inactive_after_ms: inactive_after.filter(|d| !d.is_zero()).map(|d| d.as_millis() as i64),
            readback: None,
            legacy: BTreeSet::new(),
            retired: BTreeSet::new(),
        }
    }

    /// A tracker already inside its first read-back window — the state a
    /// process is in from boot until the broker has replayed what it holds.
    pub fn booting(inactive_after: Option<Duration>) -> Self {
        let mut t = Self::new(inactive_after);
        t.begin_readback();
        t
    }

    /// A hunter saw this device. `Some` only when that is a transition
    /// (new, or back after offline), and never while a read-back is open.
    pub fn seen(&mut self, sighting: &Sighting, now_ms: i64) -> Result<Option<Publication>, String> {
        let device_id = sighting.device_id();
        let holding = self.readback.is_some();
        let entry = match self.entries.get_mut(&device_id) {
            Some(e) => e,
            None => {
                let topic = sighting.topic().map_err(|e| e.to_string())?;
                self.entries.entry(device_id.clone()).or_insert(Entry {
                    topic,
                    online: false,
                    doc: empty(),
                    last_seen_ms: None,
                    clock_ms: now_ms,
                    // Outside a window there is nothing to reconcile with.
                    settled: !holding,
                })
            }
        };
        entry.last_seen_ms = Some(now_ms);
        entry.clock_ms = now_ms;
        if entry.online {
            return Ok(None);
        }
        entry.online = true;
        set(&mut entry.doc.last_online, Some(now_ms));
        set(&mut entry.doc.last_discovered, Some(now_ms));
        if holding {
            return Ok(None);
        }
        Ok(Some(publication(&entry.topic, &entry.doc)))
    }

    /// The protocol said the device left (an mDNS goodbye or lapse, ADP
    /// departing, a stream withdrawn). Offline since now, once.
    pub fn gone(&mut self, device_id: &str, now_ms: i64) -> Option<Publication> {
        self.deactivate(device_id, now_ms)
    }

    /// This id was never a device of its own: the hunter now knows the same
    /// box by a better one (a hostname, until the MAC is known). Forgotten, and
    /// its retained document retracted — not said offline, because nothing
    /// left. Held until a read-back window closes, so the replay cannot bring
    /// it back as an adopted device.
    pub fn retire(&mut self, device_id: &str) -> Option<Publication> {
        let entry = self.entries.remove(device_id)?;
        if self.readback.is_some() {
            self.retired.insert(entry.topic);
            return None;
        }
        Some(Publication { topic: entry.topic, payload: String::new() })
    }

    /// The tick: every online device past its clock, offline since the moment
    /// it crossed the line.
    pub fn expire(&mut self, now_ms: i64) -> Vec<Publication> {
        if self.readback.is_some() {
            return Vec::new();
        }
        let grace = ADOPT_GRACE.as_millis() as i64;
        let due: Vec<(String, i64)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.online)
            .filter_map(|(id, e)| {
                let limit = match (self.inactive_after_ms, e.adopted()) {
                    (Some(after), _) => after,
                    (None, true) => grace,
                    (None, false) => return None,
                };
                (now_ms - e.clock_ms > limit).then(|| (id.clone(), e.clock_ms + limit))
            })
            .collect();
        due.into_iter().filter_map(|(id, at)| self.deactivate(&id, at)).collect()
    }

    /// How long something adopted at a boot may go unsighted before it is
    /// no longer trusted: the plugin's own clock, else [`ADOPT_GRACE`].
    pub(in crate::presence) fn grace_ms(&self) -> i64 {
        self.inactive_after_ms.filter(|ms| *ms > 0).unwrap_or(ADOPT_GRACE.as_millis() as i64)
    }

    /// The expiry clock `start` hands a tracker that was booting before it
    /// was told one. Unlike [`Tracker::new`], zero is not filtered out.
    pub(in crate::presence) fn set_inactive_after(&mut self, d: Option<Duration>) {
        self.inactive_after_ms = d.map(|d| d.as_millis() as i64);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether the document at `topic` is one this tracker holds as online —
    /// sighted, or adopted off the broker and not yet expired.
    pub fn is_online(&self, topic: &str) -> bool {
        self.entries.values().any(|e| e.online && e.topic == topic)
    }

    /// `(online, offline)` — what the tick prints when something moved.
    pub fn counts(&self) -> (usize, usize) {
        let online = self.entries.values().filter(|e| e.online).count();
        (online, self.entries.len() - online)
    }

    fn deactivate(&mut self, device_id: &str, at_ms: i64) -> Option<Publication> {
        let holding = self.readback.is_some();
        let entry = self.entries.get_mut(device_id)?;
        if !entry.online {
            return None;
        }
        entry.online = false;
        set(&mut entry.doc.last_offline, Some(at_ms));
        // The real last sighting, if this process had one; an adopted device
        // keeps the sighting the broker already recorded.
        set(&mut entry.doc.last_discovered, entry.last_seen_ms);
        if holding {
            return None;
        }
        Some(publication(&entry.topic, &entry.doc))
    }
}
