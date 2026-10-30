// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! AgentState — whether a protocol hunter can see the network, and on what.
//!
//! `APK.audio/System/Protocols/{proto}/Agent/{leaf}`, retained, one bare
//! string per leaf. Twenty-three crates live under `ComProtocols/` and exactly
//! two published this — `apkaudio-ptp` and `apkaudio-AVB-Milan` — each with
//! its own private copy of the same four lines and its own idea of which words
//! are allowed. Two crates agreeing by habit is not an agreement: the third
//! crate disagrees and nothing notices, because a bare retained string has no
//! shape to violate.
//!
//! WHY THE WORDS ARE A CLOSED SET
//!
//! `state` is read as a word, not as prose: the orchestrator files it into an
//! agent map, `launch.py` prints it on the terminal, and the shell's clock
//! reader tests it against a literal. An adopter spelling it `up`, `ok` or
//! `running` would be invisible to all three and wrong in none of them. So the
//! vocabulary is four values and `AgentState` is the only way to produce one.
//!
//! `up` and `ok` ARE state words elsewhere, and are still refused here.
//! `contracts/tokens/state.json` gives both a colour — the endpoint sweep says
//! `up` of an address, the SPOG route tree says `ok` of a source — and since
//! 2026-09-06 it names all four words below as well (PLAN-485.01), so
//! `check.sh statewords` reads `AGENT_STATES` as a third closed vocabulary
//! beside the two TypeScript enums. The two files decide different questions:
//! that one decides what a word is DRAWN as, this one decides what a word is
//! ADMITTED as. A publisher that said `up` would draw the same green as
//! `listening` and still return `None` from `parse` — the colour agreeing is
//! why the parser has to be the thing that catches it.
//!
//! WHY `retain` IS RETURNED RATHER THAN LEFT TO THE CALL SITE
//!
//! Same reasoning as `agent_counters::Sampler::publication()`, in the opposite
//! direction. A count is a sample and must NOT be retained; a state is a
//! conclusion about this host that stays true while the agent is down, and
//! must be. Both are one bare `bool` at every rumqttc call site, and both
//! failures are exactly one flipped literal away — so neither is left there.
//!
//! WHY `interface` IS ITS OWN LEAF
//!
//! An operator asking *is the hunter up* is really asking *is it up on the NIC
//! I care about*, and a bare `listening` cannot answer that. Both existing
//! publishers already knew the answer and buried it in `detail` as prose —
//! AVB joins its interface names with a comma, PTP writes a sentence about
//! port counts — where nothing can read it back out. The list is published
//! separately so a consumer can match on it.
//!
//! An `AgentReport` that names no interface publishes `interface` EMPTY, which
//! is how MQTT clears a retained topic. An agent that was listening on `eth0`
//! and is now `unavailable` must not leave `eth0` standing behind it.
//!
//! WHAT THIS DELIBERATELY DOES NOT DO
//!
//! It does not change a byte of `state` or `detail` on the wire. Four
//! consumers parse those as bare strings today; lifting a convention into a
//! contract is a refactor and must not be a bus migration wearing one.
//!
//! Rust-only, like `broker_auth` and `broker_backoff`: every publisher of this
//! topic is a Rust agent, and the readers are Python and JavaScript that parse
//! a retained string rather than import a contract.

use crate::topics::{is_segment, TopicError, ROOT};

/// The leaf under `Agent/` carrying the state word.
pub const AGENT_LEAF_STATE: &str = "state";
/// The leaf carrying one line of prose for a human.
pub const AGENT_LEAF_DETAIL: &str = "detail";
/// The leaf carrying the interfaces this agent is hunting on, comma-joined.
pub const AGENT_LEAF_INTERFACE: &str = "interface";

/// Every word `state` may carry, in the order of decreasing health.
///
/// `under_test` is the fourth, added by PLAN-459.01, and it is the one word
/// here that is not about health at all: it says an operator has taken this
/// agent off the bench on purpose. It cost three consumer edits, which is what
/// the paragraph below says a fourth word costs — paid deliberately, because
/// an agent withdrawn by a person and an agent that could not open a socket are
/// exactly the two things a diagnostic must tell apart.
///
/// `stub` is deliberately absent. Three crates under `ComProtocols/` are
/// 34-line placeholders whose docs claim they "report `status = stub` on the
/// bus"; none of them has a broker connection, so the word would be a
/// vocabulary entry with no publisher — the declared-here-undeclared-on-the-bus
/// gap this contract exists on the other side of.
pub const AGENT_STATES: [&str; 4] = ["listening", "partial", "unavailable", "under_test"];

/// What an agent can say about its own ability to see the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// Every transport this agent wanted is open.
    Listening,
    /// Some opened and some did not. A partial capture is still worth running
    /// PROVIDED the bus says which half is missing — "no gPTP clocks" and
    /// "could not listen for gPTP" are otherwise identical downstream.
    Partial,
    /// Nothing opened. The agent is running and can see nothing.
    Unavailable,
    /// Withdrawn onto the workbench by an operator — `crate::self_test`. Its
    /// findings are not to be believed and its devices have been moved off the
    /// presence sentinel. NOT a fault: nothing here is broken, and folding it
    /// into `unavailable` would send somebody to fix a socket that is fine.
    UnderTest,
}

impl AgentState {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentState::Listening => "listening",
            AgentState::Partial => "partial",
            AgentState::Unavailable => "unavailable",
            AgentState::UnderTest => "under_test",
        }
    }

    /// The consumer's half. `None` for a word outside the vocabulary — which
    /// is a publisher that did not go through this contract, and is worth
    /// showing as unknown rather than folding into `unavailable`.
    pub fn parse(word: &str) -> Option<AgentState> {
        match word {
            "listening" => Some(AgentState::Listening),
            "partial" => Some(AgentState::Partial),
            "unavailable" => Some(AgentState::Unavailable),
            "under_test" => Some(AgentState::UnderTest),
            _ => None,
        }
    }

    /// Is this agent seeing anything at all? `partial` counts: it is hunting
    /// on fewer transports than it asked for, not sitting dead. `under_test`
    /// does not: it may well still be able to see, and what it sees is not to
    /// be believed while somebody is working on it.
    pub fn is_hunting(self) -> bool {
        matches!(self, AgentState::Listening | AgentState::Partial)
    }

    /// Is this agent's silence somebody's decision rather than a fault?
    ///
    /// The question every health report actually wants: an operator-withdrawn
    /// agent must not be counted among the degraded ones, or the next person
    /// to look goes and debugs a bench that is being worked on.
    pub fn is_withdrawn(self) -> bool {
        matches!(self, AgentState::UnderTest)
    }
}

/// `APK.audio/System/Protocols/{proto}/Agent/{leaf}`.
///
/// NO HOST LEVEL, ON PURPOSE, AND ONLY FOR A ONE-PROCESS-PER-HOST PUBLISHER.
/// `apkaudio-ptp` and `apkaudio-AVB-Milan` fit that shape; a vendor agent
/// watching a fleet of REMOTE devices does not (PLAN-959.01, measured at
/// three `apkaudio-netgear` processes against one inventory, all three
/// building this same string). Growing a host segment here was rejected
/// rather than reverted: it would make every single-host publisher a
/// two-shape contract for no reason of its own, and the fleet case already
/// has a host-scoped topic to answer from — `Device/{host}/state`, which
/// every crate under `Vendors/` publishes with a `reachable` field.
/// `apkaudio-netgear`'s `agent.rs` moved its liveness there; `apkaudio-trendnet`
/// has not and still shares this one the moment a second TRENDnet switch
/// joins `switches.json`.
pub fn agent_topic(proto: &str, leaf: &str) -> Result<String, TopicError> {
    if !is_segment(proto) {
        return Err(TopicError { what: "proto", value: proto.to_string() });
    }
    if !is_segment(leaf) {
        return Err(TopicError { what: "agent leaf", value: leaf.to_string() });
    }
    Ok(format!("{ROOT}/System/Protocols/{proto}/Agent/{leaf}"))
}

/// Every agent's every leaf — what a status display subscribes to.
pub fn agent_wildcard() -> String {
    format!("{ROOT}/System/Protocols/+/Agent/+")
}

/// One agent's account of itself, ready to publish.
///
/// Built rather than published so the two are separable: an agent that cannot
/// reach the broker still has a report, and a test can assert the bytes
/// without a bus.
pub struct AgentReport {
    proto: String,
    state: AgentState,
    detail: String,
    interfaces: Vec<String>,
}

impl AgentReport {
    pub fn new(proto: &str, state: AgentState, detail: &str) -> Result<Self, TopicError> {
        if !is_segment(proto) {
            return Err(TopicError { what: "proto", value: proto.to_string() });
        }
        Ok(Self {
            proto: proto.to_string(),
            state,
            detail: detail.to_string(),
            interfaces: Vec::new(),
        })
    }

    pub fn listening(proto: &str, detail: &str) -> Result<Self, TopicError> {
        Self::new(proto, AgentState::Listening, detail)
    }

    pub fn partial(proto: &str, detail: &str) -> Result<Self, TopicError> {
        Self::new(proto, AgentState::Partial, detail)
    }

    pub fn unavailable(proto: &str, detail: &str) -> Result<Self, TopicError> {
        Self::new(proto, AgentState::Unavailable, detail)
    }

    /// Name the interfaces this report is about. Blank entries are dropped
    /// rather than published as an empty name, which would read on the bus as
    /// an interface called nothing.
    pub fn on<S: AsRef<str>>(mut self, interfaces: impl IntoIterator<Item = S>) -> Self {
        self.interfaces = interfaces
            .into_iter()
            .map(|s| s.as_ref().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self
    }

    pub fn state(&self) -> AgentState {
        self.state
    }

    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }

    /// Everything three publish calls need: topic, bytes, retain — retain
    /// always `true`, for the reason in the module docs.
    ///
    /// `interface` is always in the list, EMPTY when no interface is named,
    /// because an empty retained payload is how MQTT deletes one. Omitting the
    /// entry instead would leave the last agent's NIC standing on the bus
    /// after this one went dark.
    /// Since PLAN-336, this ALSO emits the same three facts at the item's
    /// `incoming/` landing spot, so one API can ask every bare-metal item the
    /// same question. Both shapes go up together and the old three stay FIRST:
    /// four consumers index this list positionally, and a migration that
    /// reordered it would be a silent breakage rather than a loud one.
    pub fn publications(&self) -> Result<Vec<(String, Vec<u8>, bool)>, TopicError> {
        let state = self.state.as_str().as_bytes().to_vec();
        let detail = self.detail.as_bytes().to_vec();
        let interface = self.interfaces.join(",").into_bytes();

        let mut out = vec![
            (agent_topic(&self.proto, AGENT_LEAF_STATE)?, state.clone(), true),
            (agent_topic(&self.proto, AGENT_LEAF_DETAIL)?, detail.clone(), true),
            (agent_topic(&self.proto, AGENT_LEAF_INTERFACE)?, interface.clone(), true),
        ];

        for (leaf, payload) in [
            (AGENT_LEAF_STATE, state),
            (AGENT_LEAF_DETAIL, detail),
            (AGENT_LEAF_INTERFACE, interface),
        ] {
            let (topic, retain) = crate::baremetal_io::module_incoming(&self.proto, leaf)?;
            out.push((topic, payload, retain));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes the two existing publishers put on the wire, byte for byte.
    /// This contract is a refactor of a convention, not a migration of it —
    /// if this test has to change, three consumers have to change with it.
    #[test]
    fn the_wire_does_not_move() {
        let out = AgentReport::listening("ptp", "2 UDP port(s), L2 yes")
            .unwrap()
            .publications()
            .unwrap();

        assert_eq!(out[0].0, "APK.audio/System/Protocols/ptp/Agent/state");
        assert_eq!(out[0].1, b"listening".to_vec());
        assert_eq!(out[1].0, "APK.audio/System/Protocols/ptp/Agent/detail");
        assert_eq!(out[1].1, b"2 UDP port(s), L2 yes".to_vec());
    }

    /// Retain is the contract's answer, not the call site's. A state that is
    /// not retained tells a subscriber that connects after the agent settled
    /// nothing at all.
    #[test]
    fn every_leaf_is_retained() {
        let out = AgentReport::unavailable("avb", "no capturable Ethernet interfaces")
            .unwrap()
            .publications()
            .unwrap();
        assert!(out.iter().all(|(_, _, retain)| *retain));
    }

    /// An agent that named an interface last time and cannot now must clear
    /// it, or a display reads a dead NIC as the live one.
    #[test]
    fn a_report_with_no_interface_clears_the_retained_one() {
        let out = AgentReport::unavailable("avb", "no capturable Ethernet interfaces")
            .unwrap()
            .publications()
            .unwrap();

        let (topic, payload, _) = &out[2];
        assert_eq!(topic, "APK.audio/System/Protocols/avb/Agent/interface");
        assert!(payload.is_empty());
    }

    /// The old three still come first, and the new three follow — the
    /// additive half of PLAN-336. A consumer that indexes 0..2 is untouched.
    #[test]
    fn the_incoming_landing_spot_is_published_beside_the_old_shape() {
        let out = AgentReport::listening("ptp", "2 UDP port(s), L2 yes")
            .unwrap()
            .publications()
            .unwrap();

        assert_eq!(out.len(), 6, "three legacy leaves, three landing spots");
        assert!(out[..3].iter().all(|(t, _, _)| t.contains("/Agent/")));

        assert_eq!(out[3].0, "APK.audio/System/Protocols/ptp/incoming/state");
        assert_eq!(out[3].1, b"listening".to_vec());
        assert_eq!(out[4].0, "APK.audio/System/Protocols/ptp/incoming/detail");
        assert_eq!(out[5].0, "APK.audio/System/Protocols/ptp/incoming/interface");
    }

    /// Both halves must agree. Two topics carrying different answers about one
    /// agent is worse than one topic, which is the whole risk of publishing
    /// twice during a migration.
    #[test]
    fn the_two_shapes_never_disagree() {
        let out = AgentReport::partial("avb", "1 of 3 interfaces")
            .unwrap()
            .on(["enp5s0"])
            .publications()
            .unwrap();
        for i in 0..3 {
            assert_eq!(out[i].1, out[i + 3].1, "leaf {i} differs between the two shapes");
            assert_eq!(out[i].2, out[i + 3].2, "retain differs between the two shapes");
        }
    }

    #[test]
    fn interfaces_are_named_so_a_consumer_can_match_on_them() {
        let out = AgentReport::listening("avb", "enp5s0, wlp0s20f3")
            .unwrap()
            .on(["enp5s0", "wlp0s20f3"])
            .publications()
            .unwrap();
        assert_eq!(out[2].1, b"enp5s0,wlp0s20f3".to_vec());
    }

    #[test]
    fn a_blank_interface_name_is_dropped_not_published() {
        let report = AgentReport::listening("dnssd", "browsing")
            .unwrap()
            .on(["", "  ", "eth0"]);
        assert_eq!(report.interfaces(), ["eth0"]);
    }

    /// The word is the vocabulary and nothing else. A publisher outside this
    /// contract shows as unknown rather than as healthy or dead.
    #[test]
    fn the_vocabulary_is_closed_in_both_directions() {
        for word in AGENT_STATES {
            assert_eq!(AgentState::parse(word).unwrap().as_str(), word);
        }
        assert!(AgentState::parse("up").is_none());
        // The fourth word is in the set, in both directions.
        assert_eq!(AgentState::parse("under_test"), Some(AgentState::UnderTest));
        assert_eq!(AgentState::UnderTest.as_str(), "under_test");
        // ...and only in the spelling the contract chose.
        assert!(AgentState::parse("under test").is_none());
        assert!(AgentState::parse("undertest").is_none());
        assert!(AgentState::parse("ok").is_none());
        assert!(AgentState::parse("").is_none());
    }

    #[test]
    fn partial_is_hunting_and_unavailable_is_not() {
        assert!(AgentState::Listening.is_hunting());
        assert!(AgentState::Partial.is_hunting());
        assert!(!AgentState::Unavailable.is_hunting());
        assert!(!AgentState::UnderTest.is_hunting());
    }

    /// The distinction the fourth word was added for: a withdrawn agent is not
    /// a broken one, and only one of the four is withdrawn.
    #[test]
    fn only_a_withdrawn_agent_is_withdrawn() {
        assert!(AgentState::UnderTest.is_withdrawn());
        assert!(!AgentState::Unavailable.is_withdrawn());
        assert!(!AgentState::Listening.is_withdrawn());
        assert!(!AgentState::Partial.is_withdrawn());
    }

    /// A protocol name is a topic segment. A slash in it would silently graft
    /// an extra level onto the tree a display subscribes to.
    #[test]
    fn a_protocol_that_is_not_a_segment_is_refused() {
        assert!(AgentReport::listening("ptp/extra", "x").is_err());
        assert!(agent_topic("ptp", "state/deep").is_err());
        assert!(agent_topic("", "state").is_err());
    }

    #[test]
    fn the_wildcard_reaches_every_agent_and_every_leaf() {
        assert_eq!(agent_wildcard(), "APK.audio/System/Protocols/+/Agent/+");
    }
}
