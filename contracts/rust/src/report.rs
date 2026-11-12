// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Logs and problems — the Rust twin of `contracts/src/report.ts`: one log
//! line and one problem record, minted the same way by every plugin, worker
//! and the runner, so one Log softapp and one Problems softapp read them all.
//!
//! ```
//! use apkaudio_contracts::report;
//! let m = report::log_line("visa", "warn", "GPIB0::9 did not answer", 1_793_000_000_000)
//!     .code("timeout").subject("HP 34401A").message();
//! assert_eq!(m.topic, "APK.audio/System/Log/visa/warn");
//! assert!(!m.retain);
//!
//! let p = report::problem_record("visa", "timeout", 1_793_000_000_000).subject("HP 34401A").message();
//! assert!(p.topic.starts_with("APK.audio/System/Problems/visa/timeout-"));
//! assert!(p.retain);
//! let gone = report::clear_problem("visa", "timeout", Some("HP 34401A"));
//! assert_eq!(gone.topic, p.topic);
//! assert!(gone.payload.is_empty());
//! ```
//!
//! The record carries the catalogue CODE, never the words: title, cause and
//! fix are looked up by the reader (`problems::problem`). An empty retained
//! publish on the problem's topic clears it. The payloads are checked against
//! the generated types (`gen::log_line::LogLine`,
//! `gen::problem_record::ProblemRecord`) in this module's tests.

use serde_json::{json, Map, Value};

use crate::identity::fnv1a64;
use crate::time::iso_from_unix_ms;
use crate::topics::{LOG_LEVELS, ROOT};

pub use crate::gen::log_line::LogLine;
pub use crate::gen::problem_record::ProblemRecord;

/// Five minutes: the bench's trust-decay window. Twin of `PROBLEM_DEFAULT_TTL_S`.
pub const PROBLEM_DEFAULT_TTL_S: u64 = 300;
pub const LOG_MESSAGE_MAX: usize = 500;
pub const REPORT_DETAIL_MAX: usize = 2000;

/// What a bus client needs to send one report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportMessage {
    pub topic: String,
    pub payload: String,
    pub retain: bool,
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// Any name as a topic segment — twin of `reportSource()`.
pub fn report_source(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let t = out.trim_matches('-');
    if t.is_empty() {
        "unknown".to_string()
    } else {
        t.to_string()
    }
}

/// The topic key for one problem — twin of `problemKey()`.
pub fn problem_key(code: &str, subject: Option<&str>) -> String {
    let c = report_source(code);
    match subject {
        Some(s) if !s.is_empty() => format!("{c}-{}", &fnv1a64(s)[..8]),
        _ => c,
    }
}

pub fn log_topic(source: &str, level: &str) -> String {
    format!("{ROOT}/System/Log/{}/{level}", report_source(source))
}

pub fn problem_topic(source: &str, key: &str) -> String {
    format!("{ROOT}/System/Problems/{}/{}", report_source(source), report_source(key))
}

/// A log line being built. `level` outside `LOG_LEVELS` is published as
/// `info` rather than lost: the sentence is the point of a log line.
pub struct LogLineBuilder {
    source: String,
    level: String,
    message: String,
    now_ms: i64,
    extra: Map<String, Value>,
}

pub fn log_line(source: &str, level: &str, message: &str, now_ms: i64) -> LogLineBuilder {
    let level = if LOG_LEVELS.contains(&level) { level } else { "info" };
    LogLineBuilder {
        source: report_source(source),
        level: level.to_string(),
        message: message.to_string(),
        now_ms,
        extra: Map::new(),
    }
}

impl LogLineBuilder {
    pub fn code(mut self, code: &str) -> Self {
        self.extra.insert("code".into(), json!(code));
        self
    }
    pub fn subject(mut self, subject: &str) -> Self {
        self.extra.insert("subject".into(), json!(subject));
        self
    }
    pub fn detail(mut self, detail: &str) -> Self {
        self.extra.insert("detail".into(), json!(clip(detail, REPORT_DETAIL_MAX)));
        self
    }
    pub fn app(mut self, app: &str) -> Self {
        self.extra.insert("app".into(), json!(app));
        self
    }
    pub fn value(&self) -> Value {
        let message = if self.message.is_empty() { "(empty)" } else { &self.message };
        let mut o = Map::new();
        o.insert("schemaVersion".into(), json!(1));
        o.insert("ts".into(), json!(iso_from_unix_ms(self.now_ms)));
        o.insert("source".into(), json!(self.source));
        o.insert("level".into(), json!(self.level));
        o.insert("message".into(), json!(clip(message, LOG_MESSAGE_MAX)));
        o.extend(self.extra.clone());
        Value::Object(o)
    }
    pub fn message(&self) -> ReportMessage {
        ReportMessage { topic: log_topic(&self.source, &self.level), payload: self.value().to_string(), retain: false }
    }
}

/// A problem record being built.
pub struct ProblemBuilder {
    source: String,
    code: String,
    subject: Option<String>,
    raised_at_ms: i64,
    now_ms: i64,
    count: u64,
    extra: Map<String, Value>,
}

pub fn problem_record(source: &str, code: &str, now_ms: i64) -> ProblemBuilder {
    ProblemBuilder {
        source: report_source(source),
        code: code.to_string(),
        subject: None,
        raised_at_ms: now_ms,
        now_ms,
        count: 1,
        extra: Map::new(),
    }
}

impl ProblemBuilder {
    pub fn subject(mut self, subject: &str) -> Self {
        self.subject = Some(subject.to_string());
        self
    }
    pub fn detail(mut self, detail: &str) -> Self {
        self.extra.insert("detail".into(), json!(clip(detail, REPORT_DETAIL_MAX)));
        self
    }
    /// The caller's own steps for this occurrence; the catalogue's follow.
    pub fn fix(mut self, steps: &[&str]) -> Self {
        if !steps.is_empty() {
            self.extra.insert("fix".into(), json!(steps));
        }
        self
    }
    pub fn app(mut self, app: &str) -> Self {
        self.extra.insert("app".into(), json!(app));
        self
    }
    pub fn ttl_s(mut self, ttl: u64) -> Self {
        self.extra.insert("ttl_s".into(), json!(ttl.max(1)));
        self
    }
    /// Raised again: keep the first `raised_at`, bump `count`.
    pub fn again(mut self, raised_at_ms: i64, previous_count: u64) -> Self {
        self.raised_at_ms = raised_at_ms;
        self.count = previous_count.saturating_add(1);
        self
    }
    pub fn key(&self) -> String {
        problem_key(&self.code, self.subject.as_deref())
    }
    pub fn value(&self) -> Value {
        let mut o = Map::new();
        o.insert("schemaVersion".into(), json!(1));
        o.insert("code".into(), json!(self.code));
        o.insert("source".into(), json!(self.source));
        o.insert("key".into(), json!(self.key()));
        o.insert("raised_at".into(), json!(iso_from_unix_ms(self.raised_at_ms)));
        o.insert("last_at".into(), json!(iso_from_unix_ms(self.now_ms)));
        o.insert("count".into(), json!(self.count));
        if let Some(s) = &self.subject {
            o.insert("subject".into(), json!(s));
        }
        o.extend(self.extra.clone());
        Value::Object(o)
    }
    pub fn message(&self) -> ReportMessage {
        ReportMessage { topic: problem_topic(&self.source, &self.key()), payload: self.value().to_string(), retain: true }
    }
}

/// The tombstone that clears a problem: an empty retained publish on its topic.
pub fn clear_problem(source: &str, code: &str, subject: Option<&str>) -> ReportMessage {
    ReportMessage { topic: problem_topic(source, &problem_key(code, subject)), payload: String::new(), retain: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_000_000_000;

    #[test]
    fn a_log_line_is_a_generated_log_line_on_its_topic() {
        let m = log_line("visa", "error", "no reply", NOW).code("timeout").subject("HP 34401A").app("scan").message();
        let parsed: LogLine = serde_json::from_str(&m.payload).expect("LogLine");
        assert_eq!(parsed.level.to_string(), "error");
        assert_eq!(m.topic, crate::topics::log("visa", "error").unwrap());
        assert!(!m.retain);
    }

    #[test]
    fn an_unknown_level_still_says_the_sentence() {
        let m = log_line("visa", "loud", "said anyway", NOW).message();
        assert!(m.topic.ends_with("/info"));
    }

    #[test]
    fn a_problem_record_is_a_generated_problem_record_on_its_topic() {
        let b = problem_record("os shell", "fetch_failed", NOW).subject("/api/os/jobs").detail("Failed to fetch").fix(&["Start OsApi."]).ttl_s(60);
        let m = b.message();
        let parsed: ProblemRecord = serde_json::from_str(&m.payload).expect("ProblemRecord");
        assert_eq!(parsed.count.get(), 1);
        assert_eq!(m.topic, crate::topics::problems("os-shell", &b.key()).unwrap());
        assert!(m.retain);
    }

    #[test]
    fn raised_again_keeps_the_first_raise() {
        let v = problem_record("visa", "timeout", NOW + 5000).again(NOW, 3).value();
        assert_eq!(v["count"], 4);
        assert_eq!(v["raised_at"], json!(iso_from_unix_ms(NOW)));
    }

    /// The same key TS mints — report.test.ts asserts the same string.
    #[test]
    fn keys_match_the_typescript_twin() {
        assert_eq!(problem_key("timeout", None), "timeout");
        assert_eq!(problem_key("timeout", Some("HP 34401A")), format!("timeout-{}", &fnv1a64("HP 34401A")[..8]));
        assert_eq!(report_source("os shell/jobs"), "os-shell-jobs");
        assert_eq!(report_source("///"), "unknown");
    }

    #[test]
    fn the_catalogue_explains_a_record_by_its_code() {
        let p = crate::problems::problem("timeout");
        assert!(!p.fix.is_empty());
    }
}
