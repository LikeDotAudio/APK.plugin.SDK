// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The plugin's voice on the report contract (ROAD-3695 step 3): every log
//! line the runner — or a plugin through [`elog!`](crate::elog) — prints is
//! ALSO said as a `LogLine` on `APK.audio/System/Log/{token}/{level}`, and
//! every fault the runner can name is raised as a retained `ProblemRecord` on
//! `APK.audio/System/Problems/{token}/{key}` and cleared when it is over.
//! The builders are the contract's (`apkaudio_contracts::report`); this module
//! only owns the plugin's broker session and what it has raised.
//!
//! ```ignore
//! apk_plugin_runner::elog!(warn, "⚠️  [{token}] GPIB0::9 did not answer");   // stderr + LogLine
//! apk_plugin_runner::report::raise("timeout", Some("HP 34401A"), "no reply in 2 s");
//! apk_plugin_runner::report::clear("timeout", Some("HP 34401A"));
//! ```
//!
//! WHO RAISES WHAT, in the runner itself:
//!
//! | fault                                   | code                  | subject        | cleared when                 |
//! |-----------------------------------------|-----------------------|----------------|------------------------------|
//! | the process is about to exit on a fault | `plugin_down`         | the token      | the next process connects    |
//! | a self-test check failed                | the check's own code, else `plugin_check_failed` | `<token>/<check>` | the check passes |
//! | a worker job answered `ok: false`       | `job_failed`          | the tool       | a later job of it succeeds   |
//! | an agent account says `unavailable`     | `plugin_unavailable`  | the protocol   | it says `listening`          |
//!
//! NEVER BLOCKS, NEVER THROWS. `try_publish` only: a full queue drops a report
//! (counted as a `publish_failures`, like every other publish), the stderr line
//! stands. Before the session exists (config read at start) lines are held, at
//! most [`EARLY_LINES`], and said once it does.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use apkaudio_contracts::report::{self as contract, ReportMessage, PROBLEM_DEFAULT_TTL_S};
use rumqttc::{Client, QoS};

/// Lines said before the session exists, held for it.
pub const EARLY_LINES: usize = 32;

/// The catalogue code the runner raises when a self-test check fails without one.
pub const CHECK_FAILED: &str = "plugin_check_failed";
/// …when a worker job answers `ok: false`.
pub const JOB_FAILED: &str = "job_failed";
/// …when an agent's own account says it can see nothing.
pub const UNAVAILABLE: &str = "plugin_unavailable";
/// …when the process leaves on a fault (agent returned, panicked, stalled, session died).
pub const DOWN: &str = "plugin_down";

struct Voice {
    client: Client,
    source: String,
}

static VOICE: OnceLock<Voice> = OnceLock::new();
static EARLY: Mutex<Vec<ReportMessage>> = Mutex::new(Vec::new());
/// topic → (raised_at_ms, count) of what this process raised and has not cleared.
static RAISED: OnceLock<Mutex<HashMap<String, (i64, u64)>>> = OnceLock::new();
/// What source a line is said under before [`install`] (set by [`name`]).
static NAME: OnceLock<String> = OnceLock::new();

fn raised() -> &'static Mutex<HashMap<String, (i64, u64)>> {
    RAISED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The source every report of this process is said under: the token the
/// session was installed with, else the one [`name`] gave, else the binary.
pub fn source() -> String {
    if let Some(v) = VOICE.get() {
        return v.source.clone();
    }
    if let Some(n) = NAME.get() {
        return contract::report_source(n);
    }
    let exe = std::env::args().next().unwrap_or_default();
    contract::report_source(exe.rsplit('/').next().unwrap_or("plugin"))
}

/// Say who this process is before its session exists (the runner's first
/// line). The first call wins.
pub fn name(token: &str) {
    let _ = NAME.set(token.to_string());
}

/// Give the reports a session. The first call wins (a group's first module
/// speaks for the process); the lines held until now are said.
pub fn install(client: &Client, token: &str) {
    if VOICE.set(Voice { client: client.clone(), source: contract::report_source(token) }).is_err() {
        return;
    }
    let held = std::mem::take(&mut *EARLY.lock().unwrap_or_else(|e| e.into_inner()));
    for m in held {
        send(&m);
    }
}

/// Is a session installed?
pub fn installed() -> bool {
    VOICE.get().is_some()
}

fn send(m: &ReportMessage) -> bool {
    match VOICE.get() {
        Some(v) => {
            let ok = v.client.try_publish(m.topic.clone(), QoS::AtMostOnce, m.retain, m.payload.clone().into_bytes()).is_ok();
            crate::loopback::published(ok);
            ok
        }
        None => {
            let mut early = EARLY.lock().unwrap_or_else(|e| e.into_inner());
            if early.len() < EARLY_LINES {
                early.push(m.clone());
            }
            false
        }
    }
}

/// The level a stderr line is said at, read off its own first mark: 🛑/❌ →
/// `error`, ⚠️ → `warn`, else `info`. So a call site that already says how
/// bad it is does not say it twice.
pub fn level_of(line: &str) -> &'static str {
    let head = line.trim_start();
    if head.starts_with('🛑') || head.starts_with('❌') || head.contains("❌") {
        "error"
    } else if head.starts_with('⚠') {
        "warn"
    } else {
        "info"
    }
}

/// One `LogLine` at `level` (`trace` … `error`).
pub fn log(level: &str, message: &str) -> bool {
    send(&contract::log_line(&source(), level, message.trim(), now_ms()).message())
}

/// One `LogLine` about a catalogue code, with what it is about.
pub fn log_about(level: &str, message: &str, code: &str, subject: Option<&str>, detail: &str) -> bool {
    let mut b = contract::log_line(&source(), level, message.trim(), now_ms()).code(code);
    if let Some(s) = subject.filter(|s| !s.is_empty()) {
        b = b.subject(s);
    }
    if !detail.is_empty() {
        b = b.detail(detail);
    }
    send(&b.message())
}

/// The record [`raise`] would publish, with `count` / `raised_at` carried
/// from what this process raised on that topic before.
pub fn record(code: &str, subject: Option<&str>, detail: &str) -> ReportMessage {
    let mut b = contract::problem_record(&source(), code, now_ms()).ttl_s(PROBLEM_DEFAULT_TTL_S);
    if let Some(s) = subject.filter(|s| !s.is_empty()) {
        b = b.subject(s);
    }
    if !detail.is_empty() {
        b = b.detail(detail);
    }
    let topic = contract::problem_topic(&source(), &b.key());
    let mut book = raised().lock().unwrap_or_else(|e| e.into_inner());
    if let Some((first, count)) = book.get(&topic).copied() {
        b = b.again(first, count);
    }
    let m = b.message();
    let first = book.get(&topic).map_or(now_ms(), |(f, _)| *f);
    let count = book.get(&topic).map_or(1, |(_, c)| c + 1);
    book.insert(topic, (first, count));
    m
}

/// Raise (or raise again) a problem — retained — and say it as an `error` line.
pub fn raise(code: &str, subject: Option<&str>, detail: &str) -> bool {
    let m = record(code, subject, detail);
    let sent = send(&m);
    log_about("error", &format!("problem {code}{}", subject.map(|s| format!(" — {s}")).unwrap_or_default()), code, subject, detail);
    sent
}

/// Clear a problem THIS PROCESS raised (no tombstone storm for what it never said).
pub fn clear(code: &str, subject: Option<&str>) -> bool {
    let m = contract::clear_problem(&source(), code, subject);
    let was = raised().lock().unwrap_or_else(|e| e.into_inner()).remove(&m.topic).is_some();
    if was {
        send(&m);
        log_about("info", &format!("cleared {code}{}", subject.map(|s| format!(" — {s}")).unwrap_or_default()), code, subject, "");
    }
    was
}

/// Clear a problem whoever raised it — an earlier process of this plugin
/// (its `plugin_down` once a new one is up, a check that fails no more).
pub fn clear_any(code: &str, subject: Option<&str>) -> bool {
    let m = contract::clear_problem(&source(), code, subject);
    raised().lock().unwrap_or_else(|e| e.into_inner()).remove(&m.topic);
    send(&m)
}

/// Raise `plugin_down` and give the session a moment to carry it, before an
/// exit the runner chose. The last will still says `unavailable`.
pub fn going_down(token: &str, why: &str) {
    raise(DOWN, Some(token), why);
    if installed() {
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

/// Raise or clear one problem per self-test check: a failure raises its own
/// code (else [`CHECK_FAILED`]) about `<token>/<check>`; a pass clears both
/// that default and whatever this process raised for it.
pub fn checks(token: &str, report: &apkaudio_contracts::self_test::Report) {
    use apkaudio_contracts::self_test::Outcome;
    for check in report.checks() {
        let subject = format!("{token}/{}", check.name);
        match check.outcome {
            Outcome::Fail => {
                raise(check.problem.as_deref().unwrap_or(CHECK_FAILED), Some(&subject), &check.detail);
            }
            Outcome::Pass => {
                clear_any(CHECK_FAILED, Some(&subject));
                let codes: Vec<String> = {
                    let book = raised().lock().unwrap_or_else(|e| e.into_inner());
                    let src = source();
                    let key_tail = format!("-{}", &apkaudio_contracts::identity::fnv1a64(&subject)[..8]);
                    let prefix = format!("{}/System/Problems/{src}/", apkaudio_contracts::topics::ROOT);
                    book.keys()
                        .filter_map(|t| t.strip_prefix(&prefix))
                        .filter_map(|k| k.strip_suffix(&key_tail))
                        .map(str::to_string)
                        .collect()
                };
                for code in codes {
                    clear_any(&code, Some(&subject));
                }
            }
            _ => {}
        }
    }
}

/// A worker's answer: `ok: false` raises [`JOB_FAILED`] about the tool, a
/// success clears what an earlier failure raised.
pub fn job_answer(tool: &str, answer: &serde_json::Value) {
    if answer["ok"] == serde_json::Value::Bool(false) {
        let error = answer["error"].as_str().unwrap_or("the job failed");
        let id = answer["job_id"].as_str().unwrap_or("");
        raise(JOB_FAILED, Some(tool), &if id.is_empty() { error.to_string() } else { format!("job {id}: {error}") });
    } else if answer["ok"] == serde_json::Value::Bool(true) {
        clear(JOB_FAILED, Some(tool));
    }
}

/// An agent's account of itself: `unavailable` raises [`UNAVAILABLE`] about
/// the protocol with the account's own detail; `listening` clears it.
pub fn agent_state(proto: &str, state: apkaudio_contracts::agent_state::AgentState, detail: &str) {
    use apkaudio_contracts::agent_state::AgentState;
    match state {
        AgentState::Unavailable => {
            raise(UNAVAILABLE, Some(proto), detail);
        }
        AgentState::Listening => {
            clear_any(UNAVAILABLE, Some(proto));
        }
        _ => {}
    }
}

/// `eprintln!` with a `LogLine` twin: the same text to stderr and to
/// `System/Log/{token}/{level}`. The level is read off the line's own mark
/// (🛑 error, ⚠️ warn — [`report::level_of`](crate::report::level_of)), or
/// given first: `elog!(error, "…", …)`.
#[macro_export]
macro_rules! elog {
    (trace, $($arg:tt)*) => {{ let __line = format!($($arg)*); eprintln!("{__line}"); $crate::report::log("trace", &__line); }};
    (debug, $($arg:tt)*) => {{ let __line = format!($($arg)*); eprintln!("{__line}"); $crate::report::log("debug", &__line); }};
    (info, $($arg:tt)*) => {{ let __line = format!($($arg)*); eprintln!("{__line}"); $crate::report::log("info", &__line); }};
    (warn, $($arg:tt)*) => {{ let __line = format!($($arg)*); eprintln!("{__line}"); $crate::report::log("warn", &__line); }};
    (error, $($arg:tt)*) => {{ let __line = format!($($arg)*); eprintln!("{__line}"); $crate::report::log("error", &__line); }};
    ($($arg:tt)*) => {{ let __line = format!($($arg)*); eprintln!("{__line}"); $crate::report::log($crate::report::level_of(&__line), &__line); }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use apkaudio_contracts::report::ProblemRecord;

    #[test]
    fn a_line_says_how_bad_it_is_by_its_own_mark() {
        assert_eq!(level_of("🛑 [visa] the session ended"), "error");
        assert_eq!(level_of("   📡 [visa MQTT] ❌ x not published"), "error");
        assert_eq!(level_of("⚠️  [visa] drain late"), "warn");
        assert_eq!(level_of("stale: /tmp/x is 40s old"), "info");
    }

    #[test]
    fn raised_again_counts_and_keeps_the_first_raise_cleared_only_once() {
        let a = record("timeout", Some("unit-test-meter"), "no reply");
        let b = record("timeout", Some("unit-test-meter"), "no reply");
        let ra: ProblemRecord = serde_json::from_str(&a.payload).expect("a ProblemRecord");
        let rb: ProblemRecord = serde_json::from_str(&b.payload).expect("a ProblemRecord");
        assert_eq!((ra.count.get(), rb.count.get()), (1, 2));
        assert_eq!(ra.raised_at, rb.raised_at);
        assert!(a.retain && a.topic == b.topic);
        assert!(a.topic.contains("/System/Problems/"));
        assert!(clear("timeout", Some("unit-test-meter")), "it was raised here");
        assert!(!clear("timeout", Some("unit-test-meter")), "a second clear says nothing");
    }

    #[test]
    fn without_a_session_lines_are_held_not_lost_and_nothing_blocks() {
        if installed() {
            return;
        }
        for i in 0..(EARLY_LINES + 5) {
            assert!(!log("info", &format!("line {i}")));
        }
        assert_eq!(EARLY.lock().unwrap().len(), EARLY_LINES);
    }

    #[test]
    fn a_failed_job_raises_and_a_good_one_clears() {
        job_answer("UNIT.Test", &serde_json::json!({ "ok": false, "error": "bad file", "job_id": "j1" }));
        assert!(clear(JOB_FAILED, Some("UNIT.Test")));
        job_answer("UNIT.Test", &serde_json::json!({ "ok": true }));
        assert!(!clear(JOB_FAILED, Some("UNIT.Test")));
    }
}
