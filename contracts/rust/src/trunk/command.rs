// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `trunk.rs` — see that file for the whole picture.

use super::*;

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
