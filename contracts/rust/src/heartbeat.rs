// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! AgentHeartbeat wrapper — re-exports the generated types (gen/ is typify
//! output, never hand-edited) and adds the behavior half of the contract:
//! the H2 LWT helper. Twin of `contracts/src/heartbeat.ts`.

use serde::{Deserialize, Serialize};

pub use crate::gen::agent_heartbeat::{
    AgentHeartbeat, AgentHeartbeatAgent, AgentHeartbeatLastBeat, AgentHeartbeatStartedAt,
    AgentHeartbeatStatus,
};

type ConvErr = crate::gen::agent_heartbeat::error::ConversionError;

/// H3 — how often a live agent republishes, and how old a `lastBeat` may be
/// before a reader stops believing `status: "online"`.
///
/// These two numbers are one decision and they are declared together because
/// they were not: the shell's ledger carried `BUS_AGENT_STALE_MS = 65000` on a
/// comment claiming a 20 s beat, `orchestrator::beat::BEAT_INTERVAL` was 10 s,
/// and nothing on either side could see the other. An agent author sizing a
/// beat had nothing to size it against.
///
/// The invariant is `STALE_AFTER_MS > BEAT_INTERVAL_MS`, with enough slack that
/// a missed beat on a busy box does not blink an agent out — six beats here.
pub const BEAT_INTERVAL_MS: u64 = 10_000;
pub const STALE_AFTER_MS: u64 = 65_000;

/// Is a beat of this age still worth believing?
///
/// The reason this exists as a function rather than a comparison somebody
/// writes at each call site: a RETAINED liveness topic answers every new
/// subscriber with whatever was published last, and a `status: "online"` from a
/// process that died yesterday is the most misleading value an MQTT bus can
/// carry. `status` alone is never the answer — the age of the beat is the other
/// half, and a reader that consults one without the other is the defect.
pub fn beat_is_fresh(age_ms: u64) -> bool {
    age_ms <= STALE_AFTER_MS
}

/// H2 — the exact Last Will every connecting agent (and browser) registers.
/// `startedAt` AND `lastBeat` are both the CONNECT time, and a delivered LWT
/// therefore means exactly one thing: this agent registered a will at
/// `startedAt` and its connection to the broker is now gone. THERE IS NO UPPER
/// BOUND ON WHEN IT DIED, and `last_beat` on a Will must never be read as one.
///
/// THE SENTENCE THAT USED TO STAND HERE WAS FALSE, and it was false in the same
/// words in the TypeScript twin and quoted in a third place. It read: a
/// delivered LWT means "died no later than one keepalive after `last_beat`". A
/// will is composed once, at CONNECT, and fired arbitrarily later — so an agent that
/// connects at 09:00 and dies at 15:00 delivers a will stamped 09:00, and that
/// sentence claims it died by 09:00:30. It was true only for an agent that died
/// within one keepalive of connecting, which is the one case where the frozen
/// stamp happens to still be current. PLAN-1096.01.
///
/// WHAT DOES BOUND THE DEATH IS H3, NOT THIS DOCUMENT. The broker fires the
/// will within one keepalive of the CONNECTION going quiet, so "recently
/// disconnected" is knowable — from the arrival of the will, which is a fact
/// about the subscriber's own clock and not a field in the payload. A reader
/// wanting a time of death reads the age of the last live beat it saw, through
/// [`beat_is_fresh`] against [`STALE_AFTER_MS`]; a reader with no such beat (a
/// fresh subscriber meeting only the retained will) knows the agent is gone and
/// does not know when. `beat_is_fresh` also happens to protect the naive reader
/// here: a will's frozen `last_beat` goes stale within 65 s of the connect time,
/// so a consumer applying H3 stops believing the document almost immediately
/// whatever `status` says — and on a will `status` is `Offline` in any case.
///
/// `last_beat` IS STILL SET TO THE CONNECT TIME AND THE WIRE IS NOT CHANGING.
/// The alternative considered was a deliberate sentinel, which would make the
/// misreading impossible rather than merely documented; it was declined. The
/// field is required and typed as a timestamp, so a sentinel has to be a real
/// one, and three implementations plus three readers — `MqttProvider.jsx`, the
/// shell's ledger and `agent_supervision.py` — parse it. Trading a documented
/// frozen stamp for an undocumented fake one, across five files, to fix a field
/// the corrected paragraph above now tells the reader not to use, is not the
/// better half of that trade. If it is ever revisited it is a wire change and
/// wants its own ruling, not a drive-by.
///
/// `version` IS IN THE WILL, AND THE ARGUMENT AGAINST IT LOSES ON ONE FACT.
/// The objection was that a will is composed at CONNECT and delivered
/// arbitrarily later, so a live-looking field beside a frozen `lastBeat` invites
/// a reader to trust the wrong half. That is fatal for anything MEASURED, and
/// `version` is not measured: it is the rev this process was started from, it
/// cannot change while the connection that registered the will is open, and it
/// is therefore still true at the moment the broker fires it. Nothing else in
/// that document is.
///
/// WHY THE WILL AND NOT ONLY THE LIVE BEAT. The will is the last thing a node
/// ever says and the only thing it says after it is gone — which is exactly
/// when "what was it running?" gets asked — and it OVERWRITES the retained live
/// beat that could have answered. A node that died a minute after a rebuild and
/// one dead since last week published identical wills.
///
/// EVERY CALLER IN THIS TREE BUILDS ITS ONLINE BEAT BY MUTATING WHAT THIS
/// RETURNS, so passing the rev here is also what stops the two documents
/// disagreeing: `beat.version = Some(...)` after the will was serialised is how
/// three crates came to publish a versioned beat and an unversioned will
/// (PLAN-856.03; PLAN-856.01 put the same field on the live beat).
pub fn heartbeat_lwt(
    agent: &str,
    connected_at_iso: &str,
    partition: Option<&str>,
    version: Option<&str>,
) -> Result<(String, AgentHeartbeat), String> {
    let topic = crate::topics::agents(agent).map_err(|e| e.to_string())?;
    let payload = AgentHeartbeat {
        agent: agent.parse().map_err(|e: ConvErr| e.to_string())?,
        host: None,
        last_beat: connected_at_iso.parse().map_err(|e: ConvErr| e.to_string())?,
        partition: partition.map(String::from),
        pid: None,
        schema_version: 1.0,
        started_at: connected_at_iso.parse().map_err(|e: ConvErr| e.to_string())?,
        status: AgentHeartbeatStatus::Offline,
        version: version.map(String::from),
    };
    Ok((topic, payload))
}

/// v0 (the wild west): today's browser Failover payload
/// (MqttProvider.jsx:85-93) — typed so tooling can NAME what it finds on the
/// bus. Never emitted by new code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyFailoverHeartbeatV0 {
    pub guid: String,
    pub full_id: String,
    pub partition: String,
    pub active: bool,
    pub start_ts: f64,
    pub timestamp: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A staleness window at or below the beat interval marks every healthy
    /// agent stale between its own beats. Asserted rather than commented,
    /// because the pair is only useful if changing one forces the other.
    #[test]
    fn stale_window_outlasts_the_beat() {
        assert!(STALE_AFTER_MS > BEAT_INTERVAL_MS);
        assert!(beat_is_fresh(BEAT_INTERVAL_MS));
        assert!(!beat_is_fresh(STALE_AFTER_MS + 1));
    }

    /// The will carries the rev, and omits it rather than inventing one.
    ///
    /// The `None` arm is the half worth asserting: a will that carried
    /// `Some("")` or `Some("unknown")` would answer "what was it running?" with
    /// a string, and a reader cannot tell an invented answer from a real one.
    #[test]
    fn the_will_can_name_the_code_that_died() {
        let (_, will) = heartbeat_lwt("orchestrator", "2026-09-10T12:00:00.000Z", None, Some("1.2.3"))
            .expect("the contract refused its own will");
        assert_eq!(will.status, AgentHeartbeatStatus::Offline);
        assert_eq!(will.version.as_deref(), Some("1.2.3"));

        let (_, anon) = heartbeat_lwt("orchestrator", "2026-09-10T12:00:00.000Z", None, None)
            .expect("the contract refused its own will");
        assert_eq!(anon.version, None);
    }
}
