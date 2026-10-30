// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! DiscoveryPresence wrapper — re-exports the generated type. Twin of
//! `contracts/src/discovery-presence.ts`, which carries the argument: the
//! retained `Discovery/{protocol}/{deviceId}` document is a device's STATUS
//! only — `lastOnline`, `lastOffline`, `lastDiscovered` — and it changes only on
//! a transition. PLAN-3318.01.
//!
//! The behaviour half — when to say it — is `apk-plugin-runner::presence`,
//! because it owns a timer and a client and this crate owns neither.

pub use crate::gen::discovery_presence::{
    DiscoveryPresence, DiscoveryPresenceLastDiscovered, DiscoveryPresenceLastOffline,
    DiscoveryPresenceLastOnline,
};

use crate::time::unix_ms_from_iso;

/// The three keys, and the only three. A body with any other key is not a
/// presence document — it is identity leaking back into the captured tree.
pub const KEYS: [&str; 3] = ["lastOnline", "lastOffline", "lastDiscovered"];

/// Read a retained `Discovery/{protocol}/{deviceId}` payload as presence.
///
/// `None` for anything that is not exactly the three keys — an empty
/// retraction, a v1 `DeviceRecord` still written by MIDI or VISA, the
/// state/since draft, or bytes nobody can parse. Stricter than serde alone on
/// purpose: the generated `Option` fields would accept a body missing a key,
/// and the tolerant-reader policy would accept one carrying `make`.
pub fn parse(payload: &[u8]) -> Option<DiscoveryPresence> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let map = value.as_object()?;
    if map.len() != KEYS.len() || !KEYS.iter().all(|k| map.contains_key(*k)) {
        return None;
    }
    serde_json::from_value(value).ok()
}

/// Online ⇔ `lastOnline` is later than `lastOffline`. The state the tracker
/// holds in memory, recovered from the wire by the one rule both sides share.
pub fn is_online(doc: &DiscoveryPresence) -> bool {
    let at = |s: Option<&str>| s.and_then(unix_ms_from_iso);
    match (
        at(doc.last_online.as_deref().map(String::as_str)),
        at(doc.last_offline.as_deref().map(String::as_str)),
    ) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(on), Some(off)) => on > off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_v1_record_is_not_read_as_presence() {
        let v1 = br#"{"schemaVersion":1,"protocol":"visa","deviceId":"visa:x","deviceClass":"",
            "make":"","model":"","address":"","status":"identified",
            "firstSeen":"2026-01-01T00:00:00.000Z","lastSeen":"2026-01-01T00:00:00.000Z"}"#;
        assert!(parse(v1).is_none());
        assert!(parse(b"").is_none());
    }

    #[test]
    fn identity_in_the_body_is_refused() {
        let leaked = br#"{"lastOnline":"2026-09-17T06:27:37.000Z","lastOffline":null,
            "lastDiscovered":"2026-09-17T06:27:37.000Z","make":"PreSonus"}"#;
        assert!(parse(leaked).is_none());
        let short = br#"{"lastOnline":"2026-09-17T06:27:37.000Z","lastOffline":null}"#;
        assert!(parse(short).is_none());
    }

    #[test]
    fn state_is_derived_from_the_two_moments() {
        let doc = |on: &str, off: &str| {
            parse(format!(r#"{{"lastOnline":{on},"lastOffline":{off},"lastDiscovered":null}}"#).as_bytes())
                .expect("presence")
        };
        assert!(is_online(&doc(r#""2026-09-17T06:00:00.000Z""#, "null")));
        assert!(!is_online(&doc("null", "null")));
        assert!(!is_online(&doc(r#""2026-09-17T06:00:00.000Z""#, r#""2026-09-17T09:00:05.000Z""#)));
        assert!(is_online(&doc(r#""2026-09-17T10:00:00.000Z""#, r#""2026-09-17T09:00:05.000Z""#)));
    }
}
