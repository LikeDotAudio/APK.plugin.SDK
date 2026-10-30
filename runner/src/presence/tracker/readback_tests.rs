// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The restart and reconnect read-back.

use std::collections::BTreeMap;

use apkaudio_contracts::device_publish::Sighting;
use apkaudio_contracts::discovery_presence;

use super::fixtures::{cast, doc, iso, live, AFTER, T0};
use super::Tracker;
use crate::presence::ADOPT_GRACE;

#[test]
fn presence_a_restart_keeps_the_timestamps_and_does_not_flap() {
    let mut before = live();
    let said = before.seen(&cast(), T0).unwrap().unwrap();

    // The process restarts an hour later. The hunter sights the device
    // before the broker's replay arrives, which is the order it happens in.
    let boot = T0 + 3_600_000;
    let mut after = Tracker::booting(Some(AFTER));
    assert!(after.seen(&cast(), boot).unwrap().is_none());
    after.restore(&said.topic, said.payload.as_bytes());
    let out = after.finish_readback(boot + 3_000);
    assert!(out.is_empty(), "identical to what the broker holds: nothing written, got {out:?}");

    // Steady afterwards, and when it does go, lastOnline is still the first boot's.
    for step in 1..=10 {
        assert!(after.seen(&cast(), boot + step * 5_000).unwrap().is_none());
        assert!(after.expire(boot + step * 5_000).is_empty());
    }
    let gone = after.expire(boot + 50_000 + 95_000);
    assert_eq!(gone.len(), 1);
    let d = doc(&gone[0]);
    assert_eq!(d["lastOnline"], iso(T0));
    assert_eq!(d["lastDiscovered"], iso(boot + 50_000));
}

#[test]
fn presence_a_retained_device_nobody_sights_after_restart_expires_on_the_normal_clock() {
    let mut before = live();
    let said = before.seen(&cast(), T0).unwrap().unwrap();

    let boot = T0 + 3_600_000;
    let mut after = Tracker::booting(Some(AFTER));
    after.restore(&said.topic, said.payload.as_bytes());
    assert!(after.finish_readback(boot).is_empty(), "adopted, not rewritten");
    assert!(after.expire(boot + 60_000).is_empty(), "not before inactive_after from the connect");
    let out = after.expire(boot + 95_000);
    assert_eq!(out.len(), 1);
    let d = doc(&out[0]);
    assert_eq!(d["lastOffline"], iso(boot + 90_000));
    assert_eq!(d["lastDiscovered"], iso(T0), "the last sighting anyone recorded, not the adoption");
    assert_eq!(d["lastOnline"], iso(T0));
}

#[test]
fn presence_a_departures_only_plugin_expires_an_unsighted_retained_device_after_the_grace() {
    let mut before = Tracker::new(None);
    let said = before.seen(&cast(), T0).unwrap().unwrap();

    let boot = T0 + 3_600_000;
    let mut after = Tracker::booting(None);
    after.restore(&said.topic, said.payload.as_bytes());
    assert!(after.finish_readback(boot).is_empty());
    let grace = ADOPT_GRACE.as_millis() as i64;
    assert!(after.expire(boot + grace).is_empty());
    let out = after.expire(boot + grace + 5_000);
    assert_eq!(out.len(), 1);
    assert_eq!(doc(&out[0])["lastOffline"], iso(boot + grace));
}

#[test]
fn presence_a_device_retained_offline_that_is_back_gets_a_new_last_online() {
    let mut before = live();
    before.seen(&cast(), T0).unwrap();
    let gone = before.expire(T0 + 100_000).remove(0);

    let boot = T0 + 3_600_000;
    let mut after = Tracker::booting(Some(AFTER));
    after.seen(&cast(), boot).unwrap();
    after.restore(&gone.topic, gone.payload.as_bytes());
    let out = after.finish_readback(boot + 3_000);
    assert_eq!(out.len(), 1);
    let d = doc(&out[0]);
    assert_eq!(d["lastOnline"], iso(boot));
    assert_eq!(d["lastOffline"], iso(T0 + 90_000), "the history before the restart is kept");
}

#[test]
fn presence_a_reconnect_refills_an_empty_broker_and_is_silent_to_a_full_one() {
    let mut t = live();
    let said = t.seen(&cast(), T0).unwrap().unwrap();

    t.begin_readback();
    assert_eq!(t.finish_readback(T0 + 10_000).len(), 1, "broker lost its store: say it again");

    t.begin_readback();
    t.restore(&said.topic, said.payload.as_bytes());
    assert!(t.finish_readback(T0 + 20_000).is_empty());
}

#[test]
fn presence_an_older_identity_document_is_replaced_or_retracted_on_boot() {
    let v1 = br#"{"schemaVersion":1,"protocol":"chromecast","deviceId":"chromecast:10.0.0.77","make":"Google"}"#;
    let sighted = cast().topic().unwrap();
    let unsighted = "APK.audio/Discovery/chromecast/chromecast:10.0.0.99";
    let mut t = Tracker::booting(Some(AFTER));
    t.seen(&cast(), T0).unwrap();
    t.restore(&sighted, v1);
    t.restore(unsighted, v1);
    let out = t.finish_readback(T0 + 3_000);
    assert_eq!(out.len(), 2, "{out:?}");
    let by_topic: BTreeMap<_, _> = out.iter().map(|p| (p.topic.as_str(), p.payload.as_str())).collect();
    assert!(discovery_presence::parse(by_topic[sighted.as_str()].as_bytes()).is_some());
    assert_eq!(by_topic[unsighted], "", "retracted");
}

#[test]
fn presence_an_id_retired_during_a_read_back_is_not_adopted_from_the_replay() {
    let by_name = Sighting::new("dnssd", "speaker.local");
    let by_mac = Sighting::new("dnssd", "speaker.local").serial("48:D6:D5:8D:D6:B7");
    let old = Tracker::new(None).seen(&by_name, T0).unwrap().unwrap();

    let boot = T0 + 3_600_000;
    let mut t = Tracker::booting(None);
    t.seen(&by_name, boot).unwrap();
    t.seen(&by_mac, boot + 1).unwrap();
    assert!(t.retire(&by_name.device_id()).is_none(), "held");
    t.restore(&old.topic, old.payload.as_bytes());
    let out = t.finish_readback(boot + 3_000);
    let by_topic: BTreeMap<_, _> = out.iter().map(|p| (p.topic.as_str(), p.payload.as_str())).collect();
    assert_eq!(by_topic[old.topic.as_str()], "", "retracted: {out:?}");
    assert!(discovery_presence::parse(by_topic[by_mac.topic().unwrap().as_str()].as_bytes()).is_some());
    assert_eq!(t.counts(), (1, 0), "the old id is not adopted");
    assert!(t.expire(boot + 10_000_000).is_empty());
}

#[test]
fn presence_writes_are_held_while_reading_back() {
    let mut t = Tracker::booting(Some(AFTER));
    assert!(t.seen(&cast(), T0).unwrap().is_none());
    assert!(t.expire(T0 + 1_000_000).is_empty());
    assert_eq!(t.finish_readback(T0 + 1_000_001).len(), 1);
}
