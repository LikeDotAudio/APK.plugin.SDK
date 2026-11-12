// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `topics.rs` — see that file for the whole picture.

use super::*;

// ------------------------------------- round 2 of the 2026-10-30 audit
// Shapes OsApi, NODE.BareMetal, DockTor and Frontend-Assets built on ROOT by hand.

/// `APK.audio/System`.
pub const SYSTEM_ROOT: &str = "APK.audio/System";

/// Every System topic, `APK.audio/System/#`.
pub fn system_wildcard() -> String {
    format!("{SYSTEM_ROOT}/#")
}

/// The browser's retained nearest-Rust-agent hand-off flag,
/// `System/Agents/NearestRustAgent/HandOff` (exact; retained).
pub fn nearest_rust_agent_handoff() -> String {
    format!("{SYSTEM_ROOT}/Agents/NearestRustAgent/HandOff")
}

/// DockTor's chat line, `System/Chat` (live).
pub fn system_chat() -> String {
    format!("{SYSTEM_ROOT}/Chat")
}

/// NODE.BareMetal's census of one node, `System/Census/{node}/{...path}` (retained).
pub fn census(node: &str, path: &[&str]) -> Result<String, TopicError> {
    require_segment("node", node)?;
    if path.is_empty() {
        return Err(err("census path", ""));
    }
    for s in path {
        require_segment("path segment", s)?;
    }
    Ok(format!("{SYSTEM_ROOT}/Census/{node}/{}", path.join("/")))
}

pub fn census_wildcard(node: Option<&str>) -> Result<String, TopicError> {
    match node {
        None => Ok(format!("{SYSTEM_ROOT}/Census/#")),
        Some(n) => {
            require_segment("node", n)?;
            Ok(format!("{SYSTEM_ROOT}/Census/{n}/#"))
        }
    }
}

/// The orchestrator's live discovery tables, `System/Gui/Discovered/{...path}` (retained).
pub fn gui_discovered(path: &[&str]) -> Result<String, TopicError> {
    if path.is_empty() {
        return Err(err("guiDiscovered path", ""));
    }
    for s in path {
        require_segment("path segment", s)?;
    }
    Ok(format!("{SYSTEM_ROOT}/Gui/Discovered/{}", path.join("/")))
}

pub fn gui_discovered_wildcard() -> String {
    format!("{SYSTEM_ROOT}/Gui/Discovered/#")
}

pub fn device_groups_state() -> String {
    format!("{SYSTEM_ROOT}/DeviceGroups/State")
}
pub fn device_groups_command() -> String {
    format!("{SYSTEM_ROOT}/DeviceGroups/Command")
}
pub fn hour_chime_state() -> String {
    format!("{SYSTEM_ROOT}/HourChime/State")
}
pub fn hour_chime_struck() -> String {
    format!("{SYSTEM_ROOT}/HourChime/Struck")
}
pub fn capture_filed() -> String {
    format!("{SYSTEM_ROOT}/Capture/Filed")
}
pub fn system_writes() -> String {
    format!("{SYSTEM_ROOT}/Writes")
}
pub fn client_breadcrumb() -> String {
    format!("{SYSTEM_ROOT}/Client/Breadcrumb")
}
pub fn bench_two_state() -> String {
    format!("{SYSTEM_ROOT}/Bench/TwoState")
}

/// Every instrument's `Read` leaf, `.../visa/Device/+/+/+/Read`.
pub fn visa_read_wildcard() -> String {
    format!("{ROOT}/System/Protocols/visa/Device/+/+/+/Read")
}

/// Every measurement (optionally of one family), `.../visa/Device/{family|+}/+/+/Reading/#`.
pub fn visa_reading_wildcard(family: Option<&str>) -> Result<String, TopicError> {
    let f = match family {
        None => "+",
        Some(f) => {
            require_segment("family", f)?;
            f
        }
    };
    Ok(format!("{ROOT}/System/Protocols/visa/Device/{f}/+/+/Reading/#"))
}

/// One instrument's reading for one command, `.../Reading/{command}`.
pub fn visa_reading(family: &str, model: &str, dev: &str, command: &str) -> Result<String, TopicError> {
    require_segment("command", command)?;
    Ok(format!("{}/Reading/{command}", visa_device(family, model, dev)?))
}

/// One command's reading across a family, e.g. every DMM's `Read_Next`.
pub fn visa_reading_across(family: &str, command: &str) -> Result<String, TopicError> {
    require_segment("family", family)?;
    require_segment("command", command)?;
    Ok(format!("{ROOT}/System/Protocols/visa/Device/{family}/+/+/Reading/{command}"))
}
