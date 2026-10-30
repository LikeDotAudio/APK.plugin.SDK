// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The two landing spots every bare-metal item owns: what it REPORTS, and what
//! it is TOLD.
//!
//! ```text
//!   APK.audio/System/Protocols/{module}/incoming/{leaf}   module -> bus
//!   APK.audio/System/Protocols/{module}/outgoing/{leaf}   bus -> module
//!   APK.audio/System/BareMetal/{target}/{path}/incoming/{leaf}
//!   APK.audio/System/BareMetal/{target}/{path}/outgoing/{leaf}
//! ```
//!
//! DIRECTION IS FROM THE BUS LOOKING AT THE ITEM, and it is the one thing here
//! that cannot be inferred from a topic by reading it. `incoming` is the item
//! talking about itself -- state, telemetry, what it found. `outgoing` is a
//! controller talking to the item -- commands and setpoints. An agent
//! SUBSCRIBES to its own `outgoing` and PUBLISHES to its own `incoming`, which
//! reads backwards until you hold the bus still and let the items move.
//!
//! WHY THE RETAIN FLAG IS RETURNED AND NEVER PASSED IN
//!
//! Same rule as `agent_state` and `agent_counters`, now applied to a family
//! rather than a single report. Every leaf here is one of four retain classes
//! and the class is a property of the LEAF, not of the call site:
//!
//!   conclusion  retained     `state`, `detail`, `interface` -- true while the
//!                            agent is down, which is when they matter most.
//!   sample      not retained `telemetry`, `discovered`, `result` -- a retained
//!                            sample is a stale reading presented as a live one.
//!   one-shot    not retained `command`, `job` -- a retained command re-fires on
//!                            every reconnect, which is how a mute became permanent.
//!   setpoint    retained     `enable` -- sticky on purpose; it is a position,
//!                            not a gesture.
//!
//! A caller that could pass its own `bool` would eventually pass the wrong one,
//! and both failures are invisible until the bus has been wrong for a week.
//!
//! WHY THIS DOES NOT REPLACE `System/Protocols/{proto}/Agent/{leaf}`
//!
//! Four consumers parse those leaves today. This family is published BESIDE
//! them, not instead of them; retiring the old shape is a separate act with its
//! own migration, and doing both at once would take the bus down to save a
//! commit.
use crate::topics::TopicError;

/// The item is reporting about itself. Item -> bus.
pub const DIR_INCOMING: &str = "incoming";

/// A controller is addressing the item. Bus -> item.
pub const DIR_OUTGOING: &str = "outgoing";

/// A CLOSED set. A third direction is a design change, not a spelling.
pub const DIRECTIONS: [&str; 2] = [DIR_INCOMING, DIR_OUTGOING];

/// Incoming leaves that are CONCLUSIONS about the item, and are retained.
///
/// The three `selftest*` leaves are conclusions in exactly the same sense as
/// `state`: an item withdrawn onto the workbench is still withdrawn when the
/// operator comes back tomorrow, and the verdict of the last run is the thing
/// they came back to read. See `crate::self_test` for the vocabulary each one
/// carries.
/// `api` IS A CONCLUSION IN THE SAME SENSE AS `state`, which is the only
/// reason it is retained and lives here rather than beside `telemetry`: it is
/// the item's answer to "describe yourself completely" — its token, its config
/// as actually loaded, its broker, its agents, its liveness, every topic it
/// owns — and that answer is still true for a reader who arrives tomorrow.
/// A reader gets it with NO ROUND TRIP because it is retained; `outgoing/api_get`
/// only asks for it to be taken again. Live counters stay in `telemetry`, which
/// is a sample and must not be retained.
pub const INCOMING_RETAINED: [&str; 7] = [
    "state",
    "detail",
    "interface",
    "selftest",
    "selftest_result",
    "selftest_report",
    "api",
];

/// Incoming leaves that are SAMPLES, and are never retained.
///
/// `result` is a worker's answer to one `outgoing/job` (PLAN-3423.01). It is a
/// sample in the same sense as `telemetry`: it answers the job that carried its
/// `job_id` and nothing else, and a retained one would hand the next reader an
/// answer to a question it never asked.
pub const INCOMING_SAMPLED: [&str; 3] = ["telemetry", "discovered", "result"];

/// Outgoing leaves that are GESTURES, and are never retained.
///
/// `selftest_run` is a gesture for the reason the PlayStation bridge already
/// gives about `Wake`: a retained press replays at every reconnect, and an
/// agent restart must not re-run a test any more than it must wake a console.
/// `api_get` is a gesture for the same reason `selftest_run` is: it asks the
/// item to take its self-description again NOW. Retained, it would re-fire on
/// every reconnect — harmless in isolation, and still a publish storm across
/// twenty-six plugins every time the broker restarts.
/// `job` asks a worker (`PODS/POD.workers`, PLAN-3423.01) to run its tool once
/// on the payload it carries. It is a gesture for the reason `command` is: a
/// retained job would run again on every reconnect of every worker.
pub const OUTGOING_ONESHOT: [&str; 4] = ["command", "selftest_run", "api_get", "job"];

/// Outgoing leaves that are POSITIONS, and are retained.
///
/// `selftest` is the position "this item is on the workbench". It is sticky on
/// purpose and for the same reason `enable` is: an operator who engages it and
/// walks away has taken the item out of service until somebody says otherwise,
/// and a restart in between must not quietly put it back.
pub const OUTGOING_SETPOINT: [&str; 2] = ["enable", "selftest"];

fn error(what: &'static str, value: &str) -> TopicError {
    TopicError { what, value: value.to_string() }
}

/// Whether `leaf` is a legal leaf in `direction`, and whether it retains.
///
/// One lookup rather than two, because a leaf that is legal in the wrong
/// direction is the mistake this prevents: `command` under `incoming` would be
/// an item issuing orders to itself, and nothing downstream would question it.
pub fn retains(direction: &str, leaf: &str) -> Result<bool, TopicError> {
    match direction {
        DIR_INCOMING if INCOMING_RETAINED.contains(&leaf) => Ok(true),
        DIR_INCOMING if INCOMING_SAMPLED.contains(&leaf) => Ok(false),
        DIR_OUTGOING if OUTGOING_SETPOINT.contains(&leaf) => Ok(true),
        DIR_OUTGOING if OUTGOING_ONESHOT.contains(&leaf) => Ok(false),
        DIR_INCOMING | DIR_OUTGOING => Err(error("baremetal leaf", leaf)),
        _ => Err(error("baremetal direction", direction)),
    }
}

/// Every leaf legal in `direction`, retained first. For a landing-spot census.
pub fn leaves(direction: &str) -> Result<Vec<&'static str>, TopicError> {
    match direction {
        DIR_INCOMING => Ok(INCOMING_RETAINED.iter().chain(INCOMING_SAMPLED.iter()).copied().collect()),
        DIR_OUTGOING => Ok(OUTGOING_SETPOINT.iter().chain(OUTGOING_ONESHOT.iter()).copied().collect()),
        other => Err(error("baremetal direction", other)),
    }
}

fn check(direction: &str, leaf: &str) -> Result<(), TopicError> {
    retains(direction, leaf).map(|_| ())
}

// ---------------------------------------------------------------- builders

use crate::topics::{ROOT, is_address, is_baremetal_target, is_segment};

/// `APK.audio/System/Protocols/{module}/{direction}/{leaf}`.
///
/// A MODULE is a protocol or discovery crate — the thing `ComProtocols/` holds
/// — and its token is the same one `System/Protocols/{proto}` already uses, so
/// an item has ONE identity on the bus and not a second one for its mailbox.
fn module_topic(module: &str, direction: &str, leaf: &str) -> Result<String, TopicError> {
    if !is_segment(module) {
        return Err(error("module", module));
    }
    check(direction, leaf)?;
    Ok(format!("{ROOT}/System/Protocols/{module}/{direction}/{leaf}"))
}

/// The leaf a module publishes its complete self-description on.
pub const LEAF_API: &str = "api";

/// The leaf a controller presses to ask for that description again.
pub const LEAF_API_GET: &str = "api_get";

/// The leaf a controller publishes a job on, for a worker to run once.
pub const LEAF_JOB: &str = "job";

/// The leaf a worker publishes the answer to one job on.
pub const LEAF_RESULT: &str = "result";

/// `APK.audio/System/Protocols/{module}/outgoing/job` — not retained.
pub fn module_job(module: &str) -> Result<(String, bool), TopicError> {
    module_outgoing(module, LEAF_JOB)
}

/// `APK.audio/System/Protocols/{module}/incoming/result` — not retained.
pub fn module_result(module: &str) -> Result<(String, bool), TopicError> {
    module_incoming(module, LEAF_RESULT)
}

/// `APK.audio/System/Protocols/{module}/incoming/api` — retained, always there.
pub fn module_api(module: &str) -> Result<(String, bool), TopicError> {
    module_incoming(module, LEAF_API)
}

/// `APK.audio/System/Protocols/{module}/outgoing/api_get` — the refresh press.
pub fn module_api_get(module: &str) -> Result<(String, bool), TopicError> {
    module_outgoing(module, LEAF_API_GET)
}

/// Every module's self-description at once — how a gateway reads the fleet.
pub fn all_module_api() -> String {
    format!("{ROOT}/System/Protocols/+/{DIR_INCOMING}/{LEAF_API}")
}

/// What the module reports about itself. Retain flag comes back with it.
pub fn module_incoming(module: &str, leaf: &str) -> Result<(String, bool), TopicError> {
    Ok((module_topic(module, DIR_INCOMING, leaf)?, retains(DIR_INCOMING, leaf)?))
}

/// What a controller asks of the module. Retain flag comes back with it.
pub fn module_outgoing(module: &str, leaf: &str) -> Result<(String, bool), TopicError> {
    Ok((module_topic(module, DIR_OUTGOING, leaf)?, retains(DIR_OUTGOING, leaf)?))
}

/// `APK.audio/System/BareMetal/{target}/{path…}/{direction}/{leaf}`.
///
/// The tail is address-class, not segment-class, for the reason
/// `topics::baremetal_head` gives: a `{host}` may be an FQDN.
fn device_topic(target: &str, path: &[&str], direction: &str, leaf: &str) -> Result<String, TopicError> {
    if !is_baremetal_target(target) {
        return Err(error("bareMetal target", target));
    }
    check(direction, leaf)?;
    let mut topic = format!("{ROOT}/System/BareMetal/{target}");
    for s in path {
        if !is_address(s) {
            return Err(error("path segment", s));
        }
        // A path may not spell a direction: it would put a second mailbox
        // inside the first and the parse could not tell which one it read.
        if DIRECTIONS.contains(s) {
            return Err(error("path segment", s));
        }
        topic.push('/');
        topic.push_str(s);
    }
    Ok(format!("{topic}/{direction}/{leaf}"))
}

/// What the device reports about itself. Retain flag comes back with it.
pub fn device_incoming(target: &str, path: &[&str], leaf: &str) -> Result<(String, bool), TopicError> {
    Ok((device_topic(target, path, DIR_INCOMING, leaf)?, retains(DIR_INCOMING, leaf)?))
}

/// What a controller asks of the device. Retain flag comes back with it.
pub fn device_outgoing(target: &str, path: &[&str], leaf: &str) -> Result<(String, bool), TopicError> {
    Ok((device_topic(target, path, DIR_OUTGOING, leaf)?, retains(DIR_OUTGOING, leaf)?))
}

/// Everything published under one module, both directions.
pub fn module_wildcard(module: &str) -> Result<String, TopicError> {
    if !is_segment(module) {
        return Err(error("module", module));
    }
    Ok(format!("{ROOT}/System/Protocols/{module}/+/#"))
}

/// One direction across every module — how a controller watches the fleet.
pub fn all_modules(direction: &str) -> Result<String, TopicError> {
    if !DIRECTIONS.contains(&direction) {
        return Err(error("baremetal direction", direction));
    }
    Ok(format!("{ROOT}/System/Protocols/+/{direction}/#"))
}

/// The mailbox an agent subscribes to: everything addressed TO this module.
pub fn module_inbox(module: &str) -> Result<String, TopicError> {
    if !is_segment(module) {
        return Err(error("module", module));
    }
    Ok(format!("{ROOT}/System/Protocols/{module}/{DIR_OUTGOING}/#"))
}

/// Every landing spot a module owns — the census an API answers from.
///
/// Returned rather than derived at each call site so that "which topics does
/// this item have" has exactly one answer, and a new leaf appears everywhere
/// at once instead of in whichever readers remembered to add it.
pub fn module_landing_spots(module: &str) -> Result<Vec<(String, &'static str, bool)>, TopicError> {
    let mut spots = Vec::new();
    for direction in DIRECTIONS {
        for leaf in leaves(direction)? {
            spots.push((module_topic(module, direction, leaf)?, leaf, retains(direction, leaf)?));
        }
    }
    Ok(spots)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape, spelled out, so a rename of a segment is a failing test.
    #[test]
    fn module_topics_are_the_documented_shape() {
        let (topic, retain) = module_incoming("ptp", "state").unwrap();
        assert_eq!(topic, "APK.audio/System/Protocols/ptp/incoming/state");
        assert!(retain, "a state is a conclusion and survives the agent");

        let (topic, retain) = module_outgoing("ptp", "command").unwrap();
        assert_eq!(topic, "APK.audio/System/Protocols/ptp/outgoing/command");
        assert!(!retain, "a retained command re-fires on every reconnect");
    }

    #[test]
    fn device_topics_carry_their_path() {
        let (topic, _) = device_incoming("PI", &["bench1"], "telemetry").unwrap();
        assert_eq!(topic, "APK.audio/System/BareMetal/PI/bench1/incoming/telemetry");
    }

    /// The four retain classes, asserted rather than described.
    #[test]
    fn retain_class_follows_the_leaf() {
        assert!(retains(DIR_INCOMING, "state").unwrap());
        assert!(retains(DIR_INCOMING, "interface").unwrap());
        assert!(!retains(DIR_INCOMING, "telemetry").unwrap(), "a sample is never retained");
        assert!(!retains(DIR_OUTGOING, "command").unwrap(), "a gesture is never retained");
        assert!(retains(DIR_OUTGOING, "enable").unwrap(), "a setpoint is a position");
        // The self-test pair, classed the same way and for the same reasons.
        assert!(retains(DIR_OUTGOING, "selftest").unwrap(), "withdrawal is a position");
        assert!(!retains(DIR_OUTGOING, "selftest_run").unwrap(), "a run is a gesture");
        assert!(retains(DIR_INCOMING, "selftest_result").unwrap(), "a verdict outlives the run");
    }

    /// A leaf legal in one direction must not be legal in the other.
    ///
    /// This is the mistake the single lookup exists to prevent: `command` under
    /// `incoming` would be an item issuing orders to itself.
    #[test]
    fn leaves_do_not_cross_directions() {
        assert!(retains(DIR_INCOMING, "command").is_err());
        assert!(retains(DIR_OUTGOING, "state").is_err());
        assert!(module_incoming("ptp", "command").is_err());
        assert!(module_outgoing("ptp", "telemetry").is_err());
    }

    #[test]
    fn unknown_direction_and_leaf_are_refused() {
        assert!(retains("sideways", "state").is_err());
        assert!(retains(DIR_INCOMING, "vibes").is_err());
        assert!(leaves("sideways").is_err());
    }

    /// A path segment may not spell a direction, or the parse cannot tell
    /// which mailbox it is reading.
    #[test]
    fn a_path_may_not_spell_a_direction() {
        assert!(device_incoming("PI", &["incoming"], "state").is_err());
        assert!(device_incoming("PI", &["outgoing"], "state").is_err());
    }

    #[test]
    fn bad_module_and_target_are_refused() {
        assert!(module_incoming("has space", "state").is_err());
        assert!(module_incoming("", "state").is_err());
        assert!(device_incoming("NotATarget", &[], "state").is_err());
    }

    /// The agent's own mailbox is the outgoing half and nothing else. An agent
    /// that subscribed to its own incoming half would hear its own voice.
    #[test]
    fn an_agent_subscribes_only_to_what_is_addressed_to_it() {
        let inbox = module_inbox("ptp").unwrap();
        assert_eq!(inbox, "APK.audio/System/Protocols/ptp/outgoing/#");
        assert!(!inbox.contains(DIR_INCOMING));
    }

    /// Every item has the same landing spots, and each names its retain class.
    #[test]
    fn landing_spots_cover_both_directions() {
        let spots = module_landing_spots("aes70").unwrap();
        // COUNTED FROM THE TABLES, NOT TYPED. This was the literal `12` and a
        // sentence naming the four groups, so ADDING A LEAF broke a test that
        // was not about the new leaf at all — and the fix was to retype the
        // number and rewrite the sentence, which is a test asking to be edited
        // rather than one asking a question.
        let expected = INCOMING_RETAINED.len()
            + INCOMING_SAMPLED.len()
            + OUTGOING_SETPOINT.len()
            + OUTGOING_ONESHOT.len();
        assert_eq!(spots.len(), expected, "every leaf in both directions, once each");
        assert!(spots.iter().any(|(t, _, r)| t.ends_with("/incoming/state") && *r));
        assert!(spots.iter().any(|(t, _, r)| t.ends_with("/outgoing/command") && !*r));
        // The two the plugin API is made of, and their retain classes: a
        // description that did not retain would be unreadable until the next
        // press, and a press that DID retain would re-fire on every reconnect.
        assert!(spots.iter().any(|(t, _, r)| t.ends_with("/incoming/api") && *r));
        assert!(spots.iter().any(|(t, _, r)| t.ends_with("/outgoing/api_get") && !*r));
        // Every module gets the identical set, which is what makes an API
        // able to ask the same question of all of them.
        let other = module_landing_spots("ptp").unwrap();
        assert_eq!(spots.len(), other.len());
    }

    /// The worker pair (PLAN-3423.01): a job is a gesture, a result is a
    /// sample, and neither is legal in the other direction.
    #[test]
    fn job_and_result_are_unretained_and_one_way() {
        let (topic, retain) = module_job("wireless_csv").unwrap();
        assert_eq!(topic, "APK.audio/System/Protocols/wireless_csv/outgoing/job");
        assert!(!retain, "a retained job re-runs on every reconnect");
        let (topic, retain) = module_result("wireless_csv").unwrap();
        assert_eq!(topic, "APK.audio/System/Protocols/wireless_csv/incoming/result");
        assert!(!retain, "a result answers one job, not the next reader");
        assert!(retains(DIR_INCOMING, LEAF_JOB).is_err());
        assert!(retains(DIR_OUTGOING, LEAF_RESULT).is_err());
    }

    #[test]
    fn wildcards_are_the_documented_shape() {
        assert_eq!(all_modules(DIR_INCOMING).unwrap(), "APK.audio/System/Protocols/+/incoming/#");
        assert_eq!(module_wildcard("ptp").unwrap(), "APK.audio/System/Protocols/ptp/+/#");
        assert!(all_modules("sideways").is_err());
    }
}
