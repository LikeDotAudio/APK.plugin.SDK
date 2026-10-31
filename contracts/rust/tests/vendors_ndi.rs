// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! PLUGIN.NDI's tree against ../../vectors/vendors_ndi.json.

use apkaudio_contracts::vendors_ndi as ndi;
use serde_json::Value;

fn v() -> Value {
    serde_json::from_str(include_str!("../../vectors/vendors_ndi.json")).unwrap()
}
fn strs(x: &Value) -> Vec<&str> {
    x.as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect()
}

#[test]
fn keys_and_version() {
    let d = v();
    assert_eq!(d["schemaVersion"].as_u64().unwrap() as u32, ndi::SCHEMA_VERSION);
    assert_eq!(strs(&d["sourceKeys"]), ndi::SOURCE_KEYS);
    assert_eq!(strs(&d["offlineKeeps"]), ndi::OFFLINE_KEEPS);
    assert_eq!(strs(&d["serverKeys"]), ndi::SERVER_KEYS);
}

#[test]
fn topics() {
    let d = v();
    for t in d["topics"].as_array().unwrap() {
        let got = if let Some(src) = t["source"].as_str() {
            ndi::source_field(src, t["key"].as_str().unwrap()).unwrap()
        } else {
            let seg = ndi::server_segment(t["serverHost"].as_str().unwrap(), t["serverPort"].as_u64().unwrap() as u16);
            ndi::server_field(&seg, t["key"].as_str().unwrap()).unwrap()
        };
        assert_eq!(got, t["topic"].as_str().unwrap());
    }
    for t in d["invalid"].as_array().unwrap() {
        assert!(ndi::source_field(t["source"].as_str().unwrap(), t["key"].as_str().unwrap()).is_err(), "{t}");
    }
}
