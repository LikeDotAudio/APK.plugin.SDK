// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! What an agent says about ITSELF on its own loopback — and the one rule that
//! turns those counters into a word. ROAD-3472.
//!
//! The Rust half of `src/baremetal-telemetry.ts`'s agent section and of
//! `python/apkaudio_contracts/baremetal_telemetry.py`; all three are held to
//! `vectors/baremetal_telemetry.json`.
//!
//! WHY THIS EXISTS
//!
//! A protocol agent connected to a silent broker, or listening on a segment
//! nobody announces on, looked exactly like one carrying live traffic: the PID
//! was there, the container was healthy, and the supervisor said green. The
//! process existing was being read as the protocol working. This module is the
//! vocabulary that lets the two be told apart — five words, and a rule that
//! derives the word from what the agent COUNTED, never from what it claims.
//!
//! THE FIVE STATES
//!
//! ```text
//!   running   alive, and has processed no traffic yet        (idle)
//!   serving   alive, and has processed at least one packet
//!   failed    alive enough to answer, and says it hit a fatal fault
//!   unknown   the OBSERVER's word: no telemetry could be read
//!   stub      declared intent; the manifest says there is no implementation
//! ```
//!
//! `unknown` IS NEVER SELF-REPORTED. An agent that can answer has telemetry; the
//! word exists for the supervisor that asked and heard nothing — a PID that is
//! alive and a loopback that does not answer is NOT serving, and saying
//! `running` for it would be the PID-existence check this replaces.
//!
//! THE RULE IS THE COUNTER, NOT THE CLAIM. [`observed_state`] reads
//! `counters.packets_received` and derives `running` / `serving` itself; a
//! document whose `state` says `serving` with zero packets reads `running`.
//! Only `failed` and `stub` are taken from the document, because those are facts
//! the counters cannot show.
//!
//! WHY THE SERVER IS NOT HERE
//!
//! This crate's hard rule is types and rules, no I/O (Cargo.toml). The listener
//! that answers `GET /status` and `GET /telemetry` on 127.0.0.1 lives in
//! `apk-plugin-runner` (`loopback.rs`), which every plugin and the orchestrator
//! already link and which already owns their sockets. What it serves is built
//! here, so a second server could not drift from it.
//!
//! THE MANIFEST — `agent.toml`, COMPILED IN
//!
//! [`embedded_manifest!`] expands to `include_str!` of the CALLING crate's
//! `agent.toml` and parses it with [`Manifest::parse`]. Compiled in, not read at
//! run time: an image carries binaries, not source trees, and an agent that
//! cannot find its own declaration has no honest identity to report. The file
//! is a small TOML subset (sections, strings, integers, booleans, string
//! arrays) parsed by hand, because this crate takes no TOML dependency.
//!
//! ```toml
//! [agent]
//! name = "sap"                 # the module token
//! kind = "protocol"            # protocol | vendor | service | worker
//! title = "SAP / SDP listener"
//! status = "discovery-only"    # live | discovery-only | stub
//! summary = "One sentence."
//!
//! [telemetry]
//! port = 8190                  # 127.0.0.1 only; 0 = declares no listener
//!
//! [config]
//! reads = ["raise", "presence_protocols"]   # every config.ini key the code reads
//! ```
use serde_json::{json, Value};

/// The document's own version. A reader that sees a higher one reads what it
/// knows and says it is older.
pub const SCHEMA_VERSION: u64 = 1;

/// The five words, in the order the header explains them. A CLOSED set.
pub const SERVING_STATES: [&str; 5] = ["running", "serving", "failed", "unknown", "stub"];

/// The `[agent] status` words an agent with a process may declare.
///
/// `library` and `python-only` (OneThing's other two) are not here on purpose:
/// neither has a process, so neither has a loopback to declare it on.
pub const IMPLEMENTATION_STATUSES: [&str; 3] = ["live", "discovery-only", "stub"];

/// The `[agent] kind` words.
pub const AGENT_KINDS: [&str; 4] = ["protocol", "vendor", "service", "worker"];

/// The address every loopback telemetry server binds. Not configurable: the
/// surface is for the supervisor on the same host, and a wider bind is a
/// different, deliberate surface with its own review.
pub const LOOPBACK_BIND: &str = "127.0.0.1";

/// The two routes, and nothing else.
pub const ROUTE_STATUS: &str = "/status";
pub const ROUTE_TELEMETRY: &str = "/telemetry";

/// One of [`SERVING_STATES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServingState {
    Running,
    Serving,
    Failed,
    Unknown,
    Stub,
}

impl ServingState {
    /// The word on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            ServingState::Running => "running",
            ServingState::Serving => "serving",
            ServingState::Failed => "failed",
            ServingState::Unknown => "unknown",
            ServingState::Stub => "stub",
        }
    }

    /// The word back to the state, or `None` for a word outside the set.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "running" => Some(ServingState::Running),
            "serving" => Some(ServingState::Serving),
            "failed" => Some(ServingState::Failed),
            "unknown" => Some(ServingState::Unknown),
            "stub" => Some(ServingState::Stub),
            _ => None,
        }
    }
}

/// THE RULE, from the agent's side. `stub` outranks everything (a stub has
/// nothing to fail at), then `fatal`, then the packet counter.
pub fn derive_state(stub: bool, fatal: bool, packets_received: u64) -> ServingState {
    if stub {
        ServingState::Stub
    } else if fatal {
        ServingState::Failed
    } else if packets_received == 0 {
        ServingState::Running
    } else {
        ServingState::Serving
    }
}

/// THE RULE, from the supervisor's side: what a `GET /telemetry` answer means.
///
/// `None` (nothing answered, or it did not parse) is `unknown`. A document
/// without a numeric `counters.packets_received` is `unknown` too: telemetry
/// that does not say what it counted is not evidence of traffic, and is not
/// evidence of silence either. Otherwise [`derive_state`] over the document's
/// own `state == "stub"`, `state == "failed"` and its packet counter.
pub fn observed_state(document: Option<&Value>) -> ServingState {
    let Some(doc) = document.filter(|d| d.is_object()) else {
        return ServingState::Unknown;
    };
    let claimed = doc.get("state").and_then(Value::as_str);
    if claimed == Some("stub") {
        return ServingState::Stub;
    }
    let Some(packets) = doc.pointer("/counters/packets_received").and_then(Value::as_u64) else {
        return ServingState::Unknown;
    };
    derive_state(false, claimed == Some("failed"), packets)
}

/// What the agent counted. `unread_config_keys` is `None` when the manifest
/// declares no `[config] reads` list: undeclared is null, never a guessed 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    /// Datagrams / frames / messages taken off the protocol's own wire.
    pub packets_received: u64,
    /// Publishes the client accepted.
    pub messages_published: u64,
    /// Publishes the client refused — counted, never discarded with `let _ =`.
    pub publish_failures: u64,
    /// Keys in the agent's config section that its manifest says nothing reads.
    pub unread_config_keys: Option<u64>,
}

/// The `GET /telemetry` document.
///
/// ```json
/// {"schemaVersion":1,"agent":"sap","kind":"protocol","status":"discovery-only",
///  "title":"…","version":"0.1.0","state":"running","reason":null,"uptime_s":12,
///  "counters":{"packets_received":0,"messages_published":0,"publish_failures":0,
///              "unread_config_keys":0}}
/// ```
pub fn telemetry_document(
    manifest: &Manifest,
    version: &str,
    counters: &Counters,
    fatal: Option<&str>,
    uptime_s: u64,
) -> Value {
    let state = derive_state(manifest.is_stub(), fatal.is_some(), counters.packets_received);
    json!({
        "schemaVersion": SCHEMA_VERSION,
        "agent": manifest.name,
        "kind": manifest.kind,
        "status": manifest.status,
        "title": manifest.title,
        "version": version,
        "state": state.as_str(),
        "reason": fatal,
        "uptime_s": uptime_s,
        "counters": {
            "packets_received": counters.packets_received,
            "messages_published": counters.messages_published,
            "publish_failures": counters.publish_failures,
            "unread_config_keys": counters.unread_config_keys,
        },
    })
}

/// The `GET /status` document: the word and who says it, for a probe that does
/// not want the counters.
pub fn status_document(manifest: &Manifest, counters: &Counters, fatal: Option<&str>) -> Value {
    let state = derive_state(manifest.is_stub(), fatal.is_some(), counters.packets_received);
    json!({
        "schemaVersion": SCHEMA_VERSION,
        "agent": manifest.name,
        "state": state.as_str(),
        "reason": fatal,
    })
}

/// An agent's `agent.toml`, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub summary: String,
    /// `[telemetry] port`. `0` declares no listener.
    pub telemetry_port: u16,
    /// `[config] reads`, or `None` when the section is absent.
    pub config_reads: Option<Vec<String>>,
}

impl Manifest {
    /// Parse and validate. Every refusal names the key, so a broken manifest is
    /// a sentence at startup and in the crate's own `cargo test`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table = parse_toml_subset(text)?;
        let text_of = |section: &str, key: &str| -> Result<String, String> {
            match table.iter().find(|(s, k, _)| s == section && k == key) {
                Some((_, _, Scalar::Str(v))) if !v.trim().is_empty() => Ok(v.clone()),
                Some(_) => Err(format!("[{section}] {key} must be a non-empty string")),
                None => Err(format!("[{section}] {key} is required")),
            }
        };
        let name = text_of("agent", "name")?;
        let kind = text_of("agent", "kind")?;
        if !AGENT_KINDS.contains(&kind.as_str()) {
            return Err(format!("[agent] kind {kind:?} is not one of {AGENT_KINDS:?}"));
        }
        let status = text_of("agent", "status")?;
        if !IMPLEMENTATION_STATUSES.contains(&status.as_str()) {
            return Err(format!("[agent] status {status:?} is not one of {IMPLEMENTATION_STATUSES:?}"));
        }
        let title = text_of("agent", "title")?;
        let summary = text_of("agent", "summary")?;
        let telemetry_port = match table.iter().find(|(s, k, _)| s == "telemetry" && k == "port") {
            None => 0,
            Some((_, _, Scalar::Int(n))) if (0..=u16::MAX as i64).contains(n) => *n as u16,
            Some(_) => return Err("[telemetry] port must be an integer 0-65535".into()),
        };
        let config_reads = match table.iter().find(|(s, k, _)| s == "config" && k == "reads") {
            None => None,
            Some((_, _, Scalar::List(items))) => Some(items.clone()),
            Some(_) => return Err("[config] reads must be an array of strings".into()),
        };
        Ok(Manifest { name, kind, title, status, summary, telemetry_port, config_reads })
    }

    /// Whether the agent declares itself unimplemented.
    pub fn is_stub(&self) -> bool {
        self.status == "stub"
    }

    /// How many of `present` (the keys actually in the agent's config section)
    /// the manifest does not list as read. `None` when it lists nothing.
    /// Comparison ignores ASCII case, as the runner's ini reader does.
    pub fn unread_config_keys<'a>(&self, present: impl IntoIterator<Item = &'a str>) -> Option<u64> {
        let reads = self.config_reads.as_ref()?;
        Some(
            present
                .into_iter()
                .filter(|key| !reads.iter().any(|r| r.eq_ignore_ascii_case(key)))
                .count() as u64,
        )
    }
}

/// Compile the CALLING crate's `agent.toml` (or the named file beside its
/// `Cargo.toml`) into the binary and parse it: `Result<Manifest, String>`.
///
/// ```ignore
/// let manifest = apkaudio_contracts::embedded_manifest!().expect("agent.toml");
/// ```
#[macro_export]
macro_rules! embedded_manifest {
    () => {
        $crate::embedded_manifest!("agent.toml")
    };
    ($file:literal) => {
        $crate::baremetal_telemetry::Manifest::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/",
            $file
        )))
    };
}

// `Bool` is parsed so a boolean key is accepted, though no field reads one yet.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Scalar {
    Str(String),
    Int(i64),
    Bool(bool),
    List(Vec<String>),
}

/// `(section, key, value)` rows. Refuses anything outside the subset rather
/// than guessing at it.
fn parse_toml_subset(text: &str) -> Result<Vec<(String, String, Scalar)>, String> {
    let mut rows: Vec<(String, String, Scalar)> = Vec::new();
    let mut section = String::new();
    let mut lines = text.lines().enumerate();
    while let Some((index, raw)) = lines.next() {
        let at = index + 1;
        let mut line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && !line.contains('=') {
            let name = line
                .strip_prefix('[')
                .and_then(|l| l.strip_suffix(']'))
                .ok_or_else(|| format!("line {at}: a section header is `[name]`"))?
                .trim();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err(format!("line {at}: section name {name:?} is not a bare key"));
            }
            section = name.to_string();
            continue;
        }
        let (key, _) = line.split_once('=').ok_or_else(|| format!("line {at}: expected `key = value`"))?;
        let key = key.trim().to_string();
        if section.is_empty() {
            return Err(format!("line {at}: `{key}` is outside any [section]"));
        }
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(format!("line {at}: key {key:?} is not a bare key"));
        }
        // A multi-line array: keep reading until its closing bracket.
        if line.split_once('=').map(|(_, v)| v.trim_start().starts_with('[')).unwrap_or(false) {
            while !strip_comment(&line).trim_end().ends_with(']') {
                let (_, more) = lines.next().ok_or_else(|| format!("line {at}: array never closed"))?;
                line.push(' ');
                line.push_str(strip_comment(more).trim());
            }
        }
        let value = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or_default();
        let scalar = parse_value(value).map_err(|e| format!("line {at}: [{section}] {key}: {e}"))?;
        if rows.iter().any(|(s, k, _)| *s == section && *k == key) {
            return Err(format!("line {at}: [{section}] {key} is declared twice"));
        }
        rows.push((section.clone(), key, scalar));
    }
    Ok(rows)
}

/// The line up to a `#` that is not inside a string.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            '\\' if in_string && !escaped => {
                escaped = true;
                continue;
            }
            '"' if !escaped => in_string = !in_string,
            '#' if !in_string => return &line[..i],
            _ => {}
        }
        escaped = false;
    }
    line
}

fn parse_value(value: &str) -> Result<Scalar, String> {
    if value.starts_with('"') {
        let (text, rest) = parse_string(value)?;
        if !rest.trim().is_empty() {
            return Err(format!("unexpected {rest:?} after the string"));
        }
        return Ok(Scalar::Str(text));
    }
    if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        let mut items = Vec::new();
        let mut rest = inner.trim();
        while !rest.is_empty() {
            let (item, after) = parse_string(rest).map_err(|_| "an array holds strings only".to_string())?;
            items.push(item);
            rest = after.trim_start();
            rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
            if !rest.is_empty() && !rest.starts_with('"') {
                return Err(format!("unexpected {rest:?} in the array"));
            }
        }
        return Ok(Scalar::List(items));
    }
    match value {
        "true" => return Ok(Scalar::Bool(true)),
        "false" => return Ok(Scalar::Bool(false)),
        _ => {}
    }
    value
        .replace('_', "")
        .parse::<i64>()
        .map(Scalar::Int)
        .map_err(|_| format!("{value:?} is not a string, integer, boolean or string array"))
}

/// One basic `"…"` string from the front of `input`: (its text, the rest).
fn parse_string(input: &str) -> Result<(String, &str), String> {
    let body = input.strip_prefix('"').ok_or("expected a string")?;
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &body[i + 1..])),
            '\\' => match chars.next() {
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                other => return Err(format!("unsupported escape {:?}", other.map(|(_, c)| c))),
            },
            c => out.push(c),
        }
    }
    Err("string never closed".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# a comment
[agent]
name = "sap"
kind = "protocol"
title = "SAP / SDP listener"   # trailing comment
status = "discovery-only"
summary = "Listens for \"SAP\" # announcements."

[telemetry]
port = 8190

[config]
reads = [
    "raise",
    "presence_protocols",  # the tracker
]
"#;

    #[test]
    fn a_manifest_parses_with_comments_escapes_and_a_multiline_array() {
        let m = Manifest::parse(SAMPLE).unwrap();
        assert_eq!(m.name, "sap");
        assert_eq!(m.summary, "Listens for \"SAP\" # announcements.");
        assert_eq!(m.title, "SAP / SDP listener");
        assert_eq!(m.telemetry_port, 8190);
        assert_eq!(m.config_reads.as_deref(), Some(&["raise".to_string(), "presence_protocols".to_string()][..]));
        assert!(!m.is_stub());
    }

    #[test]
    fn a_manifest_refuses_what_it_cannot_vouch_for() {
        let without = |needle: &str| SAMPLE.lines().filter(|l| !l.starts_with(needle)).collect::<Vec<_>>().join("\n");
        assert!(Manifest::parse(&without("name")).unwrap_err().contains("[agent] name is required"));
        assert!(Manifest::parse(&SAMPLE.replace("discovery-only", "library")).unwrap_err().contains("status"));
        assert!(Manifest::parse(&SAMPLE.replace("\"protocol\"", "\"plugin\"")).unwrap_err().contains("kind"));
        assert!(Manifest::parse(&SAMPLE.replace("8190", "70000")).unwrap_err().contains("port"));
        assert!(Manifest::parse(&SAMPLE.replace("8190", "\"8190\"")).unwrap_err().contains("port"));
        assert!(Manifest::parse(&format!("{SAMPLE}\n[agent]\nname = \"x\"\n")).unwrap_err().contains("twice"));
        assert!(Manifest::parse("name = \"x\"").unwrap_err().contains("outside any"));
        assert!(Manifest::parse("[agent]\nname = 'x'").is_err(), "literal strings are outside the subset");
    }

    #[test]
    fn no_port_and_no_reads_are_stated_not_guessed() {
        let bare = "[agent]\nname=\"x\"\nkind=\"service\"\ntitle=\"X\"\nstatus=\"stub\"\nsummary=\"s\"\n";
        let m = Manifest::parse(bare).unwrap();
        assert_eq!(m.telemetry_port, 0);
        assert_eq!(m.config_reads, None);
        assert_eq!(m.unread_config_keys(["raise"]), None);
        assert!(m.is_stub());
    }

    #[test]
    fn unread_keys_are_the_present_ones_nothing_declares_reading() {
        let m = Manifest::parse(SAMPLE).unwrap();
        assert_eq!(m.unread_config_keys(["raise", "Presence_Protocols"]), Some(0));
        assert_eq!(m.unread_config_keys(["raise", "multicast_address", "port"]), Some(2));
    }

    #[test]
    fn the_rule_is_the_counter() {
        assert_eq!(derive_state(false, false, 0), ServingState::Running);
        assert_eq!(derive_state(false, false, 1), ServingState::Serving);
        assert_eq!(derive_state(false, true, 99), ServingState::Failed);
        assert_eq!(derive_state(true, true, 99), ServingState::Stub);
        for word in SERVING_STATES {
            assert_eq!(ServingState::parse(word).unwrap().as_str(), word);
        }
        assert_eq!(ServingState::parse("idle"), None);
    }

    #[test]
    fn the_document_says_what_the_rule_says() {
        let m = Manifest::parse(SAMPLE).unwrap();
        let mut c = Counters { unread_config_keys: Some(0), ..Counters::default() };
        let doc = telemetry_document(&m, "0.1.0", &c, None, 5);
        assert_eq!(doc["state"], "running");
        assert_eq!(doc["counters"]["packets_received"], 0);
        assert_eq!(observed_state(Some(&doc)), ServingState::Running);
        c.packets_received = 3;
        let doc = telemetry_document(&m, "0.1.0", &c, None, 6);
        assert_eq!(doc["state"], "serving");
        assert_eq!(observed_state(Some(&doc)), ServingState::Serving);
        let doc = telemetry_document(&m, "0.1.0", &c, Some("socket closed"), 7);
        assert_eq!(doc["state"], "failed");
        assert_eq!(doc["reason"], "socket closed");
        assert_eq!(status_document(&m, &c, None)["state"], "serving");
    }
}
