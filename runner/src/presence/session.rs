// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Bringing presence up: the MQTT session thread that reads back on every
//! connect, and the one expiry tick. Nothing here decides what to say.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use apkaudio_contracts::device_publish::Publication;
use rumqttc::{Client, Connection, Event, MqttOptions, Packet, QoS};

use super::shared::{forget_said, now_ms, say, unsaid, with, Shared};
use super::sweep;
use super::{READBACK, TICK};

/// Start the presence session and the one expiry tick for this process.
///
/// Called by [`crate::serve_group`] for a plugin whose `config.ini` declares
/// `presence_protocols`, and directly by a plugin not built on the runner.
/// Idempotent: a second call does nothing.
pub fn start(token: &str, host: &str, port: u16, protocols: Vec<String>, inactive_after: Option<Duration>) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }

    let mut options = MqttOptions::new(format!("apk-presence-{token}-{}", std::process::id()), host, port);
    options.set_keep_alive(Duration::from_secs(30));
    if let Some((username, password)) = apkaudio_contracts::broker_auth::credentials() {
        options.set_credentials(username, password);
    }
    let (client, connection) = Client::new(options, 256);
    with(|s| {
        // Sightings before `start` were held by the booting tracker; keep them,
        // and give the tracker the clock it should have had.
        s.tracker.set_inactive_after(inactive_after);
        s.protocols = protocols.clone();
        s.token = token.to_string();
        s.client = Some(client.clone());
    });
    println!(
        "🛰️  [{token}] presence: {} — offline after {}",
        protocols.join(", "),
        inactive_after.map(|d| format!("{}s unseen", d.as_secs())).unwrap_or_else(|| "a departure".into())
    );

    let label = format!("{token}:presence");
    std::thread::Builder::new()
        .name(label.clone())
        .spawn(move || run_session(client, connection, protocols))
        .expect("the presence session starts");

    std::thread::Builder::new()
        .name(format!("{label}:tick"))
        .spawn(move || loop {
            std::thread::sleep(TICK);
            tick_once();
        })
        .expect("the presence tick starts");
}

/// The session: on every ConnAck open a read-back window and subscribe to this
/// plugin's own retained documents; feed the retained replay to the tracker.
fn run_session(client: Client, mut connection: Connection, protocols: Vec<String>) {
    let mut backoff = apkaudio_contracts::broker_backoff::Backoff::new();
    let filters: Vec<String> = protocols
        .iter()
        .filter_map(|p| apkaudio_contracts::topics::discovery_wildcard(Some(p)).ok())
        .collect();
    for event in connection.iter() {
        let Some(event) = apkaudio_contracts::broker_backoff::step(
            "presence",
            &mut backoff,
            event,
            |e| matches!(e, rumqttc::ConnectionError::ConnectionRefused(_)),
        ) else {
            continue;
        };
        match event {
            Event::Incoming(Packet::ConnAck(_)) => {
                with(connected);
                for f in filters.iter().cloned().chain(sweep::link_filters()) {
                    let _ = client.try_subscribe(f, QoS::AtLeastOnce);
                }
            }
            // Only the retained replay is read back. A live publish on
            // these topics is this process's own write coming home.
            Event::Incoming(Packet::Publish(p)) if p.retain => {
                if sweep::link_prefix(&p.topic).is_some() {
                    with(|s| sweep::found(s, &p.topic, &p.payload));
                } else {
                    with(|s| s.tracker.restore(&p.topic, &p.payload));
                }
            }
            _ => {}
        }
    }
}

/// A ConnAck: open a read-back window, and forget what the last connection
/// said under the hunters' prefixes so the window's close says it again.
pub(super) fn connected(s: &mut Shared) {
    s.tracker.begin_readback();
    s.connected_at = Some(Instant::now());
    forget_said(s);
}

/// One wake of the tick: close a read-back window that has run its course,
/// then expire whatever is past its clock, and say it all outside the lock.
fn tick_once() {
    let (client, out) = with(|s| (s.client.clone(), ticked(s, now_ms())));
    say(client, out);
}

/// [`tick_once`] without the lock or the bus.
pub(super) fn ticked(s: &mut Shared, now: i64) -> Vec<Publication> {
    let mut out = Vec::new();
    if s.tracker.reading_back() && s.connected_at.is_some_and(|at| at.elapsed() >= READBACK) {
        out = s.tracker.finish_readback(now);
        out.extend(sweep::swept(s, now));
        if let Some(client) = &s.client {
            for p in &s.protocols {
                if let Ok(f) = apkaudio_contracts::topics::discovery_wildcard(Some(p)) {
                    let _ = client.try_unsubscribe(f);
                }
            }
            for f in sweep::link_filters() {
                let _ = client.try_unsubscribe(f);
            }
        }
        let (online, offline) = s.tracker.counts();
        println!(
            "🛰️  [{}] presence read back: {online} online, {offline} offline; {} written",
            s.token,
            out.len()
        );
        out.extend(unsaid(s));
    }
    let expired = s.tracker.expire(now);
    for p in &expired {
        println!("⚪ [{}] {} offline (not seen)", s.token, p.topic);
    }
    out.extend(sweep::expired(s, &expired));
    out.extend(expired);
    out.extend(sweep::unvouched(s, now));
    out
}
