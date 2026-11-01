// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The plugin's answer to "describe yourself completely".
//!
//! ONE DOCUMENT, RETAINED, AT `<module>/incoming/api`. Everything this process
//! knows about itself: who it is, what it was built from, the config it
//! actually loaded (not the one on disk — the one in the binary, or the file
//! `APK_PLUGIN_CONFIG` pointed at), which broker it is on, which agent threads
//! are running, how alive it is, what presence it tracks, and every topic it
//! owns. A reader that subscribes gets it with NO REQUEST, because retained is
//! the whole point; `outgoing/api_get` only asks for it to be taken again.
//!
//! WHY IT IS HERE AND NOT IN EACH PLUGIN. Twenty-three crates link this runner
//! and none of them should carry a copy of "what does a plugin look like" —
//! that is the same argument `serve` already makes about the config publish,
//! the last will and the self-test door. A plugin gains this API by being
//! rebuilt, with no edit of its own.
//!
//! WHAT IS DELIBERATELY NOT IN IT:
//!   · LIVE COUNTERS. `uptime_s` and `last_tick_age_s` are here because they
//!     are cheap and a reader wants them in the same breath as the rest, but
//!     the sampled series is `incoming/telemetry` and stays there. A retained
//!     document that changed every fifteen seconds would be a sample wearing a
//!     conclusion's clothes, and every subscriber would wake for it.
//!   · SECRETS. `broker_auth::credentials()` is reported as a BOOLEAN — whether
//!     this session authenticates — and never as a username. A document that
//!     is retained on a broker is a document that outlives the process, and
//!     `topics.sh` prints it.
//!   · ANYTHING THE PLUGIN HUNTS FOR. What DANTE found belongs under
//!     `APK.audio/Discovery/dante/…`; this is the plugin describing the
//!     plugin, and mixing the two would make the document unbounded.

use std::time::{Duration, Instant};

use apkaudio_contracts::baremetal_io;
use serde_json::{json, Map, Value};

/// The facts that do not change for the life of the process, gathered once in
/// [`serve_group`](crate::serve_group) and shared with every session thread.
///
/// CLONED PER MODULE rather than referenced, because a grouped process (`nmos`
/// + `nmos_control`) publishes one of these per token and each names its own.
#[derive(Clone)]
pub struct Context {
    /// The broker host this session dialled — the value of `MQTT_HOST`.
    pub broker_host: String,
    /// The broker port — the value of `MQTT_PORT`.
    pub broker_port: u16,
    /// Whether that broker is one the bench owns (the `/host` publish rule).
    pub broker_is_ours: bool,
    /// Whether this session presents credentials. NEVER the credentials.
    pub broker_authenticated: bool,
    /// The agent threads `serve_group` started, by the names it reports them
    /// under. A plugin with none is a plugin that only publishes its config.
    pub agents: Vec<String>,
    /// Every other module in this process, for the grouped case. Empty for the
    /// twenty-two plugins that are one module in one container.
    pub siblings: Vec<String>,
    /// Where the config came from: `compiled-in`, or the path of the file
    /// `APK_PLUGIN_CONFIG` named. A bench that overrode it must be able to see
    /// that it did — this is the field that answers "why is it behaving oddly".
    pub config_source: String,
    /// Seconds without a [`tick`](crate::tick) that count as a stall.
    pub stall_after_s: u64,
    /// How often telemetry is sampled and the health file rewritten.
    pub tick_s: u64,
    /// When the process started, for `uptime_s`.
    pub started: Instant,
    /// The PLUGIN crate's own `CARGO_PKG_NAME` / `CARGO_PKG_VERSION`
    /// (PLAN-3407.01), stamped by [`stamp!`](crate::stamp) in the plugin's
    /// `main`. `None` for a binary that has not stamped itself — published as
    /// `null`, never as the runner's version wearing the plugin's name.
    pub plugin_crate: Option<&'static str>,
    pub plugin_version: Option<&'static str>,
    /// `plugin`, or what the binary declared through `main!(kind = …)` — a
    /// worker on this runner is not a plugin, and GET /plugins must not say so.
    pub kind: &'static str,
    /// `APK_POD` when the container sets it, else the `POD.*` folder the
    /// binary's crate lives in, off its source path (`crate::plugin_pod`) —
    /// never a constant, or every worker reads as a THICK plugin.
    pub pod: Option<&'static str>,
}

impl Context {
    /// Everything that can be read off the environment. The caller fills
    /// `agents`, `siblings` and `config_source`, which only it knows.
    pub fn from_environment(
        broker_host: String,
        broker_port: u16,
        broker_is_ours: bool,
        stall_after_s: u64,
        tick: Duration,
    ) -> Self {
        Context {
            broker_host,
            broker_port,
            broker_is_ours,
            broker_authenticated: apkaudio_contracts::broker_auth::credentials().is_some(),
            agents: Vec::new(),
            siblings: Vec::new(),
            config_source: "compiled-in".to_string(),
            stall_after_s,
            tick_s: tick.as_secs(),
            started: Instant::now(),
            plugin_crate: crate::plugin_build().map(|(name, _)| name),
            plugin_version: crate::plugin_build().map(|(_, version)| version),
            kind: crate::plugin_kind(),
            pod: crate::plugin_pod(),
        }
    }
}

/// What the runner knows only at the moment of publishing.
pub struct Liveness {
    /// Seconds since the last [`tick`](crate::tick), or `None` for an agent
    /// family that has never ticked — which is most of them, and is reported
    /// rather than hidden. See the runner's note on what the watchdog cannot
    /// see.
    pub last_tick_age_s: Option<u64>,
    /// Devices this module currently holds online through `presence`.
    pub presence_online: usize,
    /// Whether this module is withdrawn onto the self-test workbench.
    pub engaged: bool,
}

/// The whole self-description, ready to publish.
///
/// TAKES `config` BY REFERENCE AND COPIES IT IN. The same `Value` the `/config`
/// publish uses, so the two can never disagree about what this plugin loaded —
/// the failure that would otherwise be invisible until someone compared two
/// topics by hand.
pub fn document(token: &str, context: &Context, config: &Value, live: &Liveness) -> Value {
    // ⚠️ THE MIDDLE OF THE TUPLE IS THE LEAF, NOT THE DIRECTION. It reads like
    //    a direction at the call site and is `state`, `telemetry`, `api` — so
    //    the first cut of this published `"direction": "api"` and every reader
    //    would have learnt a word that is not one of the two legal ones.
    //    The direction is the segment BEFORE the leaf, taken off the topic that
    //    the contract itself built, rather than re-derived from a second table.
    let mut topics = Map::new();
    if let Ok(spots) = baremetal_io::module_landing_spots(token) {
        for (topic, leaf, retained) in spots {
            let direction = topic.rsplit('/').nth(1).unwrap_or("").to_string();
            topics.insert(topic, json!({
                "leaf": leaf,
                "direction": direction,
                "retained": retained,
            }));
        }
    }

    json!({
        // WHAT IT IS.
        "token": token,
        "kind": context.kind,
        // HOW IT IS RAISED and WHICH PROTOCOLS IT SERVES (contracts/
        // DISCOVERY-LIFECYCLE.md, stage 2), off its own config section. The
        // engine builds its launch table from these instead of a list of its own.
        "raise": crate::module_raise(config, token),
        "serves": crate::module_serves(config, token),
        "pod": context.pod,
        "siblings": context.siblings,

        // WHAT IT WAS BUILT FROM. `image_rev` is stamped by the plugin image
        // build; `unstamped` is the honest answer on a hand-built container,
        // and plugin-base.yml passes the variable through for exactly this.
        // THREE VERSIONS, because they move separately (PLAN-3407.01):
        // `runner_version` expands HERE, in the runner crate, so it is only
        // ever the runner's; `plugin_version` is the plugin crate's own,
        // stamped from its `main`; `contract_version` is the topic set it was
        // built against, which is what decides whether a reader can talk to it.
        "build": {
            "runner_version": env!("CARGO_PKG_VERSION"),
            "plugin_crate": context.plugin_crate,
            "plugin_version": context.plugin_version,
            "contract_version": apkaudio_contracts::CONTRACT_VERSION,
            "image_rev": std::env::var("APK_IMAGE_REV").ok(),
            "node": std::env::var("APK_NODE").ok().filter(|n| !n.is_empty()),
        },

        // WHAT IT IS DOING.
        "process": {
            "pid": std::process::id(),
            "uptime_s": context.started.elapsed().as_secs(),
            "agents": context.agents,
        },

        // HOW ALIVE IT IS. `stall_after_s` is published beside the age so a
        // reader can judge the number without knowing the runner's default.
        "liveness": {
            "last_tick_age_s": live.last_tick_age_s,
            "stall_after_s": context.stall_after_s,
            "tick_s": context.tick_s,
            // Stated rather than implied: a plugin that has never ticked is
            // not stalled, it is one the watchdog can only catch on exit.
            "stall_detection": live.last_tick_age_s.is_some(),
            "health_file": crate::health_file(),
        },

        // WHERE ITS OWN LOOPBACK TELEMETRY ANSWERS (ROAD-3472): `{bind, port,
        // routes}`, or null for a binary that declared no manifest. The
        // address only — the counters are samples and stay off a retained
        // document; NODE.BareMetal's supervisor asks the port for them.
        "loopback": crate::loopback::api_section(),

        // WHERE IT IS CONNECTED. No credential ever appears here.
        "broker": {
            "host": context.broker_host,
            "port": context.broker_port,
            "authenticated": context.broker_authenticated,
            "ours": context.broker_is_ours,
        },

        // WHAT IT WAS CONFIGURED WITH — the config as LOADED, the same value
        // published retained at `<topic>/config`.
        "config": {
            "source": context.config_source,
            "value": config,
        },

        // WHAT IT IS WATCHING, and whether it is on the workbench.
        "presence": { "online_devices": live.presence_online },
        "selftest": { "engaged": live.engaged },

        // EVERY LANDING SPOT IT OWNS, from the contract rather than from a
        // list here — a leaf added to baremetal_io appears in this document
        // without an edit, which is the whole reason module_landing_spots
        // exists.
        "topics": topics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        let mut c = Context::from_environment(
            "127.0.0.1".to_string(),
            1883,
            true,
            120,
            Duration::from_secs(15),
        );
        c.agents = vec!["run_browse_agent".to_string()];
        c
    }

    fn liveness() -> Liveness {
        Liveness { last_tick_age_s: Some(3), presence_online: 2, engaged: false }
    }

    /// THE DOCUMENT NAMES EVERY TOPIC THE MODULE OWNS, and it takes them from
    /// the contract rather than from a list here — so a leaf added to
    /// `baremetal_io` appears in this test without the test being edited.
    #[test]
    fn topics_come_from_the_contract() {
        let doc = document("dante", &context(), &json!({}), &liveness());
        let topics = doc["topics"].as_object().expect("a topic map");
        let expected = baremetal_io::module_landing_spots("dante").expect("spots").len();
        assert_eq!(topics.len(), expected);
        // The two leaves this API is made of are in it, both directions.
        assert!(topics.contains_key("APK.audio/System/Protocols/dante/incoming/api"));
        assert!(topics.contains_key("APK.audio/System/Protocols/dante/outgoing/api_get"));
        let api = &topics["APK.audio/System/Protocols/dante/incoming/api"];
        assert_eq!(api["retained"], json!(true));
        assert_eq!(api["leaf"], json!("api"));
        // THE DIRECTION IS ONE OF TWO WORDS. `module_landing_spots` hands back
        // the LEAF in the middle of its tuple, and reading it as the direction
        // is the mistake this line exists to catch: every spot must say
        // `incoming` or `outgoing` and nothing else.
        assert_eq!(api["direction"], json!("incoming"));

        let get = &topics["APK.audio/System/Protocols/dante/outgoing/api_get"];
        assert_eq!(get["retained"], json!(false));
        assert_eq!(get["leaf"], json!("api_get"));
        assert_eq!(get["direction"], json!("outgoing"));

        for (topic, spot) in topics {
            let direction = spot["direction"].as_str().unwrap_or("");
            assert!(matches!(direction, "incoming" | "outgoing"),
                    "{topic} says direction={direction}, which is not one of the two");
        }
    }

    /// THE CONFIG IN THE DOCUMENT IS THE CONFIG THAT WAS LOADED, byte for byte
    /// the value `<topic>/config` carries. Two topics disagreeing about what a
    /// plugin is configured with is the failure worth a test of its own.
    #[test]
    fn config_is_carried_verbatim_with_its_source() {
        let config = json!({"dante": {"enabled": true, "service_type": "_netaudio-arc._udp"}});
        let doc = document("dante", &context(), &config, &liveness());
        assert_eq!(doc["config"]["value"], config);
        assert_eq!(doc["config"]["source"], json!("compiled-in"));

        let mut overridden = context();
        overridden.config_source = "/etc/apk/dante.ini".to_string();
        let doc = document("dante", &overridden, &config, &liveness());
        assert_eq!(doc["config"]["source"], json!("/etc/apk/dante.ini"));
    }

    /// NO CREDENTIAL EVER REACHES A RETAINED TOPIC. The broker block says
    /// WHETHER this session authenticates and never with what.
    #[test]
    fn broker_block_carries_no_secret() {
        let doc = document("dante", &context(), &json!({}), &liveness());
        let broker = doc["broker"].as_object().expect("a broker block");
        assert_eq!(broker.keys().collect::<Vec<_>>(),
                   vec!["authenticated", "host", "ours", "port"]);
        assert!(broker["authenticated"].is_boolean());
    }

    /// THE THREE VERSIONS ARE THREE FIELDS (PLAN-3407.01). The contract
    /// version is the contracts crate's, and the plugin version is whatever
    /// the plugin stamped — `null` when it did not, never the runner's.
    #[test]
    fn build_names_plugin_and_contract_versions() {
        let mut c = context();
        c.plugin_crate = None;
        c.plugin_version = None;
        let doc = document("dante", &c, &json!({}), &liveness());
        assert_eq!(doc["build"]["contract_version"], json!(apkaudio_contracts::CONTRACT_VERSION));
        assert_eq!(doc["build"]["plugin_version"], json!(null));

        c.plugin_crate = Some("apkaudio-dante");
        c.plugin_version = Some("9.9.9");
        let doc = document("dante", &c, &json!({}), &liveness());
        assert_eq!(doc["build"]["plugin_crate"], json!("apkaudio-dante"));
        assert_eq!(doc["build"]["plugin_version"], json!("9.9.9"));
        assert_eq!(doc["build"]["runner_version"], json!(env!("CARGO_PKG_VERSION")));
    }

    /// KIND AND POD ARE THE BINARY'S, NOT CONSTANTS (LOOSE-runner-kind-pod):
    /// a worker built on this runner in POD.THIN must not read as a THICK plugin.
    #[test]
    fn kind_and_pod_come_from_the_context() {
        let doc = document("dante", &context(), &json!({}), &liveness());
        assert_eq!(doc["kind"], json!("plugin"));
        assert_eq!(doc["pod"], json!("POD.THICK"));

        let mut worker = context();
        worker.kind = "worker";
        worker.pod = Some("POD.THIN");
        let doc = document("wireless_csv", &worker, &json!({}), &liveness());
        assert_eq!(doc["kind"], json!("worker"));
        assert_eq!(doc["pod"], json!("POD.THIN"));
    }

    /// A plugin that has never ticked is not stalled — it is one the watchdog
    /// can only catch on exit, and the document says so rather than implying
    /// it with a missing field.
    #[test]
    fn a_plugin_that_never_ticked_says_so() {
        let live = Liveness { last_tick_age_s: None, presence_online: 0, engaged: false };
        let doc = document("printers", &context(), &json!({}), &live);
        assert_eq!(doc["liveness"]["stall_detection"], json!(false));
        assert_eq!(doc["liveness"]["last_tick_age_s"], json!(null));
        assert_eq!(doc["liveness"]["stall_after_s"], json!(120));
    }
}
