// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The `nmos` device table — one retained leaf per field under
//! `System/Protocols/nmos/Device/{id}/{key}` — written by TWO crates that had to
//! agree by convention: PLUGIN.NMOS (mDNS-advertised API endpoints, one row per
//! `{host}_{port}`, `role` ∈ Node|Query|Registration|System) and
//! PLUGIN.NMOS_CONTROL (IS-04 devices from the registry, one row per device
//! UUID, `role` = Device). This module is the row schema both test against
//! (`vectors/nmos_rows.json`); 2026-10-30 audit, PLUGIN.NMOS_CONTROL #1.
//!
//! A row is valid when its `role` is one of its writer's roles and its key set
//! is exactly that writer's keys (plus `online_since`, which both publishers
//! append beside `last_online` — see `presence`).

use crate::topics::{self, sanitize_segment, TopicError};

/// The protocol segment the table lives under.
pub const PROTO: &str = "nmos";

/// The roles PLUGIN.NMOS writes (an API endpoint's role, from its mDNS service type).
pub const ENDPOINT_ROLES: [&str; 4] = ["Node", "Query", "Registration", "System"];

/// The one role PLUGIN.NMOS_CONTROL writes.
pub const DEVICE_ROLE: &str = "Device";

/// PLUGIN.NMOS's row keys (its `DEVICE_KEYS`).
pub const ENDPOINT_KEYS: [&str; 13] = [
    "service", "role", "api_versions", "api_proto", "api_auth", "priority", "host", "port", "addresses", "mac",
    "vendor", "last_online", "online_since",
];

/// PLUGIN.NMOS_CONTROL's row keys (its `DEVICE_KEYS` plus the appended `online_since`).
pub const DEVICE_KEYS: [&str; 13] = [
    "service", "role", "senders", "receivers", "vendor", "api_versions", "api_proto", "api_auth", "host", "port",
    "addresses", "last_online", "online_since",
];

/// Which writer a role belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writer {
    /// PLUGIN.NMOS — id `{host}_{port}`.
    Endpoint,
    /// PLUGIN.NMOS_CONTROL — id = IS-04 device UUID.
    Device,
}

pub fn writer_of(role: &str) -> Option<Writer> {
    if ENDPOINT_ROLES.contains(&role) {
        Some(Writer::Endpoint)
    } else if role == DEVICE_ROLE {
        Some(Writer::Device)
    } else {
        None
    }
}

pub fn keys_of(writer: Writer) -> &'static [&'static str] {
    match writer {
        Writer::Endpoint => &ENDPOINT_KEYS,
        Writer::Device => &DEVICE_KEYS,
    }
}

/// PLUGIN.NMOS's row id: `{sanitize_segment(host)}_{port}`.
pub fn endpoint_id(host: &str, port: u16) -> String {
    format!("{}_{}", sanitize_segment(host), port)
}

/// `System/Protocols/nmos/Device/{id}`.
pub fn device_prefix(id: &str) -> Result<String, TopicError> {
    topics::protocols(PROTO, &["Device", id])
}

/// `System/Protocols/nmos/Device/{id}/{key}`.
pub fn device_field(id: &str, key: &str) -> Result<String, TopicError> {
    topics::protocols(PROTO, &["Device", id, key])
}

/// Every row's every field: `System/Protocols/nmos/Device/#`.
pub fn device_wildcard() -> String {
    format!("{}/#", topics::protocols(PROTO, &["Device"]).expect("constant segments"))
}

/// Why a row (its role and the set of keys it carries) is not a table row; empty when it is one.
pub fn row_problems<'a>(role: &str, keys: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let Some(writer) = writer_of(role) else {
        return vec![format!("role {role:?} is none of Node, Query, Registration, System, Device")];
    };
    let want = keys_of(writer);
    let got: Vec<&str> = keys.into_iter().collect();
    let mut out = vec![];
    for k in want {
        if !got.contains(k) {
            out.push(format!("missing key {k}"));
        }
    }
    for k in &got {
        if !want.contains(k) {
            out.push(format!("key {k} is not a {writer:?} row key"));
        }
    }
    out
}
