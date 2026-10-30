// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Milliseconds into the three-key presence body, and that body into the
//! retained `Publication` that says it.

use apkaudio_contracts::device_publish::Publication;
use apkaudio_contracts::discovery_presence::DiscoveryPresence;
use apkaudio_contracts::time::iso_from_unix_ms;

fn stamp(ms: i64) -> Option<String> {
    Some(iso_from_unix_ms(ms))
}

pub(super) fn set(field: &mut Option<impl std::str::FromStr>, ms: Option<i64>) {
    if let Some(ms) = ms {
        *field = stamp(ms).and_then(|s| s.parse().ok());
    }
}

pub(super) fn empty() -> DiscoveryPresence {
    DiscoveryPresence { last_discovered: None, last_offline: None, last_online: None }
}

pub(super) fn publication(topic: &str, doc: &DiscoveryPresence) -> Publication {
    Publication {
        topic: topic.to_string(),
        // Serialised with its nulls: the body is always all three keys.
        payload: serde_json::to_string(doc).unwrap_or_default(),
    }
}
