// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Presence — what a discovery agent says about a device it can see RIGHT NOW.
//! Twin of `contracts/src/presence.ts`.
//!
//! ```text
//! last_online  = -1            while the agent can see the device
//! online_since = 1788436572    unix seconds, stamped ONCE on arrival
//! ```
//!
//! Both are constant while the device stays up, so both dedupe away and a
//! stable device puts nothing on the bus. `last_online` used to carry `now`,
//! restamped on every announcement — a value that is never equal to the last
//! one, which defeated change-only publishing for the whole device.
//!
//! `-1` is a present-tense assertion, not a timestamp. It is a number so that a
//! reader which parses it lands on something impossible rather than a plausible
//! lie, and it is what liveness is read from: every hunter retracts a device's
//! whole subtree when it goes, so a `-1` still on the broker means a device an
//! agent can currently see. VISA is the exception and writes a real stamp on
//! the way out, because it keeps the row.
//!
//! Elapsed time is NOT published — "online 4h 12m" is a subtraction against the
//! reader's clock, not a message per second per device. See `render_uptime`.
//!
//! Names no MQTT client: the crate rule. The caller publishes.

use std::collections::BTreeMap;

/// The `last_online` payload meaning "the agent can see this device now".
///
/// A string rather than a number because that is what goes on the wire, and
/// every caller was already formatting one.
pub const ONLINE_NOW: &str = "-1";

/// Retained key carrying when the device was first seen in its current spell
/// of being online. Unix seconds, written once per arrival.
pub const ONLINE_SINCE_KEY: &str = "online_since";

/// Is this `last_online` payload the present-tense sentinel?
///
/// Any negative number, not just `-1` exactly: a reader must not care whether
/// it is looking at `-1`, `-1.0` or `-1 ` off a retained topic, and no real
/// stamp is ever negative. An unparseable payload is not presence — it is the
/// `unknown` case, which the caller distinguishes.
pub fn is_online_now(raw: &str) -> bool {
    raw.trim().parse::<f64>().map(|n| n < 0.0).unwrap_or(false)
}

/// Seconds a device has been up, from its `online_since` and the reader's now.
///
/// `None` when the field is absent, unreadable, or in the future — a stamp
/// ahead of the reader's clock means the two disagree about what time it is,
/// and a negative uptime rendered as "0m" would hide that.
pub fn uptime_secs(online_since: &str, now_secs: u64) -> Option<u64> {
    let since = online_since.trim().parse::<f64>().ok()?;
    if since <= 0.0 {
        return None;
    }
    let elapsed = now_secs as f64 - since;
    if elapsed < 0.0 { None } else { Some(elapsed as u64) }
}

/// Uptime as a human reads it: `4h 12m`, `3d 5h`, `47s`.
///
/// Two units at most, largest first, and the smaller dropped when it is zero.
/// A device up for eleven days does not need its minutes, and one up for
/// forty seconds does not need a leading `0h`.
pub fn render_uptime(secs: u64) -> String {
    let (d, h, m, s) = (secs / 86_400, (secs / 3_600) % 24, (secs / 60) % 60, secs % 60);
    match (d, h, m) {
        (0, 0, 0) => format!("{s}s"),
        (0, 0, _) => format!("{m}m"),
        (0, _, 0) => format!("{h}h"),
        (0, _, _) => format!("{h}h {m}m"),
        (_, 0, _) => format!("{d}d"),
        (_, _, _) => format!("{d}d {h}h"),
    }
}

/// When each device arrived, so the stamp is written once and not once per
/// announcement.
///
/// Keyed by whatever the agent uses to identify a device to itself — an entity
/// id, a sanitised topic segment, a resource string. The register does not
/// interpret the key; it only has to be the same key on the next announcement.
#[derive(Default)]
pub struct Presence {
    arrived: BTreeMap<String, u64>,
}

impl Presence {
    pub fn new() -> Self {
        Self { arrived: BTreeMap::new() }
    }

    /// The device's arrival stamp — `now_secs` the first time it is asked, and
    /// that same value on every announcement afterwards.
    ///
    /// This is the whole reason `online_since` is stable, and therefore the
    /// whole reason a stable device is silent: the value does not move, so
    /// change-only publishing drops it.
    pub fn since(&mut self, key: &str, now_secs: u64) -> u64 {
        *self.arrived.entry(key.to_string()).or_insert(now_secs)
    }

    /// Drop a device's arrival stamp, so that if it comes back it comes back
    /// with a NEW `online_since` rather than resuming the old spell.
    ///
    /// Called from wherever the agent retracts the device's retained topics. A
    /// forget that is missed is not a crash, it is a device claiming an uptime
    /// that spans the gap it was unplugged for — which is exactly the kind of
    /// plausible wrong number this module exists to stop publishing.
    pub fn forget(&mut self, key: &str) {
        self.arrived.remove(key);
    }

    /// Is this device currently on the register?
    pub fn knows(&self, key: &str) -> bool {
        self.arrived.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.arrived.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arrived.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sentinel is read as presence in every spelling a retained topic can
    /// hand back, and NOT read as presence in any spelling of a real stamp.
    #[test]
    fn the_sentinel_is_recognised_however_it_is_spelled() {
        assert!(is_online_now("-1"));
        assert!(is_online_now("-1.0"));
        assert!(is_online_now(" -1 "));
        assert!(is_online_now(ONLINE_NOW));
        // A real stamp is not presence, however fresh.
        assert!(!is_online_now("1788436572"));
        assert!(!is_online_now("0"));
        // Neither is a payload nobody can read. That is `unknown`, and the
        // caller must be able to tell it from "definitely here".
        assert!(!is_online_now(""));
        assert!(!is_online_now("-"));
        assert!(!is_online_now("yes"));
    }

    /// Uptime refuses rather than guesses. A stamp in the future means the
    /// publisher's clock and the reader's disagree; rendering that as `0m`
    /// would hide the one fact worth surfacing.
    #[test]
    fn uptime_refuses_what_it_cannot_measure() {
        assert_eq!(uptime_secs("1000", 1_600), Some(600));
        assert_eq!(uptime_secs("1000", 1_000), Some(0));
        assert_eq!(uptime_secs("2000", 1_000), None); // ahead of our clock
        assert_eq!(uptime_secs("-1", 1_000), None);   // the sentinel is not a stamp
        assert_eq!(uptime_secs("", 1_000), None);
        assert_eq!(uptime_secs("soon", 1_000), None);
    }

    /// Two units at most, largest first, smaller dropped when zero.
    #[test]
    fn uptime_reads_like_a_person_wrote_it() {
        assert_eq!(render_uptime(47), "47s");
        assert_eq!(render_uptime(60), "1m");
        assert_eq!(render_uptime(15_120), "4h 12m");
        assert_eq!(render_uptime(7_200), "2h");
        assert_eq!(render_uptime(275_400), "3d 4h");
        assert_eq!(render_uptime(259_200), "3d");
        assert_eq!(render_uptime(0), "0s");
    }

    /// The whole point of the register: the stamp does NOT move while the
    /// device stays up, which is what lets change-only publishing drop it.
    #[test]
    fn the_arrival_stamp_does_not_move_under_re_announcement() {
        let mut p = Presence::new();
        assert_eq!(p.since("dev", 1_000), 1_000);
        assert_eq!(p.since("dev", 1_030), 1_000);
        assert_eq!(p.since("dev", 9_999), 1_000);
        assert!(p.knows("dev"));
    }

    /// And a device that left and came back gets a NEW spell rather than
    /// resuming the old one — otherwise its uptime spans the gap it was
    /// unplugged for, which is the plausible-wrong number this replaces.
    #[test]
    fn a_returning_device_starts_a_new_spell() {
        let mut p = Presence::new();
        p.since("dev", 1_000);
        p.forget("dev");
        assert!(!p.knows("dev"));
        assert_eq!(p.since("dev", 5_000), 5_000);
        assert_eq!(p.len(), 1);
    }
}
