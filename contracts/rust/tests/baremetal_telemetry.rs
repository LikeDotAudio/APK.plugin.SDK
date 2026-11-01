// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Loopback agent telemetry golden vectors — the SAME ../../vectors/baremetal_telemetry.json
//! the TypeScript and Python twins read. Add cases to the vector file, not here.

use apkaudio_contracts::baremetal_telemetry::{derive_state, observed_state, SERVING_STATES};
use serde_json::Value;

static VECTORS: &str = include_str!("../../vectors/baremetal_telemetry.json");

fn vectors() -> Value {
    serde_json::from_str(VECTORS).expect("vectors/baremetal_telemetry.json parses")
}

#[test]
fn the_states_are_the_vector_states() {
    let all = vectors();
    let want: Vec<&str> = all["states"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(want, SERVING_STATES);
}

#[test]
fn observed_vectors() {
    let all = vectors();
    for v in all["observed"].as_array().unwrap() {
        let document = if v["document"].is_null() { None } else { Some(&v["document"]) };
        assert_eq!(observed_state(document).as_str(), v["state"].as_str().unwrap(), "{}", v["why"]);
    }
}

#[test]
fn derived_vectors() {
    let all = vectors();
    for v in all["derived"].as_array().unwrap() {
        let got = derive_state(
            v["stub"].as_bool().unwrap(),
            v["fatal"].as_bool().unwrap(),
            v["packets_received"].as_u64().unwrap(),
        );
        assert_eq!(got.as_str(), v["state"].as_str().unwrap(), "{v}");
    }
}
