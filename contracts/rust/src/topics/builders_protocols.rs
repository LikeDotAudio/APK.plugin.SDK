// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `topics.rs` — see that file for the whole picture.

use super::*;

/// A protocol agent's config/status tree: `System/Protocols/{proto}/{...path}`.
/// The tail belongs to the protocol, as it does for `bareMetal`. An EMPTY tail
/// builds a topic that `parse` classifies as the legacy `protocolStatus`
/// family, which is deliberate: that spelling predates this one.
pub fn protocols(proto: &str, path: &[&str]) -> Result<String, TopicError> {
    require_segment("proto", proto)?;
    let mut topic = format!("{ROOT}/System/Protocols/{proto}");
    for s in path {
        require_segment("path segment", s)?;
        topic.push('/');
        topic.push_str(s);
    }
    Ok(topic)
}

pub fn protocols_wildcard() -> String {
    format!("{ROOT}/System/Protocols/#")
}

/// The leaf that separates a device's ADDRESS from the verb pressed on it:
/// `.../Device/{category}/{device}/Control/{verb}`.
///
/// THIS STRING WAS AGREED BY FOUR LITERALS AND HELD BY NOTHING. Fourteen device
/// consoles publish onto that shape; `Discovery.ChromeCast/src/control.rs` and
/// `Discovery.AppleTV/src/control.rs` each arrived at `["Control", verb]`
/// independently, and `instruments/panel_template.rs::bind_control` writes
/// `{base}/Control/{verb}` a third time. `protocols()` builds the ROOT and takes
/// a free `&[&str]` tail, so the contract had no opinion at all about the part
/// every one of them depends on. PLAN-914.01.
pub const DEVICE_CONTROL_LEAF: &str = "Control";

/// A press on one device's own agent:
/// `System/Protocols/{proto}/Device/{category}/{device}/Control/{verb}`.
///
/// Built on `protocols()` rather than beside it, so the root stays declared in
/// one place and a segment carrying `/`, `+` or `#` is a `TopicError` HERE —
/// at the moment the topic is composed — rather than a publish into a level
/// nobody subscribed to. That is the whole gain over the free tail: a category
/// with a slash in it used to build a longer topic and fail silently.
pub fn device_control(
    proto: &str,
    category: &str,
    device: &str,
    verb: &str,
) -> Result<String, TopicError> {
    protocols(proto, &["Device", category, device, DEVICE_CONTROL_LEAF, verb])
}

/// What an agent subscribes to for ONE device: every verb, and nothing else.
///
/// `#` and not `+`, because a verb is the last segment today and a deeper one
/// tomorrow should still reach the agent that owns the device rather than
/// falling into the silent `_ => {}` arm PLAN-857.01 is about.
pub fn device_control_wildcard(
    proto: &str,
    category: &str,
    device: &str,
) -> Result<String, TopicError> {
    Ok(format!(
        "{}/#",
        protocols(proto, &["Device", category, device, DEVICE_CONTROL_LEAF])?
    ))
}

/// The leaf the VISA agent answers a query on:
/// `System/Protocols/visa/Device/{family}/{model}/{dev}/Reply`.
/// `apk-visa`'s write daemon publishes it; KOMAND subscribes to it and turns
/// each reply into a named reading.
pub const DEVICE_REPLY_LEAF: &str = "Reply";

/// Every VISA instrument's reply, and nothing else:
/// `System/Protocols/visa/Device/+/+/+/Reply`.
///
/// Three `+`, because a VISA device address is three levels (family, model,
/// dev), not the two of `device_control`. Built on `protocols()` so the root is
/// declared once; KOMAND held this filter as an inline literal until it was
/// moved here.
pub fn visa_reply_wildcard() -> String {
    let base = protocols("visa", &["Device"]).expect("constant segments are valid");
    format!("{base}/+/+/+/{DEVICE_REPLY_LEAF}")
}

/// A scan's narration leaf: `System/Protocols/{proto}/Scan/Log`.
///
/// Its own builder, and its own family, for its retain class alone. `Scan/State`
/// beside it is the scan's conclusion and goes out retained under `protocols`;
/// this one is a narration line published `retain=false`, and a retained
/// narration outlives the scan it describes. Third instance of the shape that
/// split `bareMetal` into `baremetal_command`.
pub fn protocols_scan_log(proto: &str) -> Result<String, TopicError> {
    require_segment("proto", proto)?;
    Ok(format!("{ROOT}/System/Protocols/{proto}/Scan/Log"))
}

/// `APK.audio/System/Gui/Panels/{key}` — one device panel, retained.
pub fn gui_panels(key: &str) -> Result<String, TopicError> {
    require_segment("key", key)?;
    Ok(format!("{GUI_PANELS_ROOT}/{key}"))
}

/// `APK.audio/System/Gui/Panels/+` — every device panel.
pub fn gui_panels_wildcard() -> String {
    format!("{GUI_PANELS_ROOT}/+")
}

/// The Folder Navigator's selected path / trigger — browser-only, no agent
/// serves it, which `tree.ts` states rather than leaves implied.
pub fn file_browser(key: &str) -> Result<String, TopicError> {
    require_segment("key", key)?;
    Ok(format!("{ROOT}/System/FileBrowser/{key}"))
}

/// `APK.audio/System/BareMetal/{target}/{...path}` plus a validated tail.
/// The tail is address-class, not segment-class: `{host}` may be an FQDN.
fn baremetal_head(target: &str, path: &[&str]) -> Result<String, TopicError> {
    if !is_baremetal_target(target) {
        return Err(err("bareMetal target", target));
    }
    let mut topic = format!("{ROOT}/System/BareMetal/{target}");
    for s in path {
        if !is_address(s) {
            return Err(err("path segment", s));
        }
        topic.push('/');
        topic.push_str(s);
    }
    Ok(topic)
}

/// The RETAINED half. `command`, `telemetry` and `control` are refused as the
/// last segment and minted by [`baremetal_command`] / [`baremetal_stream`], so
/// the retain class is never a function of what a caller put in last position.
pub fn baremetal(target: &str, path: &[&str]) -> Result<String, TopicError> {
    if let Some(last) = path.last() {
        if *last == BAREMETAL_COMMAND_LEAF || is_baremetal_stream_leaf(last) {
            return Err(err("bareMetal path", last));
        }
    }
    baremetal_head(target, path)
}

/// An ESP32 node's status / Last Will, `System/BareMetal/ESP32/{node}/identity/heartbeat`
/// (payload `Esp32Heartbeat`; `{node}` = the MAC without colons).
pub fn baremetal_esp32_heartbeat(node: &str) -> Result<String, TopicError> {
    baremetal_head("ESP32", &[node, "identity", "heartbeat"])
}

pub fn baremetal_command(target: &str, path: &[&str]) -> Result<String, TopicError> {
    Ok(format!("{}/{BAREMETAL_COMMAND_LEAF}", baremetal_head(target, path)?))
}

pub fn baremetal_stream(target: &str, path: &[&str], leaf: &str) -> Result<String, TopicError> {
    if !is_baremetal_stream_leaf(leaf) {
        return Err(err("bareMetal stream leaf", leaf));
    }
    Ok(format!("{}/{leaf}", baremetal_head(target, path)?))
}

pub fn baremetal_wildcard(target: Option<&str>) -> Result<String, TopicError> {
    match target {
        None => Ok(format!("{ROOT}/System/BareMetal/#")),
        Some(t) if is_baremetal_target(t) => Ok(format!("{ROOT}/System/BareMetal/{t}/#")),
        Some(t) => Err(err("bareMetal target", t)),
    }
}

pub fn log(source: &str, level: &str) -> Result<String, TopicError> {
    require_segment("source", source)?;
    if !LOG_LEVELS.contains(&level) {
        return Err(err("log level", level));
    }
    Ok(format!("{ROOT}/System/Log/{source}/{level}"))
}

/// Every log line on the bench — the Log softapp's one subscription.
pub fn log_wildcard() -> String {
    format!("{ROOT}/System/Log/#")
}

/// One open problem, retained (`ProblemRecord`, see `report`); an empty
/// retained publish clears it. `key` is `report::problem_key(code, subject)`.
pub fn problems(source: &str, key: &str) -> Result<String, TopicError> {
    require_segment("source", source)?;
    require_segment("key", key)?;
    Ok(format!("{ROOT}/System/Problems/{source}/{key}"))
}

/// Every open problem the broker holds — the Problems softapp's subscription.
pub fn problems_wildcard() -> String {
    format!("{ROOT}/System/Problems/#")
}
