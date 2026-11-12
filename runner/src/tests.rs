// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The runner's own tests (were inline in lib.rs; ROAD-3749).

#[test]
fn a_refused_panel_publication_waits_and_never_overwrites_a_newer_one() {
    // ROAD-3566: try_publish refuses when the queue is full; the change must not be lost.
    let mut pending = Vec::new();
    let p = |t: &str, v: &str| (t.to_string(), v.to_string());
    super::send_panels(&mut pending, vec![p("Panels/a", "old"), p("Panels/b", "")], |_, _| false);
    assert_eq!(pending, vec![p("Panels/a", "old"), p("Panels/b", "")], "both wait");
    super::send_panels(&mut pending, vec![p("Panels/a", "newer")], |_, _| true);
    assert_eq!(pending, vec![p("Panels/b", "")], "a newer panel that went out drops the old one waiting");
    let mut out = Vec::new();
    let waiting = std::mem::take(&mut pending);
    super::send_panels(&mut pending, waiting, |t, v| { out.push((t, v)); true });
    assert_eq!(out, vec![p("Panels/b", "")], "the clear goes out on the retry");
    assert!(pending.is_empty());
}

use super::*;

/// `enabled` switches a plugin off only on an explicit no; absent or
/// garbled is on (ROAD-3478: the runner, not each plugin, honours it).
#[test]
fn enabled_is_off_only_on_an_explicit_no() {
    let config = |v: &str| ini_to_json(&format!("[probe]\nenabled = {v}\n"));
    for no in ["false", "FALSE", "no", "off", "0", " false "] {
        assert!(!module_enabled(&config(no), "probe"), "{no:?} switches it off");
    }
    for yes in ["true", "yes", "1", "on", "flase", ""] {
        assert!(module_enabled(&config(yes), "probe"), "{yes:?} leaves it on");
    }
    assert!(module_enabled(&ini_to_json("[probe]\nraise = always-on\n"), "probe"), "absent is on");
    assert!(module_enabled(&config("false"), "other"), "another section's switch is not this one's");
    assert!(!module_enabled(&ini_to_json("[PROBE]\nenabled = false\n"), "probe"), "section matched without case");
}

/// The tick carries the loopback's counters (ROAD-3478), and contracts'
/// rule reads it as it reads `GET /telemetry`: a number, never absent.
#[test]
fn the_telemetry_tick_carries_the_counters_the_rule_reads() {
    let sample = tick_sample(7);
    assert_eq!(sample["uptime_s"], 7);
    for key in ["packets_received", "messages_published", "publish_failures"] {
        assert!(sample["counters"][key].is_u64(), "counters.{key} is a number");
    }
    assert!(sample["counters"].get("unread_config_keys").is_some(), "null when undeclared, never absent");
    let state = apkaudio_contracts::baremetal_telemetry::observed_state(Some(&sample));
    assert_ne!(state, apkaudio_contracts::baremetal_telemetry::ServingState::Unknown);
}

#[test]
fn no_argument_starts_the_agent_and_any_argument_stops_it_first() {
    let none: Vec<String> = vec![];
    assert_eq!(answer_arguments(none, &["nmos"], &["browse"]), None);
    assert_eq!(answer_arguments(vec!["--help".into()], &["nmos"], &["browse"]), Some(0));
    assert_eq!(answer_arguments(vec!["-h".into()], &["nmos"], &["browse"]), Some(0));
    assert_eq!(answer_arguments(vec!["--version".into()], &["nmos"], &["browse"]), Some(0));
    assert_eq!(answer_arguments(vec!["--port=1".into()], &["nmos"], &["browse"]), Some(2));
}

#[test]
fn the_pod_is_the_pod_folder_on_the_path() {
    assert_eq!(pod_of("/src/PODS/POD.THIN/Softapps/DATATYPE/DATATYPE.wireless.CSV/SRC"), Some("POD.THIN"));
    assert_eq!(pod_of("/home/x/APK.audio/PODS/POD.THICK/PLUGINS/PLUGIN.DANTE/SRC"), Some("POD.THICK"));
    assert_eq!(pod_of("/src/somewhere/else"), None);
    // Unstamped, a binary takes the runner's own pod: this crate's.
    assert_eq!(plugin_pod(), Some("POD.THICK"));
}

#[test]
fn apk_pod_names_the_pod_when_it_is_one() {
    assert_eq!(pod_declared(Some("POD.workers")).as_deref(), Some("POD.workers"));
    assert_eq!(pod_declared(Some("  POD.workers\n")).as_deref(), Some("POD.workers"));
    // Unset, blank or not a pod name: the path decides.
    assert_eq!(pod_declared(None), None);
    assert_eq!(pod_declared(Some("")), None);
    assert_eq!(pod_declared(Some("POD.")), None);
    assert_eq!(pod_declared(Some("workers")), None);
    assert_eq!(pod_declared(Some("POD.a/POD.b")), None);
    assert_eq!(pod_declared(Some("POD.a b")), None);
}

#[test]
fn a_config_ini_becomes_sections_of_strings() {
    let v = ini_to_json("# c\n[dante]\nenabled = true\ndomain = local.\n; x\n[other]\nk=v\n");
    assert_eq!(v["dante"]["enabled"], "true");
    assert_eq!(v["dante"]["domain"], "local.");
    assert_eq!(v["other"]["k"], "v");
}

#[test]
fn raise_and_serves_are_read_off_the_tokens_section() {
    let v = ini_to_json("[DANTE]\nraise = On-Sighting\nserves = dante, dante_ddm\n");
    assert_eq!(module_raise(&v, "dante"), Some("on-sighting"));
    assert_eq!(module_serves(&v, "dante"), vec!["dante", "dante_ddm"]);
    let bare = ini_to_json("[ptp]\nenabled = true\n");
    assert_eq!(module_raise(&bare, "ptp"), None, "undeclared is null, never guessed");
    assert_eq!(module_serves(&bare, "ptp"), vec!["ptp"]);
    let wrong = ini_to_json("[ptp]\nraise = sometimes\n");
    assert_eq!(module_raise(&wrong, "ptp"), None);
}

#[test]
fn the_topic_is_the_sections_own_or_the_tokens_default() {
    let declared = ini_to_json("[PTP]\ntopic = APK.audio/System/Protocols/ptp\n");
    assert_eq!(module_topic(&declared, "ptp"), "APK.audio/System/Protocols/ptp");
    let bare = ini_to_json("[dante]\nenabled = true\n");
    assert_eq!(module_topic(&bare, "dante"), "APK.audio/System/Protocols/dante");
}

#[test]
fn the_stalest_armed_clock_is_judged_and_unarmed_ones_are_not() {
    // control is wedged while browse keeps ticking: control is the answer.
    let clocks = [("browse", 995), ("control", 800), ("idle", 0)];
    assert_eq!(stalest(clocks.into_iter(), 1000), Some(("control", 200)));
    assert_eq!(stalest([("idle", 0)].into_iter(), 1000), None);
}

#[test]
fn a_tick_moves_only_its_own_agents_clock() {
    let _ = AGENT_TICKS.set(vec![("a", AtomicU64::new(0)), ("b", AtomicU64::new(0))]);
    std::thread::spawn(|| {
        AGENT_SLOT.with(|s| s.set(Some(1)));
        tick();
    })
    .join()
    .unwrap();
    let ticks = AGENT_TICKS.get().unwrap();
    assert_eq!(ticks[0].1.load(Ordering::Relaxed), 0);
    assert_ne!(ticks[1].1.load(Ordering::Relaxed), 0);
}

#[test]
fn drains_run_together_and_a_slow_one_is_cut_off_at_the_ceiling() {
    use std::sync::atomic::AtomicBool;
    static QUICK: AtomicBool = AtomicBool::new(false);
    let drains: Vec<Drain> = vec![
        ("quick", Box::new(|| QUICK.store(true, Ordering::Relaxed))),
        ("slow", Box::new(|| std::thread::sleep(Duration::from_secs(5)))),
        ("panics", Box::new(|| panic!("a drain that panics is finished, not late"))),
    ];
    let started = Instant::now();
    let late = run_drains("t", drains, Duration::from_millis(300));
    assert!(started.elapsed() < Duration::from_secs(2), "the ceiling bounds the wait");
    assert_eq!(late, vec!["slow"]);
    assert!(QUICK.load(Ordering::Relaxed));
    // Everything in time: nothing late, and no waiting out the ceiling.
    let started = Instant::now();
    assert!(run_drains("t", vec![("a", Box::new(|| {}))], Duration::from_secs(5)).is_empty());
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn at_stop_queues_drains_in_order() {
    at_stop("first", || {});
    at_stop("second", || {});
    let names: Vec<&str> = DRAINS.lock().unwrap().iter().map(|(n, _)| *n).collect();
    assert!(names.windows(2).any(|w| w == ["first", "second"]), "{names:?}");
}

#[test]
fn a_public_broker_is_not_ours() {
    assert!(broker_is_local("127.0.0.1"));
    assert!(broker_is_local("broker"));
    assert!(!broker_is_local("test.mosquitto.org"));
}
