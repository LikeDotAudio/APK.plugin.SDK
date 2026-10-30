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
fn is_legacy_matches_parse_kind() {
    for v in vectors()["parse"].as_array().expect("parse array") {
        let topic = s(v, "topic");
        let expected = v["parsed"]["kind"] == "legacy";
        assert_eq!(topics::is_legacy(topic), expected, "isLegacy {topic}");
    }
}
