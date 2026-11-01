// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Trunk-Caster on the bus: the Ward-Beck line, the identities, the panel
//! labels, the caster's command envelope and every topic (ROAD-3440).
//!
//! ```text
//!   APK.audio/System/Protocols/trunk/…                       the caster (PLUGIN.TRUNK_CASTER)
//!   APK.audio/System/Protocols/trunk_sim/{protocol}/…        one frame simulator per protocol (PLUGIN.TRUNK_SIM)
//! ```
//!
//! The full topic table, retain classes and payload shapes are
//! `contracts/TRUNK.md`; the cases every twin is held to are
//! `contracts/vectors/trunk.json`. Ported from the Trunk-Caster SDK
//! (`PODS/POD.THICK/INTERCOM.Caster/SRC/TRUNK-CASTER-SDK/src/wardbeck.rs`,
//! `labels.rs`, `sites.rs`, `types.rs`) — only what crosses the bus.
//!
//! THE WARD-BECK LINE IS THE UNIFYING WORD. Every crosspoint on every frame of
//! every protocol is one line, `SITE,FRAME,INPUT,INPUT_NAME,CLOSE/OPEN,
//! OUTPUT_NAME,OUTPUT,FRAME,SITE`; a check-in is the line whose INPUT is
//! `CHECKIN`. It travels on TCP 7879 and, twinned, in batches: since 0.7.0 one
//! message per PROTOCOL per flush on `trunk_sim/{protocol}/Frames/line`, each
//! entry under its frame (0.6.0 was one per frame on `…/Frame/{frame}/line`,
//! still read for one release).
//!
//! THE COMMAND REPLACES THE HTTP API. Each of the runtime's 21 POST routes is a
//! verb on `trunk/outgoing/command`, answered on `trunk/incoming/result`.
//! A body with a key this module does not know is refused whole.
use serde_json::{json, Map, Value};

use crate::topics::{protocols, TopicError};

/// The caster's bus token.
pub const TOKEN: &str = "trunk";
/// The frame simulators' bus token; the protocol is the next level.
pub const SIM_TOKEN: &str = "trunk_sim";
/// The `wire_version` every trunk document carries.
pub const WIRE_VERSION: u32 = 1;
/// The INPUT that marks a check-in line.
pub const CHECKIN: &str = "CHECKIN";
/// The two voice cuts, in seconds.
pub const VOICE_CUTS: [u32; 2] = [1, 5];
/// The non-retained event streams under `trunk/Event/{kind}`.
pub const EVENT_KINDS: [&str; 5] = ["Crosspoint", "Gpio", "Label", "Gain", "Relay"];
/// The caster's read-only views, one retained document each (`trunk/View/{name}`) — what the
/// imported runtime served as GET routes (ROAD-3453).
pub const VIEW_NAMES: [&str; 9] = ["Status", "Report", "Regions", "PhoneBook", "Bandwidth", "Captures", "Presets", "Productions", "Globe"];
/// The keyed views, one retained document per key (`trunk/View/{name}/{key}`): a frame's tap
/// strip, one capture analysed.
pub const VIEW_ITEM_NAMES: [&str; 2] = ["Taps", "Capture"];
/// Panel widths a label is cut for.
pub const LABEL_WIDTHS: [usize; 3] = [6, 8, 16];
/// Where a site sits in the fleet.
pub const TIERS: [&str; 3] = ["capital", "province", "city"];
/// A protocol's broad family.
pub const FAMILIES: [&str; 7] =
    ["analog-audio", "audio-over-ip", "telephony", "control", "wireless", "media-transport", "other"];
/// A booking's life.
pub const BOOKING_STATUSES: [&str; 5] = ["booked", "on-air", "done", "failed", "missed"];
/// What a booked port is.
pub const PORT_KINDS: [&str; 2] = ["panel", "input"];
/// The GPIO pin functions, pin 1 first.
pub const GPIO_FUNCTIONS: [&str; 8] =
    ["blink-panel", "check-lights", "set-latch", "set-listen", "unset-latch", "clear", "last-caller", "ping"];

/// The answer's `status`.
pub const STATUS_DONE: &str = "done";
pub const STATUS_REFUSED: &str = "refused";
pub const STATUS_ERROR: &str = "error";
pub const STATUSES: [&str; 3] = [STATUS_DONE, STATUS_REFUSED, STATUS_ERROR];

/// Every key a command envelope may carry.
pub const COMMAND_KEYS: [&str; 4] = ["id", "verb", "args", "issued_at_ms"];

// ---------------------------------------------------------------------------
// The Ward-Beck line
// ---------------------------------------------------------------------------

/// One Ward-Beck line. `switch` is `true` for CLOSE, `false` for OPEN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub site: String,
    pub frame: String,
    pub input: String,
    pub input_name: String,
    pub close: bool,
    pub output_name: String,
    pub output: String,
    pub out_frame: String,
    pub out_site: String,
}

impl Line {
    /// A check-in: `SITE,FRAME,CHECKIN,<when>,CLOSE|OPEN,<n> crosspoints,<n>,FRAME,SITE`.
    pub fn checkin(site: &str, frame: &str, when: &str, closed: usize) -> Line {
        Line {
            site: site.into(),
            frame: frame.into(),
            input: CHECKIN.into(),
            input_name: when.into(),
            close: closed > 0,
            output_name: format!("{closed} crosspoints"),
            output: closed.to_string(),
            out_frame: frame.into(),
            out_site: site.into(),
        }
    }
    pub fn is_checkin(&self) -> bool {
        self.input == CHECKIN
    }
    /// Same frame, same site: a crosspoint inside one matrix.
    pub fn is_local(&self) -> bool {
        self.frame == self.out_frame && self.site == self.out_site
    }
    /// The opposite switch, same crosspoint.
    pub fn flipped(&self) -> Line {
        Line { close: !self.close, ..self.clone() }
    }
    /// Parse one line. Fields are trimmed; a field holding a comma or a quote
    /// is double-quoted, a quote inside doubled.
    pub fn parse(text: &str) -> Result<Line, String> {
        let f = split_csv(text.trim());
        if f.len() != 9 {
            return Err(format!("expected 9 fields, got {}", f.len()));
        }
        let close = match f[4].to_ascii_uppercase().as_str() {
            "CLOSE" => true,
            "OPEN" => false,
            other => return Err(format!("field 5 must be CLOSE or OPEN, got {other}")),
        };
        Ok(Line {
            site: f[0].clone(),
            frame: f[1].clone(),
            input: f[2].clone(),
            input_name: f[3].clone(),
            close,
            output_name: f[5].clone(),
            output: f[6].clone(),
            out_frame: f[7].clone(),
            out_site: f[8].clone(),
        })
    }
    /// The canonical spelling.
    pub fn render(&self) -> String {
        [
            q(&self.site),
            q(&self.frame),
            q(&self.input),
            q(&self.input_name),
            (if self.close { "CLOSE" } else { "OPEN" }).to_string(),
            q(&self.output_name),
            q(&self.output),
            q(&self.out_frame),
            q(&self.out_site),
        ]
        .join(",")
    }
    /// As a JSON object (`switch` is `"CLOSE"`/`"OPEN"`) — the shape the vectors carry.
    pub fn to_value(&self) -> Value {
        json!({
            "site": self.site, "frame": self.frame, "input": self.input, "input_name": self.input_name,
            "switch": if self.close { "CLOSE" } else { "OPEN" },
            "output_name": self.output_name, "output": self.output, "out_frame": self.out_frame, "out_site": self.out_site,
        })
    }
}

fn q(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn split_csv(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_q && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => in_q = !in_q,
            ',' if !in_q => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

/// The `…/reject` payload: `<reason>|<line>`.
pub fn render_reject(reason: &str, line: &str) -> String {
    format!("{}|{line}", reason.replace('|', "/"))
}

/// Split a `…/reject` payload into (reason, line). The reason never holds `|`.
pub fn parse_reject(payload: &str) -> Option<(String, String)> {
    let (reason, line) = payload.split_once('|')?;
    Some((reason.to_string(), line.to_string()))
}

// ---------------------------------------------------------------------------
// Batches: the line twin and the ack, many lines a message (0.6.0, ROAD-3457)
// ---------------------------------------------------------------------------

/// The most entries one `…/line` or `…/ack` batch carries; a bigger one is refused.
pub const BATCH_MAX: usize = 500;
/// The key of a `…/line` batch: `{"lines": ["<line>", …]}`.
pub const LINE_BATCH_KEY: &str = "lines";
/// The key of a `…/ack` batch: `{"acks": ["<line>", …]}`.
pub const ACK_BATCH_KEY: &str = "acks";

fn render_batch(key: &str, entries: &[String]) -> String {
    let mut m = Map::new();
    m.insert(key.to_string(), Value::Array(entries.iter().cloned().map(Value::String).collect()));
    Value::Object(m).to_string()
}

fn parse_batch(key: &str, payload: &str) -> Result<Vec<String>, String> {
    let text = payload.trim();
    if text.is_empty() {
        return Err("an empty payload is no batch".into());
    }
    if !text.starts_with('{') {
        // A 0.5.0 writer's single bare line: a batch of one.
        return Ok(vec![text.to_string()]);
    }
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let Some(list) = doc.get(key).and_then(Value::as_array) else { return Err(format!("no `{key}` array")) };
    if list.is_empty() {
        return Err(format!("`{key}` is empty: an empty batch is never sent"));
    }
    if list.len() > BATCH_MAX {
        return Err(format!("`{key}` holds {} entries, more than {BATCH_MAX}", list.len()));
    }
    list.iter()
        .map(|v| v.as_str().filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string()).ok_or(format!("a `{key}` entry is not a non-empty string")))
        .collect()
}

/// The `…/line` payload: every Ward-Beck line twinned to one frame in one flush,
/// in the order they went down TCP — `{"lines": [...]}`.
pub fn render_line_batch(lines: &[String]) -> String {
    render_batch(LINE_BATCH_KEY, lines)
}
/// The `…/ack` payload: every line the frame applied in one flush, in order — `{"acks": [...]}`.
pub fn render_ack_batch(acks: &[String]) -> String {
    render_batch(ACK_BATCH_KEY, acks)
}
/// The lines of a `…/line` payload, in order. A bare Ward-Beck line (a 0.5.0
/// writer) reads as a batch of one. Entries are trimmed text, not parsed: each
/// line is still validated on its own by the reader.
pub fn parse_line_batch(payload: &str) -> Result<Vec<String>, String> {
    parse_batch(LINE_BATCH_KEY, payload)
}
/// The applied lines of a `…/ack` payload, in order; a bare line reads as one.
pub fn parse_ack_batch(payload: &str) -> Result<Vec<String>, String> {
    parse_batch(ACK_BATCH_KEY, payload)
}

/// The writer's side of a batch (Rust only; the payload rules above are the
/// contract, this is one way to keep them). Entries are collected per topic
/// (one frame's `…/line` or `…/ack`, or since 0.7.0 one protocol's
/// `…/Frames/line|ack` with `(frame, line)` entries — see [`by_frame`]); a
/// topic's batch is due once its OLDEST entry has waited `window`, and is
/// handed back at once when it reaches `cap` (clamped to 1..=`BATCH_MAX`).
/// `drain` hands back every partial batch, so a writer that stops loses
/// nothing. Order inside a topic is push order.
#[derive(Debug)]
pub struct Batcher<E = String> {
    window: std::time::Duration,
    cap: usize,
    open: std::collections::BTreeMap<String, (std::time::Instant, Vec<E>)>,
}

impl<E> Batcher<E> {
    pub fn new(window: std::time::Duration, cap: usize) -> Batcher<E> {
        Batcher { window, cap: cap.clamp(1, BATCH_MAX), open: Default::default() }
    }

    /// Add one entry to `topic`'s batch. The batch, when this entry filled it to the cap.
    pub fn push(&mut self, topic: &str, entry: E, now: std::time::Instant) -> Option<(String, Vec<E>)> {
        let (_, list) = self.open.entry(topic.to_string()).or_insert_with(|| (now, Vec::new()));
        list.push(entry);
        if list.len() >= self.cap {
            return self.open.remove(topic).map(|(_, l)| (topic.to_string(), l));
        }
        None
    }

    /// Every batch whose oldest entry has waited `window` by `now`.
    pub fn due(&mut self, now: std::time::Instant) -> Vec<(String, Vec<E>)> {
        let w = self.window;
        let ripe: Vec<String> =
            self.open.iter().filter(|(_, (first, _))| now.saturating_duration_since(*first) >= w).map(|(t, _)| t.clone()).collect();
        ripe.into_iter().filter_map(|t| self.open.remove(&t).map(|(_, l)| (t, l))).collect()
    }

    /// Every open batch, due or not: the final flush.
    pub fn drain(&mut self) -> Vec<(String, Vec<E>)> {
        std::mem::take(&mut self.open).into_iter().map(|(t, (_, l))| (t, l)).collect()
    }

    /// Entries waiting, across every topic.
    pub fn pending(&self) -> usize {
        self.open.values().map(|(_, l)| l.len()).sum()
    }
}

/// A protocol batch's `(frame, line)` entries, grouped per frame with each
/// frame's order kept: what `render_protocol_line_batch` / `…_ack_batch` take.
pub fn by_frame(entries: Vec<(String, String)>) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut out: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (frame, line) in entries {
        out.entry(frame).or_default().push(line);
    }
    out
}

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

// ---------------------------------------------------------------------------
// Identities
// ---------------------------------------------------------------------------

/// A user port: `U<nn>` (two digits at least).
pub fn user_port(n: usize) -> String {
    format!("U{n:02}")
}

/// A caster (trunk port): `C<n>`.
pub fn caster_port(n: usize) -> String {
    format!("C{n}")
}

/// `U05` → ("user", 5); `C3` → ("caster", 3). Only the canonical spelling.
pub fn parse_port(s: &str) -> Option<(&'static str, usize)> {
    let (kind, digits) = match s.as_bytes().first()? {
        b'U' => ("user", &s[1..]),
        b'C' => ("caster", &s[1..]),
        _ => return None,
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: usize = digits.parse().ok()?;
    let canonical = if kind == "user" { user_port(n) } else { caster_port(n) };
    (canonical == s).then_some((kind, n))
}

/// A frame: `<protocol or site slug>-<nn>`.
pub fn frame_id(prefix: &str, n: usize) -> String {
    format!("{prefix}-{n:02}")
}

/// A trunk: `trunk-<nnnnn>`.
pub fn trunk_id(n: usize) -> String {
    format!("trunk-{n:05}")
}

/// A site's slug: lower-case ASCII alphanumerics joined by single `-`.
pub fn slugify(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// A channel of a trunk is its index, `0..capacity`.
pub fn is_channel(index: usize, capacity: usize) -> bool {
    index < capacity
}

// ---------------------------------------------------------------------------
// Panel labels: 6, 8 and 16 characters, an emoji counted once
// ---------------------------------------------------------------------------

/// A name cut for every panel width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub full: String,
    pub l6: String,
    pub l8: String,
    pub l16: String,
    pub chars: usize,
    pub over6: bool,
    pub over8: bool,
    pub over16: bool,
}

impl Label {
    pub fn new(full: &str) -> Label {
        let full = full.trim();
        let g = graphemes(full);
        let cut = |n: usize| g.iter().take(n).cloned().collect::<String>();
        Label {
            full: full.to_string(),
            l6: cut(6),
            l8: cut(8),
            l16: cut(16),
            chars: g.len(),
            over6: g.len() > 6,
            over8: g.len() > 8,
            over16: g.len() > 16,
        }
    }
    /// Derive, then apply explicit cuts (each fitted to its width).
    pub fn with_overrides(full: &str, l6: Option<&str>, l8: Option<&str>, l16: Option<&str>) -> Label {
        let mut l = Label::new(full);
        if let Some(s) = l6 {
            l.l6 = fit(s, 6);
        }
        if let Some(s) = l8 {
            l.l8 = fit(s, 8);
        }
        if let Some(s) = l16 {
            l.l16 = fit(s, 16);
        }
        l
    }
    pub fn to_value(&self) -> Value {
        json!({
            "full": self.full, "l6": self.l6, "l8": self.l8, "l16": self.l16, "chars": self.chars,
            "over6": self.over6, "over8": self.over8, "over16": self.over16,
        })
    }
}

/// The first `n` panel characters of `s`.
pub fn fit(s: &str, n: usize) -> String {
    graphemes(s.trim()).into_iter().take(n).collect()
}

/// Panel characters in `s`.
pub fn width(s: &str) -> usize {
    graphemes(s).len()
}

fn is_combining(c: char) -> bool {
    let u = c as u32;
    (0x0300..=0x036F).contains(&u)
        || (0x1AB0..=0x1AFF).contains(&u)
        || (0x1DC0..=0x1DFF).contains(&u)
        || (0x20D0..=0x20FF).contains(&u)
        || (0xFE00..=0xFE0F).contains(&u)
        || (0xFE20..=0xFE2F).contains(&u)
        || (0x1F3FB..=0x1F3FF).contains(&u)
        || (0xE0020..=0xE007F).contains(&u)
        || u == 0x200C
        || u == 0x200D
}

fn is_regional_indicator(c: char) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&(c as u32))
}

/// Grapheme clusters, the Trunk-Caster rule: combining marks, variation
/// selectors, skin tones, ZWJ sequences, flag pairs and CRLF are one each.
pub fn graphemes(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut join_next = false;
    let mut pending_flag = false;
    for c in s.chars() {
        let glue = join_next
            || is_combining(c)
            || (pending_flag && is_regional_indicator(c))
            || (c == '\n' && out.last().is_some_and(|g| g == "\r"));
        if glue && !out.is_empty() {
            out.last_mut().unwrap().push(c);
        } else {
            out.push(c.to_string());
        }
        join_next = c == '\u{200D}';
        pending_flag = is_regional_indicator(c) && !glue;
    }
    out
}

// ---------------------------------------------------------------------------
// The command envelope
// ---------------------------------------------------------------------------

/// One argument's type: `s` non-empty string, `i` non-negative integer, `n`
/// finite number, `b` boolean, `a` array, `o` object.
type Arg = (&'static str, char);

/// One verb: the HTTP route it replaces, its required and optional arguments,
/// and whether its args are a whole document the caster reads (`free`).
pub struct VerbSpec {
    pub verb: &'static str,
    pub route: &'static str,
    pub required: &'static [Arg],
    pub optional: &'static [Arg],
    pub free: bool,
}

/// The closed verb set: one per POST route of the old runtime.
pub const VERBS: [VerbSpec; 21] = [
    VerbSpec { verb: "connect", route: "/api/request", required: &[("who", 's'), ("from", 's'), ("to", 's')],
        optional: &[("from_port", 'i'), ("to_port", 'i'), ("why", 's'), ("where", 's'), ("hold_seconds", 'n'), ("gpio", 'a')], free: false },
    VerbSpec { verb: "release", route: "/api/release", required: &[("connection", 's')], optional: &[], free: false },
    VerbSpec { verb: "rename", route: "/api/rename", required: &[("frame", 's'), ("port", 'i'), ("name", 's')],
        optional: &[("l6", 's'), ("l8", 's'), ("l16", 's')], free: false },
    VerbSpec { verb: "book", route: "/api/bookings", required: &[("frame", 's'), ("port", 'i'), ("program", 's'), ("start_hour", 'n'), ("minutes", 'n')],
        optional: &[("kind", 's'), ("who", 's'), ("to", 's'), ("daily", 'b')], free: false },
    VerbSpec { verb: "cancel_booking", route: "/api/bookings/cancel", required: &[("booking", 's')], optional: &[], free: false },
    VerbSpec { verb: "crosspoint_reset", route: "/api/crosspoints/reset", required: &[], optional: &[], free: false },
    VerbSpec { verb: "crosspoint_settings", route: "/api/crosspoints/settings", required: &[], optional: &[], free: true },
    VerbSpec { verb: "fader", route: "/api/fader", required: &[], optional: &[], free: true },
    VerbSpec { verb: "fader_preset", route: "/api/fader/preset", required: &[("preset", 's')], optional: &[], free: false },
    VerbSpec { verb: "fader_quick", route: "/api/fader/quick", required: &[], optional: &[("speed", 'n'), ("running", 'b')], free: false },
    VerbSpec { verb: "clock", route: "/api/clock", required: &[("hour", 'n')], optional: &[], free: false },
    VerbSpec { verb: "gain", route: "/api/gain", required: &[("db", 'n')],
        optional: &[("frame", 's'), ("input", 's'), ("output", 's'), ("trunk", 's'), ("end", 's'), ("connection", 's'), ("hop", 'i'), ("who", 's')], free: false },
    VerbSpec { verb: "taps_unity", route: "/api/taps/unity", required: &[], optional: &[("frame", 's'), ("who", 's')], free: false },
    VerbSpec { verb: "relay_bind", route: "/api/relay/bind",
        required: &[("matrix", 's'), ("port", 'i'), ("device_id", 's'), ("device_label", 's'), ("kind", 's')], optional: &[], free: false },
    VerbSpec { verb: "relay_unbind", route: "/api/relay/unbind", required: &[("matrix", 's'), ("port", 'i')], optional: &[], free: false },
    VerbSpec { verb: "relay_event", route: "/api/relay/event", required: &[("kind", 's'), ("device_label", 's')],
        optional: &[("matrix", 's'), ("port", 'i'), ("peak_dbfs", 'n'), ("threshold_dbfs", 'n'), ("note", 's')], free: false },
    VerbSpec { verb: "gpio", route: "/api/gpio", required: &[("pin", 'i'), ("state", 'b')],
        optional: &[("connection", 's'), ("function", 's'), ("note", 's')], free: false },
    VerbSpec { verb: "panel_seek", route: "/api/panel-seek", required: &[("who", 's'), ("from", 's'), ("panel_frame", 's')],
        optional: &[("panel_port", 'i'), ("why", 's')], free: false },
    VerbSpec { verb: "capture_start", route: "/api/capture/start", required: &[], optional: &[("note", 's')], free: false },
    VerbSpec { verb: "capture_stop", route: "/api/capture/stop", required: &[], optional: &[], free: false },
    VerbSpec { verb: "globe_config", route: "/api/globe/config", required: &[], optional: &[], free: true },
];

pub fn verb_spec(verb: &str) -> Option<&'static VerbSpec> {
    VERBS.iter().find(|v| v.verb == verb)
}

/// One command, as the caster reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub id: String,
    pub verb: String,
    pub args: Map<String, Value>,
    pub issued_at_ms: Option<u64>,
}

impl Command {
    pub fn to_json(&self) -> String {
        let mut m = Map::new();
        m.insert("id".into(), Value::String(self.id.clone()));
        m.insert("verb".into(), Value::String(self.verb.clone()));
        m.insert("args".into(), Value::Object(self.args.clone()));
        if let Some(t) = self.issued_at_ms {
            m.insert("issued_at_ms".into(), json!(t));
        }
        Value::Object(m).to_string()
    }
}

/// Why a body was not a command, with the id when one could be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub id: Option<String>,
    pub reason: String,
}

/// A command id: 1–80 of `A-Z a-z 0-9 . _ : -`.
pub fn is_command_id(s: &str) -> bool {
    (1..=80).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

fn as_int(v: &Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    let f = v.as_f64()?;
    (f.is_finite() && f >= 0.0 && f.fract() == 0.0 && f <= 9_007_199_254_740_991.0).then_some(f as u64)
}

fn type_ok(v: &Value, t: char) -> bool {
    match t {
        's' => v.as_str().is_some_and(|s| !s.trim().is_empty()),
        'i' => as_int(v).is_some(),
        'n' => v.as_f64().is_some_and(f64::is_finite),
        'b' => v.is_boolean(),
        'a' => v.is_array(),
        'o' => v.is_object(),
        _ => false,
    }
}

fn type_word(t: char) -> &'static str {
    match t {
        's' => "a non-empty string",
        'i' => "a non-negative integer",
        'n' => "a number",
        'b' => "true or false",
        'a' => "an array",
        _ => "an object",
    }
}

/// The per-verb rules the type table cannot say.
fn verb_rules(verb: &str, a: &Map<String, Value>) -> Result<(), String> {
    let num = |k: &str| a.get(k).and_then(Value::as_f64);
    let has = |k: &str| a.contains_key(k);
    match verb {
        "book" => {
            let h = num("start_hour").unwrap_or(-1.0);
            if !(0.0..24.0).contains(&h) {
                return Err("`start_hour` is 0 ≤ h < 24, the frame's local time of day".into());
            }
            if num("minutes").unwrap_or(0.0) <= 0.0 {
                return Err("`minutes` must be more than 0".into());
            }
            if let Some(k) = a.get("kind").and_then(Value::as_str) {
                if !PORT_KINDS.contains(&k) {
                    return Err(format!("`kind` is one of {PORT_KINDS:?}"));
                }
            }
        }
        "clock" => {
            let h = num("hour").unwrap_or(-1.0);
            if !(0.0..24.0).contains(&h) {
                return Err("`hour` is 0 ≤ h < 24".into());
            }
        }
        "fader_quick" => {
            if !has("speed") && !has("running") {
                return Err("`fader_quick` needs `speed`, `running` or both".into());
            }
            if num("speed").is_some_and(|s| s < 0.0) {
                return Err("`speed` cannot be negative".into());
            }
        }
        "gpio" => {
            let pin = a.get("pin").and_then(as_int).unwrap_or(0);
            if !(1..=8).contains(&pin) {
                return Err("`pin` is 1–8".into());
            }
            if let Some(f) = a.get("function").and_then(Value::as_str) {
                if !GPIO_FUNCTIONS.contains(&f) {
                    return Err(format!("`function` is one of {GPIO_FUNCTIONS:?}"));
                }
            }
        }
        "gain" => {
            const POINTS: [&[&str]; 3] = [&["frame", "input", "output"], &["trunk", "end"], &["connection", "hop"]];
            let given = POINTS.iter().flat_map(|g| g.iter()).filter(|k| has(k)).count();
            let whole: Vec<&&[&str]> = POINTS.iter().filter(|g| g.iter().all(|k| has(k))).collect();
            if whole.len() != 1 || whole[0].len() != given {
                return Err("`gain` names exactly one point: frame+input+output, trunk+end, or connection+hop".into());
            }
            if let Some(e) = a.get("end").and_then(Value::as_str) {
                if e != "a" && e != "b" {
                    return Err("`end` is `a` or `b`".into());
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Read a `trunk/outgoing/command` payload. Strict: an unknown key at either
/// level, a wrong type, or a verb the caster does not have refuses the body.
pub fn parse_command(payload: &[u8]) -> Result<Command, Refusal> {
    let refuse = |id: &Option<String>, reason: String| Refusal { id: id.clone(), reason };
    let none = None;
    let value: Value = serde_json::from_slice(payload).map_err(|_| refuse(&none, "not JSON".into()))?;
    let map = value.as_object().ok_or_else(|| refuse(&none, "not a JSON object".into()))?;
    let id = match map.get("id") {
        Some(Value::String(s)) if is_command_id(s) => Some(s.clone()),
        _ => None,
    };
    if let Some(k) = map.keys().find(|k| !COMMAND_KEYS.contains(&k.as_str())) {
        return Err(refuse(&id, format!("unknown key `{k}`")));
    }
    let id_text = id.clone().ok_or_else(|| refuse(&none, "no `id` (1–80 of A-Z a-z 0-9 . _ : -)".into()))?;
    let verb = map.get("verb").and_then(Value::as_str).unwrap_or("");
    let spec = verb_spec(verb).ok_or_else(|| refuse(&id, format!("unknown verb `{verb}`")))?;
    let args = match map.get("args") {
        None => Map::new(),
        Some(Value::Object(a)) => a.clone(),
        Some(_) => return Err(refuse(&id, "`args` is not an object".into())),
    };
    let issued_at_ms = match map.get("issued_at_ms") {
        None | Some(Value::Null) => None,
        Some(v) => Some(as_int(v).ok_or_else(|| refuse(&id, "`issued_at_ms` is not unix ms".into()))?),
    };
    if !spec.free {
        for k in args.keys() {
            if !spec.required.iter().chain(spec.optional).any(|(name, _)| name == k) {
                return Err(refuse(&id, format!("`{verb}` takes no argument `{k}`")));
            }
        }
        for (name, t) in spec.required {
            match args.get(*name) {
                None => return Err(refuse(&id, format!("`{verb}` needs `{name}`"))),
                Some(v) if !type_ok(v, *t) => return Err(refuse(&id, format!("`{name}` is not {}", type_word(*t)))),
                _ => {}
            }
        }
        for (name, t) in spec.optional {
            if let Some(v) = args.get(*name) {
                if !type_ok(v, *t) {
                    return Err(refuse(&id, format!("`{name}` is not {}", type_word(*t))));
                }
            }
        }
        verb_rules(verb, &args).map_err(|r| refuse(&id, r))?;
    }
    Ok(Command { id: id_text, verb: verb.to_string(), args, issued_at_ms })
}

/// The answer on `trunk/incoming/result`. `result` carries the verb's own
/// document (the connection reply, the booking, the fader) when there is one.
pub fn result(id: Option<&str>, verb: Option<&str>, status: &str, detail: &str, result: Option<Value>) -> String {
    let mut m = Map::new();
    m.insert("id".into(), id.map_or(Value::Null, |s| Value::String(s.into())));
    m.insert("verb".into(), verb.map_or(Value::Null, |s| Value::String(s.into())));
    m.insert("status".into(), Value::String(status.into()));
    m.insert("ok".into(), Value::Bool(status == STATUS_DONE));
    m.insert("detail".into(), Value::String(detail.into()));
    if let Some(r) = result {
        m.insert("result".into(), r);
    }
    Value::Object(m).to_string()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_round_trips() {
        let s = "HQ,dante-01,U00,Andrey,CLOSE,Sam,U05,sip-03,HQ";
        let l = Line::parse(s).unwrap();
        assert_eq!(l.render(), s);
        assert!(!l.is_local());
        assert!(!l.flipped().close);
    }

    #[test]
    fn a_batcher_flushes_at_the_window_at_the_cap_and_at_the_end() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut b: Batcher = Batcher::new(Duration::from_millis(100), 4);
        // Three entries inside one window: nothing before it, ONE batch of three at it.
        for (n, at) in [(0, 0), (1, 30), (2, 90)] {
            assert!(b.push("f1", format!("l{n}"), ms(at)).is_none());
        }
        assert!(b.due(ms(99)).is_empty());
        assert_eq!(b.due(ms(100)), vec![("f1".to_string(), vec!["l0".to_string(), "l1".into(), "l2".into()])]);
        assert_eq!(b.pending(), 0);
        // The cap hands the batch back at once; the next entry opens a new one.
        for n in 0..3 {
            assert!(b.push("f2", format!("c{n}"), ms(200)).is_none());
        }
        assert_eq!(b.push("f2", "c3".into(), ms(201)).unwrap().1.len(), 4);
        assert!(b.push("f2", "c4".into(), ms(202)).is_none());
        // Topics are separate batches; the final partial ones are drained.
        assert!(b.push("f3", "x".into(), ms(203)).is_none());
        let mut left = b.drain();
        left.sort();
        assert_eq!(left, vec![("f2".to_string(), vec!["c4".to_string()]), ("f3".to_string(), vec!["x".to_string()])]);
        assert!(b.drain().is_empty());
        assert_eq!(Batcher::<String>::new(Duration::ZERO, 10_000).cap, BATCH_MAX);
    }

    /// 0.7.0: (frame, line) entries for many frames of one protocol leave as
    /// ONE batch, each frame's lines in push order, and read back per frame.
    #[test]
    fn a_protocol_batch_keeps_each_frames_order() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut b: Batcher<(String, String)> = Batcher::new(Duration::from_millis(100), 200);
        let topic = protocol_ack_topic("dante").unwrap().0;
        for (f, l) in [("dante-02", "a"), ("dante-01", "b"), ("dante-02", "c"), ("dante-01", "d")] {
            assert!(b.push(&topic, (f.to_string(), l.to_string()), t0).is_none());
        }
        let due = b.due(t0 + Duration::from_millis(100));
        assert_eq!(due.len(), 1, "one message for the protocol");
        let frames = by_frame(due.into_iter().next().unwrap().1);
        let back = parse_protocol_ack_batch(&render_protocol_ack_batch(&frames)).unwrap();
        assert_eq!(back["dante-01"], vec!["b".to_string(), "d".into()]);
        assert_eq!(back["dante-02"], vec!["a".to_string(), "c".into()]);
        assert_eq!(parse_sim_protocol_topic(&topic), Some(("dante".into(), "ack".into())));
    }

    #[test]
    fn every_verb_is_one_route() {
        let mut routes: Vec<&str> = VERBS.iter().map(|v| v.route).collect();
        routes.sort_unstable();
        routes.dedup();
        assert_eq!(routes.len(), VERBS.len());
    }

    #[test]
    fn the_mailbox_leaves_are_the_contracts() {
        use crate::baremetal_io::{module_incoming, module_outgoing};
        assert_eq!(module_outgoing(TOKEN, "command").unwrap(), command_topic());
        assert_eq!(module_incoming(TOKEN, "result").unwrap(), result_topic());
    }
}
