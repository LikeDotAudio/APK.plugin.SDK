// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! AgentCounters wrapper — re-exports the generated types (gen/ is typify
//! output, never hand-edited) and adds the behavior half of the contract:
//! the topic builders, the differencing rule, and the sampler an agent uses
//! to publish. Twin of `contracts/src/agent-counters.ts`.
//!
//! WHY A SAMPLER AND NOT A STRUCT LITERAL
//!
//! Every honesty rule in this contract is a rule about STATE HELD BETWEEN
//! SAMPLES — monotonic since `startedAt`, a declared cadence, never retained.
//! A publisher that builds the payload by hand has to remember all three, and
//! the two agents wiring this up would each remember them separately. So the
//! state lives here: `Sampler` owns `startedAt`, owns the counters, and can
//! only be incremented. There is deliberately no way to decrease a counter.
//!
//! NOT RETAINED, AND THE HELPER ENFORCES IT
//!
//! `Sampler::publication()` returns the retain flag alongside the topic and
//! the bytes, always `false`. It is returned rather than documented because
//! the retain argument is a bare bool at every rumqttc call site, and the one
//! failure this contract exists to prevent — a counter that outlives its
//! process and keeps answering — is exactly one `true` away.
//!
//! WHERE `minimum: 0` IS ACTUALLY ENFORCED
//!
//! cargo-typify does not carry JSON-Schema numeric bounds into Rust, so the
//! generated `counters` map is `i64` and `serde` will happily read a negative
//! count that zod rejects on the TypeScript side. The bound is held in three
//! other places instead, and it is worth knowing which: `Sampler` counts in
//! `u64` and offers no way to decrease one, so this crate cannot PUBLISH a
//! negative; `AgentCountersSchema` rejects one on the consumer side; and
//! `rates_between` refuses a sample carrying one rather than dividing by it.
//! Do not add a `counters` vector under `invalid/` for a negative value — the
//! vectors are shared, and it would pass Rust and fail TypeScript.

use std::collections::HashMap;

pub use crate::gen::agent_counters::{
    AgentCounters, AgentCountersAgent, AgentCountersAt, AgentCountersCountersKey,
    AgentCountersStartedAt,
};

use crate::topics::{is_segment, TopicError, ROOT};

type ConvErr = crate::gen::agent_counters::error::ConversionError;

/// `APK.audio/System/Counters/{agent}` — not retained; see the module docs.
pub fn counters_topic(agent: &str) -> Result<String, TopicError> {
    if !is_segment(agent) {
        return Err(TopicError { what: "agent", value: agent.to_string() });
    }
    Ok(format!("{ROOT}/System/Counters/{agent}"))
}

pub fn counters_wildcard() -> String {
    format!("{ROOT}/System/Counters/+")
}

/// Difference two samples into per-second rates.
///
/// Returns `None` rather than a number when the two cannot honestly be
/// subtracted — a restart (`startedAt` moved), two different agents, samples
/// out of order, or the same instant twice. Refusing is the point: a rate
/// computed across a restart is not a slow rate, it is a wrong one.
///
/// Byte-for-byte the same rule as `ratesBetween()` in the TypeScript twin,
/// and pinned to it by `vectors/payloads/AgentCounters.json`.
pub fn rates_between(a: &AgentCounters, b: &AgentCounters) -> Option<HashMap<String, f64>> {
    if a.agent != b.agent {
        return None;
    }
    if a.started_at != b.started_at {
        return None;
    }
    let seconds = (crate::time::unix_ms_from_iso(&b.at)? - crate::time::unix_ms_from_iso(&a.at)?)
        as f64
        / 1000.0;
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }

    // typify dropped `minimum: 0`, so serde admits a negative here even
    // though zod does not. Refuse the whole sample rather than difference
    // against a value that is not a count. See the module docs.
    if a.counters.values().chain(b.counters.values()).any(|v| *v < 0) {
        return None;
    }

    let mut rates = HashMap::new();
    for (key, now) in &b.counters {
        let Some(then) = a.counters.get(key) else { continue };
        // A counter that went backwards without startedAt moving is a bug in
        // the publisher, not a negative rate. Drop it rather than draw it.
        if now < then {
            continue;
        }
        rates.insert(key.to_string(), (now - then) as f64 / seconds);
    }
    Some(rates)
}

/// The publisher's half: monotonic counters plus the moment they were zeroed.
///
/// Held for the life of the agent process. `started_at` is captured once at
/// construction and never moves, which is what lets a consumer tell a restart
/// from a lost message.
pub struct Sampler {
    agent: String,
    started_at: String,
    interval_ms: u64,
    counters: HashMap<String, u64>,
}

impl Sampler {
    /// `started_at_ms` is process start as a Unix millisecond stamp.
    ///
    /// `interval_ms` is the cadence the agent PROMISES to publish at, and it
    /// travels in the payload so a consumer that expected a sample and did
    /// not get one can tell idle from gone. Declaring a cadence the agent
    /// does not keep is the one way to make that field lie.
    pub fn new(agent: &str, started_at_ms: i64, interval_ms: u64) -> Result<Self, TopicError> {
        if !is_segment(agent) {
            return Err(TopicError { what: "agent", value: agent.to_string() });
        }
        Ok(Self {
            agent: agent.to_string(),
            started_at: crate::time::iso_from_unix_ms(started_at_ms),
            interval_ms,
            counters: HashMap::new(),
        })
    }

    /// Add to a counter, creating it at zero on first sight.
    ///
    /// There is no `set` and no `reset`: monotonic since `started_at` is the
    /// contract, and the only honest way to zero these is a new process with
    /// a new `started_at`.
    pub fn add(&mut self, key: &str, n: u64) {
        *self.counters.entry(key.to_string()).or_insert(0) += n;
    }

    pub fn inc(&mut self, key: &str) {
        self.add(key, 1);
    }

    /// Declare a counter at zero so it appears in samples before it first
    /// moves. A counter that only materialises once it is non-zero makes
    /// "nothing has happened yet" indistinguishable from "this agent does not
    /// count that", which is the same missing-vs-idle confusion `interval_ms`
    /// exists to resolve one level up.
    pub fn declare(&mut self, key: &str) {
        self.counters.entry(key.to_string()).or_insert(0);
    }

    /// Read one counter, zero if it has never been declared or moved.
    ///
    /// For a publisher deriving a CONCLUSION from its own counts — PTP's
    /// `udp_blocked` is the live case — not for a consumer, which should
    /// difference two published samples instead.
    pub fn get(&self, key: &str) -> u64 {
        self.counters.get(key).copied().unwrap_or(0)
    }

    /// Take a sample at `at_ms`.
    pub fn sample(&self, at_ms: i64) -> Result<AgentCounters, String> {
        let mut counters = HashMap::new();
        for (k, v) in &self.counters {
            let key: AgentCountersCountersKey =
                k.parse().map_err(|e: ConvErr| e.to_string())?;
            counters.insert(key, *v as i64);
        }
        Ok(AgentCounters {
            agent: self.agent.parse().map_err(|e: ConvErr| e.to_string())?,
            at: crate::time::iso_from_unix_ms(at_ms).parse().map_err(|e: ConvErr| e.to_string())?,
            counters,
            interval_ms: std::num::NonZeroU64::new(self.interval_ms)
                .ok_or_else(|| "intervalMs must be positive".to_string())?,
            schema_version: 1.0,
            started_at: self.started_at.parse().map_err(|e: ConvErr| e.to_string())?,
        })
    }

    /// Everything a publish call needs: topic, JSON bytes, and the retain
    /// flag — which is always `false`, and is returned rather than left to
    /// the call site for the reason in the module docs.
    pub fn publication(&self, at_ms: i64) -> Result<(String, Vec<u8>, bool), String> {
        let payload = self.sample(at_ms)?;
        let topic = counters_topic(&self.agent).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
        Ok((topic, bytes, false))
    }
}
