// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The rows under a hunter's `Device/…` prefix: the link and the identity
//! beside it, across the read-back windows. PLAN-3393.01.
//!
//! Driven through the same bodies `seen`, `unlink`, `retire` and the tick run
//! under the lock, on a `Shared` of the test's own, so nothing here touches
//! the process-wide handle or a broker. What comes back is what `say` would
//! publish — every one of them retained.

use std::time::Instant;

use apkaudio_contracts::device_publish::{Publication, Sighting};

use super::super::session::{connected, ticked};
use super::super::shared::Shared;
use super::super::READBACK;
use super::{retired, sighted, unlinked};

const T0: i64 = 1_789_000_000_000;
const DETAIL: &str = "APK.audio/System/Protocols/printers/Device/Brother_HL-L2405W";

fn printer() -> Sighting {
    Sighting::new("printer", "10.0.0.169")
        .class("Printer")
        .made_by("Brother", "HL-L2405W series")
        .serial("e3248000-80ce-11db-8000-44fa66e42fbf")
        .notes("Brother HL-L2405W")
}

/// A connected process whose read-back window has already run its course,
/// so the next tick closes it.
fn connected_and_due() -> Shared {
    let mut s = Shared::new();
    connected(&mut s);
    s.connected_at = Instant::now().checked_sub(READBACK);
    s
}

fn leaf<'a>(out: &'a [Publication], name: &str) -> Option<&'a str> {
    let topic = format!("{DETAIL}/{name}");
    out.iter().rev().find(|p| p.topic == topic).map(|p| p.payload.as_str())
}

fn link_of(s: &Sighting) -> String {
    s.topic().unwrap()
}

#[test]
fn presence_a_link_recorded_inside_the_boot_window_is_said_when_it_closes() {
    let mut s = connected_and_due();
    let inside = sighted(&mut s, &printer(), Some(DETAIL), T0);
    assert!(inside.is_empty(), "nothing is written while the window is open, got {inside:?}");

    let out = ticked(&mut s, T0 + 3_000);
    assert_eq!(leaf(&out, "presence_topic"), Some(link_of(&printer()).as_str()), "{out:?}");
    assert_eq!(leaf(&out, "manufacturer"), Some("Brother"));
    assert_eq!(leaf(&out, "model"), Some("HL-L2405W series"));
    assert_eq!(leaf(&out, "device_class"), Some("Printer"));
    assert_eq!(leaf(&out, "name"), Some("Brother_HL-L2405W"));
    assert_eq!(leaf(&out, "serial"), Some("e3248000-80ce-11db-8000-44fa66e42fbf"));
    assert_eq!(leaf(&out, "notes"), Some("Brother HL-L2405W"));
}

/// THE DEFECT. The link book kept "already said" across a reconnect, so a
/// broker that came back without its retained store (a crash between
/// autosaves, a recreated data volume) was refilled with every presence
/// document and none of the links: a device sighted again inside the new
/// window was never linked again, and only rows first recorded after the
/// window closed ever reached the broker.
#[test]
fn presence_a_link_seen_inside_a_reconnect_window_is_said_again() {
    let mut s = connected_and_due();
    sighted(&mut s, &printer(), Some(DETAIL), T0);
    let first = ticked(&mut s, T0 + 3_000);
    assert!(leaf(&first, "presence_topic").is_some());

    // The broker restarts empty; the session reconnects and reads back.
    connected(&mut s);
    s.connected_at = Instant::now().checked_sub(READBACK);
    assert!(sighted(&mut s, &printer(), Some(DETAIL), T0 + 60_000).is_empty());
    let out = ticked(&mut s, T0 + 63_000);
    assert_eq!(
        leaf(&out, "presence_topic"),
        Some(link_of(&printer()).as_str()),
        "the reconnect window closed without restating the link: {out:?}"
    );
    assert_eq!(leaf(&out, "model"), Some("HL-L2405W series"));
}

#[test]
fn presence_identity_is_written_only_when_it_changes() {
    let mut s = connected_and_due();
    sighted(&mut s, &printer(), Some(DETAIL), T0);
    ticked(&mut s, T0 + 3_000);

    // Identity leaves under the hunter's prefix only: the `…/verified` proof
    // (ROAD-3437) and the `…/record` document (contracts 0.5.0) are the
    // tracker's, held by its verified_* and record tests.
    let identity = |out: Vec<Publication>| -> Vec<Publication> {
        out.into_iter().filter(|p| !p.topic.ends_with("/verified") && !p.topic.ends_with("/record")).collect()
    };
    let again = identity(sighted(&mut s, &printer(), Some(DETAIL), T0 + 10_000));
    assert!(again.is_empty(), "an unchanged sighting writes nothing, got {again:?}");

    let renamed = printer().made_by("Brother", "HL-L2405W");
    let out = identity(sighted(&mut s, &renamed, Some(DETAIL), T0 + 20_000));
    assert_eq!(out, vec![Publication { topic: format!("{DETAIL}/model"), payload: "HL-L2405W".into() }]);

    let unnoted = Sighting { notes: None, ..renamed };
    let out = identity(sighted(&mut s, &unnoted, Some(DETAIL), T0 + 30_000));
    assert_eq!(out, vec![Publication { topic: format!("{DETAIL}/notes"), payload: String::new() }]);
}

#[test]
fn presence_unlink_retracts_the_link_and_the_identity() {
    let mut s = connected_and_due();
    sighted(&mut s, &printer(), Some(DETAIL), T0);
    ticked(&mut s, T0 + 3_000);

    let out = unlinked(&mut s, DETAIL);
    let mut leaves: Vec<&str> = out.iter().map(|p| p.topic.rsplit('/').next().unwrap()).collect();
    leaves.sort();
    assert_eq!(leaves, ["device_class", "manufacturer", "model", "name", "notes", "presence_topic", "serial"]);
    assert!(out.iter().all(Publication::is_retraction));
    assert!(unlinked(&mut s, DETAIL).is_empty(), "a second unlink has nothing left to retract");
}

#[test]
fn presence_retire_relinks_the_row_and_keeps_its_identity() {
    let mut s = connected_and_due();
    let by_name = Sighting::new("dnssd", "APK-Shop.local").class("Audio").made_by("APK", "Shop");
    sighted(&mut s, &by_name, Some(DETAIL), T0);
    ticked(&mut s, T0 + 3_000);

    let by_mac = by_name.clone().serial("00:0a:92:02:77:5b");
    let out = retired(&mut s, &by_name.device_id(), &by_mac);
    assert_eq!(leaf(&out, "presence_topic"), Some(link_of(&by_mac).as_str()), "{out:?}");
    assert!(leaf(&out, "model").is_none(), "identity did not change, so it is not rewritten");
}

/// The `…/record` beside the status: said once the read-back closes, silent
/// while nothing changes, said again when the model does.
#[test]
fn the_record_follows_the_sighting_through_the_hunter() {
    let mut s = connected_and_due();
    sighted(&mut s, &printer(), Some(DETAIL), T0);
    ticked(&mut s, T0 + 3_000);
    let records = |out: Vec<Publication>| -> Vec<Publication> { out.into_iter().filter(|p| p.topic.ends_with("/record")).collect() };
    let first = records(sighted(&mut s, &printer(), Some(DETAIL), T0 + 4_000));
    assert_eq!(first.len(), 1, "{first:?}");
    assert!(first[0].payload.contains("HL-L2405W series"));
    assert!(records(sighted(&mut s, &printer(), Some(DETAIL), T0 + 5_000)).is_empty());
    let renamed = printer().made_by("Brother", "HL-L2405W");
    assert_eq!(records(sighted(&mut s, &renamed, Some(DETAIL), T0 + 6_000)).len(), 1);
}

/// ROAD-3471: PLUGIN.NMOS_CONTROL holds a presence row per registry Device —
/// ~11.8k on the bench of 2026-10-31. A sighting that changes one row used to
/// re-diff every row (`unsaid` walked the book), so filling the book after a
/// restart was quadratic. Now only the changed row is compared: 12k first
/// sightings, then one changed identity among 12k, each costs one row.
#[test]
fn presence_twelve_thousand_rows_cost_one_row_per_sighting() {
    const N: usize = 12_000;
    let mut s = connected_and_due();
    ticked(&mut s, T0); // the window closes on an empty book
    let device = |i: usize| Sighting::new("nmos_device", &format!("dev-{i}")).class("NMOS Device").made_by("APK", "generic");
    let detail = |i: usize| format!("APK.audio/System/Protocols/nmos/Device/D{i}");

    let started = Instant::now();
    for i in 0..N {
        let out = sighted(&mut s, &device(i), Some(&detail(i)), T0 + 1_000);
        assert!(out.iter().any(|p| p.topic == format!("{}/presence_topic", detail(i))), "row {i} not linked");
        assert!(out.iter().all(|p| p.topic.starts_with(&detail(i)) || !p.topic.contains("/Device/")), "only row {i} is said");
    }
    let fill = started.elapsed();

    let started = Instant::now();
    let renamed = device(N / 2).made_by("APK", "renamed");
    let out = sighted(&mut s, &renamed, Some(&detail(N / 2)), T0 + 2_000);
    let one = started.elapsed();
    let said: Vec<&str> = out.iter().filter(|p| p.topic.contains("/Device/")).map(|p| p.topic.as_str()).collect();
    assert_eq!(said, [format!("{}/model", detail(N / 2))], "one changed leaf, one write");

    eprintln!("presence: {N} first sightings in {fill:?}; one changed row among {N} in {one:?}");
    // Debug build, 2026-10-31: the full walk took 190 s to fill and 46 ms per
    // change here; one row at a time, 1.6 s and 64 µs.
    assert!(fill < std::time::Duration::from_secs(20), "filling {N} rows took {fill:?}");
    assert!(one < std::time::Duration::from_millis(10), "one changed row among {N} took {one:?}");
}
