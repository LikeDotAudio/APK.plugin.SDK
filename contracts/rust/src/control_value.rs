// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! ControlValue wrapper — re-exports the generated types (gen/ is typify
//! output, never hand-edited) and adds the behavior half of the contract: the
//! constructors that make an envelope buildable, and `present_control()`, the
//! rendering rule. Twin of `contracts/src/control-value.ts`.
//!
//! WHY THIS FILE EXISTS AND WHY IT DID NOT
//!
//! `ControlError` was declared, schema'd, generated into Rust, tested in
//! TypeScript — and published by nothing, read by nothing, for the whole of
//! its life. The cause is here rather than in any agent: `gen/control_value.rs`
//! is a set of newtypes with PRIVATE fields whose only constructors are
//! `FromStr` impls that validate an ISO-8601 pattern. Every other payload
//! contract in this crate has a hand-written half beside its generated one —
//! `heartbeat` has the LWT helper, `device_record` has the v40 mapping,
//! `agent_counters` has `Sampler` — and this one had none. An agent wanting to
//! publish a faulted reading had to parse its own timestamp into a private
//! newtype and hand-roll the schema version. So it did not, and the contract
//! decayed exactly the way a contract with no adopters does.
//!
//! WHAT `faulted()` REFUSES TO LET A CALLER DO
//!
//! Drop the value. `value` STAYS PRESENT when `error` is set — a faulted
//! reading is still evidence, and an instrument that has lost lock still
//! reports the rate it thinks it has. There is deliberately no constructor
//! that takes an error without a reading.
//!
//! WHAT IS NOT HERE
//!
//! Units. `unitFamily`/`convert` are the TypeScript side's, and duplicating
//! the conversion tables here would be a second answer to an arithmetic
//! question — the receiver that converts is the browser. This half is what a
//! Rust PUBLISHER needs and what a Rust CONSUMER needs to read one back.

pub use crate::gen::control_value::{
    ControlValue, ControlValueError, ControlValueErrorCode, ControlValueErrorMessage,
    ControlValueErrorSince, ControlValueOrigin, ControlValueTs, ControlValueUnit,
    ControlValueValue,
};

type ConvErr = crate::gen::control_value::error::ConversionError;

/// One reading the node trusts.
///
/// `at_ms` is wall clock in Unix milliseconds, not an `Instant`: `ts` is what
/// lets a late or reordered message be recognised as stale rather than applied
/// as current, and a monotonic clock cannot be compared across processes.
pub fn reading(
    value: ControlValueValue,
    unit: Option<&str>,
    at_ms: i64,
    origin: &str,
) -> Result<ControlValue, String> {
    Ok(ControlValue {
        error: None,
        origin: origin.parse().map_err(|e: ConvErr| e.to_string())?,
        schema_version: 1.0,
        ts: crate::time::iso_from_unix_ms(at_ms)
            .parse()
            .map_err(|e: ConvErr| e.to_string())?,
        unit: match unit {
            Some(u) => Some(u.parse().map_err(|e: ConvErr| e.to_string())?),
            None => None,
        },
        value,
    })
}

/// The same reading, with the fault that condemns it riding in the same
/// envelope.
///
/// `code` is machine-readable and stable — `no_lock`, `over_range`, `timeout`
/// — and never a sentence. `message` is the one line a human reads. `since_ms`
/// is when the FAULT started if the node knows, which is not when this message
/// was sent: the difference is the whole question of whether something is
/// flapping or has been down since Tuesday.
pub fn faulted(
    value: ControlValueValue,
    unit: Option<&str>,
    at_ms: i64,
    origin: &str,
    code: &str,
    message: &str,
    since_ms: Option<i64>,
) -> Result<ControlValue, String> {
    let mut cv = reading(value, unit, at_ms, origin)?;
    cv.error = Some(ControlValueError {
        code: code.parse().map_err(|e: ConvErr| e.to_string())?,
        message: message.parse().map_err(|e: ConvErr| e.to_string())?,
        since: match since_ms {
            Some(ms) => Some(
                crate::time::iso_from_unix_ms(ms)
                    .parse()
                    .map_err(|e: ConvErr| e.to_string())?,
            ),
            None => None,
        },
    });
    Ok(cv)
}

/// How a reading is DRAWN. Mirror of `ControlPresentation` in the TypeScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlPresentation {
    Good,
    Faulted,
    /// No fault, and nothing has arrived for a while. Deliberately a THIRD
    /// state and not a kind of error: "the instrument says this and is
    /// unhappy" and "nobody has said anything for a while" are different
    /// facts, and a panel drawing them the same way is lying about one.
    Stale,
}

/// The single reading of a `ControlValue` every renderer should work from.
///
/// The rule, in one line: SHOW THE NUMBER, MARK IT, SAY WHY. The value is
/// never suppressed, so this returns the presentation and the caller keeps the
/// envelope — there is nothing to blank.
///
/// `stale_after_ms` is the CALLER's policy, not the contract's: a rate that
/// updates every 100 ms and a temperature that updates hourly do not share a
/// staleness threshold. `None` means never stale.
pub fn present_control(
    cv: &ControlValue,
    now_ms: i64,
    stale_after_ms: Option<i64>,
) -> ControlPresentation {
    if cv.error.is_some() {
        return ControlPresentation::Faulted;
    }
    if let Some(window) = stale_after_ms {
        if let Some(sent) = crate::time::unix_ms_from_iso(&cv.ts) {
            // A future timestamp is clock skew, not freshness; it is not stale.
            if now_ms - sent > window {
                return ControlPresentation::Stale;
            }
        }
    }
    ControlPresentation::Good
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = 1_756_800_000_000; // 2025-09-02T08:00:00Z

    #[test]
    fn a_trusted_reading_carries_no_error() {
        let cv = reading(ControlValueValue::Number(48_000.0), Some("Hz"), T, "ptp-agent").unwrap();
        assert!(cv.error.is_none());
        assert_eq!(present_control(&cv, T, None), ControlPresentation::Good);
    }

    /// The whole point of the envelope. A constructor that let the value go
    /// would throw away the diagnosis to make the schema tidier.
    #[test]
    fn a_faulted_reading_keeps_the_value_and_the_unit() {
        let cv = faulted(
            ControlValueValue::Number(48_000.0),
            Some("Hz"),
            T,
            "ptp-agent",
            "no_lock",
            "grandmaster lost 4 s ago",
            Some(T - 4_000),
        )
        .unwrap();

        assert_eq!(cv.value, ControlValueValue::Number(48_000.0));
        assert_eq!(cv.unit.as_deref().map(String::as_str), Some("Hz"));
        assert_eq!(present_control(&cv, T, None), ControlPresentation::Faulted);
    }

    /// A clock reporting `no_lock` in Hz is still a clock — `since` says when
    /// the fault started, which is not when the message was sent.
    #[test]
    fn since_is_the_faults_clock_not_the_messages() {
        let cv = faulted(
            ControlValueValue::Number(1.0),
            None,
            T,
            "ptp-agent",
            "no_lock",
            "grandmaster lost",
            Some(T - 60_000),
        )
        .unwrap();
        let error = cv.error.as_ref().unwrap();
        assert_ne!(error.since.as_deref().map(String::as_str), Some(cv.ts.as_str()));
        assert_eq!(
            error.since.as_deref().map(String::as_str),
            Some(crate::time::iso_from_unix_ms(T - 60_000).as_str())
        );
    }

    #[test]
    fn a_fault_beats_staleness() {
        let cv = faulted(
            ControlValueValue::Boolean(true),
            None,
            T,
            "ptp-agent",
            "timeout",
            "no reply",
            None,
        )
        .unwrap();
        assert_eq!(
            present_control(&cv, T + 600_000, Some(1_000)),
            ControlPresentation::Faulted
        );
    }

    #[test]
    fn nothing_for_a_while_is_stale_and_only_when_the_caller_says_so() {
        let cv = reading(ControlValueValue::Number(1.0), None, T, "ptp-agent").unwrap();
        assert_eq!(
            present_control(&cv, T + 30_000, Some(10_000)),
            ControlPresentation::Stale
        );
        assert_eq!(
            present_control(&cv, T + 30_000, None),
            ControlPresentation::Good
        );
    }

    /// Clock skew puts a timestamp in the future. That is not freshness
    /// failing; drawing it as stale would blink a healthy reading.
    #[test]
    fn clock_skew_is_fresh_not_stale() {
        let cv = reading(ControlValueValue::Number(1.0), None, T + 5_000, "ptp-agent").unwrap();
        assert_eq!(
            present_control(&cv, T, Some(1_000)),
            ControlPresentation::Good
        );
    }

    /// `minLength: 1` on the generated newtypes is the schema's, and it is
    /// what stops an empty code or an empty message reaching the bus. Asserted
    /// here because these constructors are now the only way an agent builds
    /// one, so this is where the refusal has to hold.
    #[test]
    fn an_empty_code_or_message_is_refused() {
        let empty_code = faulted(
            ControlValueValue::Number(1.0),
            None,
            T,
            "ptp-agent",
            "",
            "something",
            None,
        );
        assert!(empty_code.is_err());

        let empty_message = faulted(
            ControlValueValue::Number(1.0),
            None,
            T,
            "ptp-agent",
            "no_lock",
            "",
            None,
        );
        assert!(empty_message.is_err());

        assert!(reading(ControlValueValue::Number(1.0), Some(""), T, "ptp-agent").is_err());
        assert!(reading(ControlValueValue::Number(1.0), None, T, "").is_err());
    }

    /// The envelope crosses the boundary as JSON, and `error` is omitted
    /// rather than sent as null when there is none — a reader on the other
    /// side treats absent as "the node asserts this reading is good".
    #[test]
    fn the_envelope_round_trips_and_omits_an_absent_error() {
        let cv = reading(ControlValueValue::String("locked".into()), None, T, "ptp-agent").unwrap();
        let json = serde_json::to_string(&cv).unwrap();
        assert!(!json.contains("\"error\""));
        assert!(!json.contains("\"unit\""));

        let back: ControlValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cv);
    }
}
