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
//!               `docker stop` is a stop and not a SIGKILL.
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

pub mod api;
pub mod jobs;
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

/// The pod the binary's crate lives in: the `POD.*` folder on its source path.
///
/// READ OFF THE PATH, NOT A CONSTANT. Both images build at the checkout's own
/// paths (`/src/PODS/POD.THICK/…`, `/src/PODS/POD.THIN/…`), so the manifest dir
/// names the pod in the image exactly as it does on disk. A binary that never
/// stamped itself was built in the runner's own workspace, so it takes the
/// runner's pod; `None` only if neither path carries a `POD.` folder.
pub fn plugin_pod() -> Option<&'static str> {
    PLUGIN_HOME
        .get()
        .copied()
        .and_then(pod_of)
        .or_else(|| pod_of(env!("CARGO_MANIFEST_DIR")))
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
         \x20 APK_PLUGIN_STALL_SECS  seconds without a tick before the agent counts as stalled (default 120)\n",
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

/// Run the plugin until something says it must not keep running. Never returns.
pub fn serve(plugin: Plugin, agents: Vec<Agent>) -> ! {
    serve_group(vec![plugin], agents)
}

/// Several MODULES in one process, each with its own session, config, will and
/// self test — for the one container audit §10.2 groups: `nmos` is
/// `apkaudio-nmos` (mDNS, which finds the APIs) and `apkaudio-nmos-control`
/// (IS-04, what is registered inside them), one finding at two depths. They
/// keep two tokens on the bus because the bus already knows them as two.
/// No binary groups them any more — APK:plugin:NMOS_CONTROL is its own
/// container (`apk-nmos-control`) — but `serve` is still this with one plugin.
pub fn serve_group(plugins: Vec<Plugin>, agents: Vec<Agent>) -> ! {
    let (host, port) = broker_address();
    let names: Vec<&str> = plugins.iter().map(|p| p.token).collect();
    let token = names.join("+");
    let token = token.as_str();
    println!(
        "🚀 [{token}] plugin runner — broker {host}:{port} {}",
        apkaudio_contracts::broker_auth::describe()
    );

    // `APK_PLUGIN_CONFIG` names ONE file, so it only applies to a one-module
    // process; a group keeps the files compiled into it.
    let override_path = std::env::var("APK_PLUGIN_CONFIG").ok().filter(|_| plugins.len() == 1);

    // THE STALL WINDOW IS READ HERE NOW, not below, because the API document
    // publishes it beside `last_tick_age_s` — a reader cannot judge the age
    // without the threshold, and a second `var()` call would be a second
    // default to keep in step.
    let stall_after = std::env::var("APK_PLUGIN_STALL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(120);

    // WHAT EVERY MODULE IN THIS PROCESS SAYS ABOUT ITSELF (`incoming/api`).
    // Built ONCE, before the sessions, so every module reports the same broker,
    // the same agents and the same start time — three sessions each reading the
    // environment for themselves would eventually disagree about one of them.
    let mut api_context = api::Context::from_environment(
        host.clone(),
        port,
        broker_is_local(&host),
        stall_after,
        TICK,
    );
    api_context.agents = agents.iter().map(|(name, _)| name.to_string()).collect();
    if let Some(path) = &override_path {
        api_context.config_source = path.clone();
    }

    // 1 + 2 + 3 + 4: one session per module, each on its own thread.
    let sessions: Vec<JoinHandle<()>> = plugins
        .iter()
        .map(|plugin| {
            let config_text = match &override_path {
                Some(path) => std::fs::read_to_string(path).unwrap_or_else(|e| {
                    eprintln!("🛑 [{token}] APK_PLUGIN_CONFIG={path} did not read ({e}); using the compiled-in config.ini");
                    plugin.config_ini.to_string()
                }),
                None => plugin.config_ini.to_string(),
            };
            // Presence (PLAN-3318.01): a plugin that declares the protocols it
            // reports devices under gets the tracker's session and its one
            // expiry tick. `start` is once per process, so a group's second
            // module declaring them too is a no-op, not a second tick.
            if let Some((protocols, after)) = presence::settings_from_config(&ini_to_json(&config_text), plugin.token) {
                presence::start(plugin.token, &host, port, protocols, after);
            }
            // SIBLINGS ARE PER MODULE, so the clone is made here rather than
            // shared: `nmos` must name `nmos_control` and not itself.
            let mut context = api_context.clone();
            context.siblings =
                names.iter().filter(|n| **n != plugin.token).map(|n| n.to_string()).collect();
            spawn_session(plugin, &host, port, config_text, context)
        })
        .collect();

    // 5: the agents, each on a named thread so a panic message names it, and
    // each with its own stall clock.
    let _ = AGENT_TICKS.set(agents.iter().map(|(name, _)| (*name, AtomicU64::new(0))).collect());
    handle_termination();
    let mut running: Vec<(&'static str, JoinHandle<()>)> = Vec::new();
    for (slot, (name, agent)) in agents.into_iter().enumerate() {
        let (h, p) = (host.clone(), port);
        let handle = std::thread::Builder::new()
            .name(format!("{token}:{name}"))
            .spawn(move || {
                AGENT_SLOT.with(|s| s.set(Some(slot)));
                agent(h, p)
            })
            .unwrap_or_else(|e| {
                eprintln!("🛑 [{token}] could not start {name}: {e}");
                std::process::exit(EXIT_AGENT_RETURNED)
            });
        running.push((name, handle));
    }

    loop {
        std::thread::sleep(Duration::from_secs(1));

        if TERMINATE.load(Ordering::Relaxed) {
            println!("👋 [{token}] stop requested (SIGTERM/SIGINT) — exiting; the broker sends the last will");
            std::process::exit(EXIT_TERMINATED);
        }

        if sessions.iter().any(JoinHandle::is_finished) {
            eprintln!("🛑 [{token}] the broker session thread ended; exiting so the container restarts");
            std::process::exit(EXIT_SESSION_DIED);
        }

        // A hunter is a loop that never returns. Returning at all — the mDNS
        // daemon would not start, a socket would not bind — is a failure the
        // agent has already printed; the runner's part is not to sit there
        // publishing "alive" for a process that hunts nothing.
        if let Some(index) = running.iter().position(|(_, h)| h.is_finished()) {
            let (name, handle) = running.swap_remove(index);
            match handle.join() {
                Ok(()) => {
                    eprintln!("🛑 [{token}] {name} returned; a hunter that stops is a failure — exiting");
                    std::process::exit(EXIT_AGENT_RETURNED);
                }
                Err(_) => {
                    eprintln!("🛑 [{token}] {name} panicked — exiting");
                    std::process::exit(EXIT_AGENT_PANICKED);
                }
            }
        }

        if let Some((agent, age)) = stalest_tick().filter(|(_, age)| *age > stall_after) {
            eprintln!("🛑 [{token}] {agent}: no tick for {age}s (limit {stall_after}s) — stalled, exiting");
            std::process::exit(EXIT_STALLED);
        }
    }
}

/// The one broker session: config on connect, the self-test inbox, the
/// telemetry tick. Returns the handle the watchdog polls.
fn spawn_session(
    plugin: &Plugin,
    host: &str,
    port: u16,
    config_text: String,
    api_context: api::Context,
) -> JoinHandle<()> {
    let token = plugin.token;
    let door = plugin.self_test;

    // A UNIQUE CLIENT ID (session::client_id: node + pid + nonce), because two
    // containers of one plugin on one broker (a per-switch netgear agent) must
    // not kick each other off — MQTT drops the older session on a duplicate
    // id, silently, forever — and in a container every process is pid 1.
    //
    // A PANEL IS BIGGER THAN rumqttc's 10 KiB DEFAULT. The bound Chromecast
    // console is ~11 KiB, and an over-size publish is not refused — it kills
    // the session, which reconnects, restates the panel and dies again, a loop
    // the broker logs as "disconnected due to out of memory" (PLAN-3413.01).
    let mut session = session::Session::new(token, "plugin").to(host, port).max_packet(MAX_PACKET);
    // THE LAST WILL IS THE LIVENESS. `unavailable` is the contract's word for
    // "running and can see nothing"; a process that is gone sees nothing, and
    // the word is one every consumer of `state` already renders. Retained, so
    // a reader that arrives after the crash still reads the crash.
    if let Ok((topic, _)) = baremetal_io::module_incoming(token, "state") {
        session = session.last_will(topic, "unavailable");
    }

    let (client, mut connection) = session.connect();
    let config = ini_to_json(&config_text);
    let topic = module_topic(&config, token);
    let broker_is_ours = broker_is_local(host);

    // DEVICE PANELS (PLAN-3413.01): only for a plugin that registered a gui/
    // template; for every other plugin the book is inert and subscribes nothing.
    let book = Arc::new(Mutex::new(panels::Book::new(
        token,
        panels::templates(),
        panels::expire_after_from_config(&config, token),
    )));

    // Telemetry and the health file, on their own thread so a slow broker
    // cannot hold the health file back — and try_publish, so a full queue
    // drops a sample instead of blocking the tick (PLAN-3317.01's defect).
    {
        let client = client.clone();
        let book = book.clone();
        let started = Instant::now();
        std::thread::spawn(move || loop {
            // A silent device's panel leaves on the tick (`panels_expire_after_s`).
            let expired = book.lock().unwrap_or_else(|e| e.into_inner()).expire(Instant::now());
            publish_panels(&client, expired);
            let sample = json!({
                "uptime_s": started.elapsed().as_secs(),
                "pid": std::process::id(),
                "last_tick_age_s": stalest_tick().map(|(_, age)| age),
                "image_rev": std::env::var("APK_IMAGE_REV").ok(),
            });
            if let Ok((t, retain)) = baremetal_io::module_incoming(token, "telemetry") {
                let _ = client.try_publish(t, QoS::AtMostOnce, retain, sample.to_string());
            }
            write_health_file();
            std::thread::sleep(TICK);
        });
    }

    std::thread::Builder::new()
        .name(format!("{token}:session"))
        .spawn(move || {
            let register = Mutex::new(Engagement::new());
            let presence: Arc<Mutex<BTreeSet<String>>> = Arc::default();
            let presence_filter = format!("APK.audio/System/Protocols/{token}/Device/+/last_online");
            let mut backoff = apkaudio_contracts::broker_backoff::Backoff::new();

            for event in connection.iter() {
                let Some(event) = apkaudio_contracts::broker_backoff::step(
                    token,
                    &mut backoff,
                    event,
                    |e| matches!(e, rumqttc::ConnectionError::ConnectionRefused(_)),
                ) else {
                    continue;
                };
                match event {
                    // Every (re)connect: the broker may have lost its retained
                    // store, and a subscription does not survive a clean session.
                    Event::Incoming(Packet::ConnAck(_)) => {
                        publish_config(&client, &topic, token, &config, broker_is_ours);
                        // BESIDE THE CONFIG AND FOR THE SAME REASON: a broker
                        // that lost its retained store has lost this too, and a
                        // reader arriving after that would find the plugin
                        // undescribed with nothing to tell it to ask.
                        publish_api(&client, token, &api_context, &config, &register, &presence);
                        if let Ok(inbox) = baremetal_io::module_inbox(token) {
                            let _ = client.try_subscribe(inbox, QoS::AtLeastOnce);
                        }
                        let _ = client.try_subscribe(presence_filter.clone(), QoS::AtLeastOnce);
                        let book = book.lock().unwrap_or_else(|e| e.into_inner());
                        if book.is_active() {
                            let _ = client.try_subscribe(book.filter(), QoS::AtLeastOnce);
                            // Read back what an earlier process left, so a
                            // panel no device claims is emptied, not kept.
                            let _ = client.try_subscribe(book.panels_filter(), QoS::AtLeastOnce);
                            publish_panels(&client, book.restate());
                        }
                    }
                    Event::Incoming(Packet::Publish(publish)) => {
                        let payload = String::from_utf8_lossy(&publish.payload).to_string();
                        // Every device leaf feeds the panel book first; the
                        // presence set below still sees its own `last_online`.
                        {
                            let mut book = book.lock().unwrap_or_else(|e| e.into_inner());
                            if book.is_active() {
                                let out = book.observe(&publish.topic, &payload, Instant::now());
                                drop(book);
                                publish_panels(&client, out);
                            }
                        }
                        if publish.topic.ends_with("/last_online") {
                            let mut set = presence.lock().unwrap_or_else(|e| e.into_inner());
                            if apkaudio_contracts::presence::is_online_now(&payload) {
                                set.insert(publish.topic.clone());
                            } else {
                                set.remove(&publish.topic);
                            }
                            continue;
                        }
                        // `api_get` FIRST, and by its own test rather than
                        // through self_test::input_for — that function is the
                        // self-test vocabulary's, and teaching it a leaf that
                        // has nothing to do with self tests would put the two
                        // contracts in one table.
                        // A RETAINED PRESS IS IGNORED, the rule `selftest_run`
                        // already follows: twenty-six plugins re-describing
                        // themselves on every broker restart is a publish storm
                        // nobody asked for, and the description is retained
                        // anyway so the reader already has it.
                        if publish.topic.ends_with(&format!("/{}", baremetal_io::LEAF_API_GET)) {
                            if !publish.retain {
                                publish_api(&client, token, &api_context, &config, &register, &presence);
                            }
                            continue;
                        }
                        match apkaudio_contracts::self_test::input_for(&publish.topic) {
                            Some(LEAF_ENGAGE) => engage(&client, &register, &presence, token, &payload),
                            Some(LEAF_RUN) if apkaudio_contracts::self_test::is_live_run(publish.retain) => {
                                perform(&client, &register, token, door, &payload)
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        })
        .expect("the session thread starts")
}

/// `<topic>/config`, `/status`, and `/host` — the three retained facts
/// `mqtt.rs::publish_protocol_configs` published for every crate at once.
fn publish_config(client: &Client, topic: &str, token: &str, config: &Value, broker_is_ours: bool) {
    let payload = json!({
        "value": config,
        "path": format!("{token}/config.ini"),
        "source": "plugin",
    });
    let _ = client.try_publish(format!("{topic}/config"), QoS::AtLeastOnce, true, payload.to_string());
    let _ = client.try_publish(format!("{topic}/status"), QoS::AtLeastOnce, true, "online");
    // The hostname only to a broker the bench owns — the rule beat.rs and
    // mqtt.rs follow, for the reason mqtt.rs::broker_is_local gives.
    if broker_is_ours {
        if let Some(machine) = machine_name() {
            let _ = client.try_publish(format!("{topic}/host"), QoS::AtLeastOnce, true, machine);
        }
    }
    println!("📡 [{token}] config published retained to {topic}/config (+ /status=online)");
}

/// Device panels, retained; an empty payload clears one. `try_publish`, for the
/// reason the telemetry tick gives.
fn publish_panels(client: &Client, out: Vec<panels::Publication>) {
    for (topic, payload) in out {
        let _ = client.try_publish(topic, QoS::AtLeastOnce, true, payload);
    }
}

/// `<module>/incoming/api` — the whole self-description, retained.
///
/// THE LIVE HALF IS READ AT THE MOMENT OF PUBLISHING, inside the session thread
/// that already holds both locks: the presence set and the engagement register
/// are what make this a STATUS document rather than a manifest, and reading
/// them anywhere else would need a second pair of locks for the same two facts.
/// `try_publish`, never `publish`, for the reason the telemetry tick gives — a
/// full queue must drop a description, not wedge the session that serves the
/// self test.
fn publish_api(
    client: &Client,
    token: &str,
    context: &api::Context,
    config: &Value,
    register: &Mutex<Engagement>,
    presence: &Mutex<BTreeSet<String>>,
) {
    let live = api::Liveness {
        last_tick_age_s: stalest_tick().map(|(_, age)| age),
        presence_online: presence.lock().unwrap_or_else(|e| e.into_inner()).len(),
        engaged: register.lock().unwrap_or_else(|e| e.into_inner()).is_engaged(token),
    };
    let document = api::document(token, context, config, &live);
    if let Ok((topic, retain)) = baremetal_io::module_api(token) {
        let _ = client.try_publish(topic, QoS::AtLeastOnce, retain, document.to_string());
    }
}

/// Take the setpoint and put the withdrawal on the bus. The same steps as the
/// orchestrator's `self_test::engage`, for one module.
fn engage(
    client: &Client,
    register: &Mutex<Engagement>,
    presence: &Mutex<BTreeSet<String>>,
    token: &str,
    payload: &str,
) {
    let moved = match register.lock().unwrap_or_else(|e| e.into_inner()).set(token, Engaged::parse(payload)) {
        Ok(moved) => moved,
        Err(e) => {
            eprintln!("⚠️  [{token}] not a legal module name: {e:?}");
            return;
        }
    };
    let Some(withdrawal) = moved else {
        return;
    };
    for (topic, payload, retain) in withdrawal.publications().unwrap_or_default() {
        let _ = client.try_publish(topic, QoS::AtLeastOnce, retain, payload);
    }
    if let Some(state) = withdrawal.agent_state() {
        if let Ok(report) = AgentReport::new(token, state, "withdrawn onto the workbench — findings are not live") {
            for (topic, payload, retain) in report.publications().unwrap_or_default() {
                let _ = client.try_publish(topic, QoS::AtLeastOnce, retain, payload);
            }
        }
    }
    if withdrawal.engaged() {
        let stamp = withdrawal.device_last_online(unix_now());
        let topics: Vec<String> = presence.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
        for topic in &topics {
            let _ = client.try_publish(topic.clone(), QoS::AtLeastOnce, true, stamp.clone());
        }
        println!("🧪 [{token}] withdrawn — {} device(s) taken offline", topics.len());
    } else {
        println!("🧪 [{token}] released — its own next announcement restores it");
    }
}

/// Run the crate's self test and publish what it concluded.
fn perform(client: &Client, register: &Mutex<Engagement>, token: &str, door: fn(&str) -> String, payload: &str) {
    let Some(depth) = Depth::parse(payload) else {
        let report = Report::refused(
            token,
            Depth::Own,
            &format!("`{}` is not a depth — use self, bus, device or all", payload.trim()),
        );
        publish_report(client, &report);
        return;
    };
    // `running` BEFORE the run, so a test that wedges leaves the bus saying so.
    if let Ok((topic, retain)) =
        baremetal_io::module_incoming(token, apkaudio_contracts::self_test::LEAF_RESULT)
    {
        let _ = client.try_publish(
            topic,
            QoS::AtLeastOnce,
            retain,
            apkaudio_contracts::self_test::Verdict::Running.as_str(),
        );
    }
    let permitted = register.lock().unwrap_or_else(|e| e.into_inner()).permit(token, depth);
    let mut report = match permitted {
        Err(refusal) => refusal,
        Ok(()) => Report::from_json(&door(depth.as_str())).unwrap_or_else(|why| {
            Report::refused(token, depth, &format!("its report did not parse: {why}"))
        }),
    };
    report.record(Check::pass(
        "the_broker_session_carried_this_press",
        Depth::Bus,
        "the run request arrived over the session that answers it",
    ));
    println!("🧪 [{token}] {}", report.headline());
    publish_report(client, &report);
}

fn publish_report(client: &Client, report: &Report) {
    for (topic, payload, retain) in report.publications().unwrap_or_default() {
        let _ = client.try_publish(topic, QoS::AtLeastOnce, retain, payload);
    }
}

/// `config.ini` as `{section: {key: value}}` — the shape `mqtt.rs` published
/// through rust-ini. Written out here rather than linked: the files are flat
/// `key = value` under `[section]`, and one parser dependency per plugin
/// binary is twenty to keep in step for a grammar this small.
pub fn ini_to_json(text: &str) -> Value {
    let mut root = Map::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_string();
            root.entry(section.clone()).or_insert_with(|| Value::Object(Map::new()));
            continue;
        }
        let Some((key, value)) = line.split_once('=').or_else(|| line.split_once(':')) else {
            continue;
        };
        if let Value::Object(map) = root
            .entry(section.clone())
            .or_insert_with(|| Value::Object(Map::new()))
        {
            map.insert(key.trim().to_string(), Value::String(value.trim().to_string()));
        }
    }
    Value::Object(root)
}

/// The section's `topic` key, else `APK.audio/System/Protocols/<token>`.
///
/// The section is matched without case, for the reason `mqtt.rs::module_topic`
/// spends a page on: `[AES70]` and `aes70` are one module and MQTT topics are
/// case-sensitive, so the token's own spelling is what the default uses.
pub fn module_topic(config: &Value, token: &str) -> String {
    config
        .as_object()
        .and_then(|sections| {
            sections
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(token))
                .and_then(|(_, section)| section.get("topic"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("APK.audio/System/Protocols/{token}"))
}

/// Syntactic, no DNS — a copy of `broker_is_local` in
/// `NODE.BareMetal/SRC/Core/ORCHESTRATOR:hunter:VISA/src/mqtt.rs`,
/// which this process does not link. Anything not obviously ours is public.
pub fn broker_is_local(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost")
        || host.eq_ignore_ascii_case("broker")
        || host.ends_with(".local")
        || host.ends_with(".localhost")
    {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        Ok(std::net::IpAddr::V6(v6)) => {
            v6.is_loopback()
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
        Err(_) => false,
    }
}

/// Where the tick leaves its timestamp for `apk-plugin-health`.
///
/// /tmp, which the compose file mounts as a tmpfs because the container root is
/// read-only. `APK_PLUGIN_HEALTH_FILE` moves it.
pub fn health_file() -> String {
    std::env::var("APK_PLUGIN_HEALTH_FILE").unwrap_or_else(|_| "/tmp/apk-plugin.alive".to_string())
}

/// The `apk-plugin-health` probe as a function: 0 when the runner's telemetry
/// tick wrote the health file within `APK_PLUGIN_HEALTH_MAX_AGE` (60 s), 1
/// otherwise, with the reason on stderr. A worker in its own workspace ships a
/// one-line `fn main() { std::process::exit(apk_plugin_runner::health_check()) }`
/// instead of compiling this crate's bin file by path (P08 handoff).
///
/// A file that says `not built …` is the image's stub for a crate that failed
/// to compile (Dockerfile, PLAN-3398.01): reported as such, not as "missing".
pub fn health_check() -> i32 {
    let path = health_file();
    let max_age: u64 = std::env::var("APK_PLUGIN_HEALTH_MAX_AGE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let contents = std::fs::read_to_string(&path).ok();
    if let Some(text) = contents.as_deref().map(str::trim) {
        if text.starts_with("not built") {
            eprintln!("{text}");
            return 1;
        }
    }
    match contents.and_then(|s| s.trim().parse::<u64>().ok()) {
        Some(then) if unix_now().saturating_sub(then) <= max_age => 0,
        Some(then) => {
            eprintln!("stale: {path} is {}s old (limit {max_age}s)", unix_now().saturating_sub(then));
            1
        }
        None => {
            eprintln!("missing: {path} has no timestamp yet");
            1
        }
    }
}

fn write_health_file() {
    let _ = std::fs::write(health_file(), unix_now().to_string());
}

fn machine_name() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

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
    ($(kind = $kind:literal,)? token = $token:literal, lib = $lib:ident, run = [$($run:ident),+ $(,)?], panels = [$($family:ident),+ $(,)?] $(,)?) => {
        fn main() {
            $($crate::stamp_plugin_kind($kind);)?
            $($crate::panel!($family);)+
            $crate::__main_body!($token, $lib, [$($run),+]);
        }
    };
    ($(kind = $kind:literal,)? token = $token:literal, lib = $lib:ident, run = [$($run:ident),+ $(,)?] $(,)?) => {
        fn main() {
            $($crate::stamp_plugin_kind($kind);)?
            $crate::__main_body!($token, $lib, [$($run),+]);
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
mod tests {
    use super::*;

    #[test]
    fn no_argument_starts_the_agent_and_any_argument_stops_it_first() {
        let none: Vec<String> = vec![];
        assert_eq!(answer_arguments(none, &["nmos"], &["browse"]), None);
        assert_eq!(answer_arguments(vec!["--help".into()], &["nmos"], &["browse"]), Some(0));
        assert_eq!(answer_arguments(vec!["-h".into()], &["nmos"], &["browse"]), Some(0));
        assert_eq!(answer_arguments(vec!["--version".into()], &["nmos"], &["browse"]), Some(0));
        assert_eq!(answer_arguments(vec!["--port=1".into()], &["nmos"], &["browse"]), Some(2));
    }

    #[test]
    fn the_pod_is_the_pod_folder_on_the_path() {
        assert_eq!(pod_of("/src/PODS/POD.THIN/Softapps/DATATYPE/DATATYPE.wireless.CSV/SRC"), Some("POD.THIN"));
        assert_eq!(pod_of("/home/x/APK.audio/PODS/POD.THICK/PLUGINS/PLUGIN.DANTE/SRC"), Some("POD.THICK"));
        assert_eq!(pod_of("/src/somewhere/else"), None);
        // Unstamped, a binary takes the runner's own pod: this crate's.
        assert_eq!(plugin_pod(), Some("POD.THICK"));
    }

    #[test]
    fn a_config_ini_becomes_sections_of_strings() {
        let v = ini_to_json("# c\n[dante]\nenabled = true\ndomain = local.\n; x\n[other]\nk=v\n");
        assert_eq!(v["dante"]["enabled"], "true");
        assert_eq!(v["dante"]["domain"], "local.");
        assert_eq!(v["other"]["k"], "v");
    }

    #[test]
    fn the_topic_is_the_sections_own_or_the_tokens_default() {
        let declared = ini_to_json("[PTP]\ntopic = APK.audio/System/Protocols/ptp\n");
        assert_eq!(module_topic(&declared, "ptp"), "APK.audio/System/Protocols/ptp");
        let bare = ini_to_json("[dante]\nenabled = true\n");
        assert_eq!(module_topic(&bare, "dante"), "APK.audio/System/Protocols/dante");
    }

    #[test]
    fn the_stalest_armed_clock_is_judged_and_unarmed_ones_are_not() {
        // control is wedged while browse keeps ticking: control is the answer.
        let clocks = [("browse", 995), ("control", 800), ("idle", 0)];
        assert_eq!(stalest(clocks.into_iter(), 1000), Some(("control", 200)));
        assert_eq!(stalest([("idle", 0)].into_iter(), 1000), None);
    }

    #[test]
    fn a_tick_moves_only_its_own_agents_clock() {
        let _ = AGENT_TICKS.set(vec![("a", AtomicU64::new(0)), ("b", AtomicU64::new(0))]);
        std::thread::spawn(|| {
            AGENT_SLOT.with(|s| s.set(Some(1)));
            tick();
        })
        .join()
        .unwrap();
        let ticks = AGENT_TICKS.get().unwrap();
        assert_eq!(ticks[0].1.load(Ordering::Relaxed), 0);
        assert_ne!(ticks[1].1.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_public_broker_is_not_ours() {
        assert!(broker_is_local("127.0.0.1"));
        assert!(broker_is_local("broker"));
        assert!(!broker_is_local("test.mosquitto.org"));
    }
}
