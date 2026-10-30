// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The boot sweep of links an earlier process left. Driven through the same
//! bodies the session runs under the lock, on a `Shared` of the test's own.

use std::time::{Duration, Instant};

use apkaudio_contracts::device_publish::{Publication, Sighting};

use super::super::hunter::sighted;
use super::super::session::{connected, ticked};
use super::super::shared::Shared;
use super::super::READBACK;
use super::{found, link_filters, link_prefix};

const T0: i64 = 1_789_000_000_000;
const GHOST: &str = "APK.audio/System/Protocols/nmos/Device/WBS-32ME-RIGHT_local_80";
const DOC: &str = "APK.audio/Discovery/nmos/wbs-32me-right";

fn nmos_process() -> Shared {
    let mut s = Shared::new();
    s.protocols = vec!["nmos".into()];
    s.tracker.set_inactive_after(Some(Duration::from_secs(90)));
    connected(&mut s);
    s.connected_at = Instant::now().checked_sub(READBACK);
    s
}

fn retracted(out: &[Publication], prefix: &str) -> Vec<String> {
    let mut leaves: Vec<String> = out
        .iter()
        .filter(|p| p.is_retraction())
        .filter_map(|p| p.topic.strip_prefix(&format!("{prefix}/")).map(str::to_string))
        .collect();
    leaves.sort();
    leaves
}

const ALL: [&str; 7] = ["device_class", "manufacturer", "model", "name", "notes", "presence_topic", "serial"];

#[test]
fn presence_sweep_filters_reach_every_row_depth_and_name_the_link() {
    assert_eq!(link_filters()[0], "APK.audio/System/Protocols/+/Device/+/presence_topic");
    assert_eq!(link_prefix(&format!("{GHOST}/presence_topic")), Some(GHOST));
    assert_eq!(link_prefix("APK.audio/Discovery/nmos/presence_topicx"), None);
}

/// THE DEFECT, as measured: the link of a device whose document went offline
/// six days ago, retained with nobody sighting it.
#[test]
fn presence_a_ghost_link_to_an_offline_document_is_retracted_at_boot() {
    let mut s = nmos_process();
    s.tracker.restore(
        DOC,
        br#"{"lastOnline":"2026-10-20T10:00:00Z","lastOffline":"2026-10-24T10:00:00Z","lastDiscovered":"2026-10-24T09:58:30Z"}"#,
    );
    found(&mut s, &format!("{GHOST}/presence_topic"), DOC.as_bytes());

    let out = ticked(&mut s, T0 + 3_000);
    assert_eq!(retracted(&out, GHOST), ALL, "{out:?}");
    assert!(s.orphans.is_empty());
}

#[test]
fn presence_a_link_to_no_document_at_all_is_retracted() {
    let mut s = nmos_process();
    found(&mut s, &format!("{GHOST}/presence_topic"), DOC.as_bytes());
    assert_eq!(retracted(&ticked(&mut s, T0 + 3_000), GHOST), ALL);
}

#[test]
fn presence_another_protocols_link_is_not_ours_to_sweep() {
    let mut s = nmos_process();
    let theirs = "APK.audio/System/Protocols/printers/Device/Brother";
    found(&mut s, &format!("{theirs}/presence_topic"), b"APK.audio/Discovery/printer/abc");
    assert!(retracted(&ticked(&mut s, T0 + 3_000), theirs).is_empty());
}

#[test]
fn presence_a_sighted_row_keeps_its_link() {
    let mut s = nmos_process();
    let box_ = Sighting::new("nmos", "wbs-32me-right").class("Audio");
    found(&mut s, &format!("{GHOST}/presence_topic"), box_.topic().unwrap().as_bytes());
    sighted(&mut s, &box_, Some(GHOST), T0);
    let out = ticked(&mut s, T0 + 3_000);
    assert!(retracted(&out, GHOST).is_empty(), "{out:?}");
}

/// Online but unsighted: adopted, so the link stays — until the adopted
/// document expires, and then the link goes with it.
#[test]
fn presence_a_link_to_an_adopted_online_document_goes_when_it_expires() {
    let mut s = nmos_process();
    let online = br#"{"lastOnline":"2026-10-30T10:00:00Z","lastOffline":null,"lastDiscovered":"2026-10-30T10:00:00Z"}"#;
    s.tracker.restore(DOC, online);
    found(&mut s, &format!("{GHOST}/presence_topic"), DOC.as_bytes());

    let out = ticked(&mut s, T0 + 3_000);
    assert!(retracted(&out, GHOST).is_empty(), "{out:?}");
    assert_eq!(s.orphans.get(GHOST).map(String::as_str), Some(DOC));

    let out = ticked(&mut s, T0 + 3_000 + 91_000);
    assert!(out.iter().any(|p| p.topic == DOC && !p.is_retraction()), "the document is said offline: {out:?}");
    assert_eq!(retracted(&out, GHOST), ALL, "{out:?}");
    assert!(s.orphans.is_empty());
}
