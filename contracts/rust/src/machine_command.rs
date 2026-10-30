// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! A machine command: power, a named command, a deploy — what TRUNK-CASTER's
//! ⏻ ›_ ⤒ buttons say, after the shell has confirmed them (PLAN-3394.01).
//!
//! ```text
//!   APK.audio/System/BareMetal/MonitorProbes/{probe}/{host}/outgoing/command   shell -> agent, NOT retained
//!   APK.audio/System/BareMetal/MonitorProbes/{probe}/{host}/incoming/state     agent -> bus, RETAINED answer
//!   APK.audio/System/BareMetal/MonitorProbes/{probe}/{host}/incoming/detail    agent -> bus, RETAINED: what it can do
//! ```
//!
//! The `command` leaf is the existing one-shot leaf of `baremetal_io`; this
//! module only says what may be written on it. The address is the probe's own
//! (`{probe}` and `{host}` are the two topic levels PLUGIN.DESKTOP_MONITOR
//! publishes under), so a machine has ONE place on the bus, not a second
//! mailbox somewhere else.
//!
//! WHO MAY WRITE IT. Only the broker user [`OPERATOR_ROLE`]; the broker ACL
//! denies every other client every `…/outgoing/command` under
//! `System/BareMetal` ([`acl_deny_patterns`] is that list, and the broker's
//! ACL files are checked against it). The shell is the only holder of that
//! credential, and it publishes only after a person confirmed the action and
//! the confirmation was logged — the doctrine `A frame may not write to
//! hardware`. The envelope carries that log line's id as `session`, so an
//! agent can refuse a command nobody confirmed even on a broker whose ACL is
//! not yet applied.
//!
//! NEVER A SHELL STRING. `run` and `deploy` name an entry in the agent's own
//! allow-list (its config.ini); there is no field a caller could put a
//! command line in, and a body with a key this module does not know is refused
//! whole — an `argv` riding along on a valid `run` is exactly the payload that
//! must not be half-read.
use serde_json::{json, Map, Value};

use crate::baremetal_io::{device_incoming, device_outgoing};
use crate::topics::TopicError;

/// The broker identity that alone may publish a machine command.
pub const OPERATOR_ROLE: &str = "apkaudio-operator";

/// The broker identity a machine agent LISTENS as: read-only on the command
/// leaf, because the anonymous client may not even read it once the ACL is on.
pub const MACHINE_ROLE: &str = "apkaudio-machine";

/// The bare-metal target machines live under — the monitor probes' root.
pub const TARGET: &str = "MonitorProbes";

/// The three verbs. A CLOSED set: a fourth is a design change.
pub const VERB_POWER: &str = "power";
pub const VERB_RUN: &str = "run";
pub const VERB_DEPLOY: &str = "deploy";
pub const VERBS: [&str; 3] = [VERB_POWER, VERB_RUN, VERB_DEPLOY];

/// What `power` may ask. `on` of a machine that is off cannot be answered by
/// that machine's own agent: a wake relay (another agent) sends the magic
/// packet to the `mac` the envelope carries, or a BMC turns it on.
pub const POWER_ACTIONS: [&str; 3] = ["on", "off", "cycle"];

/// The answer's `status`, in the order an answer moves through it.
pub const STATUS_ACCEPTED: &str = "accepted";
pub const STATUS_DONE: &str = "done";
pub const STATUS_REFUSED: &str = "refused";
pub const STATUS_ERROR: &str = "error";
pub const STATUSES: [&str; 4] = [STATUS_ACCEPTED, STATUS_DONE, STATUS_REFUSED, STATUS_ERROR];

/// Every key a command may carry. Anything else refuses the whole body.
pub const KEYS: [&str; 9] =
    ["id", "verb", "action", "name", "session", "operator", "issued_at_ms", "mac", "dry_run"];

/// One confirmed machine command, as the agent reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The command's own id; the answer carries it back.
    pub id: String,
    /// `power`, `run` or `deploy`.
    pub verb: String,
    /// `on` / `off` / `cycle` for `power`; `None` otherwise.
    pub action: Option<String>,
    /// The allow-list entry for `run` / `deploy`; `None` for `power`.
    pub name: Option<String>,
    /// The id of the shell's confirmation log line. No log line, no command.
    pub session: String,
    /// Who confirmed it: `{seat}/{name}` as the shell signed them in.
    pub operator: String,
    /// When the shell sent it, unix ms. A stale command is a replay.
    pub issued_at_ms: u64,
    /// `power on` only: the MAC a wake relay sends the magic packet to.
    pub mac: Option<String>,
    /// The caller asks for a rehearsal. An agent configured dry-run is dry
    /// whatever this says; this can only make a live agent dry, never the reverse.
    pub dry_run: bool,
}

/// Why a body was not a command, with the id when one could be read — so the
/// refusal can still be answered on `incoming/state` against that id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub id: Option<String>,
    pub reason: String,
}

fn refuse(id: &Option<String>, reason: impl Into<String>) -> Refusal {
    Refusal { id: id.clone(), reason: reason.into() }
}

/// An id or a session: 8–80 of `A-Z a-z 0-9 . _ : -`.
pub fn is_token(s: &str) -> bool {
    (8..=80).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

/// An allow-list name: 1–64 of `A-Z a-z 0-9 . _ -`, not starting with `-` or `.`.
pub fn is_name(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && !s.starts_with('-')
        && !s.starts_with('.')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// `aa:bb:cc:dd:ee:ff`, either case. Identity, never an IP.
pub fn is_mac(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn text(map: &Map<String, Value>, key: &str, id: &Option<String>) -> Result<Option<String>, Refusal> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(refuse(id, format!("`{key}` is not a string"))),
    }
}

/// Read a `…/outgoing/command` payload. Strict: an unknown key, a missing
/// session, a verb with the wrong companion field — each refuses the whole body.
pub fn parse(payload: &[u8]) -> Result<Command, Refusal> {
    let none = None;
    let value: Value = serde_json::from_slice(payload).map_err(|_| refuse(&none, "not JSON"))?;
    let map = value.as_object().ok_or_else(|| refuse(&none, "not a JSON object"))?;
    let id = match map.get("id") {
        Some(Value::String(s)) if is_token(s) => Some(s.clone()),
        _ => None,
    };
    if let Some(key) = map.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(refuse(&id, format!("unknown key `{key}` — a command names an allow-list entry, never a command line")));
    }
    let id_text = id.clone().ok_or_else(|| refuse(&none, "no `id` (8–80 of A-Z a-z 0-9 . _ : -)"))?;
    let session = text(map, "session", &id)?.unwrap_or_default();
    if !is_token(&session) {
        return Err(refuse(&id, "no `session` — the shell's confirmation log id; nothing unconfirmed is obeyed"));
    }
    let operator = text(map, "operator", &id)?.unwrap_or_default();
    if operator.trim().is_empty() || operator.len() > 80 {
        return Err(refuse(&id, "no `operator` — who confirmed it"));
    }
    let issued_at_ms = map
        .get("issued_at_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| refuse(&id, "no `issued_at_ms` (unix ms)"))?;
    let verb = text(map, "verb", &id)?.unwrap_or_default();
    let action = text(map, "action", &id)?;
    let name = text(map, "name", &id)?;
    let mac = text(map, "mac", &id)?;
    let dry_run = match map.get("dry_run") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(refuse(&id, "`dry_run` is not a boolean")),
    };
    match verb.as_str() {
        VERB_POWER => {
            let a = action.as_deref().unwrap_or("");
            if !POWER_ACTIONS.contains(&a) {
                return Err(refuse(&id, format!("power `action` must be one of {POWER_ACTIONS:?}")));
            }
            if name.is_some() {
                return Err(refuse(&id, "power takes no `name`"));
            }
            if let Some(m) = &mac {
                if a != "on" || !is_mac(m) {
                    return Err(refuse(&id, "`mac` is for power on only, as aa:bb:cc:dd:ee:ff"));
                }
            }
        }
        VERB_RUN | VERB_DEPLOY => {
            if action.is_some() || mac.is_some() {
                return Err(refuse(&id, format!("{verb} takes only a `name`")));
            }
            if !name.as_deref().is_some_and(is_name) {
                return Err(refuse(&id, format!("{verb} needs a `name` from the agent's allow-list")));
            }
        }
        other => return Err(refuse(&id, format!("unknown verb `{other}` — one of {VERBS:?}"))),
    }
    Ok(Command { id: id_text, verb, action, name, session, operator, issued_at_ms, mac, dry_run })
}

impl Command {
    /// The wire body — what the shell publishes and [`parse`] reads back.
    pub fn to_json(&self) -> String {
        let mut doc = json!({
            "id": self.id, "verb": self.verb, "session": self.session,
            "operator": self.operator, "issued_at_ms": self.issued_at_ms,
        });
        let m = doc.as_object_mut().expect("object");
        if let Some(a) = &self.action { m.insert("action".into(), json!(a)); }
        if let Some(n) = &self.name { m.insert("name".into(), json!(n)); }
        if let Some(mac) = &self.mac { m.insert("mac".into(), json!(mac)); }
        if self.dry_run { m.insert("dry_run".into(), json!(true)); }
        doc.to_string()
    }

    /// One line for a log or an answer: `power cycle`, `run uptime`.
    pub fn describe(&self) -> String {
        format!("{} {}", self.verb, self.action.as_deref().or(self.name.as_deref()).unwrap_or(""))
    }
}

/// The retained `incoming/state` answer to one command.
///
/// `by` is the host of the agent that answered — the machine itself, or the
/// wake relay that sent its packet. `command` is `None` for a body that could
/// not be read at all; the refusal still lands, against whatever id it had.
pub fn answer(
    command: Option<&Command>,
    id: Option<&str>,
    status: &str,
    detail: &str,
    by: &str,
    dry_run: bool,
    at_ms: u64,
) -> String {
    debug_assert!(STATUSES.contains(&status));
    let mut doc = json!({
        "id": id.or(command.map(|c| c.id.as_str())),
        "status": status, "detail": detail, "by": by, "dry_run": dry_run, "at_ms": at_ms,
    });
    if let Some(c) = command {
        let m = doc.as_object_mut().expect("object");
        m.insert("verb".into(), json!(c.verb));
        if let Some(a) = &c.action { m.insert("action".into(), json!(a)); }
        if let Some(n) = &c.name { m.insert("name".into(), json!(n)); }
        m.insert("session".into(), json!(c.session));
        m.insert("operator".into(), json!(c.operator));
    }
    doc.to_string()
}

/// `…/MonitorProbes/{probe}/{host}/outgoing/command` — not retained.
pub fn command_topic(probe: &str, host: &str) -> Result<(String, bool), TopicError> {
    device_outgoing(TARGET, &[probe, host], "command")
}

/// `…/MonitorProbes/{probe}/{host}/incoming/state` — retained.
pub fn state_topic(probe: &str, host: &str) -> Result<(String, bool), TopicError> {
    device_incoming(TARGET, &[probe, host], "state")
}

/// `…/MonitorProbes/{probe}/{host}/incoming/detail` — retained.
pub fn detail_topic(probe: &str, host: &str) -> Result<(String, bool), TopicError> {
    device_incoming(TARGET, &[probe, host], "detail")
}

/// What one agent listens on: every probe level, its own host.
pub fn inbox(host: &str) -> String {
    format!("{}/System/BareMetal/{TARGET}/+/{host}/outgoing/command", crate::topics::ROOT)
}

/// What a wake relay listens on: every machine's command.
pub fn all_inboxes() -> String {
    format!("{}/System/BareMetal/{TARGET}/+/+/outgoing/command", crate::topics::ROOT)
}

/// `(probe, host)` out of a command topic, or `None` for any other topic.
pub fn address_of(topic: &str) -> Option<(String, String)> {
    let rest = topic.strip_prefix(&format!("{}/System/BareMetal/{TARGET}/", crate::topics::ROOT))?;
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [probe, host, "outgoing", "command"] => Some((probe.to_string(), host.to_string())),
        _ => None,
    }
}

/// The topic filters the broker ACL DENIES every client but the operator.
/// One per depth of `{path}` between the target and the leaf, 0 to 3 levels —
/// MQTT filters cannot say "ends in outgoing/command" at any depth, so the
/// depths are spelled, and the broker's ACL files are tested against this list.
pub fn acl_deny_patterns() -> Vec<String> {
    (1..=4)
        .map(|depth| {
            let plus = vec!["+"; depth].join("/");
            format!("{}/System/BareMetal/{plus}/outgoing/command", crate::topics::ROOT)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> Command {
        Command {
            id: "cmd-0001-abcd".into(),
            verb: "power".into(),
            action: Some("cycle".into()),
            name: None,
            session: "mlog-20261030-0001".into(),
            operator: "APK/Anthony".into(),
            issued_at_ms: 1_793_000_000_000,
            mac: None,
            dry_run: false,
        }
    }

    #[test]
    fn a_command_round_trips() {
        let c = good();
        assert_eq!(parse(c.to_json().as_bytes()), Ok(c));
    }

    #[test]
    fn the_topics_are_the_probes_own() {
        let (t, r) = command_topic("LinuxDesktop", "bench1").unwrap();
        assert_eq!(t, "APK.audio/System/BareMetal/MonitorProbes/LinuxDesktop/bench1/outgoing/command");
        assert!(!r, "a retained command re-fires on every reconnect");
        let (t, r) = state_topic("LinuxDesktop", "bench1").unwrap();
        assert_eq!(t, "APK.audio/System/BareMetal/MonitorProbes/LinuxDesktop/bench1/incoming/state");
        assert!(r, "the answer outlives the agent");
        assert!(detail_topic("LinuxDesktop", "bench1").unwrap().1);
        assert_eq!(address_of(&command_topic("P", "h").unwrap().0), Some(("P".into(), "h".into())));
        assert_eq!(address_of("APK.audio/System/BareMetal/MonitorProbes/P/h/incoming/state"), None);
    }

    #[test]
    fn the_acl_patterns_cover_the_machine_command_topic() {
        let topic = command_topic("LinuxDesktop", "bench1").unwrap().0;
        let depth = topic.split('/').count();
        assert!(acl_deny_patterns().iter().any(|p| p.split('/').count() == depth));
    }

    #[test]
    fn no_command_line_rides_along() {
        let mut v: Value = serde_json::from_str(&good().to_json()).unwrap();
        v["argv"] = json!(["rm", "-rf", "/"]);
        let r = parse(v.to_string().as_bytes()).unwrap_err();
        assert_eq!(r.id.as_deref(), Some("cmd-0001-abcd"), "the refusal is still answerable");
        assert!(r.reason.contains("argv"));
    }

    #[test]
    fn unconfirmed_is_refused() {
        let mut v: Value = serde_json::from_str(&good().to_json()).unwrap();
        v.as_object_mut().unwrap().remove("session");
        assert!(parse(v.to_string().as_bytes()).unwrap_err().reason.contains("session"));
    }

    #[test]
    fn the_answer_carries_the_id_and_the_session() {
        let c = good();
        let a: Value = serde_json::from_str(&answer(Some(&c), None, STATUS_DONE, "ok", "bench1", true, 5)).unwrap();
        assert_eq!(a["id"], "cmd-0001-abcd");
        assert_eq!(a["session"], "mlog-20261030-0001");
        assert_eq!(a["status"], "done");
        assert_eq!(a["dry_run"], true);
    }
}
