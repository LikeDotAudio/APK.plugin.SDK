// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! `APK_SERIAL_SETTINGS` golden vectors — the SAME ../../vectors/serial_settings.json
//! the Python binding's tests read. Add cases to the vector file, not here.

use apkaudio_contracts::serial_settings::{parse_settings, pinned, SerialPins};
use serde_json::Value;

static VECTORS: &str = include_str!("../../vectors/serial_settings.json");

fn vectors() -> Value {
    serde_json::from_str(VECTORS).expect("vectors/serial_settings.json parses")
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_else(|| panic!("missing string {key} in {v}"))
}

#[test]
fn parse_vectors() {
    for v in vectors()["parse"].as_array().unwrap() {
        let got = parse_settings(s(v, "text")).map(|l| l.label());
        assert_eq!(got.as_deref(), v["label"].as_str(), "parse {:?}", v["text"]);
    }
}

#[test]
fn pin_vectors() {
    for v in vectors()["pins"].as_array().unwrap() {
        let got = pinned(s(v, "spec"), s(v, "tty"), s(v, "serial")).map(|l| l.label());
        assert_eq!(got.as_deref(), v["label"].as_str(), "pin {v}");
    }
}

#[test]
fn rejected_vectors() {
    for v in vectors()["rejected"].as_array().unwrap() {
        let want: Vec<String> =
            v["rejected"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
        assert_eq!(SerialPins::parse(s(v, "spec")).rejected, want, "rejected {v}");
    }
}
