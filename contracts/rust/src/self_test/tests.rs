// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The tests of `self_test.rs`, out of line.

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
