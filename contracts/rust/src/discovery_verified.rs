// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! DiscoveryVerified wrapper — re-exports the generated type. Twin of
//! `contracts/src/discovery-verified.ts`, which carries the argument: the
//! retained `Discovery/{protocol}/{deviceId}/verified` leaf is an agent's
//! PROOF that it re-verified the thing — `verifiedAt`, `by` — kept beside the
//! status document because that one is transition-only and must stay silent
//! while a device stays. ROAD-3437.
//!
//! The agent restates it at its own restatement period on each successful
//! re-verification; the engine's lifecycle `check` stage reads it
//! (`contracts/DISCOVERY-LIFECYCLE.md`). It ticks by design: never capture it.

pub use crate::gen::discovery_verified::{
    DiscoveryVerified, DiscoveryVerifiedBy, DiscoveryVerifiedVerifiedAt,
};

use crate::time::{iso_from_unix_ms, unix_ms_from_iso};

/// The two keys, and the only two.
pub const KEYS: [&str; 2] = ["verifiedAt", "by"];

/// The document an agent retains after a successful re-verification at
/// `now_ms`. `by` is the agent's plugin token; it must be one topic segment,
/// or [`parse`] will refuse what this wrote.
pub fn document(by: &str, now_ms: i64) -> String {
    serde_json::json!({ "verifiedAt": iso_from_unix_ms(now_ms), "by": by }).to_string()
}

/// Read a retained `…/verified` payload. `None` for anything that is not
/// exactly the two keys with a `Z` stamp and a segment token — an empty
/// retraction, an extra key, a numeric stamp. Stricter than serde alone on
/// purpose: the tolerant-reader policy would accept extra keys.
pub fn parse(payload: &[u8]) -> Option<DiscoveryVerified> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let map = value.as_object()?;
    if map.len() != KEYS.len() || !KEYS.iter().all(|k| map.contains_key(*k)) {
        return None;
    }
    serde_json::from_value(value).ok()
}

/// The proof's instant in unix ms — what the lifecycle `check` compares with
/// its window. `None` when the stamp does not parse as an instant.
pub fn verified_ms(doc: &DiscoveryVerified) -> Option<i64> {
    unix_ms_from_iso(doc.verified_at.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_round_trips() {
        let doc = document("PLUGIN_SNMP", 1_788_436_572_000);
        let parsed = parse(doc.as_bytes()).expect("verified");
        assert_eq!(parsed.by.as_str(), "PLUGIN_SNMP");
        assert_eq!(verified_ms(&parsed), Some(1_788_436_572_000));
        assert!(parsed.verified_at.ends_with('Z'));
    }

    #[test]
    fn anything_else_is_refused() {
        assert!(parse(b"").is_none());
        assert!(parse(br#"{"verifiedAt":"2026-10-31T10:00:00.000Z"}"#).is_none());
        assert!(parse(br#"{"verifiedAt":"2026-10-31T10:00:00.000Z","by":"snmp","ip":"10.0.0.1"}"#).is_none());
        assert!(parse(br#"{"verifiedAt":1788436572,"by":"snmp"}"#).is_none());
        assert!(parse(br#"{"verifiedAt":"2026-10-31T10:00:00+01:00","by":"snmp"}"#).is_none());
        assert!(parse(br#"{"verifiedAt":"2026-10-31T10:00:00.000Z","by":"a/b"}"#).is_none());
    }
}
