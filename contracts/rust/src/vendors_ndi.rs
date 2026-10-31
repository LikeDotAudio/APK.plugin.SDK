// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! PLUGIN.NDI's retained tree, `System/Vendors/NDI/...`, as a versioned schema
//! (2026-10-30 audit, PLUGIN.NDI IMPROVEMENTS #1). Held to the plugin's own
//! key lists by `vectors/vendors_ndi.json`, which the plugin's tests can
//! `include_str!` from the SDK.
//!
//! ```text
//! System/Vendors/NDI/Source/{id}/{SOURCE_KEYS}                 retained; id = the D2 device id (ndi:…)
//! System/Vendors/NDI/DiscoveryServer/{host_port}/{SERVER_KEYS}  retained; host_port = sanitize_segment("{host}_{port}")
//! ```
//!
//! Every field is retained and an absent value is an EMPTY retained payload
//! (retained state is the database). Offline keeps [`OFFLINE_KEEPS`] and
//! empties the rest; a removed source empties every key. The plugin's
//! liveness and description are NOT here: they are the runner's
//! `System/Protocols/ndi/incoming/{state,api}` (shape held by the runner's
//! api-shape check), and a reader of `state == unavailable` treats this tree
//! as stale.
//!
//! [`SCHEMA_VERSION`] moves when a key is added, removed or renamed; it is
//! part of the contract version (VERSIONING.md).

use crate::topics::{is_address, sanitize_segment, TopicError};

/// Version of this tree's key sets.
pub const SCHEMA_VERSION: u32 = 1;

/// Root of the tree.
pub const TOPIC_ROOT: &str = "APK.audio/System/Vendors/NDI";

/// Every field published per source, in table order.
pub const SOURCE_KEYS: [&str; 10] =
    ["online", "full_name", "machine", "stream", "host", "addresses", "port", "groups", "seen_by", "last_seen"];

/// The source fields an OFFLINE source keeps (with `online` = `false`); the rest are emptied.
pub const OFFLINE_KEEPS: [&str; 6] = ["full_name", "machine", "stream", "host", "port", "last_seen"];

/// Every field published per configured discovery server.
pub const SERVER_KEYS: [&str; 5] = ["reachable", "host", "port", "rtt_ms", "last_seen"];

fn bad(what: &'static str, value: &str) -> TopicError {
    TopicError { what, value: value.to_string() }
}

/// `System/Vendors/NDI/Source/{id}/{key}`.
pub fn source_field(id: &str, key: &str) -> Result<String, TopicError> {
    if !is_address(id) {
        return Err(bad("ndi source id", id));
    }
    if !SOURCE_KEYS.contains(&key) {
        return Err(bad("ndi source key", key));
    }
    Ok(format!("{TOPIC_ROOT}/Source/{id}/{key}"))
}

/// The discovery-server segment, `sanitize_segment("{host}_{port}")`.
pub fn server_segment(host: &str, port: u16) -> String {
    sanitize_segment(&format!("{host}_{port}"))
}

/// `System/Vendors/NDI/DiscoveryServer/{segment}/{key}`.
pub fn server_field(segment: &str, key: &str) -> Result<String, TopicError> {
    if !crate::topics::is_segment(segment) {
        return Err(bad("ndi server segment", segment));
    }
    if !SERVER_KEYS.contains(&key) {
        return Err(bad("ndi server key", key));
    }
    Ok(format!("{TOPIC_ROOT}/DiscoveryServer/{segment}/{key}"))
}

pub fn sources_wildcard() -> String {
    format!("{TOPIC_ROOT}/Source/#")
}

pub fn servers_wildcard() -> String {
    format!("{TOPIC_ROOT}/DiscoveryServer/#")
}
