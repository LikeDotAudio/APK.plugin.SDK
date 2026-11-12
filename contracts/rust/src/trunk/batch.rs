// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `trunk.rs` — see that file for the whole picture.

use super::*;

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
    pub(super) cap: usize,
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
