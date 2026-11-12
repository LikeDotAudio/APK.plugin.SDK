// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The one broker session a module runs: config on connect, the self-test
//! inbox, the telemetry tick, its device panels and `incoming/api`.
//! Split out of lib.rs (ROAD-3749).

use super::*;

/// The one broker session: config on connect, the self-test inbox, the
/// telemetry tick. Returns the handle the watchdog polls.
pub(crate) fn spawn_session(
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
    // THE REPORTS RIDE THIS SESSION (ROAD-3695): elog! lines and the
    // problems below go out as LogLines / ProblemRecords under this token.
    report::install(&client, token);
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
            retry_panels(&client);   // what a full queue refused last time goes first
            let expired = book.lock().unwrap_or_else(|e| e.into_inner()).expire(Instant::now());
            publish_panels(&client, expired);
            let sample = tick_sample(started.elapsed().as_secs());
            if let Ok((t, retain)) = baremetal_io::module_incoming(token, "telemetry") {
                // Counted like every other publish: a full queue drops this
                // sample, and the drop is a `publish_failures`, not silence.
                loopback::published(client.try_publish(t, QoS::AtMostOnce, retain, sample.to_string()).is_ok());
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
                        // Up again: whatever an earlier process of this plugin
                        // said on its way down is over.
                        report::clear_any(report::DOWN, Some(token));
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
pub(crate) fn publish_config(client: &Client, topic: &str, token: &str, config: &Value, broker_is_ours: bool) {
    let payload = json!({
        "value": config,
        "path": format!("{token}/config.ini"),
        "source": "plugin",
    });
    loopback::published(client.try_publish(format!("{topic}/config"), QoS::AtLeastOnce, true, payload.to_string()).is_ok());
    loopback::published(client.try_publish(format!("{topic}/status"), QoS::AtLeastOnce, true, "online").is_ok());
    // The hostname only to a broker the bench owns — the rule beat.rs and
    // mqtt.rs follow, for the reason mqtt.rs::broker_is_local gives.
    if broker_is_ours {
        if let Some(machine) = machine_name() {
            loopback::published(client.try_publish(format!("{topic}/host"), QoS::AtLeastOnce, true, machine).is_ok());
        }
    }
    println!("📡 [{token}] config published retained to {topic}/config (+ /status=online)");
}

/// Device panels, retained; an empty payload clears one. `try_publish`, for the
/// reason the telemetry tick gives.
/// Panel publications the client's queue refused, newest per topic, retried on the next tick.
///
/// ROAD-3566: a panel change is said ONCE — a repaint, a clear when a device goes or an
/// earlier process's panel is not claimed — and `try_publish` drops it when the request
/// queue is full. At start the runner restates and reads back thousands of retained panels,
/// the queue fills, and the NMOS runner counted 6,785 publish_failures: an orphan panel's
/// clear and a node's repaint were simply lost, so a dead node's panel stayed retained with
/// whatever identity it last had. A refused publication now waits here instead.
pub(crate) static PENDING_PANELS: Mutex<Vec<panels::Publication>> = Mutex::new(Vec::new());

pub(crate) fn publish_panels(client: &Client, out: Vec<panels::Publication>) {
    let mut pending = PENDING_PANELS.lock().unwrap_or_else(|e| e.into_inner());
    send_panels(&mut pending, out, |topic, payload| client.try_publish(topic, QoS::AtLeastOnce, true, payload).is_ok());
}

/// Send each publication. A refused one waits in `pending` (newest per topic); one that
/// went out supersedes anything for its topic still waiting, so a retry never overwrites
/// a newer panel with an older one.
pub(crate) fn send_panels(pending: &mut Vec<panels::Publication>, out: Vec<panels::Publication>, send: impl FnMut(String, String) -> bool) {
    let mut sent = Vec::new();
    let refused = try_panels(out.clone(), send);
    for (topic, _) in &out {
        if !refused.iter().any(|(t, _)| t == topic) {
            sent.push(topic.clone());
        }
    }
    pending.retain(|(t, _)| !sent.contains(t));
    hold_back(pending, refused);
}

/// Retry what was refused, oldest change first; what is refused again waits again.
pub(crate) fn retry_panels(client: &Client) {
    let waiting = std::mem::take(&mut *PENDING_PANELS.lock().unwrap_or_else(|e| e.into_inner()));
    if !waiting.is_empty() {
        publish_panels(client, waiting);
    }
}

/// Send each publication; return the ones `send` refused (counted, as before).
pub(crate) fn try_panels(out: Vec<panels::Publication>, mut send: impl FnMut(String, String) -> bool) -> Vec<panels::Publication> {
    let mut refused = Vec::new();
    for (topic, payload) in out {
        let ok = send(topic.clone(), payload.clone());
        loopback::published(ok);
        if !ok {
            refused.push((topic, payload));
        }
    }
    refused
}

/// Queue refused publications, keeping only the newest for a topic: a later repaint or
/// clear of the same panel supersedes the one that waited.
pub(crate) fn hold_back(pending: &mut Vec<panels::Publication>, refused: Vec<panels::Publication>) {
    for (topic, payload) in refused {
        pending.retain(|(t, _)| *t != topic);
        pending.push((topic, payload));
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
pub(crate) fn publish_api(
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
        loopback::published(client.try_publish(topic, QoS::AtLeastOnce, retain, document.to_string()).is_ok());
    }
}

/// Take the setpoint and put the withdrawal on the bus. The same steps as the
/// orchestrator's `self_test::engage`, for one module.
pub(crate) fn engage(
    client: &Client,
    register: &Mutex<Engagement>,
    presence: &Mutex<BTreeSet<String>>,
    token: &str,
    payload: &str,
) {
    let moved = match register.lock().unwrap_or_else(|e| e.into_inner()).set(token, Engaged::parse(payload)) {
        Ok(moved) => moved,
        Err(e) => {
            elog!("⚠️  [{token}] not a legal module name: {e:?}");
            return;
        }
    };
    let Some(withdrawal) = moved else {
        return;
    };
    for (topic, payload, retain) in withdrawal.publications().unwrap_or_default() {
        loopback::published(client.try_publish(topic, QoS::AtLeastOnce, retain, payload).is_ok());
    }
    if let Some(state) = withdrawal.agent_state() {
        if let Ok(report) = AgentReport::new(token, state, "withdrawn onto the workbench — findings are not live") {
            for (topic, payload, retain) in report.publications().unwrap_or_default() {
                loopback::published(client.try_publish(topic, QoS::AtLeastOnce, retain, payload).is_ok());
            }
        }
    }
    if withdrawal.engaged() {
        let stamp = withdrawal.device_last_online(unix_now());
        let topics: Vec<String> = presence.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
        for topic in &topics {
            loopback::published(client.try_publish(topic.clone(), QoS::AtLeastOnce, true, stamp.clone()).is_ok());
        }
        println!("🧪 [{token}] withdrawn — {} device(s) taken offline", topics.len());
    } else {
        println!("🧪 [{token}] released — its own next announcement restores it");
    }
}

/// Run the crate's self test and publish what it concluded.
pub(crate) fn perform(client: &Client, register: &Mutex<Engagement>, token: &str, door: fn(&str) -> String, payload: &str) {
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
        loopback::published(client.try_publish(
            topic,
            QoS::AtLeastOnce,
            retain,
            apkaudio_contracts::self_test::Verdict::Running.as_str(),
        ).is_ok());
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
    report::checks(token, &report);
    publish_report(client, &report);
}

pub(crate) fn publish_report(client: &Client, report: &Report) {
    for (topic, payload, retain) in report.publications().unwrap_or_default() {
        loopback::published(client.try_publish(topic, QoS::AtLeastOnce, retain, payload).is_ok());
    }
}
