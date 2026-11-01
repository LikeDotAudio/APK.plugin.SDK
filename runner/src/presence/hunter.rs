// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The four calls a hunter makes: each is a lock, a tracker call and a `say`.
//!
//! NONE OF THEM BLOCKS ON THE BUS. Call them from a plain thread or from a
//! `tokio::spawn` task alike: `say` hands the write to presence's own speaker
//! thread, so a broker that has gone away never parks the caller — not even a
//! runtime worker with a full request queue behind it. PLAN-3369.01.

use apkaudio_contracts::device_publish::{Publication, Sighting};

use super::shared::{identity_of, now_ms, say, unsaid_rows, with, Shared};
use super::sweep;
use super::{KEY_PROTOCOLS, LINK_LEAF};

#[cfg(test)]
#[path = "hunter_tests.rs"]
mod tests;

/// A hunter saw a device. Cheap to call on every announcement: a device that
/// is already online, with the same identity, costs a map lookup and writes
/// nothing.
///
/// `detail_topic` is the hunter's own `System/Protocols/{proto}/Device/…`
/// prefix for this device when it still writes one. The runner keeps two
/// things retained under it: `{prefix}/presence_topic`, the link the
/// Discovered mirror joins through, and the sighting's identity beside it —
/// `manufacturer`, `model`, `device_class`, `name`, `serial`, `notes` — so a
/// snapshot of the retained tree can register the device without the
/// hunter's own unretained leaves. Each leaf is written when it changes, not
/// on every sighting. The presence document itself still carries no identity.
///
/// Never waits on the broker; see the note at the top of this file.
pub fn seen(sighting: &Sighting, detail_topic: Option<&str>) {
    let (client, out) = with(|s| {
        if s.client.is_none() && !s.warned_unstarted {
            s.warned_unstarted = true;
            eprintln!(
                "   ⚠️  presence: {} sighted but the tracker was never started — declare {KEY_PROTOCOLS} in config.ini",
                sighting.protocol
            );
        }
        (s.client.clone(), sighted(s, sighting, detail_topic, now_ms()))
    });
    say(client, out);
}

/// [`seen`] without the lock, the clock or the bus.
pub(super) fn sighted(s: &mut Shared, sighting: &Sighting, detail_topic: Option<&str>, now: i64) -> Vec<Publication> {
    let mut out = Vec::new();
    match s.tracker.seen(sighting, now) {
        Ok(Some(p)) => {
            println!("🟢 [{}] {} online", s.token, p.topic);
            out.push(p);
        }
        Ok(None) => {}
        Err(e) => eprintln!("   ⚠️  [{}] {} not tracked for presence: {e}", s.token, sighting.address),
    }
    // The proof beside it (ROAD-3437): restated at most every verify period.
    let token = s.token.clone();
    out.extend(s.tracker.verify(&sighting.device_id(), &token, now));
    // And its identity (contracts 0.5.0): only when the record changed shape.
    match s.tracker.record(sighting, now) {
        Ok(p) => out.extend(p),
        Err(e) => eprintln!("   ⚠️  [{}] {} has no record: {e}", s.token, sighting.address),
    }
    if let (Some(detail), Ok(topic)) = (detail_topic, sighting.topic()) {
        s.orphans.remove(detail);
        let mut want = identity_of(sighting, detail);
        want.insert(LINK_LEAF, topic);
        if s.rows.get(detail).map(|r| &r.want) != Some(&want) {
            s.rows.entry(detail.to_string()).or_default().want = want;
            out.extend(unsaid_rows(s, [detail]));
        }
    }
    out
}

/// The protocol said this device left. `device_id` is the D2 id —
/// [`Sighting::device_id`] — which a departure can carry without a sighting.
/// Never waits on the broker, like [`seen`].
pub fn gone(device_id: &str) {
    let (client, out) = with(|s| {
        let mut p: Vec<_> = s.tracker.gone(device_id, now_ms()).into_iter().collect();
        p.extend(s.tracker.unverify(device_id));
        for p in &p {
            println!("⚪ [{}] {} offline (departed)", s.token, p.topic);
        }
        let mut out = sweep::expired(s, &p);
        out.extend(p);
        (s.client.clone(), out)
    });
    say(client, out);
}

/// The hunter knew one box by `old_id` and now knows it as `by` — the id moved
/// from a name to the MAC once the neighbour cache answered. The old document
/// is retracted rather than left saying online for ever (a departures-only
/// plugin would never expire it), and every row linked to it is re-linked to
/// the new one. A no-op when `old_id` is `by`'s own id.
pub fn retire(old_id: &str, by: &Sighting) {
    let (client, out) = with(|s| (s.client.clone(), retired(s, old_id, by)));
    say(client, out);
}

/// [`retire`] without the lock or the bus.
pub(super) fn retired(s: &mut Shared, old_id: &str, by: &Sighting) -> Vec<Publication> {
    let Ok(new_topic) = by.topic() else {
        return Vec::new();
    };
    if by.device_id() == old_id {
        return Vec::new();
    }
    let mut out = Vec::new();
    out.extend(s.tracker.unverify(old_id));
    if let Some(p) = s.tracker.retire(old_id) {
        println!("♻️  [{}] {} retired — now {new_topic}", s.token, p.topic);
        out.extend(sweep::expired(s, std::slice::from_ref(&p)));
        out.push(p);
    }
    let old_topic = apkaudio_contracts::topics::discovery(&by.protocol, old_id).ok();
    let mut relinked = Vec::new();
    for (detail, row) in s.rows.iter_mut() {
        if old_topic.is_some() && row.link() == old_topic.as_ref() {
            row.want.insert(LINK_LEAF, new_topic.clone());
            relinked.push(detail.clone());
        }
    }
    out.extend(unsaid_rows(s, relinked.iter().map(String::as_str)));
    out
}

/// The hunter retracted the row at `detail_topic`: retract its link and the
/// identity beside it too, so a vanished service leaves no leaf of the
/// runner's behind on its own.
pub fn unlink(detail_topic: &str) {
    let (client, out) = with(|s| (s.client.clone(), unlinked(s, detail_topic)));
    say(client, out);
}

/// [`unlink`] without the lock or the bus. Every leaf the row wanted or said
/// is retracted, inside a read-back window too: a row this connection has
/// not said yet may still stand on the broker from the last one, and an
/// empty retained write to a topic that holds nothing costs nothing.
pub(super) fn unlinked(s: &mut Shared, detail_topic: &str) -> Vec<Publication> {
    let Some(row) = s.rows.remove(detail_topic) else {
        return Vec::new();
    };
    let leaves: std::collections::BTreeSet<&'static str> = row.want.into_keys().chain(row.said.into_keys()).collect();
    leaves
        .into_iter()
        .map(|leaf| Publication { topic: format!("{detail_topic}/{leaf}"), payload: String::new() })
        .collect()
}
