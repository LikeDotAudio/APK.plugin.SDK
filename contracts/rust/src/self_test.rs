// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! SelfTest — taking one bare-metal item off the bench to prove it still works.
//!
//! Owns: the two inputs an operator presses, the words their answers may carry,
//! the ORDER the two must be pressed in, and the withdrawal that engaging one
//! performs. Must not: name an MQTT client. Same rule as every module in this
//! crate — [`Engagement`] hands back `(topic, payload, retain)` triples and the
//! caller publishes them.
//!
//! ```text
//!   outgoing/selftest        setpoint, RETAINED       engaged | off
//!   outgoing/selftest_run    one-shot, NOT retained   self | bus | device | all
//!   incoming/selftest        conclusion, RETAINED     engaged | off
//!   incoming/selftest_result conclusion, RETAINED     idle|running|pass|fail|refused
//!   incoming/selftest_report conclusion, RETAINED     the per-check table, JSON
//! ```
//!
//! WHY IT IS TWO INPUTS AND NOT ONE BUTTON
//!
//! A single "run self test" press has to decide, on the operator's behalf,
//! whether the findings it is about to disturb were being believed by anything.
//! It cannot know. So engaging and running are separated: `selftest` withdraws
//! the item — [`AgentState::UnderTest`] on the bus and every device it was
//! asserting moved off the presence sentinel — and ONLY THEN is a run accepted.
//! A run against a disengaged item is [`Verdict::Refused`] and says why, rather
//! than quietly testing something the registry is still reading.
//!
//! This is also why the two leaves are in different retain classes and neither
//! call site chooses: the withdrawal is a position an operator walked away
//! from, the run is a gesture. `baremetal_io` files them accordingly.
//!
//! WHY A RUN NAMES ITS DEPTH
//!
//! The three depths cost different things and the person at the bench is the
//! one who knows which they can afford — see [`Depth`]. An item that cannot
//! perform a depth reports its checks `skipped`, never `pass`: a check that did
//! not run must not read as one that did, which is the entire failure mode of
//! a test suite that silently does nothing.
//!
//! WHY THE REPORT IS A TABLE AND NOT A BOOLEAN
//!
//! `fail` alone sends somebody to read logs. The report names every check, its
//! outcome and one line of detail, so the bus carries the answer rather than a
//! pointer to it. The overall [`Verdict`] is DERIVED from the table by
//! [`Report::verdict`] and never set beside it — two fields that can disagree
//! about the same run is the defect `agent_state` avoids by returning `retain`.

use std::collections::BTreeMap;

use crate::agent_state::AgentState;
use crate::baremetal_io::{module_incoming, module_outgoing};
use crate::presence;
use crate::topics::TopicError;

/// The retained setpoint carrying whether this item is on the workbench.
pub const LEAF_ENGAGE: &str = "selftest";
/// The momentary gesture that runs the test. Payload names the depth.
pub const LEAF_RUN: &str = "selftest_run";
/// The retained verdict of the last run.
pub const LEAF_RESULT: &str = "selftest_result";
/// The retained per-check table of the last run, as JSON.
pub const LEAF_REPORT: &str = "selftest_report";

/// Every word `outgoing/selftest` and `incoming/selftest` may carry.
pub const ENGAGEMENTS: [&str; 2] = ["engaged", "off"];

/// Every word `incoming/selftest_result` may carry, in the order a display
/// would rank them.
pub const VERDICTS: [&str; 5] = ["pass", "running", "idle", "refused", "fail"];

/// Every word `outgoing/selftest_run` may carry.
pub const DEPTHS: [&str; 4] = ["self", "bus", "device", "all"];

// ------------------------------------------------------------------ words

/// Whether an item has been taken out of service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engaged {
    /// On the workbench. Its findings are not to be believed, and a run is
    /// accepted.
    Engaged,
    /// In service. A run is refused.
    Off,
}

impl Engaged {
    pub fn as_str(self) -> &'static str {
        match self {
            Engaged::Engaged => "engaged",
            Engaged::Off => "off",
        }
    }

    /// The consumer's half, and DELIBERATELY generous about the press.
    ///
    /// A retained setpoint is written by panels, by `mosquitto_pub` and by
    /// scripts, and the ways a human spells "on" are not a vocabulary worth
    /// policing at the cost of an engage that silently does nothing. What is
    /// NOT generous is the other direction: anything unrecognised is `Off`,
    /// so a garbled payload leaves the item IN SERVICE rather than withdrawing
    /// it on a typo.
    pub fn parse(raw: &str) -> Engaged {
        let word = raw.trim().trim_matches('"').to_ascii_lowercase();
        match word.as_str() {
            "engaged" | "engage" | "on" | "true" | "1" | "yes" | "test" => Engaged::Engaged,
            _ => Engaged::Off,
        }
    }

    pub fn is_engaged(self) -> bool {
        matches!(self, Engaged::Engaged)
    }
}

/// How far a run is asked to reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Depth {
    /// The item's own machinery, with no traffic of any kind: its config
    /// parses, its topics build, its codec round-trips a known frame, its
    /// publication comes back the shape the contract says. Deterministic, safe
    /// on a live bench, and passable with nothing plugged in.
    Own,
    /// The broker path — a token published to the item's own `incoming/` and
    /// read back. Proves the session end to end and fails when the broker is
    /// down, which is not a fault in the item. Asked for, never default.
    Bus,
    /// The real hardware. Catches the most and touches somebody else's
    /// equipment, which is only honest while the item is withdrawn.
    Device,
    /// Every depth the item implements. The ones it does not are `skipped`.
    All,
}

impl Depth {
    pub fn as_str(self) -> &'static str {
        match self {
            Depth::Own => "self",
            Depth::Bus => "bus",
            Depth::Device => "device",
            Depth::All => "all",
        }
    }

    /// `None` for a word outside the set — which is a caller that did not go
    /// through this contract, and is worth refusing rather than guessing a
    /// depth on somebody's live bench.
    pub fn parse(raw: &str) -> Option<Depth> {
        match raw.trim().trim_matches('"').to_ascii_lowercase().as_str() {
            "self" | "own" => Some(Depth::Own),
            "bus" | "broker" => Some(Depth::Bus),
            "device" | "hardware" => Some(Depth::Device),
            "all" => Some(Depth::All),
            _ => None,
        }
    }

    /// Does a run at this depth include checks declared at `other`?
    pub fn covers(self, other: Depth) -> bool {
        self == Depth::All || self == other
    }
}

/// What one named check concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    /// Declared, not run — the depth was not asked for, or the item cannot
    /// perform it here. NEVER folded into `pass`.
    Skipped,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Pass => "pass",
            Outcome::Fail => "fail",
            Outcome::Skipped => "skipped",
        }
    }
}

/// The state of the last run, as a display reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Engaged, nothing run yet.
    Idle,
    /// A run is in flight.
    Running,
    /// Every check that ran passed. At least one has to have run — an all-
    /// skipped report is not a pass, it is [`Verdict::Idle`].
    Pass,
    /// At least one check failed.
    Fail,
    /// A run arrived while the item was in service. Nothing was executed.
    Refused,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Idle => "idle",
            Verdict::Running => "running",
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
            Verdict::Refused => "refused",
        }
    }
}

// ----------------------------------------------------------------- checks

/// One named thing a run proved, or did not.
#[derive(Debug, Clone)]
pub struct Check {
    /// A stable, lower-case, underscore-joined name. It is a key a display
    /// matches on across runs, not a sentence.
    pub name: String,
    /// Which depth this check belongs to, so a `self` run does not pretend to
    /// have reached the hardware.
    pub depth: Depth,
    pub outcome: Outcome,
    /// One line for a human. On a failure this is the whole reason somebody
    /// does not have to open a log.
    pub detail: String,
    /// The problems-catalogue code for a failure (`device_unreachable`,
    /// `timeout`, …), so the report carries the way back and not only the
    /// diagnosis. `None` for a pass, a skip, or a failure nobody has named.
    pub problem: Option<String>,
}

impl Check {
    pub fn pass(name: &str, depth: Depth, detail: &str) -> Check {
        Check::new(name, depth, Outcome::Pass, detail)
    }

    pub fn fail(name: &str, depth: Depth, detail: &str) -> Check {
        Check::new(name, depth, Outcome::Fail, detail)
    }

    /// A failure with a name from `contracts/problems/problems.json`: the
    /// report then says what to do about it (`fix`), looked up by that code.
    pub fn fail_as(name: &str, depth: Depth, code: &str, detail: &str) -> Check {
        let mut c = Check::new(name, depth, Outcome::Fail, detail);
        c.problem = Some(code.to_string());
        c
    }

    pub fn skipped(name: &str, depth: Depth, detail: &str) -> Check {
        Check::new(name, depth, Outcome::Skipped, detail)
    }

    /// A check written as an assertion: `Ok` passes, `Err` fails carrying its
    /// own message. The shape most crate checks actually have, so they do not
    /// each hand-roll the branch.
    pub fn from_result<E: std::fmt::Display>(
        name: &str,
        depth: Depth,
        pass_detail: &str,
        result: Result<(), E>,
    ) -> Check {
        match result {
            Ok(()) => Check::pass(name, depth, pass_detail),
            Err(e) => Check::fail(name, depth, &e.to_string()),
        }
    }

    fn new(name: &str, depth: Depth, outcome: Outcome, detail: &str) -> Check {
        Check {
            name: name.to_string(),
            depth,
            outcome,
            detail: detail.to_string(),
            problem: None,
        }
    }
}

/// Everything one run concluded about one item.
///
/// Built rather than published, so the two are separable: a crate's self test
/// is a plain function returning one of these, callable from a `#[test]` with
/// no bus anywhere near it. That is the property that lets the ten crates which
/// link no MQTT client have a self test at all.
#[derive(Debug, Clone)]
pub struct Report {
    module: String,
    depth: Depth,
    checks: Vec<Check>,
    refused: Option<String>,
}

impl Report {
    /// A report for a run that is going ahead.
    pub fn new(module: &str, depth: Depth) -> Report {
        Report {
            module: module.to_string(),
            depth,
            checks: Vec::new(),
            refused: None,
        }
    }

    /// A run that never happened, and the reason it did not.
    ///
    /// The reason is on the bus rather than in a log because the commonest one
    /// — the item was never withdrawn — is fixed by pressing the other input,
    /// and an operator who cannot see that reads a refusal as a fault.
    pub fn refused(module: &str, depth: Depth, why: &str) -> Report {
        Report {
            module: module.to_string(),
            depth,
            checks: Vec::new(),
            refused: Some(why.to_string()),
        }
    }

    /// Record a check. A check declared at a depth this run does not cover is
    /// recorded as `skipped` WHATEVER outcome it was built with — a crate that
    /// runs a device probe during a `self` run has made a mistake the contract
    /// can catch, and folding it in silently is how it would not be.
    pub fn record(&mut self, check: Check) -> &mut Self {
        let check = if self.depth.covers(check.depth) {
            check
        } else {
            Check::skipped(&check.name, check.depth, "not in this run's depth")
        };
        self.checks.push(check);
        self
    }

    pub fn with(mut self, check: Check) -> Self {
        self.record(check);
        self
    }

    pub fn checks(&self) -> &[Check] {
        &self.checks
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn depth(&self) -> Depth {
        self.depth
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        for c in &self.checks {
            match c.outcome {
                Outcome::Pass => passed += 1,
                Outcome::Fail => failed += 1,
                Outcome::Skipped => skipped += 1,
            }
        }
        (passed, failed, skipped)
    }

    /// The one verdict, DERIVED — never stored beside the table it summarises.
    ///
    /// An all-skipped report is `idle` and not `pass`, which is the whole
    /// reason `skipped` is a word: a run that proved nothing must not read as
    /// a run that proved everything.
    pub fn verdict(&self) -> Verdict {
        if self.refused.is_some() {
            return Verdict::Refused;
        }
        let (passed, failed, _) = self.counts();
        if failed > 0 {
            Verdict::Fail
        } else if passed > 0 {
            Verdict::Pass
        } else {
            Verdict::Idle
        }
    }

    /// The table as JSON, for `incoming/selftest_report`.
    pub fn to_json(&self) -> serde_json::Value {
        let (passed, failed, skipped) = self.counts();
        let checks: Vec<serde_json::Value> = self
            .checks
            .iter()
            .map(|c| {
                let mut row = serde_json::json!({
                    "name": c.name,
                    "depth": c.depth.as_str(),
                    "outcome": c.outcome.as_str(),
                    "detail": c.detail,
                });
                if let Some(code) = &c.problem {
                    row["problem"] = serde_json::Value::String(code.clone());
                    row["fix"] = serde_json::json!(crate::problems::fix_for(code));
                }
                row
            })
            .collect();
        let mut out = serde_json::json!({
            "module": self.module,
            "depth": self.depth.as_str(),
            "verdict": self.verdict().as_str(),
            "passed": passed,
            "failed": failed,
            "skipped": skipped,
            "checks": checks,
        });
        if let Some(why) = &self.refused {
            out["refused"] = serde_json::Value::String(why.clone());
        }
        out
    }

    /// Read a report back out of the JSON [`Report::to_json`] wrote.
    ///
    /// THE READER'S HALF, AND IT EXISTS FOR A BUILD REASON RATHER THAN A BUS
    /// ONE. The twenty-eight crates that produce reports are in the
    /// `APK:BareMetal` workspace and the supervisor that collects them is in
    /// `Core` — two workspaces with SEPARATE lockfiles sharing one `target/`.
    /// Let those locks drift anywhere below this crate and it becomes two
    /// compiled units, at which point a `Report` produced on one side is a
    /// different type from a `Report` on the other and the compiler says so in
    /// a message that names neither the lockfile nor the workspace.
    ///
    /// The boundary between two independently resolved workspaces IS a
    /// serialization boundary; this is that admission written down. Crates hand
    /// the supervisor a JSON string, the supervisor parses it with its own copy
    /// of this type, and no lockfile can make the two disagree. See
    /// `.cargo/config.toml`.
    ///
    /// `depth` and the outcomes come back through the same closed vocabularies
    /// they went out through, so a payload from a stranger cannot invent one.
    pub fn from_json(raw: &str) -> Result<Report, String> {
        let value: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| format!("not JSON: {e}"))?;
        let module = value["module"].as_str().unwrap_or_default().to_string();
        let depth = value["depth"]
            .as_str()
            .and_then(Depth::parse)
            .ok_or_else(|| format!("depth {:?} is outside the vocabulary", value["depth"]))?;

        let mut report = match value.get("refused").and_then(|r| r.as_str()) {
            Some(why) => Report::refused(&module, depth, why),
            None => Report::new(&module, depth),
        };

        for check in value["checks"].as_array().unwrap_or(&Vec::new()) {
            let name = check["name"].as_str().unwrap_or_default();
            let detail = check["detail"].as_str().unwrap_or_default();
            let check_depth = check["depth"]
                .as_str()
                .and_then(Depth::parse)
                .ok_or_else(|| format!("check {name} has no readable depth"))?;
            let outcome = match check["outcome"].as_str() {
                Some("pass") => Outcome::Pass,
                Some("fail") => Outcome::Fail,
                Some("skipped") => Outcome::Skipped,
                other => return Err(format!("check {name} has outcome {other:?}")),
            };
            // Pushed rather than `record`ed: the depth filing already happened
            // on the producing side, and re-applying it here would re-skip a
            // check that legitimately ran.
            report.checks.push(Check {
                name: name.to_string(),
                depth: check_depth,
                outcome,
                detail: detail.to_string(),
                problem: check["problem"].as_str().map(str::to_string),
            });
        }
        Ok(report)
    }

    /// One line for a terminal: `ptp self — 6 passed, 1 failed, 2 skipped`.
    pub fn headline(&self) -> String {
        let (passed, failed, skipped) = self.counts();
        match &self.refused {
            Some(why) => format!("{} {} — refused: {why}", self.module, self.depth.as_str()),
            None => format!(
                "{} {} — {passed} passed, {failed} failed, {skipped} skipped",
                self.module,
                self.depth.as_str()
            ),
        }
    }

    /// The two retained leaves a finished run writes: verdict, then table.
    pub fn publications(&self) -> Result<Vec<(String, Vec<u8>, bool)>, TopicError> {
        let (result_topic, result_retain) = module_incoming(&self.module, LEAF_RESULT)?;
        let (report_topic, report_retain) = module_incoming(&self.module, LEAF_REPORT)?;
        Ok(vec![
            (
                result_topic,
                self.verdict().as_str().as_bytes().to_vec(),
                result_retain,
            ),
            (
                report_topic,
                self.to_json().to_string().into_bytes(),
                report_retain,
            ),
        ])
    }
}

/// Run a crate's self test and hand the answer across a workspace boundary.
///
/// The one function every crate's `self_test.rs` exposes to the supervisor. It
/// takes the depth as the WORD off the bus and returns the report as JSON, so
/// nothing but `&str` and `String` crosses — see [`Report::from_json`] for why
/// that matters and what it costs when it is not obeyed.
///
/// A depth outside the vocabulary is a refusal carrying the word that was sent,
/// not a guess. Guessing `device` on somebody's live bench is the reason.
pub fn run_as_json(module: &str, depth_word: &str, run: impl FnOnce(Depth) -> Report) -> String {
    let report = match Depth::parse(depth_word) {
        Some(depth) => run(depth),
        None => Report::refused(
            module,
            Depth::Own,
            &format!(
                "`{}` is not a depth — use self, bus, device or all",
                depth_word.trim()
            ),
        ),
    };
    report.to_json().to_string()
}

// ------------------------------------------------------------- the inputs

/// `APK.audio/System/Protocols/{module}/outgoing/selftest` — the setpoint a
/// panel writes to withdraw an item. Retained.
pub fn engage_topic(module: &str) -> Result<(String, bool), TopicError> {
    module_outgoing(module, LEAF_ENGAGE)
}

/// `.../outgoing/selftest_run` — the momentary press. Never retained.
pub fn run_topic(module: &str) -> Result<(String, bool), TopicError> {
    module_outgoing(module, LEAF_RUN)
}

/// Every item's self-test inputs, across the fleet — what the thing that
/// performs runs subscribes to.
///
/// It is the whole `outgoing/` half rather than a `selftest` filter, because
/// `selftest` and `selftest_run` are siblings and MQTT has no wildcard for a
/// prefix within a level. Two subscriptions would work; one that also carries
/// `enable` and `command` costs nothing and cannot drift out of step with the
/// leaf set.
pub fn inbox_wildcard() -> Result<String, TopicError> {
    crate::baremetal_io::all_modules(crate::baremetal_io::DIR_OUTGOING)
}

/// Which self-test input a topic's last level names, if either.
///
/// The parse the subscriber above needs, kept beside the leaf constants so a
/// rename moves both. `None` for `enable`, `command` and anything else in the
/// same mailbox.
pub fn input_for(topic: &str) -> Option<&'static str> {
    match topic.rsplit('/').next()? {
        LEAF_ENGAGE => Some(LEAF_ENGAGE),
        LEAF_RUN => Some(LEAF_RUN),
        _ => None,
    }
}

/// Is this press live, or a retained one replaying at reconnect?
///
/// The rule is `control.rs`'s, lifted here because it is not the PlayStation
/// bridge's rule — it is every momentary input's. A retained run would re-test
/// an item on every broker reconnect, and an item under test is an item whose
/// devices are offline, so the replay is not harmless.
pub fn is_live_run(retained: bool) -> bool {
    !retained
}

// -------------------------------------------------------------- the state

/// Which items are on the workbench, and what the last run said.
///
/// Held by whatever owns the bus session — for the in-process protocol crates
/// that is the orchestrator, which is the only reason a crate with no broker
/// client can be withdrawn at all.
#[derive(Default)]
pub struct Engagement {
    engaged: BTreeMap<String, Engaged>,
}

impl Engagement {
    pub fn new() -> Engagement {
        Engagement {
            engaged: BTreeMap::new(),
        }
    }

    pub fn is_engaged(&self, module: &str) -> bool {
        matches!(self.engaged.get(module), Some(Engaged::Engaged))
    }

    pub fn engaged_modules(&self) -> Vec<&str> {
        self.engaged
            .iter()
            .filter(|(_, e)| e.is_engaged())
            .map(|(m, _)| m.as_str())
            .collect()
    }

    /// Take the setpoint, and answer with everything that must go on the bus
    /// because of it.
    ///
    /// `None` when the position did not move: re-publishing an unchanged
    /// retained leaf is how a stable bench stops being silent, and the
    /// withdrawal it would re-perform includes moving device stamps.
    pub fn set(
        &mut self,
        module: &str,
        want: Engaged,
    ) -> Result<Option<Withdrawal>, TopicError> {
        let before = self
            .engaged
            .get(module)
            .copied()
            .unwrap_or(Engaged::Off);
        if before == want {
            return Ok(None);
        }
        self.engaged.insert(module.to_string(), want);
        Ok(Some(Withdrawal::build(module, want)?))
    }

    /// The report a run gets when the item was never withdrawn.
    ///
    /// This is the ordering the whole contract exists to hold: engage, THEN
    /// run. A caller that skips the check is testing something the registry is
    /// still reading.
    pub fn permit(&self, module: &str, depth: Depth) -> Result<(), Report> {
        if self.is_engaged(module) {
            Ok(())
        } else {
            Err(Report::refused(
                module,
                depth,
                "not under test — publish `engaged` to outgoing/selftest first",
            ))
        }
    }
}

/// What engaging or releasing one item puts on the bus.
///
/// Returned as a value rather than published so the withdrawal is testable
/// without a broker, and so the caller can append its own device retractions
/// to the same batch and send them together.
pub struct Withdrawal {
    module: String,
    want: Engaged,
}

impl Withdrawal {
    fn build(module: &str, want: Engaged) -> Result<Withdrawal, TopicError> {
        // Prove the module names a legal topic HERE, so a caller that only
        // reads `publications()` later cannot have already acted on a bad name.
        module_incoming(module, LEAF_ENGAGE)?;
        Ok(Withdrawal {
            module: module.to_string(),
            want,
        })
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn engaged(&self) -> bool {
        self.want.is_engaged()
    }

    /// The state word this item now reports.
    ///
    /// Engaging always answers [`AgentState::UnderTest`]. RELEASING answers
    /// `None` and not `listening`: this module does not know whether the
    /// agent's sockets came back, and asserting that they did is exactly the
    /// unfalsifiable claim the whole plan is about. The caller re-reports its
    /// real state.
    pub fn agent_state(&self) -> Option<AgentState> {
        match self.want {
            Engaged::Engaged => Some(AgentState::UnderTest),
            Engaged::Off => None,
        }
    }

    /// What a device this item was asserting must now say for itself.
    ///
    /// The presence sentinel `-1` means "an agent can see this NOW". While the
    /// item is on the workbench nothing can honestly say that, so the stamp
    /// moves to a real time — the same thing VISA already does on the way out,
    /// and for the same reason: the row stays, the claim does not.
    pub fn device_last_online(&self, now_secs: u64) -> String {
        match self.want {
            Engaged::Engaged => now_secs.to_string(),
            Engaged::Off => presence::ONLINE_NOW.to_string(),
        }
    }

    /// The item's own leaves. Device stamps are the caller's, because only the
    /// caller knows which devices it is currently asserting.
    pub fn publications(&self) -> Result<Vec<(String, Vec<u8>, bool)>, TopicError> {
        let word = self.want.as_str().as_bytes().to_vec();
        let (engage_topic, engage_retain) = module_incoming(&self.module, LEAF_ENGAGE)?;
        let mut out = vec![(engage_topic, word, engage_retain)];

        // Engaging resets the verdict: the last run's answer was about the
        // last time this item was on the bench, and leaving it standing beside
        // a fresh withdrawal reads as though it had just been produced.
        if self.want.is_engaged() {
            let (result_topic, result_retain) = module_incoming(&self.module, LEAF_RESULT)?;
            out.push((
                result_topic,
                Verdict::Idle.as_str().as_bytes().to_vec(),
                result_retain,
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_named_failure_carries_its_fix() {
        let r = super::Report::new("ptp", super::Depth::Device).with(super::Check::fail_as(
            "grandmaster_reply",
            super::Depth::Device,
            "device_unreachable",
            "no Announce from 10.0.0.9 in 6 s",
        ));
        let j = r.to_json();
        assert_eq!(j["checks"][0]["problem"], "device_unreachable");
        assert!(j["checks"][0]["fix"].as_array().is_some_and(|a| !a.is_empty()));
        let back = super::Report::from_json(&j.to_string()).unwrap();
        assert_eq!(back.checks()[0].problem.as_deref(), Some("device_unreachable"));
    }

    use super::*;

    // ---------------------------------------------------------- the shape

    #[test]
    fn the_two_inputs_are_the_documented_topics_and_retain_classes() {
        let (topic, retain) = engage_topic("ptp").unwrap();
        assert_eq!(topic, "APK.audio/System/Protocols/ptp/outgoing/selftest");
        assert!(retain, "withdrawal is a position an operator walked away from");

        let (topic, retain) = run_topic("ptp").unwrap();
        assert_eq!(topic, "APK.audio/System/Protocols/ptp/outgoing/selftest_run");
        assert!(!retain, "a retained run re-tests the item at every reconnect");
    }

    #[test]
    fn a_module_that_is_not_a_segment_is_refused_at_every_door() {
        assert!(engage_topic("ptp/extra").is_err());
        assert!(run_topic("").is_err());
        assert!(Report::new("has space", Depth::Own).publications().is_err());
    }

    // ------------------------------------------------------- the ordering

    /// The rule the whole contract exists to hold: engage, THEN run.
    #[test]
    fn a_run_against_a_module_in_service_is_refused_and_says_why() {
        let register = Engagement::new();
        let refusal = register.permit("dnssd", Depth::Own).unwrap_err();
        assert_eq!(refusal.verdict(), Verdict::Refused);
        assert!(refusal.headline().contains("outgoing/selftest"),
                "the refusal names the input that fixes it: {}", refusal.headline());
    }

    #[test]
    fn a_run_against_a_withdrawn_module_is_permitted() {
        let mut register = Engagement::new();
        register.set("dnssd", Engaged::Engaged).unwrap();
        assert!(register.permit("dnssd", Depth::Own).is_ok());
        assert_eq!(register.engaged_modules(), ["dnssd"]);
    }

    /// Releasing an item puts it back in service, and a run stops being
    /// allowed the moment it is.
    #[test]
    fn releasing_closes_the_door_again() {
        let mut register = Engagement::new();
        register.set("dnssd", Engaged::Engaged).unwrap();
        register.set("dnssd", Engaged::Off).unwrap();
        assert!(!register.is_engaged("dnssd"));
        assert!(register.permit("dnssd", Depth::All).is_err());
        assert!(register.engaged_modules().is_empty());
    }

    /// An unchanged setpoint puts nothing on the bus. Re-performing the
    /// withdrawal would move device stamps that had not moved.
    #[test]
    fn a_setpoint_that_did_not_move_publishes_nothing() {
        let mut register = Engagement::new();
        assert!(register.set("ptp", Engaged::Engaged).unwrap().is_some());
        assert!(register.set("ptp", Engaged::Engaged).unwrap().is_none());
        assert!(register.set("ptp", Engaged::Off).unwrap().is_some());
        // Never engaged is already `off`, so releasing it is not a move.
        assert!(register.set("sap", Engaged::Off).unwrap().is_none());
    }

    // ------------------------------------------------------ the withdrawal

    /// Engaging takes the item out of service on the one topic three consumers
    /// read, and moves its devices off the presence sentinel.
    #[test]
    fn engaging_withdraws_the_module_and_its_devices() {
        let mut register = Engagement::new();
        let w = register.set("appletv", Engaged::Engaged).unwrap().unwrap();

        assert_eq!(w.agent_state(), Some(AgentState::UnderTest));
        assert_eq!(w.device_last_online(1_788_436_572), "1788436572");
        assert!(
            !presence::is_online_now(&w.device_last_online(1_788_436_572)),
            "a withdrawn item's devices must not claim to be visible now"
        );
    }

    /// Releasing does NOT claim the sockets came back. This module cannot know
    /// that, and asserting it is the unfalsifiable claim the plan is about.
    #[test]
    fn releasing_does_not_assert_a_state_it_cannot_know() {
        let mut register = Engagement::new();
        register.set("appletv", Engaged::Engaged).unwrap();
        let w = register.set("appletv", Engaged::Off).unwrap().unwrap();
        assert_eq!(w.agent_state(), None);
        assert!(presence::is_online_now(&w.device_last_online(1_788_436_572)));
    }

    /// A fresh withdrawal clears the last run's verdict; a release leaves it,
    /// because the answer to "how did it do" outlives the session it was in.
    #[test]
    fn engaging_resets_the_verdict_and_releasing_leaves_it_standing() {
        let mut register = Engagement::new();
        let engage = register.set("sap", Engaged::Engaged).unwrap().unwrap();
        let out = engage.publications().unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "APK.audio/System/Protocols/sap/incoming/selftest");
        assert_eq!(out[0].1, b"engaged".to_vec());
        assert_eq!(out[1].0, "APK.audio/System/Protocols/sap/incoming/selftest_result");
        assert_eq!(out[1].1, b"idle".to_vec());
        assert!(out.iter().all(|(_, _, retain)| *retain));

        let release = register.set("sap", Engaged::Off).unwrap().unwrap();
        let out = release.publications().unwrap();
        assert_eq!(out.len(), 1, "the last verdict is not erased by a release");
        assert_eq!(out[0].1, b"off".to_vec());
    }

    // ---------------------------------------------------------- the words

    /// Generous about engaging, strict about the other direction: a payload
    /// nobody can read leaves the item IN SERVICE rather than withdrawing it
    /// on a typo.
    #[test]
    fn engagement_parses_what_a_person_would_actually_publish() {
        for word in ["engaged", "ENGAGED", " on ", "true", "1", "yes", "\"engaged\""] {
            assert!(Engaged::parse(word).is_engaged(), "{word:?} should engage");
        }
        for word in ["off", "false", "0", "", "engagd", "no"] {
            assert!(!Engaged::parse(word).is_engaged(), "{word:?} must not engage");
        }
    }

    /// A depth outside the set is refused rather than guessed. Guessing
    /// `device` on somebody's live bench is the reason.
    #[test]
    fn the_depth_vocabulary_is_closed() {
        for word in DEPTHS {
            assert_eq!(Depth::parse(word).unwrap().as_str(), word);
        }
        assert_eq!(Depth::parse("own"), Some(Depth::Own));
        assert_eq!(Depth::parse("HARDWARE"), Some(Depth::Device));
        assert!(Depth::parse("deep").is_none());
        assert!(Depth::parse("").is_none());
    }

    #[test]
    fn all_covers_every_depth_and_a_named_depth_covers_only_itself() {
        assert!(Depth::All.covers(Depth::Device));
        assert!(Depth::All.covers(Depth::Own));
        assert!(Depth::Own.covers(Depth::Own));
        assert!(!Depth::Own.covers(Depth::Device));
        assert!(!Depth::Bus.covers(Depth::Own));
    }

    #[test]
    fn a_retained_press_never_runs_anything() {
        assert!(is_live_run(false));
        assert!(!is_live_run(true));
    }

    // --------------------------------------------------------- the report

    /// A check declared at a depth outside the run is recorded `skipped`, not
    /// dropped and not run. A crate that probes hardware during a `self` run
    /// is a mistake the contract catches.
    #[test]
    fn a_check_outside_the_runs_depth_is_skipped_whatever_it_claimed() {
        let report = Report::new("visa", Depth::Own)
            .with(Check::pass("config_parses", Depth::Own, "6 keys"))
            .with(Check::pass("instrument_answers_idn", Depth::Device, "34401A"));

        assert_eq!(report.checks()[1].outcome, Outcome::Skipped);
        assert_eq!(report.checks()[1].detail, "not in this run's depth");
        assert_eq!(report.counts(), (1, 0, 1));
        assert_eq!(report.verdict(), Verdict::Pass);
    }

    /// The one that matters: nothing ran, so nothing passed. An all-skipped
    /// report reading `pass` is a test suite that silently does nothing.
    #[test]
    fn an_all_skipped_report_is_idle_and_never_pass() {
        let report = Report::new("osc", Depth::Own)
            .with(Check::skipped("socket_binds", Depth::Bus, "no broker configured"));
        assert_eq!(report.counts(), (0, 0, 1));
        assert_eq!(report.verdict(), Verdict::Idle);
    }

    #[test]
    fn one_failure_fails_the_run_however_many_passed() {
        let report = Report::new("nmos", Depth::All)
            .with(Check::pass("topics_build", Depth::Own, ""))
            .with(Check::pass("codec_round_trips", Depth::Own, ""))
            .with(Check::fail("registry_answers", Depth::Device, "connection refused"));
        assert_eq!(report.counts(), (2, 1, 0));
        assert_eq!(report.verdict(), Verdict::Fail);
    }

    #[test]
    fn a_result_carries_its_own_message_through_from_result() {
        let ok: Result<(), String> = Ok(());
        let bad: Result<(), String> = Err("port 319 in use".to_string());
        assert_eq!(Check::from_result("bind", Depth::Own, "bound", ok).outcome, Outcome::Pass);
        let failed = Check::from_result("bind", Depth::Own, "bound", bad);
        assert_eq!(failed.outcome, Outcome::Fail);
        assert_eq!(failed.detail, "port 319 in use");
    }

    /// The verdict is derived from the table and published beside it. Two
    /// fields that can disagree about one run is the defect this avoids.
    #[test]
    fn the_published_verdict_is_the_tables_own_answer() {
        let report = Report::new("ptp", Depth::All)
            .with(Check::pass("topics_build", Depth::Own, "3 topics"))
            .with(Check::fail("l2_capture", Depth::Device, "no cap_net_raw"));

        let out = report.publications().unwrap();
        assert_eq!(out[0].0, "APK.audio/System/Protocols/ptp/incoming/selftest_result");
        assert_eq!(out[0].1, b"fail".to_vec());
        assert!(out.iter().all(|(_, _, retain)| *retain), "a verdict outlives its run");

        let table: serde_json::Value = serde_json::from_slice(&out[1].1).unwrap();
        assert_eq!(table["verdict"], "fail");
        assert_eq!(table["module"], "ptp");
        assert_eq!(table["depth"], "all");
        assert_eq!(table["passed"], 1);
        assert_eq!(table["failed"], 1);
        assert_eq!(table["checks"][1]["name"], "l2_capture");
        assert_eq!(table["checks"][1]["detail"], "no cap_net_raw");
    }

    /// A refusal reaches the bus as a refusal, carrying the reason, rather
    /// than as a failure somebody would go and debug.
    #[test]
    fn a_refusal_is_published_as_itself() {
        let report = Report::refused("mdns", Depth::All, "not under test");
        let out = report.publications().unwrap();
        assert_eq!(out[0].1, b"refused".to_vec());
        let table: serde_json::Value = serde_json::from_slice(&out[1].1).unwrap();
        assert_eq!(table["refused"], "not under test");
        assert_eq!(table["checks"].as_array().unwrap().len(), 0);
    }

    /// The filter is one a broker will actually accept. `selftest#` is not a
    /// legal wildcard — MQTT has none for a prefix within a level — and a
    /// subscription the broker rejects is an agent that hears no presses.
    /// The boundary crossing, both ways. A report that survives JSON is a
    /// report two independently locked workspaces can agree about.
    #[test]
    fn a_report_survives_the_workspace_boundary() {
        let sent = Report::new("ptp", Depth::All)
            .with(Check::pass("topics_build", Depth::Own, "3 topics"))
            .with(Check::fail("l2_capture", Depth::Device, "no cap_net_raw"))
            .with(Check::skipped("broker", Depth::Bus, "no broker configured"));

        let back = Report::from_json(&sent.to_json().to_string()).unwrap();

        assert_eq!(back.module(), "ptp");
        assert_eq!(back.depth(), Depth::All);
        assert_eq!(back.counts(), sent.counts());
        assert_eq!(back.verdict(), sent.verdict());
        assert_eq!(back.headline(), sent.headline());
        assert_eq!(back.checks()[1].name, "l2_capture");
        assert_eq!(back.checks()[1].detail, "no cap_net_raw");
        assert_eq!(back.checks()[1].depth, Depth::Device);
        assert_eq!(back.checks()[2].outcome, Outcome::Skipped);
    }

    /// A skipped check does NOT come back re-filed. `record` would skip it a
    /// second time; the depth decision belongs to the side that ran it.
    #[test]
    fn a_check_that_ran_out_of_depth_is_not_re_skipped_on_arrival() {
        // A `device` run's device check passed. Reading it back at that depth
        // must leave it passing.
        let sent = Report::new("appletv", Depth::Device)
            .with(Check::pass("device_answers", Depth::Device, "200 OK"));
        let back = Report::from_json(&sent.to_json().to_string()).unwrap();
        assert_eq!(back.checks()[0].outcome, Outcome::Pass);
        assert_eq!(back.verdict(), Verdict::Pass);
    }

    #[test]
    fn a_refusal_survives_the_boundary_as_a_refusal() {
        let sent = Report::refused("mdns", Depth::All, "not under test");
        let back = Report::from_json(&sent.to_json().to_string()).unwrap();
        assert_eq!(back.verdict(), Verdict::Refused);
        assert!(back.headline().contains("not under test"));
    }

    /// Anything that is not a report this crate wrote is an error, not an
    /// empty report. An empty report reads as `idle`, which is a claim.
    #[test]
    fn a_payload_this_crate_did_not_write_is_refused() {
        assert!(Report::from_json("not json").is_err());
        assert!(Report::from_json(r#"{"module":"ptp"}"#).is_err(), "no depth");
        assert!(
            Report::from_json(r#"{"module":"ptp","depth":"deep","checks":[]}"#).is_err(),
            "a depth outside the vocabulary"
        );
        assert!(
            Report::from_json(
                r#"{"module":"ptp","depth":"self","checks":[{"name":"x","depth":"self","outcome":"maybe","detail":""}]}"#
            )
            .is_err(),
            "an outcome outside the vocabulary"
        );
    }

    /// The producing side's helper refuses an unknown depth rather than
    /// guessing one, and says which word it was sent.
    #[test]
    fn run_as_json_refuses_a_depth_it_does_not_know() {
        let raw = run_as_json("sap", "deep", |_| unreachable!("the run must not happen"));
        let back = Report::from_json(&raw).unwrap();
        assert_eq!(back.verdict(), Verdict::Refused);
        assert!(back.headline().contains("deep"), "{}", back.headline());

        let raw = run_as_json("sap", "all", |depth| {
            Report::new("sap", depth).with(Check::pass("ran", Depth::Own, ""))
        });
        assert_eq!(Report::from_json(&raw).unwrap().verdict(), Verdict::Pass);
    }

    #[test]
    fn the_inbox_is_a_filter_a_broker_accepts() {
        let filter = inbox_wildcard().unwrap();
        assert_eq!(filter, "APK.audio/System/Protocols/+/outgoing/#");
        assert!(!filter.contains("selftest#"));
    }

    /// And the subscriber can tell the two presses apart from the topic, and
    /// tell both from the neighbours sharing their mailbox.
    #[test]
    fn the_two_inputs_are_told_apart_from_their_neighbours() {
        assert_eq!(
            input_for("APK.audio/System/Protocols/ptp/outgoing/selftest"),
            Some(LEAF_ENGAGE)
        );
        assert_eq!(
            input_for("APK.audio/System/Protocols/ptp/outgoing/selftest_run"),
            Some(LEAF_RUN)
        );
        assert_eq!(input_for("APK.audio/System/Protocols/ptp/outgoing/enable"), None);
        assert_eq!(input_for("APK.audio/System/Protocols/ptp/outgoing/command"), None);
    }
}
