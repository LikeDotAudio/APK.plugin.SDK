// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! `serve` and `serve_group`: the runner's main loop — one session per module,
//! the agents on their threads, and the watchdog that decides when to exit.
//! Split out of lib.rs (ROAD-3749); the public paths stay `apk_plugin_runner::serve`.

use super::*;

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
    report::name(names.first().copied().unwrap_or(token));
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

    // THE LOOPBACK TELEMETRY (ROAD-3472), before the sessions, so the port
    // it bound is in the first `incoming/api` they publish. Nothing happens
    // here for a binary that declared no manifest.
    //
    // `enabled = false` (ROAD-3478) is read HERE, once, for every plugin: the
    // process still connects, describes itself and ticks — so it is seen to be
    // alive and switched off rather than missing — but starts no agent, and
    // says `stub` / "disabled in config" on the loopback and in the tick. A
    // plugin that reads the key itself (nmos-control) is not contradicted: it
    // is simply never started when the answer is no.
    let mut disabled = false;
    if let Some(first) = plugins.first() {
        let config_text = override_path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_else(|| first.config_ini.to_string());
        let config = ini_to_json(&config_text);
        if !module_enabled(&config, first.token) {
            disabled = true;
            loopback::disable(DISABLED_REASON);
            api_context.agents.clear();
            println!("💤 [{token}] enabled = false in config.ini — idling, no agent started");
        }
        loopback::start(first.token, &config);
    }

    // 1 + 2 + 3 + 4: one session per module, each on its own thread.
    let sessions: Vec<JoinHandle<()>> = plugins
        .iter()
        .map(|plugin| {
            let config_text = match &override_path {
                Some(path) => std::fs::read_to_string(path).unwrap_or_else(|e| {
                    elog!("🛑 [{token}] APK_PLUGIN_CONFIG={path} did not read ({e}); using the compiled-in config.ini");
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
    let agents = if disabled { Vec::new() } else { agents };
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
                elog!("🛑 [{token}] could not start {name}: {e}");
                report::going_down(token, &format!("could not start {name}: {e}"));
                std::process::exit(EXIT_AGENT_RETURNED)
            });
        running.push((name, handle));
    }

    loop {
        std::thread::sleep(Duration::from_secs(1));

        if TERMINATE.load(Ordering::Relaxed) {
            let drains = std::mem::take(&mut *DRAINS.lock().unwrap_or_else(|e| e.into_inner()));
            if !drains.is_empty() {
                let ceiling = std::env::var("APK_PLUGIN_DRAIN_SECS")
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .map_or(DRAIN_CEILING, Duration::from_secs);
                let names: Vec<&str> = drains.iter().map(|(name, _)| *name).collect();
                println!("🧹 [{token}] stop requested — draining {} (at most {ceiling:?})", names.join(", "));
                let late = run_drains(token, drains, ceiling);
                if !late.is_empty() {
                    elog!("⚠️  [{token}] drain(s) {} still running after {ceiling:?} — exiting anyway", late.join(", "));
                }
            }
            println!("👋 [{token}] stop requested (SIGTERM/SIGINT) — exiting; the broker sends the last will");
            std::process::exit(EXIT_TERMINATED);
        }

        if sessions.iter().any(JoinHandle::is_finished) {
            elog!("🛑 [{token}] the broker session thread ended; exiting so the container restarts");
            report::going_down(token, "the broker session thread ended (exit 69); Docker restarts it");
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
                    elog!("🛑 [{token}] {name} returned; a hunter that stops is a failure — exiting");
                    report::going_down(token, &format!("{name} returned (exit 70); Docker restarts it"));
                    std::process::exit(EXIT_AGENT_RETURNED);
                }
                Err(_) => {
                    elog!("🛑 [{token}] {name} panicked — exiting");
                    report::going_down(token, &format!("{name} panicked (exit 101); Docker restarts it"));
                    std::process::exit(EXIT_AGENT_PANICKED);
                }
            }
        }

        if let Some((agent, age)) = stalest_tick().filter(|(_, age)| *age > stall_after) {
            elog!("🛑 [{token}] {agent}: no tick for {age}s (limit {stall_after}s) — stalled, exiting");
            report::going_down(token, &format!("{agent}: no tick for {age}s (limit {stall_after}s) — stalled (exit 75); Docker restarts it"));
            std::process::exit(EXIT_STALLED);
        }
    }
}
