// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! `apk-plugin-runner` — one plugin, one process, one restart domain.
//!
//! Owns: the broker address and credentials, the plugin's retained config, the
//! last will that says the process is gone, the telemetry tick, the answer to
//! `selftest_run`, and the decision to exit. Must not: know what the plugin
//! hunts for. That is the `run_*_agent` function the plugin already exports,
//! unchanged, and this crate only calls it.
//!
//! ```ignore
//! // PLUGINS/PLUGIN:audio:DANTE/src/bin/apk-dante.rs
//! apk_plugin_runner::main!(token = "dante", lib = apkaudio_dante, run = [run_browse_agent]);
//! ```
//!
//! WHY THIS EXISTS (PLAN-3316.01, audit §10.4)
//!
//! The orchestrator ran sixteen of these as threads beside the web API. A hung
//! mDNS browse was never restarted, because the supervisor restarted on EXIT
//! only and a thread that stops making progress does not exit; and a restart
//! that did happen took every hunter and the UI down together. A container per
//! plugin gives each its own restart, and Docker restarts on exit — so the one
//! job that cannot be skipped here is turning "this plugin is no longer doing
//! its work" into a non-zero exit.
//!
//! THE FIVE THINGS, in the order `serve` does them:
//!
//!   1. Broker   `MQTT_HOST`/`MQTT_PORT`, `broker_auth::credentials()`, and a
//!               reconnect paced by `broker_backoff` — never a hot loop.
//!   2. Config   this plugin's own `config.ini`, compiled in, published
//!               retained to `<topic>/config` + `/status` (+ `/host` on a
//!               broker the bench owns). `mqtt.rs::publish_protocol_configs`
//!               narrowed from "walk every crate" to "this one".
//!   3. Liveness the session's LAST WILL is `incoming/state = unavailable`,
//!               retained, so a dead container is on the bus as dead without
//!               anybody polling; `incoming/telemetry` ticks while it lives.
//!   4. Selftest `outgoing/selftest` and `outgoing/selftest_run` for THIS
//!               module only, through the crate's `self_test_json` door —
//!               the same vocabulary `orchestrator/src/self_test.rs` spoke for
//!               all of them.
//!   5. Watchdog every agent runs on its own thread. A thread that returns,
//!               or panics, exits the process non-zero; an agent that has
//!               called [`tick`] and then stops calling it is a stall and
//!               exits too — judged per agent, so a ticking sibling cannot
//!               hide it. SIGTERM/SIGINT exit 143 within a second, so a
//!               `docker stop` is a stop and not a SIGKILL — after the
//!               drains plugins registered with [`at_stop`] have run.
//!
//! AND ONE THING BESIDE THE FIVE: [`presence`]. A plugin whose `config.ini`
//! declares `presence_protocols` reports every sighting through
//! [`presence::seen`], and the runner turns those into the retained
//! active-since / inactive-since document per device (PLAN-3318.01), the
//! retained `presence_topic` link beside the device's own leaves, and the
//! sighting's identity — manufacturer, model, device_class, name, serial,
//! notes — retained beside that link, so the retained tree alone registers it
//! (PLAN-3393.01).
//!
//! WHAT THE WATCHDOG CANNOT SEE, stated rather than implied: an agent that has
//! never called [`tick`] and wedges inside its own loop without returning.
//! Most plugins' agents now tick from their receive loops (DANTE, PTP, AES67,
//! AES70, APPLETV, CHROMECAST, VISA, TRENDNET, …); for an agent that does not
//! (`grep -rL 'tick()'` over a plugin's `SRC/` names them), the watchdog
//! catches exit and panic only. The container
//! healthcheck (`apk-plugin-health`) reads the file the telemetry tick writes,
//! which proves THIS process is scheduling threads — not that the browse loop
//! inside it is finding anything. Teaching a hunter to call `tick()` from its
//! receive loop is the one-line change that closes the gap for that hunter.

// First, so `elog!` is in scope for every module after it.
#[macro_use]
pub mod report;
pub mod api;
pub mod jobs;
pub mod loopback;
pub mod panels;
pub mod presence;
pub mod republish;
pub mod session;
pub mod unkept;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use apkaudio_contracts::agent_state::AgentReport;
/// For [`main!`]'s `manifest =`: the contract's `embedded_manifest!`, reached
/// through this crate so a plugin need not name the contracts crate itself.
#[doc(hidden)]
pub use apkaudio_contracts as __contracts;
use apkaudio_contracts::baremetal_io;
use apkaudio_contracts::self_test::{Check, Depth, Engaged, Engagement, Report, LEAF_ENGAGE, LEAF_RUN};
use rumqttc::{Client, Event, Packet, QoS};
use serde_json::{json, Map, Value};

/// What a plugin binary hands the runner. Built by [`main!`]; written out by
/// hand only by the plugins audit §10.4 lists as exceptions (SQL, PlayStation).
pub struct Plugin {
    /// The module token the bus knows it by — `dante`, `nmos`, `ptp`. The same
    /// word as the `[section]` in its `config.ini`.
    pub token: &'static str,
    /// The crate's `config.ini`, compiled in with `include_str!`. Compiled in
    /// rather than read at run time because the image holds binaries, not
    /// source trees, and `CARGO_MANIFEST_DIR` names a build-stage path that is
    /// not in it — the exact failure the sql-capture Dockerfile documents.
    /// `APK_PLUGIN_CONFIG` overrides it with a file for a bench that wants to.
    pub config_ini: &'static str,
    /// The crate's self-test door, `fn(depth_word) -> report JSON`.
    pub self_test: fn(&str) -> String,
}

/// The plugin crate's `(CARGO_PKG_NAME, CARGO_PKG_VERSION)`, once stamped.
static PLUGIN_BUILD: std::sync::OnceLock<(&'static str, &'static str)> = std::sync::OnceLock::new();

/// Record which plugin crate this binary is (PLAN-3407.01). Call through
/// [`stamp!`], which expands `env!` in the CALLER — calling `env!` here would
/// name this crate, the mistake `runner_version` already is. The first stamp
/// wins; a second is ignored.
pub fn stamp_plugin_build(name: &'static str, version: &'static str) {
    let _ = PLUGIN_BUILD.set((name, version));
}

/// What [`stamp_plugin_build`] recorded, if anything did.
pub fn plugin_build() -> Option<(&'static str, &'static str)> {
    PLUGIN_BUILD.get().copied()
}

/// The CALLING crate's `CARGO_MANIFEST_DIR`, once stamped — what the `pod` of
/// `incoming/api` is read off (LOOSE-runner-kind-pod).
static PLUGIN_HOME: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// What the binary calls itself in `incoming/api`'s `kind`, when it is not a
/// plugin. Set by [`main!`]'s `kind = "worker"`; unset means `plugin`.
static PLUGIN_KIND: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// Record where the plugin crate's source sits. Through [`stamp!`], for the
/// same reason as [`stamp_plugin_build`]. The first stamp wins.
pub fn stamp_plugin_home(manifest_dir: &'static str) {
    let _ = PLUGIN_HOME.set(manifest_dir);
}

/// Record the binary's `kind` (`worker`, …). Through [`main!`]'s `kind =`.
pub fn stamp_plugin_kind(kind: &'static str) {
    let _ = PLUGIN_KIND.set(kind);
}

/// `plugin`, unless the binary said otherwise.
pub fn plugin_kind() -> &'static str {
    PLUGIN_KIND.get().copied().unwrap_or("plugin")
}

/// The pod the binary runs for: `APK_POD` when the container sets it, else
/// the `POD.*` folder on its crate's source path.
///
/// `APK_POD` FIRST (PLAN-3430.01). An image that builds outside the checkout's
/// own paths has no `POD.` folder on its manifest dir — the POD.workers images
/// build their `SRC/` at `/src/SRC` — and reported `pod: null`. Their compose
/// says `APK_POD=POD.workers` instead. A value that is blank, or not a `POD.`
/// name, is ignored in favour of the path, so a typo cannot invent a pod.
///
/// OTHERWISE READ OFF THE PATH, NOT A CONSTANT. Both engine images build at the
/// checkout's own paths (`/src/PODS/POD.THICK/…`, `/src/PODS/POD.THIN/…`), so
/// the manifest dir names the pod in the image exactly as it does on disk. A
/// binary that never stamped itself was built in the runner's own workspace,
/// so it takes the runner's pod; `None` only if nothing names a `POD.`.
pub fn plugin_pod() -> Option<&'static str> {
    static FROM_ENV: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();
    let declared = *FROM_ENV.get_or_init(|| {
        pod_declared(std::env::var("APK_POD").ok().as_deref()).map(|pod| &*Box::leak(pod.into_boxed_str()))
    });
    declared.or_else(|| PLUGIN_HOME.get().copied().and_then(pod_of)).or_else(|| pod_of(env!("CARGO_MANIFEST_DIR")))
}

/// `APK_POD`'s value as a pod, if it is one: trimmed, and a single `POD.<name>` segment.
fn pod_declared(raw: Option<&str>) -> Option<String> {
    let pod = raw?.trim();
    (pod.len() > "POD.".len() && pod.starts_with("POD.") && !pod.contains(['/', '\\']) && !pod.contains(char::is_whitespace))
        .then(|| pod.to_string())
}

fn pod_of(path: &str) -> Option<&str> {
    path.split(['/', '\\']).find(|segment| segment.starts_with("POD."))
}

/// One blocking entry point and the name it is reported under.
pub type Agent = (&'static str, Box<dyn FnOnce(String, u16) + Send>);

/// How often telemetry is published and the health file rewritten.
const TICK: Duration = Duration::from_secs(15);

/// The largest packet the session sends or accepts: room for a device panel
/// (the bench's biggest instrument panel is about 2 MiB) with headroom.
const MAX_PACKET: usize = 4 * 1024 * 1024;

/// Exit codes, distinct so `docker ps -a` says which rule fired.
const EXIT_AGENT_RETURNED: i32 = 70;
const EXIT_AGENT_PANICKED: i32 = 101;
const EXIT_SESSION_DIED: i32 = 69;
const EXIT_STALLED: i32 = 75;

/// Exit code for a SIGTERM/SIGINT stop: 128 + SIGTERM, what a shell reports
/// for a process the signal ended, so `docker ps -a` reads "stopped" not "killed".
const EXIT_TERMINATED: i32 = 143;

/// Seconds since the epoch of the last [`tick`] made from a thread no agent
/// owns (a pool thread, a tokio worker), or 0 for never.
static LAST_TICK: AtomicU64 = AtomicU64::new(0);

/// ONE STALL CLOCK PER AGENT (LOOSE-discovery-engine-the-runners-stall). A
/// single clock let a process with two agents — AppleTV browse + control,
/// AES67 daemon + web — hide one wedged agent behind the other's ticks. Each
/// agent thread is given its slot in [`AGENT_SLOT`] before it runs, so a
/// `tick()` from that thread moves only its own clock; the watchdog judges the
/// stalest armed clock. Indexed like the `agents` list `serve_group` was given.
static AGENT_TICKS: std::sync::OnceLock<Vec<(&'static str, AtomicU64)>> = std::sync::OnceLock::new();

thread_local! {
    /// The index into [`AGENT_TICKS`] of the agent this thread runs, if any.
    static AGENT_SLOT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// An agent says "still making progress". Cheap enough for a receive loop.
///
/// Opting in is what arms stall detection, PER AGENT: once an agent's thread
/// has ticked, that agent going `APK_PLUGIN_STALL_SECS` (default 120) without
/// another is an exit, whatever its siblings in the process are doing. A tick
/// from a thread the agent spawned itself counts on the shared process clock,
/// which is armed and judged the same way.
pub fn tick() {
    let now = unix_now();
    let slot = AGENT_SLOT.with(std::cell::Cell::get);
    match slot.and_then(|i| AGENT_TICKS.get()?.get(i)) {
        Some((_, last)) => last.store(now, Ordering::Relaxed),
        None => LAST_TICK.store(now, Ordering::Relaxed),
    }
}

/// The armed clock that ticked longest ago, as `(agent, age_s)` — what the
/// watchdog judges and what telemetry / `incoming/api` publish as
/// `last_tick_age_s`. `None` while nothing has ticked.
fn stalest_tick() -> Option<(&'static str, u64)> {
    let agents = AGENT_TICKS.get().map(Vec::as_slice).unwrap_or_default();
    let clocks = agents
        .iter()
        .map(|(name, last)| (*name, last.load(Ordering::Relaxed)))
        .chain(std::iter::once(("process", LAST_TICK.load(Ordering::Relaxed))));
    stalest(clocks, unix_now())
}

/// [`stalest_tick`] over given `(name, last_tick)` pairs; 0 means never ticked.
fn stalest<'a>(clocks: impl Iterator<Item = (&'a str, u64)>, now: u64) -> Option<(&'a str, u64)> {
    clocks
        .filter(|(_, last)| *last != 0)
        .map(|(name, last)| (name, now.saturating_sub(last)))
        .max_by_key(|(_, age)| *age)
}

/// Set by the SIGTERM/SIGINT handler; the watchdog loop acts on it.
static TERMINATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_terminate(_signal: libc::c_int) {
    // Async-signal-safe: one atomic store, nothing else.
    TERMINATE.store(true, Ordering::Relaxed);
}

/// SIGTERM and SIGINT end the process within a second (LOOSE-discovery-engine-
/// plugin-containers-ignore). The binary is PID 1 in its container unless
/// compose adds `init: true`, and PID 1 has NO default action for SIGTERM — so
/// without a handler every `docker stop` waited out its grace and SIGKILLed
/// (Exited 137). Exiting closes the broker socket without a DISCONNECT, so the
/// broker still sends the last will and the bus reads `unavailable`.
fn handle_termination() {
    // SAFETY: installing a handler that only stores to an atomic.
    unsafe {
        libc::signal(libc::SIGTERM, on_terminate as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_terminate as *const () as libc::sighandler_t);
    }
}

/// A plugin's last word before a stop: run once, on the watchdog's thread
/// pool, between SIGTERM and the runner's exit 143.
type Drain = (&'static str, Box<dyn FnOnce() + Send>);

/// The drains [`at_stop`] registered, in registration order.
static DRAINS: Mutex<Vec<Drain>> = Mutex::new(Vec::new());

/// The longest the runner waits for every drain together, unless
/// `APK_PLUGIN_DRAIN_SECS` says otherwise: under Docker's 10 s stop grace, so
/// a slow drain is cut short by the runner's 143, not by a SIGKILL's 137.
const DRAIN_CEILING: Duration = Duration::from_secs(8);

/// Register something that must happen when the container is stopped
/// (PLAN-3428.01, RP-01): VISA destroying its open VXI-11 links,
/// DESKTOP_MONITOR stamping its `graceful_shutdown` status before its own
/// DISCONNECT. THE PLUGIN NEVER CATCHES SIGTERM ITSELF — a second handler
/// replaces the runner's (one per signal per process), and the plugin then
/// decided the exit code too (two of them exited 0 instead of 143). The
/// Python runner's `at_stop(hook)` is the same door (its budget is 2 s).
///
/// On SIGTERM/SIGINT the runner starts every registered drain on its own
/// thread, waits until all have returned or `APK_PLUGIN_DRAIN_SECS` (default
/// 8) has passed, and exits 143. A drain bounds itself by its own grace; the
/// ceiling is the backstop. Call it from the agent once it holds what the
/// drain needs; a drain registered after the stop began is not run.
pub fn at_stop(name: &'static str, drain: impl FnOnce() + Send + 'static) {
    DRAINS.lock().unwrap_or_else(|e| e.into_inner()).push((name, Box::new(drain)));
}

/// Run `drains` concurrently, each on a named thread, for at most `ceiling`.
/// Returns the names of the drains still running when it gave up (none when
/// every drain returned in time). A drain that panics counts as finished.
fn run_drains(token: &str, drains: Vec<Drain>, ceiling: Duration) -> Vec<&'static str> {
    let until = Instant::now() + ceiling;
    let running: Vec<(&'static str, Option<JoinHandle<()>>)> = drains
        .into_iter()
        .map(|(name, drain)| {
            let handle = std::thread::Builder::new().name(format!("{token}:drain:{name}")).spawn(drain);
            if let Err(e) = &handle {
                elog!("⚠️  [{token}] could not start drain {name}: {e}");
            }
            (name, handle.ok())
        })
        .collect();
    while Instant::now() < until && running.iter().any(|(_, h)| h.as_ref().is_some_and(|h| !h.is_finished())) {
        std::thread::sleep(Duration::from_millis(20));
    }
    running
        .into_iter()
        .filter(|(_, h)| h.as_ref().is_some_and(|h| !h.is_finished()))
        .map(|(name, _)| name)
        .collect()
}

/// `MQTT_HOST` / `MQTT_PORT`, the two names every Rust agent in this repository
/// already reads, with the loopback defaults `agents.json` assumed.
pub fn broker_address() -> (String, u16) {
    let host = std::env::var("MQTT_HOST")
        .ok()
        .filter(|h| !h.trim().is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = std::env::var("MQTT_PORT")
        .ok()
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(1883);
    (host, port)
}

/// What the command line asks for, answered without a broker: `Some(exit code)`
/// when the process must stop here (help, version, or an argument this runner
/// does not take), `None` when there are no arguments and the agent may start.
pub fn answer_arguments(
    args: impl IntoIterator<Item = String>,
    tokens: &[&str],
    agents: &[&str],
) -> Option<i32> {
    let args: Vec<String> = args.into_iter().collect();
    let first = args.first()?;
    let exe = std::env::args().next().unwrap_or_else(|| "apk-plugin".into());
    let exe = exe.rsplit('/').next().unwrap_or("apk-plugin").to_string();
    let usage = format!(
        "{exe} — APK.audio plugin runner for [{}] (agents: {})\n\n\
         Usage: {exe}            run until stopped; settings come from the environment\n\
         \x20      {exe} --help     this text, and exit (touches no broker)\n\
         \x20      {exe} --version  the build, and exit\n\n\
         Environment:\n\
         \x20 MQTT_HOST              broker host (default 127.0.0.1)\n\
         \x20 MQTT_PORT              broker port (default 1883)\n\
         \x20 APK_PLUGIN_CONFIG      a config.ini to use instead of the compiled-in one\n\
         \x20 APK_PLUGIN_STALL_SECS  seconds without a tick before the agent counts as stalled (default 120)\n\
         \x20 APK_PLUGIN_DRAIN_SECS  seconds a stop waits for the plugin's shutdown drains (default 8)\n\
         \x20 APK_POD                the pod incoming/api reports (default: the POD.* folder it was built in)\n",
        tokens.join("+"),
        agents.join(", "),
    );
    match first.as_str() {
        "-h" | "--help" | "help" => {
            print!("{usage}");
            Some(0)
        }
        "-V" | "--version" => {
            match plugin_build() {
                Some((name, version)) => println!("{name} {version}"),
                None => println!("{exe} (unstamped build)"),
            }
            Some(0)
        }
        other => {
            eprintln!("{exe}: takes no arguments (got {other:?}); nothing was started.\n");
            eprint!("{usage}");
            Some(2)
        }
    }
}

// serve, serve_group: group.rs
mod group;
pub use group::{serve, serve_group};

/// The `incoming/telemetry` tick document.
///
/// `counters` is the loopback telemetry's own block (ROAD-3478) — the same
/// four numbers `GET /telemetry` serves, in every binary whether or not it
/// declared a manifest (the counters run regardless; only the HTTP surface is
/// opt-in). So a bus reader can apply contracts' rule
/// (`baremetal_telemetry::observed_state`, which reads
/// `counters.packets_received`) to the tick exactly as the supervisor applies
/// it to the loopback, without a port. No contract schema describes this
/// document; adding a key is compatible with every reader of the other four.
pub fn tick_sample(uptime_s: u64) -> Value {
    let counters = loopback::counters();
    let mut sample = json!({
        "uptime_s": uptime_s,
        "pid": std::process::id(),
        "last_tick_age_s": stalest_tick().map(|(_, age)| age),
        "image_rev": std::env::var("APK_IMAGE_REV").ok(),
        "counters": {
            "packets_received": counters.packets_received,
            "messages_published": counters.messages_published,
            "publish_failures": counters.publish_failures,
            "unread_config_keys": counters.unread_config_keys,
        },
    });
    // Switched off in config: the tick says so in the loopback's words, so a
    // bus reader does not read zero packets as an idle-but-working hunter.
    if let (Some(reason), Some(object)) = (loopback::disabled(), sample.as_object_mut()) {
        object.insert("state".into(), json!("stub"));
        object.insert("reason".into(), json!(reason));
    }
    sample
}

/// What the loopback and the tick say while `enabled = false`.
pub const DISABLED_REASON: &str = "disabled in config";

/// `enabled` in the token's own section: only an explicit no (`false`, `no`,
/// `off`, `0`, any case) switches the plugin off. Absent, empty or anything
/// else is on — the key is a switch, and a typo must not silence a hunter.
pub fn module_enabled(config: &Value, token: &str) -> bool {
    match module_key(config, token, "enabled") {
        Some(value) => !["false", "no", "off", "0"].iter().any(|no| no.eq_ignore_ascii_case(value.trim())),
        None => true,
    }
}

// The module's broker session: wire.rs
mod wire;
use wire::spawn_session;
#[cfg(test)]
use wire::send_panels;

// config.ini and the module's section keys: module_config.rs
mod module_config;
pub use module_config::{broker_is_local, ini_to_json, module_raise, module_serves, module_topic, RAISE_MODES};
use module_config::module_key;

// The health file: health.rs
mod health;
pub use health::{health_check, health_file};
use health::{machine_name, unix_now, write_health_file};


/// A plugin binary in one line.
///
/// `lib` is the plugin's library crate; `run` names one or more of its blocking
/// `fn(&str, u16)` entry points, each on its own thread. Browse and control of
/// one vendor go in ONE binary on purpose (audit §10.2, "exactly one copy"):
/// two containers of the same pair would put two publishers on one retained tree.
///
/// ```ignore
/// apk_plugin_runner::main!(token = "appletv", lib = apkaudio_appletv,
///                          run = [run_browse_agent, run_control_agent]);
/// ```
///
/// A binary that is not a plugin says so first — `kind = "worker", token = …` —
/// and `incoming/api` publishes that `kind`; the `pod` is read off the crate's
/// own path (LOOSE-runner-kind-pod).
#[macro_export]
macro_rules! main {
    ($(kind = $kind:literal,)? token = $token:literal, lib = $lib:ident, run = [$($run:ident),+ $(,)?], panels = [$($family:ident),+ $(,)?] $(, manifest = $manifest:literal)? $(,)?) => {
        fn main() {
            $($crate::stamp_plugin_kind($kind);)?
            $($crate::__declare_manifest!($manifest);)?
            $($crate::panel!($family);)+
            $crate::__main_body!($token, $lib, [$($run),+]);
        }
    };
    ($(kind = $kind:literal,)? token = $token:literal, lib = $lib:ident, run = [$($run:ident),+ $(,)?] $(, manifest = $manifest:literal)? $(,)?) => {
        fn main() {
            $($crate::stamp_plugin_kind($kind);)?
            $($crate::__declare_manifest!($manifest);)?
            $crate::__main_body!($token, $lib, [$($run),+]);
        }
    };
}

/// Compile the CALLING crate's manifest in and declare it to [`loopback`]
/// (ROAD-3472). [`main!`]'s `manifest = "agent.toml"` writes this; a
/// hand-written `main` may write `apk_plugin_runner::__declare_manifest!("agent.toml");`
/// before [`serve`]. A manifest that does not parse is a build defect said at
/// startup, exit 78 (EX_CONFIG) — the crate's own `cargo test` should parse it first.
#[doc(hidden)]
#[macro_export]
macro_rules! __declare_manifest {
    ($manifest:literal) => {
        match $crate::__contracts::embedded_manifest!($manifest) {
            Ok(manifest) => $crate::loopback::declare(manifest, env!("CARGO_PKG_VERSION")),
            Err(error) => {
                eprintln!("🛑 {} does not parse: {error}", $manifest);
                ::std::process::exit(78);
            }
        }
    };
}

/// The body both arms of [`main!`] share. Not for direct use.
#[doc(hidden)]
#[macro_export]
macro_rules! __main_body {
    ($token:literal, $lib:ident, [$($run:ident),+]) => {
        {
            $crate::stamp!();
            // BEFORE THE BROKER IS TOUCHED, and only HERE: a `main!` binary
            // takes its settings from the environment and no argument, so an
            // argument is a question (`--help`, `--version`) or a mistake, and
            // neither may start the agent -- `apk-nmos --help` used to publish
            // its config and a retained device row (LOOSE-bin-runner-main). A
            // hand-written main that parses its own argv (soundcard, VISA, …)
            // calls `serve` directly and is not touched by this.
            if let Some(code) = $crate::answer_arguments(
                ::std::env::args().skip(1),
                &[$token],
                &[$(stringify!($run)),+],
            ) {
                ::std::process::exit(code);
            }
            $crate::serve(
                $crate::Plugin {
                    token: $token,
                    config_ini: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config.ini")),
                    self_test: $lib::self_test::self_test_json,
                },
                vec![$(
                    (
                        stringify!($run),
                        Box::new(|host: String, port: u16| $lib::$run(&host, port))
                            as Box<dyn FnOnce(String, u16) + Send>,
                    )
                ),+],
            )
        }
    };
}

/// Register one `gui/` family's panel (PLAN-3413.01): compiles in
/// `gui/manifest.json` and `gui/<family>/<family>.json` from the CALLING crate.
/// [`main!`]'s `panels = [...]` does this; a hand-built `main` writes
/// `apk_plugin_runner::panel!("family");` before [`serve`].
#[macro_export]
macro_rules! panel {
    ($family:ident) => {
        $crate::panel!(@ stringify!($family))
    };
    ($family:literal) => {
        $crate::panel!(@ $family)
    };
    (@ $family:expr) => {
        $crate::panels::register(
            $family,
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/gui/manifest.json")),
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/gui/", $family, "/", $family, ".json")),
        )
    };
}

/// Stamp the CALLING crate's name and version into `incoming/api`
/// (`build.plugin_crate` / `build.plugin_version`). [`main!`] does it for you;
/// a binary that builds its [`Plugin`] by hand writes `apk_plugin_runner::stamp!();`
/// as the first line of its `main`, before [`serve`].
#[macro_export]
macro_rules! stamp {
    () => {{
        $crate::stamp_plugin_build(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        $crate::stamp_plugin_home(env!("CARGO_MANIFEST_DIR"));
    }};
}

#[cfg(test)]
mod tests;
