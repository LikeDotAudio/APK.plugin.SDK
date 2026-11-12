// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `trunk.rs` — see that file for the whole picture.

use super::*;

// ---------------------------------------------------------------------------
// The per-frame document: one retained word per frame (0.8.0, ROAD-3467)
// ---------------------------------------------------------------------------

/// The level of the caster's per-frame documents: `trunk/Frame/{frame}`.
pub const FRAME_DOC_LEVEL: &str = "Frame";

/// One frame's whole state as the caster folds it (`trunk/Frame/{frame}`, 0.8.0):
/// the frame's own advertisement (`device`, the 0.7.0 **Frame** document), the
/// trunks whose `a_frame` it is, the connections whose `from_frame` it is, and
/// its closed crosspoints (the 0.7.0 **Crosspoints** document). Every trunk and
/// every connection sits in exactly one frame's document, so a reader that
/// unions the documents has every one once.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameDoc {
    pub frame: String,
    pub device: Option<Value>,
    pub trunks: std::collections::BTreeMap<String, Value>,
    pub connections: std::collections::BTreeMap<String, Value>,
    pub crosspoints: Option<Value>,
}

impl FrameDoc {
    /// Nothing left to say for the frame: its document is erased, never stated empty.
    pub fn is_empty(&self) -> bool {
        self.device.is_none() && self.trunks.is_empty() && self.connections.is_empty() && self.crosspoints.is_none()
    }
}

/// The `trunk/Frame/{frame}` payload: `{"wire_version":1,"frame":"…","device":{…}?,
/// "trunks":{"<trunk>":Trunk},"connections":{"<connection>":Connection},"crosspoints":{…}?}`.
/// `device` and `crosspoints` are left out when the frame has none; `trunks`
/// and `connections` are always written (sorted by id), empty or not.
pub fn render_frame_doc(doc: &FrameDoc) -> String {
    let mut m = Map::new();
    m.insert("wire_version".into(), WIRE_VERSION.into());
    m.insert("frame".into(), doc.frame.clone().into());
    if let Some(d) = &doc.device {
        m.insert("device".into(), d.clone());
    }
    m.insert("trunks".into(), Value::Object(doc.trunks.iter().map(|(k, v)| (k.clone(), v.clone())).collect()));
    m.insert("connections".into(), Value::Object(doc.connections.iter().map(|(k, v)| (k.clone(), v.clone())).collect()));
    if let Some(x) = &doc.crosspoints {
        m.insert("crosspoints".into(), x.clone());
    }
    Value::Object(m).to_string()
}

fn frame_doc_entries(doc: &Value, key: &str, owner: &str, frame: &str) -> Result<std::collections::BTreeMap<String, Value>, String> {
    let mut out = std::collections::BTreeMap::new();
    let Some(v) = doc.get(key) else { return Ok(out) };
    if v.is_null() {
        return Ok(out);
    }
    let Some(by) = v.as_object() else { return Err(format!("`{key}` is not an object keyed by id")) };
    for (id, e) in by {
        if !crate::topics::is_segment(id) {
            return Err(format!("`{key}.{id}` is not an id (one topic segment)"));
        }
        let Some(m) = e.as_object() else { return Err(format!("`{key}.{id}` is not an object")) };
        if m.get("id").is_some_and(|x| x.as_str() != Some(id)) {
            return Err(format!("`{key}.{id}` names another id"));
        }
        if m.get(owner).is_some_and(|x| x.as_str() != Some(frame)) {
            return Err(format!("`{key}.{id}` belongs in its `{owner}`'s document, not `{frame}`'s"));
        }
        out.insert(id.clone(), e.clone());
    }
    Ok(out)
}

/// A `trunk/Frame/{frame}` document read back. Refused: an empty payload (that
/// is the erase), not a JSON object, no `frame` segment, a `device` that is not
/// an object or names another frame, a trunk or connection whose key is not a
/// segment, whose entry is not an object, names another id, or belongs in
/// another frame's document (`a_frame` / `from_frame`), `crosspoints` that is
/// not an object, names another frame or has a non-array `closed`, and a
/// document with nothing in it. Other top-level keys are ignored.
pub fn parse_frame_doc(payload: &str) -> Result<FrameDoc, String> {
    let text = payload.trim();
    if text.is_empty() {
        return Err("an empty payload is the frame's erase, not a document".into());
    }
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    if !doc.is_object() {
        return Err("not a JSON object".into());
    }
    let frame = doc.get("frame").and_then(Value::as_str).filter(|f| crate::topics::is_segment(f)).ok_or("no `frame` segment")?.to_string();
    let device = match doc.get("device") {
        None | Some(Value::Null) => None,
        Some(d) => {
            let Some(m) = d.as_object() else { return Err("`device` is not a Frame object".into()) };
            if m.get("id").is_some_and(|x| x.as_str() != Some(frame.as_str())) {
                return Err("`device` names another frame".into());
            }
            Some(d.clone())
        }
    };
    let trunks = frame_doc_entries(&doc, "trunks", "a_frame", &frame)?;
    let connections = frame_doc_entries(&doc, "connections", "from_frame", &frame)?;
    let crosspoints = match doc.get("crosspoints") {
        None | Some(Value::Null) => None,
        Some(x) => {
            let Some(m) = x.as_object() else { return Err("`crosspoints` is not a Crosspoints object".into()) };
            if m.get("frame").is_some_and(|f| f.as_str() != Some(frame.as_str())) {
                return Err("`crosspoints` names another frame".into());
            }
            if m.get("closed").is_some_and(|c| !c.is_array()) {
                return Err("`crosspoints.closed` is not an array".into());
            }
            Some(x.clone())
        }
    };
    let out = FrameDoc { frame, device, trunks, connections, crosspoints };
    if out.is_empty() {
        return Err("a frame with nothing to say is erased, not stated".into());
    }
    Ok(out)
}
