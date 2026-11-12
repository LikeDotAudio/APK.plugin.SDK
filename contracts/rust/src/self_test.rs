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
mod tests;
