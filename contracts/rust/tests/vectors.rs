// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Golden-vector suite — consumes the SAME ../../vectors/topics.json as the
//! TypeScript tests. Add cases to the vector file, not here.

use apkaudio_contracts::agent_counters;
use apkaudio_contracts::topics;
use serde_json::Value;

static VECTORS: &str = include_str!("../../vectors/topics.json");

fn vectors() -> Value {
    serde_json::from_str(VECTORS).expect("vectors/topics.json parses")
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_else(|| panic!("missing string {key} in {v}"))
}

/// The `path` argument of every family that takes one, defaulting to empty.
fn path_argument(args: &Value) -> Vec<&str> {
    args["path"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().expect("path segment is a string")).collect())
        .unwrap_or_default()
}

fn build(family: &str, args: &Value) -> Result<String, topics::TopicError> {
    match family {
        "webrtcSignal" => topics::webrtc_signal(s(args, "room"), s(args, "role")),
        "webrtcSignalWildcard" => topics::webrtc_signal_wildcard(args["room"].as_str()),
        "audioCue" => topics::audio_cue(s(args, "name")),
        "audioEngine" => topics::audio_engine(s(args, "app"), s(args, "leaf")),
        "audioEngineWildcard" => topics::audio_engine_wildcard(args["app"].as_str()),
        "yoSpeech" => topics::yo_speech(s(args, "leaf")),
        "yoAlarm" => Ok(topics::yo_alarm()),
        "yoStatus" => Ok(topics::yo_status()),
        "yoAnnounce" => Ok(topics::yo_announce()),
        "yoEcho" => Ok(topics::yo_echo()),
        "yoChatInput" => topics::yo_chat_input(s(args, "room")),
        "yoVoiceCapture" => Ok(topics::yo_voice_capture()),
        "midiControl" => topics::midi_control(s(args, "device"), s(args, "channel"), s(args, "control")),
        "midiWildcard" => topics::midi_wildcard(args["device"].as_str()),
        "timeMark" => Ok(topics::time_mark()),
        "discovery" => topics::discovery(s(args, "protocol"), s(args, "deviceId")),
        "discoveryWildcard" => topics::discovery_wildcard(args["protocol"].as_str()),
        "guiWildcard" => Ok(topics::gui_wildcard()),
        "guiState" => topics::gui_state(s(args, "stateRoot"), &path_argument(args)),
        "guiStateWildcard" => topics::gui_state_wildcard(s(args, "stateRoot")),
        "komandCmd" => topics::komand_cmd(s(args, "verb"), s(args, "deviceClass"), s(args, "model")),
        "komandState" => topics::komand_state(s(args, "deviceClass"), s(args, "model"), s(args, "capability")),
        "komandMonitor" => topics::komand_monitor(s(args, "dir")),
        "komandClockArmed" => Ok(topics::komand_clock_armed()),
        "tests" => {
            let path: Vec<&str> = args["path"]
                .as_array()
                .map(|a| a.iter().map(|v| v.as_str().expect("path segment is a string")).collect())
                .unwrap_or_default();
            topics::tests(s(args, "suite"), &path)
        }
        "testsPrefix" => topics::tests_prefix(s(args, "suite")),
        "testsWildcard" => topics::tests_wildcard(args["suite"].as_str()),
        "agents" => topics::agents(s(args, "agent")),
        "agentsWildcard" => Ok(topics::agents_wildcard()),
        "config" => topics::config(s(args, "agent")),
        "log" => topics::log(s(args, "source"), s(args, "level")),
        "dataMigration" => topics::data_migration(s(args, "source"), s(args, "key")),
        "dataMigrationWildcard" => Ok(topics::data_migration_wildcard()),
        "repositoryManager" => {
            let path: Vec<&str> = args["path"]
                .as_array()
                .map(|a| a.iter().map(|v| v.as_str().expect("path segment is a string")).collect())
                .unwrap_or_default();
            topics::repository_manager(&path)
        }
        "repositoryManagerWildcard" => Ok(topics::repository_manager_wildcard()),
        "capturePoint" => topics::capture_point(s(args, "category"), s(args, "key"), s(args, "terminal")),
        "capturePointSweep" => Ok(topics::capture_point_sweep()),
        "capturePointPoke" => Ok(topics::capture_point_poke()),
        "capturePointsWildcard" => Ok(topics::capture_points_wildcard()),
        "capture" => topics::capture(s(args, "kind"), s(args, "capture")),
        "capturesWildcard" => Ok(topics::captures_wildcard()),
        "protocols" => topics::protocols(s(args, "proto"), &path_argument(args)),
        "protocolsScanLog" => topics::protocols_scan_log(s(args, "proto")),
        "protocolsWildcard" => Ok(topics::protocols_wildcard()),
        "fileBrowser" => topics::file_browser(s(args, "key")),
        "discoveryVerified" => topics::discovery_verified(s(args, "protocol"), s(args, "deviceId")),
        "discoveryRecord" => topics::discovery_record(s(args, "protocol"), s(args, "deviceId")),
        "discoveryEngine" => topics::discovery_engine(s(args, "leaf"), &path_argument(args)),
        "discoveryLifecycle" => Ok(topics::discovery_lifecycle()),
        "discoveryDeath" => Ok(topics::discovery_death()),
        "guiPanels" => topics::gui_panels(s(args, "key")),
        "guiPanelsWildcard" => Ok(topics::gui_panels_wildcard()),
        // Minted by agent_counters rather than topics: the payload and its
        // topic are one contract, and the family has no parse arm on either
        // side. See the module docs there.
        "agentCounters" => agent_counters::counters_topic(s(args, "agent")),
        "agentCountersWildcard" => Ok(agent_counters::counters_wildcard()),
        "bareMetal" => topics::baremetal(s(args, "target"), &path_argument(args)),
        "bareMetalCommand" => topics::baremetal_command(s(args, "target"), &path_argument(args)),
        "bareMetalStream" => {
            topics::baremetal_stream(s(args, "target"), &path_argument(args), s(args, "leaf"))
        }
        "bareMetalWildcard" => topics::baremetal_wildcard(args["target"].as_str()),
        "guiTopic" => topics::gui(&path_argument(args)),
        "guiChatMessage" => Ok(topics::gui_chat_message()),
        "guiChatPresence" => topics::gui_chat_presence(s(args, "clientId")),
        "guiChatSos" => topics::gui_chat_sos(s(args, "caller")),
        "guiSqlStatus" => topics::gui_sql_status(s(args, "field")),
        "guiSqlCapture" => topics::gui_sql_capture(s(args, "field")),
        "guiSqlWildcard" => Ok(topics::gui_sql_wildcard()),
        "visaDevice" => topics::visa_device(s(args, "family"), s(args, "model"), s(args, "dev")),
        "visaWrite" => topics::visa_write(s(args, "family"), s(args, "model"), s(args, "dev")),
        "visaWriteWildcard" => Ok(topics::visa_write_wildcard()),
        "visaReplyWildcard" => Ok(topics::visa_reply_wildcard()),
        "midiDevice" => Ok(topics::midi_device()),
        "midiOutputWildcard" => Ok(topics::midi_output_wildcard()),
        "games" => topics::games(s(args, "game"), s(args, "leaf")),
        "gamesBingoDrawn" => Ok(topics::games_bingo_drawn()),
        "gamesWildcard" => topics::games_wildcard(args["game"].as_str()),
        "cepo" => topics::cepo(s(args, "unit"), &path_argument(args)),
        "cepoWildcard" => topics::cepo_wildcard(args["unit"].as_str()),
        "containerManagerEvent" => Ok(topics::container_manager_events()),
        "containerManagerStatus" => Ok(topics::container_manager_status()),
        "containerManagerManager" => Ok(topics::container_manager_manager()),
        "containerManagerContainer" => topics::container_manager_container(s(args, "stack"), s(args, "name")),
        "containerManagerApp" => topics::container_manager_app(s(args, "stack"), s(args, "name"), s(args, "app")),
        "containerManagerContainersWildcard" => topics::container_manager_containers_wildcard(args["stack"].as_str()),
        "containerManagerDockerRequest" => Ok(topics::container_manager_docker_request()),
        "containerManagerDockerReply" => topics::container_manager_docker_reply(s(args, "client")),
        "containerManagerDockerReplyWildcard" => Ok(topics::container_manager_docker_reply_wildcard()),
        "systemChat" => Ok(topics::system_chat()),
        "bareMetalEsp32Heartbeat" => topics::baremetal_esp32_heartbeat(s(args, "node")),
        "nearestRustAgentHandOff" => Ok(topics::nearest_rust_agent_handoff()),
        "census" => topics::census(s(args, "node"), &path_argument(args)),
        "censusWildcard" => topics::census_wildcard(args["node"].as_str()),
        "guiDiscovered" => topics::gui_discovered(&path_argument(args)),
        "guiDiscoveredWildcard" => Ok(topics::gui_discovered_wildcard()),
        "deviceGroupsState" => Ok(topics::device_groups_state()),
        "deviceGroupsCommand" => Ok(topics::device_groups_command()),
        "hourChimeState" => Ok(topics::hour_chime_state()),
        "hourChimeStruck" => Ok(topics::hour_chime_struck()),
        "captureFiled" => Ok(topics::capture_filed()),
        "systemWrites" => Ok(topics::system_writes()),
        "clientBreadcrumb" => Ok(topics::client_breadcrumb()),
        "benchTwoState" => Ok(topics::bench_two_state()),
        "systemWildcard" => Ok(topics::system_wildcard()),
        "visaReadWildcard" => Ok(topics::visa_read_wildcard()),
        "visaReadingWildcard" => topics::visa_reading_wildcard(args["family"].as_str()),
        "visaReading" => topics::visa_reading(s(args, "family"), s(args, "model"), s(args, "dev"), s(args, "command")),
        "visaReadingAcross" => topics::visa_reading_across(s(args, "family"), s(args, "command")),
        other => panic!("vector family not implemented in Rust: {other}"),
    }
}

#[test]
fn build_vectors() {
    for v in vectors()["build"].as_array().expect("build array") {
        let family = s(v, "family");
        let topic = build(family, &v["args"]).unwrap_or_else(|e| panic!("{family}: {e}"));
        assert_eq!(topic, s(v, "topic"), "family {family}");
    }
}

#[test]
fn build_invalid_vectors() {
    for v in vectors()["buildInvalid"].as_array().expect("buildInvalid array") {
        let family = s(v, "family");
        let why = s(v, "why");
        assert!(build(family, &v["args"]).is_err(), "{family} should reject: {why}");
    }
}

#[test]
fn parse_vectors() {
    for v in vectors()["parse"].as_array().expect("parse array") {
        let topic = s(v, "topic");
        let got = serde_json::to_value(topics::parse(topic)).expect("serialize parse result");
        assert_eq!(got, v["parsed"], "parse {topic}");
    }
}

#[test]
fn gui_from_panel_path_vectors() {
    for v in vectors()["guiFromPanelPath"].as_array().expect("guiFromPanelPath array") {
        let path = s(v, "filePath");
        assert_eq!(topics::gui_prefix_from_panel_path(path), s(v, "topic"), "path {path:?}");
    }
}

#[test]
fn declared_filter_vectors() {
    let all = vectors();
    let cases = all["filters"].as_array().expect("filters array");
    for v in cases {
        let filter = s(v, "filter");
        let declared = v["declared"].as_bool().expect("declared is a bool");
        assert_eq!(topics::is_declared_filter(filter), declared, "isDeclaredFilter {filter}: {}", s(v, "why"));
    }
    for f in topics::DECLARED_FILTERS {
        assert!(cases.iter().any(|v| v["filter"] == f && v["declared"] == true), "{f} has no vector");
        assert_eq!(serde_json::to_value(topics::parse(f)).expect("serialize")["kind"], "unknown", "{f} is a filter, not a topic");
    }
}

#[test]
fn is_legacy_matches_parse_kind() {
    for v in vectors()["parse"].as_array().expect("parse array") {
        let topic = s(v, "topic");
        let expected = v["parsed"]["kind"] == "legacy";
        assert_eq!(topics::is_legacy(topic), expected, "isLegacy {topic}");
    }
}
