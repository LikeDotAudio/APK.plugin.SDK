// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! A module's `config.ini`: the INI subset, the token's own section keys
//! (`topic`, `raise`, `serves`) and whether the broker is the bench's own.
//! Split out of lib.rs (ROAD-3749); re-exported at the crate root.

use super::*;

/// `config.ini` as `{section: {key: value}}` — the shape `mqtt.rs` published
/// through rust-ini. Written out here rather than linked: the files are flat
/// `key = value` under `[section]`, and one parser dependency per plugin
/// binary is twenty to keep in step for a grammar this small.
pub fn ini_to_json(text: &str) -> Value {
    let mut root = Map::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_string();
            root.entry(section.clone()).or_insert_with(|| Value::Object(Map::new()));
            continue;
        }
        let Some((key, value)) = line.split_once('=').or_else(|| line.split_once(':')) else {
            continue;
        };
        if let Value::Object(map) = root
            .entry(section.clone())
            .or_insert_with(|| Value::Object(Map::new()))
        {
            map.insert(key.trim().to_string(), Value::String(value.trim().to_string()));
        }
    }
    Value::Object(root)
}

/// The section's `topic` key, else `APK.audio/System/Protocols/<token>`.
///
/// The section is matched without case, for the reason `mqtt.rs::module_topic`
/// spends a page on: `[AES70]` and `aes70` are one module and MQTT topics are
/// case-sensitive, so the token's own spelling is what the default uses.
pub fn module_topic(config: &Value, token: &str) -> String {
    config
        .as_object()
        .and_then(|sections| {
            sections
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(token))
                .and_then(|(_, section)| section.get("topic"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("APK.audio/System/Protocols/{token}"))
}

/// How a plugin is raised (contracts/DISCOVERY-LIFECYCLE.md, stage 2), as its
/// own `[token]` section declares it: `raise = on-sighting | always-on | compose`.
pub const RAISE_MODES: [&str; 3] = ["on-sighting", "always-on", "compose"];

/// The token's own section, matched without case like [`module_topic`].
pub(crate) fn module_key<'a>(config: &'a Value, token: &str, key: &str) -> Option<&'a str> {
    config
        .as_object()?
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))?
        .1
        .get(key)?
        .as_str()
}

/// The declared `raise`, or `None` when it is missing or not one of
/// [`RAISE_MODES`] — published as `null`, which `check-api-shape.py` and the
/// engine read as "this plugin has not said how it is raised".
pub fn module_raise(config: &Value, token: &str) -> Option<&'static str> {
    let declared = module_key(config, token, "raise")?.trim();
    RAISE_MODES.iter().copied().find(|mode| mode.eq_ignore_ascii_case(declared))
}

/// The protocols this plugin serves: `serves = a, b` in its section, else its
/// own token. These are the `{protocol}` segments of the things it reports at
/// `APK.audio/Discovery/{protocol}/{deviceId}`.
pub fn module_serves(config: &Value, token: &str) -> Vec<String> {
    let listed: Vec<String> = module_key(config, token, "serves")
        .unwrap_or("")
        .split(',')
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    if listed.is_empty() { vec![token.to_ascii_lowercase()] } else { listed }
}

/// Syntactic, no DNS — a copy of `broker_is_local` in
/// `NODE.BareMetal/SRC/Core/ORCHESTRATOR.hunter.VISA/src/mqtt.rs`,
/// which this process does not link. Anything not obviously ours is public.
pub fn broker_is_local(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost")
        || host.eq_ignore_ascii_case("broker")
        || host.ends_with(".local")
        || host.ends_with(".localhost")
    {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        Ok(std::net::IpAddr::V6(v6)) => {
            v6.is_loopback()
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
        Err(_) => false,
    }
}
