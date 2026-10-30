// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The restart reconciliation: hold writes while the broker replays this
//! plugin's retained documents, then say only what differs. See the no-flap
//! rule in the module doc of `presence`.

use std::collections::BTreeMap;

use apkaudio_contracts::device_publish::Publication;
use apkaudio_contracts::discovery_presence;

use super::super::document::publication;
use super::{Entry, Tracker};

impl Tracker {
    /// Open a read-back window: hold writes, collect what the broker retains.
    pub fn begin_readback(&mut self) {
        self.readback = Some(BTreeMap::new());
        self.legacy.clear();
    }

    /// One retained document off the broker. Ignored outside a window and for
    /// anything that is not a presence document.
    pub fn restore(&mut self, topic: &str, payload: &[u8]) {
        let Some(held) = self.readback.as_mut() else {
            return;
        };
        match discovery_presence::parse(payload) {
            Some(doc) => {
                held.insert(topic.to_string(), doc);
                self.legacy.remove(topic);
            }
            None => {
                held.remove(topic);
                if payload.is_empty() {
                    self.legacy.remove(topic);
                } else {
                    self.legacy.insert(topic.to_string());
                }
            }
        }
    }

    pub fn reading_back(&self) -> bool {
        self.readback.is_some()
    }

    /// Close the window and say whatever differs from what the broker holds.
    pub fn finish_readback(&mut self, now_ms: i64) -> Vec<Publication> {
        let Some(mut held) = self.readback.take() else {
            return Vec::new();
        };
        let mut out = Vec::new();

        // Retired inside the window: retracted if the broker holds them, and
        // never adopted below.
        for topic in std::mem::take(&mut self.retired) {
            if held.remove(&topic).is_some() || self.legacy.remove(&topic) {
                out.push(Publication { topic, payload: String::new() });
            }
        }

        for entry in self.entries.values_mut() {
            let retained = held.remove(&entry.topic);
            if !entry.settled {
                entry.settled = true;
                if let Some(prior) = &retained {
                    if entry.online && discovery_presence::is_online(prior) {
                        // It never left, as far as anyone recorded: keep every
                        // stamp the broker holds. This is the no-flap case.
                        entry.doc = prior.clone();
                    } else {
                        // A real return (or a departure heard inside the
                        // window): carry the half of the history this process
                        // did not live through.
                        if entry.online {
                            entry.doc.last_offline = prior.last_offline.clone();
                        } else {
                            entry.doc.last_online = prior.last_online.clone();
                            if entry.last_seen_ms.is_none() {
                                entry.doc.last_discovered = prior.last_discovered.clone();
                            }
                        }
                    }
                }
            }
            self.legacy.remove(&entry.topic);
            if retained.as_ref() != Some(&entry.doc) {
                out.push(publication(&entry.topic, &entry.doc));
            }
        }

        // An older document nobody has sighted since: identity with no status
        // this process can vouch for. Emptied, once, rather than left captured.
        for topic in std::mem::take(&mut self.legacy) {
            out.push(Publication { topic, payload: String::new() });
        }

        // Documents the broker holds that nobody here has sighted: adopt them,
        // with the connect as their clock.
        for (topic, doc) in held {
            let Some(device_id) = topic.rsplit('/').next().map(str::to_string) else {
                continue;
            };
            let online = discovery_presence::is_online(&doc);
            self.entries.insert(
                device_id,
                Entry { topic, online, doc, last_seen_ms: None, clock_ms: now_ms, settled: true },
            );
        }
        out
    }
}
