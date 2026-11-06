// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The capture SLOTS — Rust twin of `src/capture-rule.ts` and Python
//! `apkaudio_contracts.capture_rule` (ROAD-3554).
//!
//! Nothing is restated: the slots are read from the generated grammar
//! (`python/apkaudio_contracts/topic_grammar.json` `captureSlots`, which
//! `scripts/gen.ts` writes from capture-rule.ts, and which the SDK tag ships).
//! A producer of `System/Captures/{kind}/{capture}` checks `problems` first, so
//! a document that does not fill its slot never reaches the bus.

use std::sync::OnceLock;

use serde_json::Value;

const GRAMMAR: &str = include_str!("../../python/apkaudio_contracts/topic_grammar.json");

/// The rig snapshot's schema, `proto/capture_rig.proto` — what `rigs.state` holds,
/// zlib'd. An encoder holds its tags to this text (CAPTURE.Recorder's rig.rs does).
pub const RIG_SCHEMA: &str = include_str!("../../proto/capture_rig.proto");

/// `vectors/capture_rig.json`: snapshot bytes every encoder must produce and every
/// decoder (WEB.OsApi capture_store.py) must read back.
pub const RIG_VECTORS: &str = include_str!("../../vectors/capture_rig.json");

/// THE UNIT GATE (ROAD-3553, ROAD-3558): where a program asks the recorder agent to
/// write to a VISA unit for it, behind its one-exchange-per-unit gate. Twin of
/// `src/unit-gate.ts` UNIT_GATE_TOPIC; held to `vectors/unit_gate.json`.
pub const UNIT_GATE_TOPIC: &str = "APK.audio/System/Protocols/capture-recorder/outgoing/write";

/// `vectors/unit_gate.json`: what a write becomes, the agent online or not.
pub const UNIT_GATE_VECTORS: &str = include_str!("../../vectors/unit_gate.json");

fn slots() -> &'static Value {
    static S: OnceLock<Value> = OnceLock::new();
    S.get_or_init(|| serde_json::from_str::<Value>(GRAMMAR).map(|g| g["captureSlots"].clone()).unwrap_or(Value::Null))
}

fn holds(kind: &str, v: Option<&Value>) -> bool {
    let nullable = kind.ends_with("|null");
    match v {
        None | Some(Value::Null) => nullable,
        Some(v) => match kind.trim_end_matches("|null") {
            "string" => v.is_string(),
            "number" => v.as_f64().map(f64::is_finite).unwrap_or(false),
            "array" => v.is_array(),
            "object" => v.is_object(),
            _ => false,
        },
    }
}

/// Why `doc` may not fill the `kind` slot; empty when it may (Python `capture_problems`).
pub fn problems(kind: &str, doc: &Value) -> Vec<String> {
    let Some(slot) = slots().get(kind).filter(|s| s.is_object()) else {
        return vec![format!("{kind:?} is not a capture kind")];
    };
    let Some(obj) = doc.as_object() else {
        return vec!["not an object".into()];
    };
    let mut out = Vec::new();
    for (name, t) in slot["fields"].as_object().into_iter().flatten() {
        let t = t.as_str().unwrap_or("");
        if !obj.contains_key(name) {
            out.push(format!("{kind}: missing {name}"));
        } else if !holds(t, obj.get(name)) {
            out.push(format!("{kind}: {name} must be {t}"));
        }
    }
    for (name, t) in slot["optional"].as_object().into_iter().flatten() {
        let t = t.as_str().unwrap_or("");
        if obj.contains_key(name) && !holds(t, obj.get(name)) {
            out.push(format!("{kind}: {name} must be {t}"));
        }
    }
    out
}

/// A device key as one capture level: `visa/DMM/34401A/DevttyUSB0` -> `visa-DMM-34401A-DevttyUSB0`.
pub fn capture_segment(device: &str) -> String {
    let plain = device.split('/').collect::<Vec<_>>().join("-");
    if !plain.is_empty() && plain.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        plain
    } else {
        crate::topics::sanitize_segment(&plain)
    }
}

/// `APK.audio/System/Captures/<kind>/<capture>` for one device.
pub fn slot_topic(kind: &str, device: &str) -> Result<String, crate::topics::TopicError> {
    crate::topics::capture(kind, &capture_segment(device))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_rig_schema_and_vectors_are_shipped() {
        assert!(RIG_SCHEMA.contains("message RigSnapshot"));
        let v: Value = serde_json::from_str(RIG_VECTORS).unwrap();
        assert!(!v["cases"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_gate_topic_is_the_vectors() {
        let v: Value = serde_json::from_str(UNIT_GATE_VECTORS).unwrap();
        assert_eq!(v["gate_topic"], UNIT_GATE_TOPIC);
    }

    #[test]
    fn every_closed_kind_has_its_slot() {
        for kind in crate::topics::CAPTURE_KINDS {
            assert!(slots().get(kind).is_some(), "{kind} has no captureSlots entry");
        }
        assert_eq!(slots().as_object().unwrap().len(), crate::topics::CAPTURE_KINDS.len());
    }

    #[test]
    fn a_rig_fills_its_slot_and_a_half_made_one_does_not() {
        assert!(problems("Rig", &json!({"device": "visa/a/b/c", "ts": 1.0, "rig": {}})).is_empty());
        assert_eq!(problems("Rig", &json!({"device": "visa/a/b/c", "rig": {}})), vec!["Rig: missing ts"]);
        assert!(!problems("Nope", &json!({})).is_empty());
        assert!(problems("Attention", &json!({"device": "d", "leaf": "status", "word": "offline", "ts": 2, "was": null})).is_empty());
        assert_eq!(slot_topic("Rig", "visa/DMM/34401A/Dev7").unwrap(), "APK.audio/System/Captures/Rig/visa-DMM-34401A-Dev7");
    }
}
