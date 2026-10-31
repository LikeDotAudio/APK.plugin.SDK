// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Republication for the event-pump plugins: OSC, MIDI and AES70.
//!
//! MOVED HERE FROM `orchestrator/src/services/bridges.rs` BY PLAN-3316.01, with
//! the three pumps that call it. Those pumps now run as `apk-osc`, `apk-midi`
//! and `apk-aes70`, each its own container, and the two rules this file holds
//! are the ones PLAN-1120.01 and PLAN-3317.01 paid for — so they moved as they
//! were rather than being written a fourth time into three binaries.
//!
//! WHY THE RULE STILL MATTERS WITH NO WEB SERVER IN THE PROCESS. PLAN-3317.01
//! was found because a parked tokio worker took `axum::serve` down with it.
//! That process is gone from here, but the same parked worker in a plugin
//! container stops the pump, the runtime's timers AND the reader feeding it —
//! a fader moved during a broker restart would wedge the bridge until the
//! broker came back. The runner's watchdog cannot see that: nothing returned
//! and nothing panicked. So the queue still never waits.

use std::time::Instant;

/// One bridge's republication results, COUNTED RATHER THAN PRINTED.
///
/// `rumqttc::Client::try_publish` returns `Result<(), ClientError>` and fails
/// when the outgoing request channel is full or closed -- which is exactly what
/// a broker that has gone away produces. (It is `try_publish` and not `publish`
/// for a reason [`republish`] states: in rumqttc 0.24 `publish` does not fail on
/// a full queue, it WAITS on one.) Every republication site used to spell it
/// `let _ = …publish(…)`, with a `📡 ⮜ topic = payload` line printed on the row
/// ABOVE, so the bench log claimed the event was published whether or not it
/// reached the request queue. PLAN-1120.01.
///
/// A DROPPED CONTROL EVENT IS NOT WORTH STOPPING A BRIDGE FOR, AND IT IS NOT
/// WORTH A LINE EACH. These loops run at fader rate; a per-event error line is
/// the log flood `broker_backoff` exists to prevent. So this counts, and speaks
/// on two events only: the moment a healthy bridge starts failing or a failing
/// one recovers (a CHANGE is news), and once per [`REFUSAL_SPEAK_EVERY`] while
/// it stays broken.
///
/// [`REFUSAL_SPEAK_EVERY`]: apkaudio_contracts::broker_backoff::REFUSAL_SPEAK_EVERY
pub struct PublishTally {
    /// What the bridge calls itself in a log line -- `OSC`, `MIDI`, `AES70`.
    label: &'static str,
    /// Failures in the current unbroken run of them. Reset by a success.
    failing: u64,
    /// Failures since the last line this tally printed, so a quiet log still
    /// carries the true count when it finally speaks.
    suppressed: u64,
    /// When it last printed. `None` means it has not printed in this run.
    spoke_at: Option<Instant>,
}

impl PublishTally {
    pub fn new(label: &'static str) -> Self {
        Self { label, failing: 0, suppressed: 0, spoke_at: None }
    }

    /// Failures in the current unbroken run.
    pub fn failing(&self) -> u64 {
        self.failing
    }

    /// Record one publish result. Returns whether it reached the queue, so the
    /// caller can print its `📡 ⮜` line for a republication that HAPPENED and
    /// not for one it merely attempted.
    pub fn record<E: std::fmt::Display>(&mut self, topic: &str, result: Result<(), E>) -> bool {
        self.record_at(topic, result, Instant::now())
    }

    /// `record`, with the clock supplied. The tests drive the throttle through
    /// this: an `Instant` cannot be advanced, and a schedule nothing can move
    /// is a schedule nothing can test.
    pub fn record_at<E: std::fmt::Display>(
        &mut self, topic: &str, result: Result<(), E>, now: Instant,
    ) -> bool {
        match result {
            Ok(()) => {
                if self.failing > 0 {
                    // THE RECOVERY IS NEWS TOO. A bridge that was told it was
                    // dropping events and never told it had stopped has to be
                    // polled to find out.
                    println!("   📡 [{} MQTT] ✅ publishing again — {} event(s) were dropped",
                             self.label, self.failing);
                    self.failing = 0;
                    self.suppressed = 0;
                    self.spoke_at = None;
                }
                true
            }
            Err(why) => {
                self.failing += 1;
                let due = self.spoke_at.map_or(true, |at| {
                    now.duration_since(at)
                        >= apkaudio_contracts::broker_backoff::REFUSAL_SPEAK_EVERY
                });
                if due {
                    let also = if self.suppressed > 0 {
                        format!(" (+{} more since the last line)", self.suppressed)
                    } else {
                        String::new()
                    };
                    eprintln!("   📡 [{} MQTT] ❌ {topic} not published — {why}{also}",
                              self.label);
                    self.suppressed = 0;
                    self.spoke_at = Some(now);
                } else {
                    self.suppressed += 1;
                }
                false
            }
        }
    }
}

/// Put one event on a bridge's MQTT request queue WITHOUT WAITING, and count
/// the result. Returns what [`PublishTally::record`] returns.
///
/// `try_publish`, NEVER `publish`. In rumqttc 0.24 `Client::publish` is a
/// blocking flume `send`: when the broker is gone the drain thread sits in
/// `broker_backoff` for up to a minute, the queue (capacity 10) is full after
/// ten events, and the next `publish` parks THE THREAD IT IS CALLED ON — a
/// tokio worker, in all three pumps. A full queue is a dropped control event,
/// which is what the tally above always said it was counting. PLAN-3317.01;
/// `outage_tests::a_dead_broker_never_parks_the_pump` holds it.
pub fn republish(client: &rumqttc::Client, tally: &mut PublishTally, topic: &str, payload: &str) -> bool {
    let sent = client.try_publish(topic, rumqttc::QoS::AtLeastOnce, false, payload.as_bytes());
    tally.record(topic, sent)
}

/// A publisher client for a pump — the shape all three bridges had. The
/// caller drains the connection (on a plain thread, paced by `broker_backoff`).
///
/// `id` names the pump; the client id is [`crate::session::client_id`]'s, unique
/// per node and process, for the reason the runner's session gives: MQTT
/// silently drops the older of two sessions with one client id.
pub fn publisher(id: &str, host: &str, port: u16) -> (rumqttc::Client, rumqttc::Connection) {
    crate::session::Session::new(id, "pump").to(host, port).capacity(10).connect()
}

#[cfg(test)]
mod publish_tally_tests {
    use super::PublishTally;
    use apkaudio_contracts::broker_backoff::REFUSAL_SPEAK_EVERY;
    use std::time::{Duration, Instant};

    /// A failed publish is not a published event, and the caller must be able
    /// to tell.
    #[test]
    fn success_and_failure_are_distinguishable_to_the_caller() {
        let mut tally = PublishTally::new("TEST");
        let now = Instant::now();
        assert!(tally.record_at("t/ok", Ok::<(), String>(()), now));
        assert!(!tally.record_at("t/bad", Err::<(), String>("queue full".into()), now));
    }

    /// THE FLOOD THIS EXISTS TO PREVENT. A fader-rate loop against a dead
    /// broker must produce ONE line, not one per event.
    #[test]
    fn a_run_of_failures_speaks_once_and_counts_the_rest() {
        let mut tally = PublishTally::new("TEST");
        let now = Instant::now();
        for _ in 0..10_000 {
            assert!(!tally.record_at("t", Err::<(), String>("gone".into()), now));
        }
        assert_eq!(tally.failing, 10_000, "every failure is counted");
        assert_eq!(tally.suppressed, 9_999, "and 9,999 of them were not printed");
        assert!(tally.spoke_at.is_some(), "the first one was");
    }

    /// Quiet, but not forever: the schedule is `broker_backoff`'s.
    #[test]
    fn it_speaks_again_when_the_schedule_comes_round() {
        let mut tally = PublishTally::new("TEST");
        let start = Instant::now();
        tally.record_at("t", Err::<(), String>("gone".into()), start);
        let first = tally.spoke_at.expect("the first failure speaks");

        tally.record_at("t", Err::<(), String>("gone".into()),
                        start + REFUSAL_SPEAK_EVERY - Duration::from_millis(1));
        assert_eq!(tally.spoke_at, Some(first), "inside the quiet stretch, still quiet");
        assert_eq!(tally.suppressed, 1);

        tally.record_at("t", Err::<(), String>("gone".into()), start + REFUSAL_SPEAK_EVERY);
        assert_ne!(tally.spoke_at, Some(first), "the schedule came round");
        assert_eq!(tally.suppressed, 0, "and the suppressed count went into that line");
    }

    /// The recovery resets the throttle too, so the NEXT outage is news
    /// immediately.
    #[test]
    fn recovery_is_news_and_rearms_the_throttle() {
        let mut tally = PublishTally::new("TEST");
        let now = Instant::now();
        tally.record_at("t", Err::<(), String>("gone".into()), now);
        tally.record_at("t", Err::<(), String>("gone".into()), now);
        assert_eq!(tally.failing, 2);

        assert!(tally.record_at("t", Ok::<(), String>(()), now));
        assert_eq!(tally.failing, 0);
        assert_eq!(tally.suppressed, 0);
        assert_eq!(tally.spoke_at, None, "the next outage speaks at once");
    }
}

/// THE OUTAGE. PLAN-3317.01, carried out of the orchestrator with the pumps.
#[cfg(test)]
mod outage_tests {
    use super::{republish, PublishTally};
    use std::time::{Duration, Instant};

    /// A broker that is not there, 1,000 events through the same `republish`
    /// the three pumps call, and every one of them accounted for in well under
    /// a second.
    ///
    /// The orchestrator's version watched `/api/health` answer beside the pump;
    /// there is no web server in a plugin container, so what is measured is the
    /// thing that server was the witness for — the calling thread is never
    /// parked. Against the blocking `publish` this test does not fail, it
    /// HANGS on event eleven, so the loop runs on its own thread and the
    /// verdict is a join with a deadline.
    ///
    /// The connection is held and never iterated. That is what the production
    /// drain thread amounts to while it sleeps in `broker_backoff`: a live
    /// request channel with nobody taking from it. Dropping it instead would
    /// close the channel, and a closed channel fails fast under either call.
    #[test]
    fn a_dead_broker_never_parks_the_pump() {
        let dead = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let dead_port = dead.local_addr().expect("its address").port();
        drop(dead);
        let options = rumqttc::MqttOptions::new("apk-plugin-runner-outage-test", "127.0.0.1", dead_port);
        // Capacity 10, the number the three bridges use.
        let (client, connection) = rumqttc::Client::new(options, 10);

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut tally = PublishTally::new("OUTAGE TEST");
            let mut queued = 0u64;
            for n in 0..1_000u32 {
                if republish(&client, &mut tally, &format!("APK.audio/Test/Outage/{n}"), &n.to_string()) {
                    queued += 1;
                }
            }
            let _ = done_tx.send((queued, tally.failing()));
        });

        let asked = Instant::now();
        let (queued, failed) = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("1,000 republications did not finish in 5 s -- the pump is parked on a full MQTT queue");
        assert!(asked.elapsed() < Duration::from_secs(1), "took {:?}", asked.elapsed());
        assert!(queued <= 10, "{queued} events queued past a capacity of 10");
        assert_eq!(queued + failed, 1_000, "queued {queued} + dropped {failed} is not every event");
        drop(connection);
    }
}
