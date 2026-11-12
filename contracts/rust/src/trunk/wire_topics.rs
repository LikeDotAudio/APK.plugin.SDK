// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `trunk.rs` — see that file for the whole picture.

use super::*;

// ---------------------------------------------------------------------------
// Topics — (topic, retained)
// ---------------------------------------------------------------------------

fn caster(path: &[&str], retain: bool) -> Result<(String, bool), TopicError> {
    Ok((protocols(TOKEN, path)?, retain))
}

fn sim(protocol: &str, path: &[&str], retain: bool) -> Result<(String, bool), TopicError> {
    let mut full = vec![protocol];
    full.extend_from_slice(path);
    Ok((protocols(SIM_TOKEN, &full)?, retain))
}

/// `trunk/Frame/{frame}` — the frame's whole state folded into one document
/// ([`FrameDoc`]), RETAINED, restated at most once per publish pass when any of
/// it changed; erased when the frame has nothing left (0.8.0, ROAD-3467).
pub fn frame_doc_topic(frame: &str) -> Result<(String, bool), TopicError> {
    caster(&[FRAME_DOC_LEVEL, frame], true)
}
/// `trunk/Device/Frame/{frame}` — the frame's advertisement, RETAINED. 0.7.0; a
/// 0.8.0 caster folds it into [`frame_doc_topic`] and erases it; read for one release.
pub fn frame_topic(frame: &str) -> Result<(String, bool), TopicError> {
    caster(&["Device", "Frame", frame], true)
}
/// `trunk/Trunk/{trunk}` — RETAINED. 0.7.0; folded into [`frame_doc_topic`] since 0.8.0, read for one release.
pub fn trunk_topic(trunk: &str) -> Result<(String, bool), TopicError> {
    caster(&["Trunk", trunk], true)
}
/// `trunk/Connection/{connection}` — RETAINED. 0.7.0; folded since 0.8.0, read for one release.
pub fn connection_topic(connection: &str) -> Result<(String, bool), TopicError> {
    caster(&["Connection", connection], true)
}
/// `trunk/Crosspoint/{frame}` — the frame's closed crosspoints, RETAINED. 0.7.0; folded since 0.8.0, read for one release.
pub fn crosspoints_topic(frame: &str) -> Result<(String, bool), TopicError> {
    caster(&["Crosspoint", frame], true)
}
/// `trunk/Booking/{booking}` — RETAINED; erased when cancelled or past.
pub fn booking_topic(booking: &str) -> Result<(String, bool), TopicError> {
    caster(&["Booking", booking], true)
}
/// `trunk/Site/{slug}` — RETAINED.
pub fn site_topic(slug: &str) -> Result<(String, bool), TopicError> {
    caster(&["Site", slug], true)
}
/// `trunk/Relay/{matrix}/{port}` — a relay binding, RETAINED; erased on unbind.
pub fn relay_binding_topic(matrix: &str, port: usize) -> Result<(String, bool), TopicError> {
    let p = port.to_string();
    caster(&["Relay", matrix, &p], true)
}
/// `trunk/View/{name}` — one read-only view, RETAINED, sent when it changes. `name` is one of
/// [`VIEW_NAMES`].
pub fn view_topic(name: &str) -> Result<(String, bool), TopicError> {
    if !VIEW_NAMES.contains(&name) {
        return Err(TopicError { what: "trunk view", value: name.to_string() });
    }
    caster(&["View", name], true)
}
/// `trunk/View/{name}/{key}` — one keyed view, RETAINED; erased when its key goes. `name` is
/// one of [`VIEW_ITEM_NAMES`] (`Taps/{frame}`, `Capture/{id}`).
pub fn view_item_topic(name: &str, key: &str) -> Result<(String, bool), TopicError> {
    if !VIEW_ITEM_NAMES.contains(&name) {
        return Err(TopicError { what: "trunk keyed view", value: name.to_string() });
    }
    caster(&["View", name, key], true)
}
/// `trunk/Event/{kind}` — one event, NOT retained. `kind` is one of [`EVENT_KINDS`].
pub fn event_topic(kind: &str) -> Result<(String, bool), TopicError> {
    if !EVENT_KINDS.contains(&kind) {
        return Err(TopicError { what: "trunk event kind", value: kind.to_string() });
    }
    caster(&["Event", kind], false)
}
/// `trunk/outgoing/command` — NOT retained.
pub fn command_topic() -> (String, bool) {
    caster(&["outgoing", "command"], false).expect("literal segments")
}
/// `trunk/incoming/result` — NOT retained.
pub fn result_topic() -> (String, bool) {
    caster(&["incoming", "result"], false).expect("literal segments")
}
/// `trunk_sim/{protocol}/Frame/{frame}/state` — the simulator's frame, RETAINED.
pub fn sim_frame_topic(protocol: &str, frame: &str) -> Result<(String, bool), TopicError> {
    sim(protocol, &["Frame", frame, "state"], true)
}
/// `trunk_sim/{protocol}/Frame/{frame}/line` — caster → sim, the MQTT twins of TCP lines as a batch (`render_line_batch`), NOT retained.
pub fn line_topic(protocol: &str, frame: &str) -> Result<(String, bool), TopicError> {
    sim(protocol, &["Frame", frame, "line"], false)
}
/// `trunk_sim/{protocol}/Frame/{frame}/ack` — sim → caster, the applied lines as a batch (`render_ack_batch`), NOT retained.
pub fn ack_topic(protocol: &str, frame: &str) -> Result<(String, bool), TopicError> {
    sim(protocol, &["Frame", frame, "ack"], false)
}
/// `trunk_sim/{protocol}/Frame/{frame}/reject` — sim → caster, `<reason>|<line>`, NOT retained.
pub fn reject_topic(protocol: &str, frame: &str) -> Result<(String, bool), TopicError> {
    sim(protocol, &["Frame", frame, "reject"], false)
}
/// `trunk_sim/{protocol}/Frame/{frame}/voice{cut}` — one voice chunk, NOT retained. `cut` is 1 or 5.
pub fn voice_topic(protocol: &str, frame: &str, cut: u32) -> Result<(String, bool), TopicError> {
    if !VOICE_CUTS.contains(&cut) {
        return Err(TopicError { what: "voice cut", value: cut.to_string() });
    }
    let leaf = format!("voice{cut}");
    sim(protocol, &["Frame", frame, &leaf], false)
}
/// `trunk_sim/{protocol}/Frames/line` — caster → sim, every twin to the protocol's frames in one flush (`render_protocol_line_batch`), NOT retained (0.7.0).
pub fn protocol_line_topic(protocol: &str) -> Result<(String, bool), TopicError> {
    sim(protocol, &[PROTOCOL_LEVEL, "line"], false)
}
/// `trunk_sim/{protocol}/Frames/ack` — sim → caster, every line the protocol's frames applied in one flush (`render_protocol_ack_batch`), NOT retained (0.7.0).
pub fn protocol_ack_topic(protocol: &str) -> Result<(String, bool), TopicError> {
    sim(protocol, &[PROTOCOL_LEVEL, "ack"], false)
}
/// `trunk_sim/{protocol}/Frames/state` — the whole protocol's SimFrames (`render_protocol_state`), RETAINED; erased when the protocol hosts nothing (0.7.0).
pub fn protocol_state_topic(protocol: &str) -> Result<(String, bool), TopicError> {
    sim(protocol, &[PROTOCOL_LEVEL, "state"], true)
}
/// `APK.audio/System/Protocols/trunk/#`.
pub fn caster_wildcard() -> String {
    format!("{}/#", protocols(TOKEN, &[]).expect("literal segment"))
}
/// `trunk_sim/{protocol}/#`, or every simulator's `trunk_sim/#`.
pub fn sim_wildcard(protocol: Option<&str>) -> Result<String, TopicError> {
    Ok(format!("{}/#", protocols(SIM_TOKEN, protocol.as_slice())?))
}
/// Split `…/trunk_sim/{protocol}/Frame/{frame}/{leaf}` into its three parts.
pub fn parse_sim_topic(topic: &str) -> Option<(String, String, String)> {
    let head = format!("{}/", protocols(SIM_TOKEN, &[]).ok()?);
    let rest = topic.strip_prefix(&head)?;
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [p, "Frame", f, leaf] if !p.is_empty() && !f.is_empty() && !leaf.is_empty() => {
            Some((p.to_string(), f.to_string(), leaf.to_string()))
        }
        _ => None,
    }
}

/// Split `…/trunk_sim/{protocol}/Frames/{leaf}` into (protocol, leaf); the leaf
/// is one of [`PROTOCOL_LEAVES`], so the runner's `trunk_sim/incoming/state`
/// and the per-frame topics are never taken for one.
pub fn parse_sim_protocol_topic(topic: &str) -> Option<(String, String)> {
    let head = format!("{}/", protocols(SIM_TOKEN, &[]).ok()?);
    let rest = topic.strip_prefix(&head)?;
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [p, level, leaf] if !p.is_empty() && *level == PROTOCOL_LEVEL && PROTOCOL_LEAVES.contains(leaf) => Some((p.to_string(), leaf.to_string())),
        _ => None,
    }
}

/// The frame of a `trunk/Frame/{frame}` topic; `None` for anything else.
pub fn parse_frame_doc_topic(topic: &str) -> Option<String> {
    let head = format!("{}/{FRAME_DOC_LEVEL}/", protocols(TOKEN, &[]).ok()?);
    let frame = topic.strip_prefix(&head)?;
    crate::topics::is_segment(frame).then(|| frame.to_string())
}
