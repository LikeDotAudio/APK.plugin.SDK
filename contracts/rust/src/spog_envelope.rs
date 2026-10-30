// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! SPOG'S WIRE ENVELOPE, read into Rust — the shape half of the sampler bridge.
//!
//! SPOG (`APK:Softapps/TOUCH/spog/`) publishes every console
//! parameter as plaintext protobuf on `SPOG/<path>/params/<slug>`. That tree is
//! VENDORED and `CLAUDE.md` §4 names it as not-source, so this module is a
//! read-only MIRROR of two files in it and must be corrected against them
//! rather than the reverse:
//!   * `src/platform/mqtt/schema.proto` — the four messages below.
//!   * `src/platform/mqtt/codec.ts`     — `decodePayload`, coercion for coercion.
//!
//! WHAT IT OWNS: turning wire bytes into [`SpogEnvelope`]. Nothing more — no
//! client, no topic parsing, no arbitration. The crate's hard rule (see
//! `Cargo.toml`) is serde + serde_json + regress and nothing else, so the proto3
//! reader here is hand-written rather than a `prost` dependency; it decodes the
//! ONE branch a translator needs (`ValueMsg`) and skips every other field by
//! wire type, exactly as a generated decoder would.
//!
//! THE FACT THAT COSTS THE MOST TO LEARN LATE: `seat` and `label` are NOT ON
//! THE WIRE in protobuf mode. `client.ts publishValue` puts them in the payload
//! object, and `codec.ts encodePayload` builds a `ValueMsg` that has no field
//! for either — measured 2026-08-30 by encoding through the vendored encoder
//! and reading the bytes back. So a protobuf write always decodes as
//! `seat: 0`, and `vendor/spog/src/platform/merge/seat.ts` says the same thing
//! in its Phase-2 note. Seats arrive only in the broker's `plaintext` JSON
//! mode. A translator must therefore still apply the seat rule — it is the
//! contract, and it is what a plaintext bench and a later wire field will
//! feed — while knowing that today it usually falls through to `full_id`.

use serde_json::Value;

/// One decoded value write, in the shape `merge/manager.ts ingestForeign`
/// accepts: `{value, ts, full_id, seat?, label?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpogEnvelope {
    /// The proposed value. `Value::Null` is SPOG's "no opinion" sentinel, and
    /// `Bool` is its off/pass-through pair — see `merge/sentinel.rs` callers.
    pub value: Value,
    /// Wall-clock milliseconds. `ts_ms` (varint) when set, else the legacy
    /// `ts` double, else 0 — `codec.ts` reads them in that order.
    pub ts: u64,
    /// The writer identity consumers read. A legacy message carries the full
    /// `"<hex8>:TWIST:<birth>"` string; a v2 message carries only the 4-byte
    /// origin, which is rebuilt here as its 8-hex string so
    /// `full_id.split(':').next()` still yields the origin either way.
    pub full_id: String,
    /// Seat rank. **Absent on the protobuf wire — see the module header.**
    pub seat: i64,
    /// Human seat label. Absent on the protobuf wire, for the same reason.
    pub label: Option<String>,
}

impl SpogEnvelope {
    /// The 8-hex origin — the self-echo and bucketing key.
    pub fn origin(&self) -> &str {
        match self.full_id.split_once(':') {
            Some((head, _)) => head,
            None => &self.full_id,
        }
    }
}

/// Decode one payload from the SPOG bus.
///
/// Accepts both forms `codec.ts` accepts, and detects them the same way: a
/// leading `{` or `[` is raw JSON (the broker's `plaintext` mode, and the
/// legacy shape), anything else is a `PayloadWrapper`. Returns `None` for a
/// payload that is neither, or for a wrapper carrying a presence/rate arm —
/// those are not value writes and a translator has nothing to do with them.
pub fn decode_value(bytes: &[u8]) -> Option<SpogEnvelope> {
    if matches!(bytes.first(), Some(b'{') | Some(b'[')) {
        if let Some(envelope) = decode_json(bytes) {
            return Some(envelope);
        }
        // Fall through, exactly as codec.ts does: a payload that starts with a
        // brace and does not parse is still offered to the protobuf reader.
    }
    decode_protobuf(bytes)
}

/// The `plaintext`/legacy arm: a JSON object carrying the whole envelope —
/// and the ONLY arm on which `seat` and `label` ever arrive.
fn decode_json(bytes: &[u8]) -> Option<SpogEnvelope> {
    let parsed: Value = serde_json::from_slice(bytes).ok()?;
    let object = parsed.as_object()?;
    // `ingestForeign` treats a payload with no `value` key as the value
    // itself; a value write always has the key, and anything without it is
    // presence or rate traffic this decoder deliberately does not answer for.
    let value = object.get("value")?.clone();
    Some(SpogEnvelope {
        value,
        ts: object.get("ts").and_then(number_as_u64).unwrap_or(0),
        full_id: object
            .get("full_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        seat: object.get("seat").and_then(number_as_i64).unwrap_or(0),
        label: object.get("label").and_then(Value::as_str).map(str::to_string),
    })
}

/// A JSON number that may have been written as a float (`ts: 1.7565e12`).
fn number_as_u64(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_f64().map(|f| f.max(0.0) as u64))
}

fn number_as_i64(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
}

// ---------------------------------------------------------------------------
// proto3, by hand
//
// Only the field numbers schema.proto declares are read; everything else is
// skipped by wire type, which is what makes an older or newer encoder safe to
// listen to. `codec.ts` relies on exactly that property (its v2 note says an
// old decoder "skips unknown tags"), so this reader must have it too.
// ---------------------------------------------------------------------------

/// A cursor over one protobuf message body.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn varint(&mut self) -> Option<u64> {
        let mut out: u64 = 0;
        let mut shift = 0u32;
        loop {
            let byte = *self.bytes.get(self.at)?;
            self.at += 1;
            out |= u64::from(byte & 0x7f).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                return Some(out);
            }
            shift += 7;
            // 10 groups of 7 bits is the most a u64 can carry; a longer run is
            // a corrupt payload, not a big number.
            if shift > 63 {
                return None;
            }
        }
    }

    fn fixed64(&mut self) -> Option<u64> {
        let end = self.at.checked_add(8)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(u64::from_le_bytes(slice.try_into().ok()?))
    }

    fn fixed32(&mut self) -> Option<u32> {
        let end = self.at.checked_add(4)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    }

    fn length_delimited(&mut self) -> Option<&'a [u8]> {
        let len = usize::try_from(self.varint()?).ok()?;
        let end = self.at.checked_add(len)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    /// Read the next tag as `(field number, wire type)`.
    fn tag(&mut self) -> Option<(u64, u8)> {
        let key = self.varint()?;
        Some((key >> 3, (key & 0x07) as u8))
    }

    /// Step over a field this decoder does not read. Returns `None` on a wire
    /// type proto3 does not define, which ends the parse rather than
    /// misreading the rest of the buffer as data.
    fn skip(&mut self, wire: u8) -> Option<()> {
        match wire {
            0 => self.varint().map(|_| ()),
            1 => self.fixed64().map(|_| ()),
            2 => self.length_delimited().map(|_| ()),
            5 => self.fixed32().map(|_| ()),
            _ => None,
        }
    }
}

/// `PayloadWrapper` → the `value_msg` arm, or nothing.
fn decode_protobuf(bytes: &[u8]) -> Option<SpogEnvelope> {
    let mut reader = Reader::new(bytes);
    while !reader.done() {
        let (field, wire) = reader.tag()?;
        match (field, wire) {
            // value_msg = 1
            (1, 2) => return decode_value_message(reader.length_delimited()?),
            // json_fallback = 4 — a whole payload object as a JSON string.
            (4, 2) => {
                let text = reader.length_delimited()?;
                return decode_json(text);
            }
            // presence_msg = 2, rate_msg = 3, and anything a newer encoder adds.
            _ => reader.skip(wire)?,
        }
    }
    None
}

fn decode_value_message(bytes: &[u8]) -> Option<SpogEnvelope> {
    let mut reader = Reader::new(bytes);
    let mut value: Option<Value> = None;
    let mut list: Vec<Value> = Vec::new();
    let mut timestamp_double: f64 = 0.0;
    let mut timestamp_millis: u64 = 0;
    let mut full_id: Option<String> = None;
    let mut origin_id: u32 = 0;

    while !reader.done() {
        let (field, wire) = reader.tag()?;
        match (field, wire) {
            (1, 2) => value = decode_parameter_value(reader.length_delimited()?),
            (2, 1) => timestamp_double = f64::from_bits(reader.fixed64()?),
            (3, 2) => full_id = text_of(reader.length_delimited()?),
            (4, 2) => {
                if let Some(text) = text_of(reader.length_delimited()?) {
                    list.push(Value::String(text));
                }
            }
            (5, 5) => origin_id = reader.fixed32()?,
            (6, 0) => timestamp_millis = reader.varint()?,
            _ => reader.skip(wire)?,
        }
    }

    // F2: a non-empty repeated-string field IS the value, and it outranks the
    // ParamValue arm — the order `codec.ts decodePayload` reads them in.
    let value = if list.is_empty() {
        value.unwrap_or(Value::Null)
    } else {
        Value::Array(list)
    };

    // `numOf(vm.tsMs) || vm.ts || 0` — the varint first, then the legacy double.
    let timestamp = if timestamp_millis != 0 {
        timestamp_millis
    } else if timestamp_double > 0.0 {
        timestamp_double as u64
    } else {
        0
    };

    Some(SpogEnvelope {
        value,
        ts: timestamp,
        full_id: full_id.unwrap_or_else(|| origin_hex(origin_id)),
        // Not on this wire. See the module header.
        seat: 0,
        label: None,
    })
}

/// `ParamValue`'s three-way oneof. A `string_value` that opens with `[` or `{`
/// is re-parsed as JSON, because that is how an object or an empty array was
/// encoded in the first place (`encodePayload`).
fn decode_parameter_value(bytes: &[u8]) -> Option<Value> {
    let mut reader = Reader::new(bytes);
    let mut out: Option<Value> = None;
    while !reader.done() {
        let (field, wire) = reader.tag()?;
        match (field, wire) {
            (1, 1) => {
                let number = f64::from_bits(reader.fixed64()?);
                out = serde_json::Number::from_f64(number).map(Value::Number);
            }
            (2, 2) => {
                let text = text_of(reader.length_delimited()?)?;
                out = Some(if text.starts_with('[') || text.starts_with('{') {
                    serde_json::from_str(&text).unwrap_or(Value::String(text))
                } else {
                    Value::String(text)
                });
            }
            (3, 0) => out = Some(Value::Bool(reader.varint()? != 0)),
            _ => reader.skip(wire)?,
        }
    }
    out
}

fn text_of(bytes: &[u8]) -> Option<String> {
    String::from_utf8(bytes.to_vec()).ok()
}

/// `idOf`: the compact origin becomes its 8-hex string; a zero origin has no
/// identity at all and stays empty, as the TypeScript does.
fn origin_hex(origin_id: u32) -> String {
    if origin_id == 0 {
        String::new()
    } else {
        format!("{origin_id:08x}")
    }
}
