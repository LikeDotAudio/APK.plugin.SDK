// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The live transitions: arrival, steady sightings, expiry, departure, retirement.

use std::collections::BTreeSet;

use apkaudio_contracts::device_publish::Sighting;
use apkaudio_contracts::discovery_presence;
use serde_json::Value;

use super::fixtures::{cast, doc, iso, live, T0};
use super::Tracker;

#[test]
fn presence_a_new_device_is_one_publish_with_last_online_set() {
    let mut t = live();
    let p = t.seen(&cast(), T0).unwrap().expect("a new device is said");
    assert_eq!(p.topic, "APK.audio/Discovery/chromecast/chromecast:10.0.0.77");
    let d = doc(&p);
    assert_eq!(d["lastOnline"], iso(T0));
    assert_eq!(d["lastDiscovered"], iso(T0));
    assert_eq!(d["lastOffline"], Value::Null);
    // It is a legal document by the contract's own reader, and reads online.
    let parsed = discovery_presence::parse(p.payload.as_bytes()).expect("presence");
    assert!(discovery_presence::is_online(&parsed));
}

/// The owner's rule in one assertion: the captured body is status only.
#[test]
fn presence_the_published_body_is_exactly_the_three_keys() {
    let mut t = live();
    let mut said = vec![t.seen(&cast().serial("AA:BB").firmware("1.56").notes("x"), T0).unwrap().unwrap()];
    said.extend(t.expire(T0 + 100_000));
    said.extend(t.seen(&cast(), T0 + 200_000).unwrap());
    said.push(t.gone(&cast().device_id(), T0 + 210_000).unwrap());
    assert_eq!(said.len(), 4);
    for p in said {
        let keys: BTreeSet<String> = doc(&p).as_object().unwrap().keys().cloned().collect();
        assert_eq!(
            keys,
            ["lastDiscovered", "lastOffline", "lastOnline"].iter().map(|k| k.to_string()).collect(),
            "{}",
            p.payload
        );
    }
}

#[test]
fn presence_steady_sightings_publish_nothing() {
    let mut t = live();
    t.seen(&cast(), T0).unwrap();
    let mut writes = 0;
    // Ten minutes of announcements every 5 s, and the tick between them.
    for step in 1..=120 {
        let now = T0 + step * 5_000;
        writes += t.seen(&cast(), now).unwrap().into_iter().count();
        writes += t.expire(now).len();
    }
    assert_eq!(writes, 0);
}

#[test]
fn presence_an_identity_change_publishes_nothing() {
    let mut t = live();
    t.seen(&cast(), T0).unwrap();
    let moved = cast().firmware("1.56").notes("16 channel(s)").class("Speaker Group");
    assert!(t.seen(&moved, T0 + 30_000).unwrap().is_none());
}

#[test]
fn presence_expiry_is_one_publish_with_last_offline_at_the_expiry_time() {
    let mut t = live();
    t.seen(&cast(), T0).unwrap();
    t.seen(&cast(), T0 + 20_000).unwrap();
    assert!(t.expire(T0 + 110_000).is_empty(), "exactly inactive_after is not yet expired");
    // The tick that notices is late; the stamp is not.
    let out = t.expire(T0 + 114_000);
    assert_eq!(out.len(), 1);
    let d = doc(&out[0]);
    assert_eq!(d["lastOffline"], iso(T0 + 110_000));
    assert_eq!(d["lastDiscovered"], iso(T0 + 20_000), "the real last sighting");
    assert_eq!(d["lastOnline"], iso(T0));
    assert!(!discovery_presence::is_online(&discovery_presence::parse(out[0].payload.as_bytes()).unwrap()));
    assert!(t.expire(T0 + 500_000).is_empty(), "said once");
}

#[test]
fn presence_reappearing_updates_last_online_and_keeps_last_offline() {
    let mut t = live();
    t.seen(&cast(), T0).unwrap();
    t.expire(T0 + 100_000);
    let back = t.seen(&cast(), T0 + 200_000).unwrap().expect("a return is a transition");
    let d = doc(&back);
    assert_eq!(d["lastOnline"], iso(T0 + 200_000));
    assert_eq!(d["lastOffline"], iso(T0 + 90_000));
    assert_eq!(d["lastDiscovered"], iso(T0 + 200_000));
}

/// `lastDiscovered` is the real last sighting, and still never a write of
/// its own: across a whole life of sightings it only ever moves in a
/// document that is also a transition.
#[test]
fn presence_last_discovered_changes_only_inside_a_transition_publish() {
    let mut t = live();
    let mut said: Vec<Value> = Vec::new();
    let mut now = T0;
    for spell in 0..3 {
        for _ in 0..20 {
            said.extend(t.seen(&cast(), now).unwrap().map(|p| doc(&p)));
            said.extend(t.expire(now).iter().map(doc));
            now += 4_000;
        }
        if spell < 2 {
            now += 200_000;
            said.extend(t.expire(now).iter().map(doc));
        }
    }
    // Three arrivals and two expiries — nothing else, however many sightings.
    assert_eq!(said.len(), 5, "{said:?}");
    for pair in said.windows(2) {
        let moved_status = pair[0]["lastOnline"] != pair[1]["lastOnline"] || pair[0]["lastOffline"] != pair[1]["lastOffline"];
        assert!(moved_status, "every publish is a transition: {pair:?}");
    }
}

#[test]
fn presence_a_departure_is_offline_since_now_once() {
    let mut t = Tracker::new(None);
    let id = cast().device_id();
    t.seen(&cast(), T0).unwrap();
    let out = t.gone(&id, T0 + 7_000).expect("departure said");
    assert_eq!(doc(&out)["lastOffline"], iso(T0 + 7_000));
    assert_eq!(doc(&out)["lastDiscovered"], iso(T0));
    assert!(t.gone(&id, T0 + 8_000).is_none());
    // Departure-only trackers never expire a sighted device on a clock.
    t.seen(&cast(), T0 + 9_000).unwrap();
    assert!(t.expire(T0 + 10_000_000).is_empty());
}

/// Measured 2026-09-17 on DNS-SD: a Cast group said `…local` online at
/// 12:06:17.281 and `48:D6:…` online a millisecond later — one box, two
/// documents, and the first would have said online for ever.
#[test]
fn presence_a_retired_id_is_retracted_not_said_offline() {
    let mut t = Tracker::new(None);
    let by_name = Sighting::new("dnssd", "speaker.local");
    let by_mac = Sighting::new("dnssd", "speaker.local").serial("48:D6:D5:8D:D6:B7");
    t.seen(&by_name, T0).unwrap().unwrap();
    t.seen(&by_mac, T0 + 1).unwrap().unwrap();
    let out = t.retire(&by_name.device_id()).expect("retracted");
    assert_eq!(out.topic, by_name.topic().unwrap());
    assert_eq!(out.payload, "");
    assert!(t.retire(&by_name.device_id()).is_none(), "once");
    assert_eq!(t.counts(), (1, 0));
}
