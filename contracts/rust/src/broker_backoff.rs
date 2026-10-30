// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! WHAT A REFUSED CONNECT COSTS — the one place a reconnect is paced.
//!
//! `broker_auth` decides who an agent is to the broker. This module decides
//! what happens when the broker says no, and it exists because the answer used
//! to be "immediately, forever".
//!
//! Measured on this bench 2026-08-30: an orchestrator started without
//! `.rig/broker.env` sourced against a broker with `allow_anonymous false`.
//! Every one of its ~20 agent threads connected, was refused, disconnected and
//! reconnected inside the same millisecond. `.rig/mosquitto.log` reached
//! **366 GiB growing at 24 GB/hour**, mosquitto held 97.5% of a core, and the
//! disk had nine hours left. Nothing was broken; nothing was retrying wrong in
//! a way any single agent could see. Twenty agents each doing the obvious
//! thing is what filled the disk.
//!
//! THE DISTINCTION THAT MATTERS: a refused CONNACK is not a transient fault.
//! A broker that is down comes back, and an agent should be beating at it when
//! it does — that is why the drain loops never break out. A broker that
//! *refused these credentials* will refuse them on the ten-millionth attempt
//! too, because the fault is in this process's environment and no amount of
//! retrying changes it. So the two back off on different curves, and the
//! refusal says so out loud on a schedule a human can read.
//!
//! NO CLIENT LIBRARY HERE, by the crate's hard rule (see Cargo.toml). [`drain`]
//! is generic over any `Iterator<Item = Result<T, E>>`, which is the shape
//! `rumqttc::Connection::iter()` happens to have — so the policy lives here
//! once and the transport stays in the agents, exactly as `broker_auth` splits
//! it. The caller supplies `is_refusal`, because only the caller knows its own
//! error type.
//!
//! NO ASYNC RUNTIME HERE EITHER, AND THE ASYNC HALF STILL LIVES HERE. [`step`]
//! and [`drain`] end in `std::thread::sleep`, which is exactly right for the 35
//! blocking `rumqttc::Connection::iter()` loops they replaced and catastrophic
//! inside a `tokio::spawn`, where that sleep holds a worker thread carrying
//! other agents' futures. The obvious fix — a `step_async` awaiting
//! `tokio::time::sleep` — would need `tokio` in this crate, and Cargo.toml
//! refuses it. So [`wait_out`] takes the TIMER as an argument: the caller hands
//! in `tokio::time::sleep`, this crate names no runtime, gains no dependency,
//! and the pacing decision still lives in one place. `async fn` and
//! `std::future::Future` are core Rust; neither is a dependency. PLAN-1019.01.
//!
//! `is_refusal` IS THE CONFIGURATION-VERSUS-NETWORK AXIS, NOT THE
//! MQTT-VERSUS-ANYTHING-ELSE AXIS, and callers on other transports read it that
//! way rather than re-deriving it. A TCP `ConnectionRefused` — nothing is
//! listening on that port at all — is a refusal: it is a fact about how this
//! bench is configured and it will not fix itself while the agent beats on it.
//! A read that failed mid-session, a clean remote close, a subsystem that is
//! merely empty: transient. What the two curves separate is "retrying cannot
//! help" from "retrying is the whole plan", and that question is the same on
//! every transport.

use std::future::Future;
use std::time::Duration;

/// First wait after a transient failure. Short enough that a broker restart is
/// picked up without anyone noticing the gap.
pub const TRANSIENT_FIRST: Duration = Duration::from_millis(250);

/// Ceiling for a transient failure. A broker that has been down a while is
/// retried every few seconds, not every few milliseconds.
pub const TRANSIENT_CEILING: Duration = Duration::from_secs(5);

/// First wait after a refusal. Already two orders of magnitude above the
/// transient curve: a refusal is a configuration error, and the only thing
/// retrying it fast achieves is filling a log.
pub const REFUSAL_FIRST: Duration = Duration::from_secs(2);

/// Ceiling for a refusal. One attempt a minute is enough to notice that
/// somebody has fixed the environment and restarted the broker, and it is
/// 15,000 times slower than what filled 366 GiB.
pub const REFUSAL_CEILING: Duration = Duration::from_secs(60);

/// How often a persisting refusal is allowed to speak. It must be loud enough
/// to find and quiet enough to keep — the failure this replaces produced
/// 15,187 identical lines in two megabytes, which is the same thing as
/// producing none.
pub const REFUSAL_SPEAK_EVERY: Duration = Duration::from_secs(60);

/// How often a persisting TRANSIENT failure is allowed to speak.
///
/// The same minute the refusal axis gets, and reached a different way. A
/// refusal's own wait grows to `REFUSAL_CEILING` (60 s), so `wait >=
/// REFUSAL_SPEAK_EVERY` IS "about once a minute" up there. The transient
/// ceiling is 5 s, so that test can never be true on this curve — it is why
/// the transient axis said nothing at all rather than saying it slowly. The
/// waits are accumulated instead, and the line comes when a minute of them has
/// gone by, wherever on the curve that happens to be. PLAN-1118.01.
pub const TRANSIENT_SPEAK_EVERY: Duration = Duration::from_secs(60);

/// The paced retry state for ONE connection.
///
/// Deliberately not `Copy` and deliberately owned by the loop that drives the
/// connection: two connections that shared a backoff would slow each other
/// down for faults neither of them had.
#[derive(Debug, Clone)]
pub struct Backoff {
    consecutive_refusals: u32,
    consecutive_transients: u32,
    /// Failures suppressed since the last time this backoff spoke. Reported in
    /// the next line it prints, so a quiet log still carries the true count.
    suppressed: u64,
    /// Paced waiting accumulated on the TRANSIENT curve since it last spoke.
    ///
    /// A clock, kept without one. This module takes no `Instant` on purpose —
    /// it is pure, so a test can drive a six-hour outage in a microsecond — and
    /// the waits it hands out are the only measure of elapsed time it is
    /// entitled to. They are also the right one: what matters is how long the
    /// caller has been quiet, and the caller is quiet for exactly as long as
    /// this module told it to be. PLAN-1118.01.
    transient_since_spoke: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    pub fn new() -> Self {
        Self {
            consecutive_refusals: 0,
            consecutive_transients: 0,
            suppressed: 0,
            transient_since_spoke: Duration::ZERO,
        }
    }

    /// A packet moved. The connection works, so every curve resets.
    ///
    /// Reset on the FIRST success rather than after a settling period: an agent
    /// that has just reconnected is the normal case, and making it carry a
    /// penalty from a broker restart it survived would be a bug wearing
    /// caution's clothes.
    pub fn on_success(&mut self) {
        self.consecutive_refusals = 0;
        self.consecutive_transients = 0;
        self.suppressed = 0;
        self.transient_since_spoke = Duration::ZERO;
    }

    /// A connect attempt failed. Returns how long to wait before letting the
    /// client try again, and — when it is time to say so — the line to print.
    ///
    /// `refused` is the caller's classification of its own error: true for a
    /// CONNACK that named a reason (not authorised, bad credentials, client id
    /// rejected), false for anything that might fix itself.
    pub fn on_failure(&mut self, refused: bool) -> Retry {
        if refused {
            self.consecutive_transients = 0;
            self.consecutive_refusals = self.consecutive_refusals.saturating_add(1);
            let wait = exponential(REFUSAL_FIRST, REFUSAL_CEILING, self.consecutive_refusals);
            // Speak on the first refusal, then once per REFUSAL_SPEAK_EVERY.
            // At the ceiling that is one line a minute; before the ceiling the
            // waits are short, so the modulo below is what throttles them.
            let speak = self.consecutive_refusals == 1 || wait >= REFUSAL_SPEAK_EVERY;
            if speak {
                let suppressed = std::mem::take(&mut self.suppressed);
                Retry {
                    wait,
                    should_speak: true,
                    suppressed,
                    message: Some(refusal_message(self.consecutive_refusals, suppressed, wait)),
                }
            } else {
                self.suppressed = self.suppressed.saturating_add(1);
                Retry { wait, should_speak: false, suppressed: 0, message: None }
            }
        } else {
            self.consecutive_refusals = 0;
            self.consecutive_transients = self.consecutive_transients.saturating_add(1);
            let wait = exponential(TRANSIENT_FIRST, TRANSIENT_CEILING, self.consecutive_transients);
            self.transient_since_spoke = self.transient_since_spoke.saturating_add(wait);
            // SPEAK ON A SCHEDULE HERE TOO. This arm used to return
            // `should_speak: false` unconditionally, with the reason that "a
            // broker being down is ordinary and the agent's own logging already
            // says it is not connected". That was true of the MQTT drain loops
            // that were the only callers when it was written, and false of
            // every async agent added since: they say what the reason IS and
            // then let this module decide when to repeat it, and on this axis
            // the decision was permanently no. So a peer that keeps ending
            // sessions the same way, a MIDI port list that stays empty, and a
            // `recv_from` that keeps failing each printed one line and then
            // nothing, for ever — and a bench quietly broken for six hours read
            // exactly like a bench that had been fine for six hours.
            //
            // `message` STAYS `None`, and that is not an oversight. The only
            // sentence this module owns names `.rig/broker.env` and two MQTT
            // credential variables, which is a lie in a UDP listener and in a
            // port enumerator. The caller has the true reason; what it lacked
            // was permission to repeat it, and `suppressed` to say how many it
            // stands for. PLAN-1118.01.
            let speak = self.consecutive_transients == 1
                || self.transient_since_spoke >= TRANSIENT_SPEAK_EVERY;
            if speak {
                self.transient_since_spoke = Duration::ZERO;
                let suppressed = std::mem::take(&mut self.suppressed);
                Retry { wait, should_speak: true, suppressed, message: None }
            } else {
                self.suppressed = self.suppressed.saturating_add(1);
                Retry { wait, should_speak: false, suppressed: 0, message: None }
            }
        }
    }

    /// Refusals seen in a row. Zero once anything succeeds.
    pub fn consecutive_refusals(&self) -> u32 {
        self.consecutive_refusals
    }
}

/// What the caller should do about one failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retry {
    /// Sleep this long before allowing the next connect attempt.
    pub wait: Duration,
    /// THE THROTTLE DECISION, WITH NO PROSE ATTACHED: is this the failure that
    /// gets to be a line, or is it inside the quiet stretch?
    ///
    /// Separate from [`message`](Self::message) because they are separate
    /// facts and only one of them is about MQTT. `message` names
    /// `.rig/broker.env` and two broker credential variables, which is a lie in
    /// a TCP or MIDI client — so a non-broker caller supplies its own sentence
    /// and needs only the schedule. Until PLAN-1019.01 the only way to read the
    /// schedule was `message.is_some()`, i.e. every such caller read a field
    /// for a meaning it does not carry.
    ///
    /// True on BOTH curves now. It used to be false on the transient one
    /// always, which meant a transient condition that never changed was
    /// reported exactly once and then never again — see the transient arm of
    /// [`Backoff::on_failure`]. PLAN-1118.01.
    pub should_speak: bool,
    /// How many identical failures this report stands for, beyond itself.
    ///
    /// Zero when this failure was throttled (there is no line to carry a
    /// count), and zero on a line that follows another line. Non-zero only on
    /// the line that ends a quiet stretch, where it is the whole point: a
    /// caller printing one sentence a minute is otherwise indistinguishable
    /// from a caller failing once a minute. [`suppressed_note`](Self::suppressed_note)
    /// renders it. PLAN-1118.01.
    pub suppressed: u64,
    /// Print this, if present. Absent means the failure was throttled, OR that
    /// the failure is on the transient curve, where this module has no
    /// truthful sentence to offer — not that it did not happen. Only ever
    /// present when [`should_speak`](Self::should_speak) is true.
    pub message: Option<String>,
}

impl Retry {
    /// Is this failure worth a line, given what was last said out loud?
    ///
    /// THE SPEAK-OR-STAY-QUIET RULE, for the callers that supply their own
    /// sentence. Two different things earn a line and conflating them loses
    /// one of them: the backoff's own throttle has come round
    /// ([`should_speak`](Self::should_speak)), or the reason is not the one
    /// last reported — a condition that has just CHANGED is news even inside
    /// the quiet stretch, and a condition merely persisting is not.
    ///
    /// `last_reason` is the caller's latch and is advanced here, so a loop
    /// cannot report the change and then forget to record it. Hand it a
    /// `None` that lives across iterations of the supervision loop.
    ///
    /// It lived hand-rolled in `Aes70Agent::start` and in two shapes in
    /// `apkaudio-midi`; three copies of a throttle is how a throttle drifts.
    /// PLAN-1019.01.
    pub fn is_news(&self, reason: &str, last_reason: &mut Option<String>) -> bool {
        let changed = last_reason.as_deref() != Some(reason);
        if changed {
            *last_reason = Some(reason.to_string());
        }
        self.should_speak || changed
    }

    /// The suppressed count as a clause to hang off a caller's own sentence, or
    /// `""` when there is nothing to report.
    ///
    /// Here rather than at each call site so the three agents that use the
    /// transient curve cannot word the same fact three ways — the reason
    /// [`is_news`](Self::is_news) is here too. PLAN-1118.01.
    pub fn suppressed_note(&self) -> String {
        if self.suppressed == 0 {
            String::new()
        } else {
            format!(" ({} identical since the last line)", self.suppressed)
        }
    }
}

/// Wait out a paced [`Retry`] WITHOUT BLOCKING THE THREAD — the async half of
/// [`step`], for a loop living on a `tokio::spawn`.
///
/// THE TIMER IS AN ARGUMENT, and that is the whole design. This crate's
/// Cargo.toml bans `tokio` and every other async dependency, so the module
/// cannot name a runtime; it can perfectly well take one. Callers pass
/// `tokio::time::sleep`, which is `fn(Duration) -> Sleep`, and the pacing
/// decision stays in this file instead of being retyped beside every
/// supervision loop.
///
/// ```ignore
/// let retry = backoff.on_failure(refused);
/// if retry.is_news(&reason, &mut last_reason) {
///     eprintln!("🔊⚙️ [AES70 AGENT] {reason} — reconnecting in {:?}", retry.wait);
/// }
/// broker_backoff::wait_out(&retry, tokio::time::sleep).await;
/// ```
///
/// Do NOT reach for [`step`] or [`drain`] from an async task: they end in
/// `std::thread::sleep`, and a 60-second refusal ceiling taken on a tokio
/// worker stalls every other future that worker is carrying for a minute.
/// PLAN-1019.01.
pub async fn wait_out<S, F>(retry: &Retry, sleep: S)
where
    S: FnOnce(Duration) -> F,
    F: Future<Output = ()>,
{
    sleep(retry.wait).await;
}

fn refusal_message(attempt: u32, suppressed: u64, wait: Duration) -> String {
    let also = if suppressed > 0 {
        format!(" ({suppressed} identical refusals suppressed since the last line)")
    } else {
        String::new()
    };
    format!(
        "the broker REFUSED this connection — attempt {attempt}{also}. \
         This is a configuration fault, not a network one: check {} and {} in \
         this process's environment (source .rig/broker.env). Retrying in {:?}.",
        crate::broker_auth::USERNAME_VARIABLE,
        crate::broker_auth::PASSWORD_VARIABLE,
        wait
    )
}

/// `first * 2^(n-1)`, clamped to `ceiling`, saturating rather than wrapping.
///
/// `n` is the attempt number and is 1-based, so the first failure waits
/// exactly `first`. Written with `u32::pow` on the multiplier rather than on
/// the Duration so a long outage cannot overflow into a short wait — which is
/// the failure mode that would turn a backoff back into a hot loop.
fn exponential(first: Duration, ceiling: Duration, attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(32);
    let factor = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    let millis = (first.as_millis() as u64).saturating_mul(factor);
    Duration::from_millis(millis).min(ceiling)
}

/// One turn of a HAND-WRITTEN event loop: the event, or a paced failure.
///
/// For the loops that do real work with what they receive and therefore cannot
/// hand their body to [`drain`] as a closure — the ones shaped
/// `for event in connection.iter() { let Ok(..) = event else { continue }; … }`.
/// That `else { continue }` is the same defect as the bare drain wearing a
/// different sweater: it discards the error and asks the iterator for the next
/// one, which reconnects, immediately.
///
/// Returns `Some(event)` to be matched as before, or `None` after it has
/// already slept for the failure. Callers `continue` on `None`.
pub fn step<T, E>(
    agent: &str,
    backoff: &mut Backoff,
    event: Result<T, E>,
    is_refusal: impl Fn(&E) -> bool,
) -> Option<T> {
    match event {
        Ok(item) => {
            backoff.on_success();
            Some(item)
        }
        Err(error) => {
            let retry = backoff.on_failure(is_refusal(&error));
            if let Some(message) = retry.message {
                eprintln!("🔒 [{agent}] {message}");
            }
            std::thread::sleep(retry.wait);
            None
        }
    }
}

/// Drive a broker event iterator forever, pacing every reconnect.
///
/// This replaces the `for _ in connection.iter() {}` that appeared in 35 places
/// in this repository. That loop is correct about one thing and catastrophic
/// about another: it is right that the loop must never break — the iterator
/// yields reconnection attempts and a broker that is down is one this agent
/// should still be beating at — and it is wrong that it consumes a refusal at
/// whatever rate the kernel can open sockets.
///
/// It never returns. `is_refusal` classifies the caller's own error type;
/// `on_event` sees every successful event and is `|_| {}` for a pure drain.
pub fn drain<T, E>(
    agent: &str,
    events: impl Iterator<Item = Result<T, E>>,
    is_refusal: impl Fn(&E) -> bool,
    mut on_event: impl FnMut(T),
) {
    let mut backoff = Backoff::new();
    for event in events {
        if let Some(item) = step(agent, &mut backoff, event, &is_refusal) {
            on_event(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_starts_seconds_apart_not_milliseconds() {
        // The whole defect in one assertion: the first refused connect must
        // not come back round inside the same millisecond.
        let mut backoff = Backoff::new();
        let first = backoff.on_failure(true);
        assert_eq!(first.wait, REFUSAL_FIRST);
        assert!(first.wait >= Duration::from_secs(1));
    }

    #[test]
    fn a_refusal_reaches_the_ceiling_and_stays_there() {
        let mut backoff = Backoff::new();
        let mut last = Duration::ZERO;
        for _ in 0..64 {
            last = backoff.on_failure(true).wait;
        }
        assert_eq!(last, REFUSAL_CEILING);
    }

    #[test]
    fn the_first_refusal_speaks_and_the_flood_does_not() {
        let mut backoff = Backoff::new();
        assert!(backoff.on_failure(true).message.is_some(), "the first refusal must say so");
        // The next few are inside the ramp and must be silent.
        let quiet = (0..4).filter(|_| backoff.on_failure(true).message.is_none()).count();
        assert_eq!(quiet, 4, "a ramping refusal must not print every attempt");
    }

    #[test]
    fn a_suppressed_count_is_carried_into_the_next_line() {
        let mut backoff = Backoff::new();
        backoff.on_failure(true); // speaks
        for _ in 0..3 {
            backoff.on_failure(true); // suppressed
        }
        // Walk to the ceiling, where it speaks again.
        let mut message = None;
        for _ in 0..64 {
            if let Some(m) = backoff.on_failure(true).message {
                message = Some(m);
                break;
            }
        }
        let message = message.expect("a persisting refusal must speak again");
        assert!(message.contains("suppressed"), "got: {message}");
    }

    #[test]
    fn a_refusal_names_the_variables_an_operator_has_to_set() {
        let mut backoff = Backoff::new();
        let message = backoff.on_failure(true).message.unwrap();
        assert!(message.contains("APK_MQTT_USERNAME"));
        assert!(message.contains("APK_MQTT_PASSWORD"));
        assert!(message.contains("broker.env"));
    }

    #[test]
    fn a_transient_fault_backs_off_far_more_gently() {
        // A broker restart must not cost the agent a minute of silence.
        let mut backoff = Backoff::new();
        assert_eq!(backoff.on_failure(false).wait, TRANSIENT_FIRST);
        assert!(TRANSIENT_CEILING < REFUSAL_CEILING);
    }

    #[test]
    fn a_transient_fault_is_not_announced() {
        let mut backoff = Backoff::new();
        for _ in 0..20 {
            assert!(backoff.on_failure(false).message.is_none());
        }
    }

    #[test]
    fn one_success_clears_the_penalty() {
        let mut backoff = Backoff::new();
        for _ in 0..10 {
            backoff.on_failure(true);
        }
        assert!(backoff.consecutive_refusals() > 0);
        backoff.on_success();
        assert_eq!(backoff.consecutive_refusals(), 0);
        // And the next failure starts from the bottom of the curve again.
        assert_eq!(backoff.on_failure(true).wait, REFUSAL_FIRST);
    }

    #[test]
    fn the_two_curves_do_not_contaminate_each_other() {
        let mut backoff = Backoff::new();
        for _ in 0..8 {
            backoff.on_failure(true);
        }
        // A transient failure after refusals is a different fault: it must not
        // inherit the refusal's minute-long wait.
        assert_eq!(backoff.on_failure(false).wait, TRANSIENT_FIRST);
    }

    #[test]
    fn a_very_long_outage_cannot_overflow_back_into_a_hot_loop() {
        // The regression that would silently restore the original defect.
        assert_eq!(exponential(REFUSAL_FIRST, REFUSAL_CEILING, u32::MAX), REFUSAL_CEILING);
        assert_eq!(exponential(TRANSIENT_FIRST, TRANSIENT_CEILING, u32::MAX), TRANSIENT_CEILING);
    }

    #[test]
    fn drain_paces_a_refusing_broker_instead_of_spinning() {
        // Twelve refusals through the real entry point. Unpaced this is what
        // wrote 366 GiB; paced, the first two waits alone exceed it.
        let events = std::iter::repeat_with(|| Err::<(), &str>("not authorised")).take(3);
        let started = std::time::Instant::now();
        drain("test", events, |_| true, |_| {});
        assert!(
            started.elapsed() >= REFUSAL_FIRST * 3,
            "drain returned in {:?} — it did not pace the refusals",
            started.elapsed()
        );
    }

    #[test]
    fn step_hands_back_the_event_and_swallows_only_the_failure() {
        let mut backoff = Backoff::new();
        assert_eq!(step("test", &mut backoff, Ok::<u8, &str>(7), |_| false), Some(7));
        assert_eq!(step("test", &mut backoff, Err::<u8, &str>("x"), |_| false), None);
    }

    #[test]
    fn step_paces_a_refusal_the_same_way_drain_does() {
        let mut backoff = Backoff::new();
        let started = std::time::Instant::now();
        assert_eq!(step("test", &mut backoff, Err::<u8, &str>("no"), |_| true), None);
        assert!(started.elapsed() >= REFUSAL_FIRST);
    }

    #[test]
    fn the_throttle_decision_is_readable_without_the_mqtt_sentence() {
        // The defect step 2 of PLAN-1019.01 names: a TCP or MIDI caller wanting
        // the SCHEDULE had to read `message.is_some()`, a field whose contents
        // are a lie about its transport.
        let mut backoff = Backoff::new();
        let first = backoff.on_failure(true);
        assert!(first.should_speak, "the first refusal is the one that speaks");
        assert_eq!(first.should_speak, first.message.is_some());
        let quiet = backoff.on_failure(true);
        assert!(!quiet.should_speak);
        assert_eq!(quiet.should_speak, quiet.message.is_some());
    }

    /// THE PLAN'S CLOSE CONDITION. A transient that persists across a minute of
    /// the paced curve gets a second line, and it says how many it stands for.
    ///
    /// This test used to be `a_transient_failure_never_claims_the_floor` and
    /// asserted `!should_speak` twenty times in a row — the behaviour
    /// PLAN-1118.01 exists to overturn. One line and then silence for ever is
    /// how a bench broken for six hours reads exactly like a bench that has
    /// been fine for six hours.
    #[test]
    fn a_transient_that_persists_across_a_minute_speaks_again_and_says_how_many() {
        let mut backoff = Backoff::new();

        let first = backoff.on_failure(false);
        assert!(first.should_speak, "the first transient is the one that speaks");
        assert_eq!(first.suppressed, 0, "nothing has been swallowed yet");

        // Walk the curve until it speaks again, accumulating what it told the
        // caller to wait — the same clock the module itself keeps.
        let mut waited = Duration::ZERO;
        let mut quiet = 0u64;
        let second = loop {
            let retry = backoff.on_failure(false);
            waited = waited.saturating_add(retry.wait);
            if retry.should_speak {
                break retry;
            }
            quiet += 1;
            assert_eq!(retry.suppressed, 0, "a throttled failure carries no line to count on");
            assert!(waited < Duration::from_secs(120), "it never spoke again: {waited:?}");
        };

        assert!(waited >= TRANSIENT_SPEAK_EVERY,
                "it spoke early — after only {waited:?} of paced waiting");
        assert_eq!(second.suppressed, quiet,
                   "the second line does not stand for the failures it swallowed");
        assert!(second.suppressed > 0);
        assert_eq!(second.suppressed_note(),
                   format!(" ({quiet} identical since the last line)"));

        // And the third stretch starts over rather than inheriting the count.
        let third = backoff.on_failure(false);
        assert!(!third.should_speak, "the quiet stretch restarts after a line");
        assert_eq!(third.suppressed, 0);
    }

    /// The transient axis still offers no SENTENCE, only a schedule.
    ///
    /// `message` names `.rig/broker.env` and two MQTT credential variables,
    /// which is a lie in a UDP listener and in a MIDI port enumerator. The
    /// caller owns the reason; this module owns when it may be repeated.
    #[test]
    fn a_transient_failure_is_given_a_schedule_and_never_a_broker_sentence() {
        let mut backoff = Backoff::new();
        for _ in 0..40 {
            assert!(backoff.on_failure(false).message.is_none());
        }
    }

    /// A success in the middle of an outage clears the quiet stretch too, so
    /// the next failure is a first failure and speaks.
    #[test]
    fn a_success_restarts_the_transient_quiet_stretch() {
        let mut backoff = Backoff::new();
        for _ in 0..6 {
            backoff.on_failure(false);
        }
        backoff.on_success();
        assert!(backoff.on_failure(false).should_speak,
                "after a success the next transient is a new fault and says so");
    }

    #[test]
    fn a_changed_reason_is_news_even_inside_the_quiet_stretch() {
        let mut backoff = Backoff::new();
        backoff.on_failure(true); // burns the first, speaking, refusal
        let quiet = backoff.on_failure(true);
        assert!(!quiet.should_speak, "this one is throttled");

        let mut last = Some("nothing is listening at 10.0.0.9:65000".to_string());
        assert!(
            quiet.is_news("the peer is not speaking OCP.1", &mut last),
            "a condition that has just changed is news whatever the throttle says"
        );
        assert_eq!(last.as_deref(), Some("the peer is not speaking OCP.1"));
        assert!(
            !quiet.is_news("the peer is not speaking OCP.1", &mut last),
            "and the same condition, again, is not"
        );
    }

    #[test]
    fn the_throttle_speaks_over_an_unchanged_reason() {
        let mut backoff = Backoff::new();
        let first = backoff.on_failure(true);
        let mut last = Some("nothing is listening".to_string());
        assert!(
            first.is_news("nothing is listening", &mut last),
            "an unchanged reason still speaks when the throttle comes round"
        );
    }

    #[test]
    fn wait_out_hands_the_retrys_own_wait_to_the_timer_it_was_given() {
        // The async driver names no runtime, so this proves the only thing it
        // owns: the duration the caller's timer is asked for. That it YIELDS
        // rather than blocking is proved on a real runtime, in
        // `apkaudio-aes70` — a crate allowed to depend on tokio.
        let mut backoff = Backoff::new();
        let retry = backoff.on_failure(true);
        let asked = std::cell::Cell::new(Duration::ZERO);
        block_on(wait_out(&retry, |d| {
            asked.set(d);
            std::future::ready(())
        }));
        assert_eq!(asked.get(), REFUSAL_FIRST);
    }

    /// Poll a future to completion on this thread. Only sound for futures that
    /// never return `Pending` — which is every one this file's tests build,
    /// since the timer they hand in is `std::future::ready`. It exists so the
    /// contracts crate can test [`wait_out`] without an async runtime it is
    /// forbidden to depend on.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(out) => out,
            std::task::Poll::Pending => panic!("this test's future must not park"),
        }
    }

    #[test]
    fn drain_hands_every_successful_event_to_its_caller() {
        let events = vec![Ok::<u8, &str>(1), Ok(2), Ok(3)].into_iter();
        let mut seen = Vec::new();
        drain("test", events, |_| false, |item| seen.push(item));
        assert_eq!(seen, vec![1, 2, 3]);
    }
}
