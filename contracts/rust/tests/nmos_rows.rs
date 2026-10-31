// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The nmos table row schema against ../../vectors/nmos_rows.json.

use apkaudio_contracts::nmos_rows;
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("../../vectors/nmos_rows.json")).unwrap()
}

#[test]
fn rows() {
    for v in vectors()["rows"].as_array().unwrap() {
        let keys: Vec<&str> = v["keys"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
        let problems = nmos_rows::row_problems(v["role"].as_str().unwrap(), keys);
        assert_eq!(problems.is_empty(), v["ok"].as_bool().unwrap(), "{v} -> {problems:?}");
    }
}

#[test]
fn ids_and_topics() {
    for v in vectors()["ids"].as_array().unwrap() {
        let port = v["port"].as_u64().unwrap() as u16;
        assert_eq!(nmos_rows::endpoint_id(v["host"].as_str().unwrap(), port), v["id"].as_str().unwrap());
    }
    for v in vectors()["topics"].as_array().unwrap() {
        let t = nmos_rows::device_field(v["id"].as_str().unwrap(), v["key"].as_str().unwrap()).unwrap();
        assert_eq!(t, v["topic"].as_str().unwrap());
    }
    assert_eq!(nmos_rows::device_wildcard(), "APK.audio/System/Protocols/nmos/Device/#");
}
