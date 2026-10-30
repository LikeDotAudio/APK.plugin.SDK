// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The narration line a discovery scan publishes — generated shape, plus the
//! one piece of behaviour the producer side needs.
//!
//! A thin wrapper in the same sense as `heartbeat` and `device_record`: `gen/`
//! owns the struct, this owns what a caller does with a level that arrived as a
//! `&str`.
//!
//! WHY THAT BEHAVIOUR IS HERE AND NOT AT THE CALL SITE. The level travels as a
//! plain `&str` from the first line of the scan to the wire —
//! `serial_publish::Row::level` is a `&'static str`, and `scan_log()` took one
//! — so the only place it could be judged is where it becomes a payload. It was
//! judged nowhere at all until PLAN-1040.01: the set was declared by a trailing
//! comment on the `json!` literal in `visa.rs`, which named four values while
//! the package's only level vocabulary, `topics::LOG_LEVELS`, named five
//! different ones. See `contracts/src/scan-log-line.ts` for why the two sets
//! are different sets and must stay that way.

pub use crate::gen::scan_log_line::{ScanLogLine, ScanLogLineLevel, ScanLogLineMessage};

/// What one narration line can report — the closed set, as strings, for the
/// callers that still hold a `&str`.
///
/// The Rust twin of `SCAN_LOG_LEVELS` in `contracts/src/scan-log-line.ts`. Both
/// are held to `vectors/payloads/ScanLogLine/` by the two suites that read it.
pub const SCAN_LOG_LEVELS: [&str; 4] = ["info", "ok", "warn", "error"];

/// Why a `&str` pair could not be a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanLogFault {
    /// A level outside `SCAN_LOG_LEVELS`. Carries what was said, because the
    /// interesting case is a typo and the typo is the evidence.
    UnknownLevel(String),
    /// A line with nothing in it. A narration feed exists to be read.
    EmptyMessage,
}

impl std::fmt::Display for ScanLogFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLevel(got) => write!(
                f,
                "scan log level {got:?} is in no vocabulary; expected one of {SCAN_LOG_LEVELS:?}"
            ),
            Self::EmptyMessage => f.write_str("a scan log line with an empty message says nothing"),
        }
    }
}

/// Build a line from the three things a producer actually holds.
///
/// It REFUSES rather than coercing, and the caller decides what a refusal
/// costs. That is deliberate: this is live-event narration, so a producer is
/// entirely within its rights to downgrade a bad level to `info` and still say
/// the sentence — what it must not do is publish a level no consumer can tone,
/// silently, which is the state the bus was in.
pub fn line(level: &str, message: &str, ts: u64) -> Result<ScanLogLine, ScanLogFault> {
    let level: ScanLogLineLevel = level
        .parse()
        .map_err(|_| ScanLogFault::UnknownLevel(level.to_string()))?;
    let message: ScanLogLineMessage = message.parse().map_err(|_| ScanLogFault::EmptyMessage)?;
    Ok(ScanLogLine {
        level,
        message,
        // `ts` is epoch SECONDS and the schema floors it at 0. A clock before
        // the epoch is the producer's problem, not the wire's: it saturates
        // rather than overflowing into a negative instant no reader expects.
        ts: i64::try_from(ts).unwrap_or(i64::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admits_every_level_the_set_declares() {
        for level in SCAN_LOG_LEVELS {
            let l = line(level, "a sentence", 1).expect("declared level");
            assert_eq!(l.level.to_string(), level);
        }
    }

    /// The defect PLAN-1040.01 names, from the producer's side: `ok` is a scan
    /// level and is NOT a `LOG_LEVELS` rung, and the two rungs a scan never
    /// narrates are not scan levels.
    #[test]
    fn the_two_vocabularies_are_different_sets() {
        assert!(SCAN_LOG_LEVELS.contains(&"ok"));
        assert!(!crate::topics::LOG_LEVELS.contains(&"ok"));
        assert!(!SCAN_LOG_LEVELS.contains(&"trace"));
        assert!(!SCAN_LOG_LEVELS.contains(&"debug"));
    }

    #[test]
    fn refuses_a_level_no_consumer_can_tone() {
        assert_eq!(
            line("debug", "a sentence", 1),
            Err(ScanLogFault::UnknownLevel("debug".into()))
        );
        assert_eq!(line("OK", "a sentence", 1), Err(ScanLogFault::UnknownLevel("OK".into())));
    }

    #[test]
    fn refuses_a_line_that_says_nothing() {
        assert_eq!(line("ok", "", 1), Err(ScanLogFault::EmptyMessage));
    }

    /// THE WIRE MUST NOT MOVE. Putting the payload under a schema replaced a
    /// `serde_json::json!` literal in `visa.rs` with a serialized struct, and
    /// two front-end files parse that JSON by field name today. This asserts
    /// the bytes are the ones the literal produced -- same three keys, same
    /// order, `ts` as a bare number -- so the contract is a description of the
    /// running bus rather than a redesign of it.
    #[test]
    fn serializes_to_the_bytes_the_json_literal_produced() {
        let l = line("ok", "GPIB0::9::INSTR is answering again", 1_788_698_912).unwrap();
        assert_eq!(
            serde_json::to_string(&l).unwrap(),
            r#"{"level":"ok","message":"GPIB0::9::INSTR is answering again","ts":1788698912}"#
        );
    }
}
