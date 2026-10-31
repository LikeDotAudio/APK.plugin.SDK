// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The machine command vectors (PLAN-3394.01), shared with the Python and TypeScript mirrors.

use apkaudio_contracts::machine_command::{acl_deny_patterns, parse};
use serde_json::Value;
use std::path::PathBuf;

fn vectors() -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vectors/machine_command.json");
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
}

#[test]
fn valid_commands_parse_and_round_trip() {
    for case in vectors()["valid"].as_array().unwrap() {
        let body = case["doc"].to_string();
        let c = parse(body.as_bytes()).unwrap_or_else(|r| panic!("{}: {}", case["why"], r.reason));
        assert_eq!(parse(c.to_json().as_bytes()).as_ref(), Ok(&c), "{}", case["why"]);
    }
}

#[test]
fn invalid_commands_are_refused_against_their_id() {
    for case in vectors()["invalid"].as_array().unwrap() {
        // `body` is the exact wire text, for spellings a parsed doc cannot keep (1.0 vs 1).
        let body = case["body"].as_str().map(str::to_string).unwrap_or_else(|| case["doc"].to_string());
        let r = parse(body.as_bytes()).err().unwrap_or_else(|| panic!("accepted: {}", case["why"]));
        assert_eq!(r.id.as_deref(), case["refused_id"].as_str(), "{}", case["why"]);
    }
}

#[test]
fn the_acl_list_is_the_vectors_list() {
    let want: Vec<String> =
        vectors()["acl_deny"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(acl_deny_patterns(), want);
}
