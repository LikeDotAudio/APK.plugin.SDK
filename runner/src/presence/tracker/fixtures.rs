// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Test fixtures shared by the tracker's two test files.

use std::time::Duration;

use apkaudio_contracts::device_publish::{Publication, Sighting};
use apkaudio_contracts::time::iso_from_unix_ms;
use serde_json::Value;

use super::Tracker;

pub(super) const T0: i64 = 1_789_000_000_000;
pub(super) const AFTER: Duration = Duration::from_secs(90);

pub(super) fn cast() -> Sighting {
    Sighting::new("chromecast", "10.0.0.77").class("Speaker").made_by("Google", "Nest Audio")
}

pub(super) fn doc(p: &Publication) -> Value {
    serde_json::from_str(&p.payload).unwrap()
}

pub(super) fn iso(ms: i64) -> Value {
    Value::String(iso_from_unix_ms(ms))
}

pub(super) fn live() -> Tracker {
    Tracker::new(Some(AFTER))
}
