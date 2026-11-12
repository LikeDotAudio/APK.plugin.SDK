// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `trunk.rs` — see that file for the whole picture.

use super::*;

// ---------------------------------------------------------------------------
// Protocol batches: one message per PROTOCOL per flush (0.7.0, ROAD-3467)
// ---------------------------------------------------------------------------

/// The topic level that says "every frame of this protocol": `trunk_sim/{protocol}/Frames/{leaf}`.
pub const PROTOCOL_LEVEL: &str = "Frames";
/// The protocol-level leaves: the twins, the acks, the whole protocol's SimFrame state.
pub const PROTOCOL_LEAVES: [&str; 3] = ["line", "ack", "state"];

fn render_frames_batch(key: &str, frames: &std::collections::BTreeMap<String, Vec<String>>) -> String {
    let mut by = Map::new();
    for (frame, entries) in frames {
        if !entries.is_empty() {
            by.insert(frame.clone(), Value::Array(entries.iter().cloned().map(Value::String).collect()));
        }
    }
    let mut m = Map::new();
    m.insert(key.to_string(), Value::Object(by));
    Value::Object(m).to_string()
}

fn parse_frames_batch(key: &str, payload: &str) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    let text = payload.trim();
    if text.is_empty() {
        return Err("an empty payload is no batch".into());
    }
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let Some(by) = doc.get(key).and_then(Value::as_object) else { return Err(format!("no `{key}` object of frames")) };
    if by.is_empty() {
        return Err(format!("`{key}` names no frame: an empty batch is never sent"));
    }
    let mut out = std::collections::BTreeMap::new();
    let mut total = 0usize;
    for (frame, list) in by {
        if !crate::topics::is_segment(frame) {
            return Err(format!("`{frame}` is not a frame id (one topic segment)"));
        }
        let Some(list) = list.as_array().filter(|l| !l.is_empty()) else { return Err(format!("`{key}.{frame}` is not a non-empty array")) };
        total += list.len();
        if total > BATCH_MAX {
            return Err(format!("`{key}` holds more than {BATCH_MAX} entries"));
        }
        let entries: Result<Vec<String>, String> = list
            .iter()
            .map(|v| v.as_str().filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string()).ok_or(format!("a `{key}.{frame}` entry is not a non-empty string")))
            .collect();
        out.insert(frame.clone(), entries?);
    }
    Ok(out)
}

/// The `…/Frames/line` payload: every twin to a protocol's frames in one flush,
/// per frame in TCP order — `{"lines": {"<frame>": ["<line>", …], …}}` (frames
/// sorted; a frame with no entry is left out).
pub fn render_protocol_line_batch(frames: &std::collections::BTreeMap<String, Vec<String>>) -> String {
    render_frames_batch(LINE_BATCH_KEY, frames)
}
/// The `…/Frames/ack` payload: every line a protocol's frames applied in one
/// flush, per frame in order — `{"acks": {"<frame>": [...], …}}`.
pub fn render_protocol_ack_batch(frames: &std::collections::BTreeMap<String, Vec<String>>) -> String {
    render_frames_batch(ACK_BATCH_KEY, frames)
}
/// Frame → its twins of a `…/Frames/line` payload, each frame's in order.
/// At most `BATCH_MAX` entries across all frames; every frame key a segment.
pub fn parse_protocol_line_batch(payload: &str) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    parse_frames_batch(LINE_BATCH_KEY, payload)
}
/// Frame → its applied lines of a `…/Frames/ack` payload, each frame's in order.
pub fn parse_protocol_ack_batch(payload: &str) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    parse_frames_batch(ACK_BATCH_KEY, payload)
}

/// The `…/Frames/state` document: EVERY frame the protocol hosts, each its
/// whole SimFrame document — `{"wire_version":1,"protocol":"…","frames":{"<frame>": SimFrame, …}}`.
/// RETAINED and restated whole, so a frame missing from it is a frame not hosted.
pub fn render_protocol_state(protocol: &str, frames: &std::collections::BTreeMap<String, Value>) -> String {
    let by: Map<String, Value> = frames.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    json!({"wire_version": WIRE_VERSION, "protocol": protocol, "frames": by}).to_string()
}

/// (protocol, frame → SimFrame) of a `…/Frames/state` document. Refused: no
/// protocol segment, no frames (a protocol with none is erased, not stated),
/// a frame key that is not a segment, an entry that is not an object, or an
/// entry whose own `frame` / `protocol` disagrees with where it sits.
pub fn parse_protocol_state(payload: &str) -> Result<(String, std::collections::BTreeMap<String, Value>), String> {
    let doc: Value = serde_json::from_str(payload.trim()).map_err(|e| format!("not JSON: {e}"))?;
    let protocol = doc.get("protocol").and_then(Value::as_str).filter(|p| crate::topics::is_segment(p)).ok_or("no `protocol` segment")?;
    let Some(by) = doc.get("frames").and_then(Value::as_object).filter(|m| !m.is_empty()) else { return Err("no `frames`: a protocol hosting none is erased".into()) };
    let mut out = std::collections::BTreeMap::new();
    for (frame, sf) in by {
        if !crate::topics::is_segment(frame) {
            return Err(format!("`{frame}` is not a frame id"));
        }
        let Some(m) = sf.as_object() else { return Err(format!("frames.{frame} is not a SimFrame object")) };
        if m.get("frame").is_some_and(|f| f.as_str() != Some(frame)) || m.get("protocol").is_some_and(|p| p.as_str() != Some(protocol)) {
            return Err(format!("frames.{frame} names another frame or protocol"));
        }
        out.insert(frame.clone(), sf.clone());
    }
    Ok((protocol.to_string(), out))
}
