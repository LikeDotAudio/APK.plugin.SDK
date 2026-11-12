// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Unkept statements: `say`, `forget`, `restate`, `report` (audit 2026-10-30, W02).
//!
//! Twelve hunters (APPLETV, CHROMECAST, DNSSD, MILAN, NMOS, PING, PRINTERS, PTP,
//! RAVENNA, SAP, AES67, …) each carried the same four private functions, word
//! for word: `say` (QoS 0, never retained — the one combination the broker
//! neither stores, queues nor persists), `restate` (say every known field again
//! on the timer, because an unkept statement lives one restate interval),
//! `report` (an `AgentReport` said that way, its entries returned for the
//! timer) and `forget` (a zero-length retained payload: delete what an older,
//! retaining build left). One copy now, so "unkept" is a property the tree
//! cannot half-apply.
//!
//! ```ignore
//! use apk_plugin_runner::unkept::{say, restate, report};
//!
//! say(&client, format!("{prefix}/model"), model);
//! restate(&client, &published);                           // HashMap<String, String>
//! let state = report(&client, "PTP", AgentReport::listening("ptp", "on eth0"));
//! restate(&client, &state);                               // Vec<(String, Vec<u8>)>
//! ```
//!
//! `say` uses `Client::publish`, which WAITS while the request queue is full —
//! exactly what every copy did, so adopting it changes nothing. A thread that
//! must never wait (an event pump on a tokio worker) uses [`try_say`] or
//! [`crate::republish::republish`] instead.

use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasher;

use apkaudio_contracts::agent_state::AgentReport;
use apkaudio_contracts::topics::TopicError;
use rumqttc::{Client, QoS};

/// Say one thing: at most once, never retained, never stored.
///
/// A refusal is COUNTED, not discarded (ROAD-3472): `publish_failures` in the
/// loopback telemetry, beside `messages_published`. The signature stays `()`
/// because an unkept statement is said again on the next restate anyway.
pub fn say(client: &Client, topic: impl Into<String>, payload: impl Into<Vec<u8>>) {
    crate::loopback::published(client.publish(topic.into(), QoS::AtMostOnce, false, payload.into()).is_ok());
}

/// [`say`] without waiting: `false` when the queue was full or closed (the
/// statement is dropped; the next restate says it again).
pub fn try_say(client: &Client, topic: impl Into<String>, payload: impl Into<Vec<u8>>) -> bool {
    crate::loopback::published(client.try_publish(topic.into(), QoS::AtMostOnce, false, payload.into()).is_ok())
}

/// Delete whatever an older agent KEPT on `topic`: a zero-length retained
/// payload is MQTT's delete. An unretained statement cannot overwrite a
/// retained one, so without this an old build's kept leaf answers every fresh
/// subscriber beside this agent's unkept restates.
pub fn forget(client: &Client, topic: impl Into<String>) {
    crate::loopback::published(client.publish(topic.into(), QoS::AtMostOnce, true, Vec::<u8>::new()).is_ok());
}

/// Anything [`restate`] can say again: `{topic: payload}` maps and
/// `[(topic, payload)]` lists, by reference.
pub trait Statements {
    fn each_statement(&self, said: &mut dyn FnMut(&str, &[u8]));
}

impl<S: BuildHasher> Statements for HashMap<String, String, S> {
    fn each_statement(&self, said: &mut dyn FnMut(&str, &[u8])) {
        self.iter().for_each(|(t, p)| said(t, p.as_bytes()));
    }
}

impl<S: BuildHasher> Statements for HashMap<String, Vec<u8>, S> {
    fn each_statement(&self, said: &mut dyn FnMut(&str, &[u8])) {
        self.iter().for_each(|(t, p)| said(t, p));
    }
}

impl Statements for BTreeMap<String, String> {
    fn each_statement(&self, said: &mut dyn FnMut(&str, &[u8])) {
        self.iter().for_each(|(t, p)| said(t, p.as_bytes()));
    }
}

impl Statements for BTreeMap<String, Vec<u8>> {
    fn each_statement(&self, said: &mut dyn FnMut(&str, &[u8])) {
        self.iter().for_each(|(t, p)| said(t, p));
    }
}

impl Statements for [(String, Vec<u8>)] {
    fn each_statement(&self, said: &mut dyn FnMut(&str, &[u8])) {
        self.iter().for_each(|(t, p)| said(t, p));
    }
}

impl Statements for [(String, String)] {
    fn each_statement(&self, said: &mut dyn FnMut(&str, &[u8])) {
        self.iter().for_each(|(t, p)| said(t, p.as_bytes()));
    }
}

impl<T> Statements for Vec<T>
where
    [T]: Statements,
{
    fn each_statement(&self, said: &mut dyn FnMut(&str, &[u8])) {
        self.as_slice().each_statement(said)
    }
}

impl<T: Statements + ?Sized> Statements for &T {
    fn each_statement(&self, said: &mut dyn FnMut(&str, &[u8])) {
        (**self).each_statement(said)
    }
}

/// Say every statement again, from values already in hand.
pub fn restate<S: Statements + ?Sized>(client: &Client, statements: &S) {
    statements.each_statement(&mut |topic, payload| say(client, topic, payload.to_vec()));
}

/// An agent-state account as unkept `(topic, payload)` entries. The contract's
/// retain flag is dropped on purpose: unkept, the account leaves the bus one
/// restate after the agent stops saying it. A malformed account (a bad token
/// literal — a bug in the caller) is logged under `label` and yields nothing.
pub fn publications_of(label: &str, account: Result<AgentReport, TopicError>) -> Vec<(String, Vec<u8>)> {
    match account.and_then(|a| a.publications()) {
        Ok(entries) => entries.into_iter().map(|(topic, payload, _retain)| (topic, payload)).collect(),
        Err(e) => {
            elog!("⚠️  [{label}] agent state not published: {e}");
            Vec::new()
        }
    }
}

/// The account as a problem (ROAD-3695): `unavailable` raises
/// `plugin_unavailable` about the protocol with the account's own detail,
/// `listening` clears it — `crate::report::agent_state`. The protocol and the
/// detail are read off the entries, `…/Protocols/{proto}/Agent/{leaf}`.
fn raise_for(state: Option<apkaudio_contracts::agent_state::AgentState>, entries: &[(String, Vec<u8>)]) {
    let Some(state) = state else { return };
    let leaf = |name: &str| entries.iter().find(|(t, _)| t.ends_with(&format!("/Agent/{name}")));
    let Some((topic, _)) = leaf(apkaudio_contracts::agent_state::AGENT_LEAF_STATE) else { return };
    let proto = topic.rsplit('/').nth(2).unwrap_or("agent");
    let detail = leaf(apkaudio_contracts::agent_state::AGENT_LEAF_DETAIL).map(|(_, p)| String::from_utf8_lossy(p).to_string()).unwrap_or_default();
    crate::report::agent_state(proto, state, &detail);
}

/// Say an agent-state account once; returns its entries for the restate timer.
pub fn report(client: &Client, label: &str, account: Result<AgentReport, TopicError>) -> Vec<(String, Vec<u8>)> {
    let state = account.as_ref().ok().map(AgentReport::state);
    let entries = publications_of(label, account);
    raise_for(state, &entries);
    restate(client, &entries);
    entries
}

/// The agent's FIRST account: [`forget`] what an older agent kept on these
/// leaves, then [`report`]. After that nothing here is kept, so plain
/// [`report`] / [`restate`] suffice.
pub fn report_first(client: &Client, label: &str, account: Result<AgentReport, TopicError>) -> Vec<(String, Vec<u8>)> {
    let state = account.as_ref().ok().map(AgentReport::state);
    let entries = publications_of(label, account);
    raise_for(state, &entries);
    for (topic, _) in &entries {
        forget(client, topic.clone());
    }
    restate(client, &entries);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect<S: Statements + ?Sized>(s: &S) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        s.each_statement(&mut |t, p| out.push((t.to_string(), p.to_vec())));
        out.sort();
        out
    }

    #[test]
    fn every_shape_the_copies_restated_is_a_statement_list() {
        let mut map: HashMap<String, String> = HashMap::new();
        map.insert("a/x".into(), "1".into());
        let list: Vec<(String, Vec<u8>)> = vec![("a/x".into(), b"1".to_vec())];
        let slice: &[(String, Vec<u8>)] = &list;
        assert_eq!(collect(&map), list);
        assert_eq!(collect(&list), list);
        assert_eq!(collect(&slice), list);
        let tree: BTreeMap<String, String> = map.into_iter().collect();
        assert_eq!(collect(&tree), list);
    }

    #[test]
    fn an_account_is_said_unkept() {
        let entries = publications_of("PTP", AgentReport::listening("ptp", "on eth0"));
        assert!(!entries.is_empty());
        assert!(entries.iter().any(|(t, p)| t.ends_with("/Agent/state") && p == b"listening"), "{entries:?}");
        assert!(publications_of("X", AgentReport::listening("not a token/", "x")).is_empty());
    }

    #[test]
    fn a_dead_broker_never_parks_try_say() {
        let (client, _connection) = Client::new(rumqttc::MqttOptions::new("unkept-outage-test", "127.0.0.1", 9), 1);
        let started = std::time::Instant::now();
        for _ in 0..10 {
            let _ = try_say(&client, "a/b", "x");
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
