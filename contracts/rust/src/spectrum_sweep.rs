// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! One complete spectrum-analyzer sweep — generated shape, plus the two rules
//! the structural subset cannot carry.
//!
//! The Rust twin of `contracts/src/spectrum-sweep.ts`. `gen/` owns the struct
//! (cargo-typify from `schemas/SpectrumSweep.schema.json`); the zod refinements
//! — `points` counts `trace`, `stopHz >= startHz`, `schemaVersion` is 1 — are
//! not JSON Schema, so they are [`check`] here and a reader calls [`read`],
//! which is deserialise-then-check. Retained at
//! `…/visa/Device/{family}/{model}/{dev}/Reading/Sweep` by AUDIO.komand's sweep
//! recorder (`topics::visa_reading(…, SWEEP_LEAF)`).

pub use crate::gen::spectrum_sweep::SpectrumSweep;

/// The leaf under `…/Reading/` a sweep is retained at (`SPECTRUM_SWEEP_LEAF`).
pub const SWEEP_LEAF: &str = "Sweep";

/// The refinements the schema cannot state. `Err` says which.
pub fn check(s: &SpectrumSweep) -> Result<(), String> {
    if s.schema_version != 1.0 {
        return Err(format!("schemaVersion {} is not 1", s.schema_version));
    }
    if s.points < 2 {
        return Err(format!("{} point(s) is not a sweep", s.points));
    }
    if s.trace.len() as i64 != s.points {
        return Err(format!("points says {} and the trace holds {}", s.points, s.trace.len()));
    }
    if s.stop_hz < s.start_hz {
        return Err(format!("stop {} Hz is below start {} Hz", s.stop_hz, s.start_hz));
    }
    if s.trace.iter().any(|v| !v.is_finite()) {
        return Err("a sample is not a finite number".to_string());
    }
    Ok(())
}

/// Deserialise and check — the one way a Rust reader should take a sweep.
pub fn read(json: &str) -> Result<SpectrumSweep, String> {
    let s: SpectrumSweep = serde_json::from_str(json).map_err(|e| e.to_string())?;
    check(&s)?;
    Ok(s)
}

/// The frequency, Hz, `trace[i]` was measured at: evenly spaced, start to stop
/// inclusive (`sweepFrequencyHz`).
pub fn frequency_hz(s: &SpectrumSweep, i: usize) -> f64 {
    if s.points < 2 {
        return s.start_hz;
    }
    s.start_hz + (i as f64) * (s.stop_hz - s.start_hz) / ((s.points - 1) as f64)
}
