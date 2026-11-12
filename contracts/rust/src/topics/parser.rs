// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `topics.rs` — see that file for the whole picture.

use super::*;

pub fn parse(raw: &str) -> ParsedTopic {
    let segs: Vec<&str> = raw.split('/').collect();
    let unknown = ParsedTopic::Unknown { raw: raw.to_string() };
    if segs.first() != Some(&ROOT) || segs.iter().any(|s| s.is_empty()) {
        return unknown;
    }
    if let Some(legacy) = classify_legacy(&segs) {
        return ParsedTopic::Legacy(legacy);
    }
    let s2 = segs.get(2).copied();
    let s3 = segs.get(3).copied();
    let s4 = segs.get(4).copied();
    let s5 = segs.get(5).copied();
    match segs.get(1).copied() {
        Some("Audio") => match (s2, s3, s4, s5) {
            (Some("WebRTC"), Some(room), Some(role), None)
                if is_segment(room) && WEBRTC_SIGNAL_ROLES.contains(&role) =>
            {
                ParsedTopic::WebrtcSignal { room: room.to_string(), role: role.to_string() }
            }
            (Some("Cue"), Some(name), None, None) if is_segment(name) => {
                ParsedTopic::AudioCue { name: name.to_string() }
            }
            (Some("Engine"), Some(app), Some(leaf), None)
                if is_segment(app) && AUDIO_ENGINE_LEAVES.contains(&leaf) =>
            {
                ParsedTopic::AudioEngine { app: app.to_string(), leaf: leaf.to_string() }
            }
            _ => unknown,
        },
        Some("Yo") => match (s2, s3, s4) {
            (Some("speech"), Some(leaf), None) if YO_SPEECH_LEAVES.contains(&leaf) => {
                ParsedTopic::YoSpeech { leaf: leaf.to_string() }
            }
            (Some("alarm"), None, _) => ParsedTopic::YoAlarm,
            (Some("status"), None, _) => ParsedTopic::YoStatus,
            (Some("announce"), None, _) => ParsedTopic::YoAnnounce,
            (Some("echo"), None, _) => ParsedTopic::YoEcho,
            (Some("chat"), Some("input"), Some(room)) if s5.is_none() && is_segment(room) => {
                ParsedTopic::YoChatInput { room: room.to_string() }
            }
            (Some("voice"), Some("capture"), None) => ParsedTopic::YoVoiceCapture,
            _ => unknown,
        },
        Some("MIDI") => match (s2, s3, s4, s5) {
            (Some(device), Some(channel), Some(control), None)
                if is_segment(device) && MIDI_CHANNELS.contains(&channel) && is_segment(control) =>
            {
                ParsedTopic::Midi {
                    device: device.to_string(),
                    channel: channel.to_string(),
                    control: control.to_string(),
                }
            }
            _ => unknown,
        },
        // The whole family, and an exact leaf: `Time/Mark` alone. Anything
        // else under `Time/` is a topic nobody declared, and says so.
        Some("Games") => match (s2, s3, s4) {
            (Some("Bingo"), Some("Drawn"), None) => ParsedTopic::GamesBingoDrawn,
            (Some(game), Some(leaf), None) if is_segment(game) && is_segment(leaf) => {
                ParsedTopic::Games { game: game.to_string(), leaf: leaf.to_string() }
            }
            _ => unknown,
        },
        Some("Cepo") => match (s2, s3) {
            (Some(unit), Some(_)) if is_segment(unit) && segs.iter().skip(3).all(|s| is_segment(s)) => {
                ParsedTopic::Cepo {
                    unit: unit.to_string(),
                    path: segs.iter().skip(3).map(|s| s.to_string()).collect(),
                }
            }
            _ => unknown,
        },
        Some("Time") => match (s2, s3) {
            (Some("Mark"), None) => ParsedTopic::TimeMark,
            (Some("Hour"), None) => ParsedTopic::TimeHour,
            _ => unknown,
        },
        Some("Discovery") => match (s2, s3, s4) {
            (Some("_lifecycle"), None, None) => ParsedTopic::DiscoveryLifecycle,
            (Some("_death"), None, None) => ParsedTopic::DiscoveryDeath,
            (Some("_engine"), Some(leaf), _) if is_segment(leaf) => {
                let path: Vec<String> = segs[4..].iter().map(|s| s.to_string()).collect();
                if path.iter().all(|s| is_segment(s)) {
                    ParsedTopic::DiscoveryEngine { leaf: leaf.to_string(), path }
                } else {
                    unknown
                }
            }
            (Some(p), Some(id), Some("verified")) if segs.len() == 5 && is_segment(p) && is_device_id(id) => {
                ParsedTopic::DiscoveryVerified { protocol: p.to_string(), device_id: id.to_string() }
            }
            (Some(p), Some(id), Some("record")) if segs.len() == 5 && is_segment(p) && is_device_id(id) => {
                ParsedTopic::DiscoveryRecord { protocol: p.to_string(), device_id: id.to_string() }
            }
            (Some(p), Some(id), None) if is_segment(p) && is_device_id(id) => ParsedTopic::Discovery {
                protocol: p.to_string(),
                device_id: id.to_string(),
            },
            _ => unknown,
        },
        // The shell's retained seats FIRST, then the retained state roots, then
        // the general form — so a claim keyed by a client can never be read as a
        // gesture. The arm order match-rules.ts pins.
        Some("Gui") => {
            let tail: Vec<&str> = segs.iter().skip(2).copied().collect();
            let seat = if tail.len() >= 2 {
                let leaf = tail[tail.len() - 2];
                let client_id = tail[tail.len() - 1];
                if SHELL_SEAT_LEAVES.contains(&leaf)
                    && is_segment(client_id)
                    && tail[..tail.len() - 2].iter().all(|s| is_segment(s))
                {
                    Some(ParsedTopic::ShellSeat {
                        path: tail[..tail.len() - 2].iter().map(|s| s.to_string()).collect(),
                        leaf: leaf.to_string(),
                        client_id: client_id.to_string(),
                    })
                } else {
                    None
                }
            } else {
                None
            };
            match seat {
                Some(parsed) => parsed,
                None => match s2 {
                    Some(root) if GUI_STATE_ROOTS.contains(&root) => ParsedTopic::GuiState {
                        state_root: root.to_string(),
                        path: segs.iter().skip(3).map(|s| s.to_string()).collect(),
                    },
                    _ => ParsedTopic::Gui {
                        segments: tail.iter().map(|s| s.to_string()).collect(),
                    },
                },
            }
        }
        Some("Komand") => match (s2, s3, s4, s5) {
            (Some("monitor"), Some(dir), None, _) if MONITOR_DIRS.contains(&dir) => {
                ParsedTopic::KomandMonitor { dir: dir.to_string() }
            }
            (Some("cmd"), Some(verb), Some(class), Some(model))
                if segs.len() == 6 && KOMAND_VERBS.contains(&verb) && is_segment(class) && is_segment(model) =>
            {
                ParsedTopic::KomandCmd {
                    verb: verb.to_string(),
                    device_class: class.to_string(),
                    model: model.to_string(),
                }
            }
            (Some("state"), Some(class), Some(model), Some(cap))
                if segs.len() == 6 && is_segment(class) && is_segment(model) =>
            {
                ParsedTopic::KomandState {
                    device_class: class.to_string(),
                    model: model.to_string(),
                    capability: cap.to_string(),
                }
            }
            // The exact leaf, and nothing under it: `Komand/Clock` alone, or
            // anything below `Armed`, is a topic nobody declared and says so.
            (Some("Clock"), Some("Armed"), None, _) => ParsedTopic::KomandClockArmed,
            _ => unknown,
        },
        Some("System")
            if matches!(
                (s2, s3, s4),
                (Some("Chat"), None, _)
                    | (Some("Agents"), Some("NearestRustAgent"), Some("HandOff"))
                    | (Some("Writes"), None, _)
                    | (Some("Census"), Some(_), Some(_))
                    | (Some("Gui"), Some("Discovered"), Some(_))
                    | (Some("DeviceGroups"), Some("State" | "Command"), None)
                    | (Some("HourChime"), Some("State" | "Struck"), None)
                    | (Some("Capture"), Some("Filed"), None)
                    | (Some("Client"), Some("Breadcrumb"), None)
                    | (Some("Bench"), Some("TwoState"), None)
            ) =>
        {
            let tail = |from: usize| -> Option<Vec<String>> {
                let t: Vec<&str> = segs.iter().skip(from).copied().collect();
                t.iter().all(|s| is_segment(s)).then(|| t.iter().map(|s| s.to_string()).collect())
            };
            match (s2, s3) {
                (Some("Chat"), _) => ParsedTopic::SystemChat,
                (Some("Agents"), _) if segs.len() == 5 => ParsedTopic::NearestRustAgentHandOff,
                (Some("Writes"), _) => ParsedTopic::SystemWrites,
                (Some("Census"), Some(node)) if is_segment(node) => match tail(4) {
                    Some(path) => ParsedTopic::Census { node: node.to_string(), path },
                    None => unknown,
                },
                (Some("Gui"), _) => match tail(4) {
                    Some(path) => ParsedTopic::GuiDiscovered { path },
                    None => unknown,
                },
                (Some("DeviceGroups"), Some("State")) => ParsedTopic::DeviceGroupsState,
                (Some("DeviceGroups"), _) => ParsedTopic::DeviceGroupsCommand,
                (Some("HourChime"), Some("State")) => ParsedTopic::HourChimeState,
                (Some("HourChime"), _) => ParsedTopic::HourChimeStruck,
                (Some("Capture"), _) => ParsedTopic::CaptureFiled,
                (Some("Client"), _) => ParsedTopic::ClientBreadcrumb,
                (Some("Bench"), _) => ParsedTopic::BenchTwoState,
                _ => unknown,
            }
        }
        Some("System") if s2 == Some("ContainerManager") => {
            let tail: Vec<&str> = segs.iter().skip(3).copied().collect();
            match tail.as_slice() {
                [] => ParsedTopic::ContainerManagerEvent,
                ["status"] => ParsedTopic::ContainerManagerStatus,
                ["Manager"] => ParsedTopic::ContainerManagerManager,
                ["docker", "request"] => ParsedTopic::ContainerManagerDockerRequest,
                ["docker", "reply", client] if is_address(client) => {
                    ParsedTopic::ContainerManagerDockerReply { client: client.to_string() }
                }
                ["containers", stack, name] if is_address(stack) && is_address(name) => {
                    ParsedTopic::ContainerManagerContainer { stack: stack.to_string(), name: name.to_string() }
                }
                ["containers", stack, name, "apps", app]
                    if is_address(stack) && is_address(name) && is_address(app) =>
                {
                    ParsedTopic::ContainerManagerApp {
                        stack: stack.to_string(),
                        name: name.to_string(),
                        app: app.to_string(),
                    }
                }
                _ => unknown,
            }
        }
        Some("System") => match (s2, s3, s4, s5) {
            (Some("Agents"), Some(agent), None, _) if is_segment(agent) => {
                ParsedTopic::Agents { agent: agent.to_string() }
            }
            (Some("Config"), Some(agent), None, _) if is_segment(agent) => {
                ParsedTopic::Config { agent: agent.to_string() }
            }
            (Some("Log"), Some(source), Some(level), None) if is_segment(source) && LOG_LEVELS.contains(&level) => {
                ParsedTopic::Log {
                    source: source.to_string(),
                    level: level.to_string(),
                }
            }
            (Some("Problems"), Some(source), Some(key), None) if is_segment(source) && is_segment(key) => {
                ParsedTopic::Problems {
                    source: source.to_string(),
                    key: key.to_string(),
                }
            }
            (Some("DataMigration"), Some(source), Some(key), None) if is_segment(source) && is_segment(key) => {
                ParsedTopic::DataMigration {
                    source: source.to_string(),
                    key: key.to_string(),
                }
            }
            (Some("RepositoryManager"), Some(head), _, _)
                if is_segment(head) && segs.iter().skip(4).all(|s| is_segment(s)) =>
            {
                ParsedTopic::RepositoryManager {
                    path: segs.iter().skip(3).map(|s| s.to_string()).collect(),
                }
            }
            // Exact leaves BEFORE the three-segment point form, so a stream can
            // never be read as a point and a point can never shadow a stream.
            (Some("CapturePoints"), Some("Sweep"), None, _) => ParsedTopic::CapturePointSweep,
            (Some("CapturePoints"), Some("Poke"), None, _) => ParsedTopic::CapturePointPoke,
            (Some("CapturePoints"), Some(category), Some(key), Some(terminal))
                if segs.len() == 6
                    && is_segment(category)
                    && is_address(key)
                    && is_address(terminal) =>
            {
                ParsedTopic::CapturePoint {
                    category: category.to_string(),
                    key: key.to_string(),
                    terminal: terminal.to_string(),
                }
            }
            (Some("Captures"), Some(kind), Some(capture), None)
                if CAPTURE_KINDS.contains(&kind) && is_segment(capture) =>
            {
                ParsedTopic::Capture {
                    capture_kind: kind.to_string(),
                    capture: capture.to_string(),
                }
            }
            (Some("Gui"), Some("Panels"), Some(key), None) if is_segment(key) => {
                ParsedTopic::GuiPanels { key: key.to_string() }
            }
            (Some("FileBrowser"), Some(key), None, _) if is_segment(key) => {
                ParsedTopic::FileBrowser { key: key.to_string() }
            }
            // The builder and the schema live in `agent_counters`, beside the
            // payload they describe; the parse arm belongs here, with every
            // other family's, or `System/Counters/{agent}` reads back as
            // `unknown` — a declared family its own parser could not name.
            (Some("Counters"), Some(agent), None, _) if is_segment(agent) => {
                ParsedTopic::AgentCounters { agent: agent.to_string() }
            }
            // Reached only when `classify_legacy` declined: `{proto}` alone,
            // `/status`, `/config` and the visa/midi/komand trees are the older
            // spelling of this root and are classified above.
            // The live-event leaf BEFORE the general retained form, so a scan's
            // narration can never be read as the scan's state. `Scan/State`
            // beside it stays `Protocols` and stays retained.
            (Some("Protocols"), Some(proto), Some("Scan"), Some("Log"))
                if is_segment(proto) && segs.len() == 6 =>
            {
                ParsedTopic::ProtocolsScanLog { proto: proto.to_string() }
            }
            (Some("Protocols"), Some(proto), _, _)
                if is_segment(proto) && segs.iter().skip(4).all(|s| is_segment(s)) =>
            {
                ParsedTopic::Protocols {
                    proto: proto.to_string(),
                    path: segs.iter().skip(4).map(|s| s.to_string()).collect(),
                }
            }
            // The two live-event leaves are tested BEFORE the general retained
            // form, so a one-shot or a sample can never be read as retained
            // state. Tail segments are addresses: `{host}` may be an FQDN.
            (Some("BareMetal"), Some(target), _, _) if is_baremetal_target(target) => {
                let tail: Vec<&str> = segs.iter().skip(4).copied().collect();
                if !tail.iter().all(|s| is_address(s)) {
                    return unknown;
                }
                if target == "ESP32" && tail.len() == 3 && tail[1] == "identity" && tail[2] == "heartbeat" {
                    return ParsedTopic::BareMetalEsp32Heartbeat { node: tail[0].to_string() };
                }
                let own = |t: &[&str]| -> Vec<String> { t.iter().map(|s| s.to_string()).collect() };
                match tail.last().copied() {
                    Some(BAREMETAL_COMMAND_LEAF) => ParsedTopic::BareMetalCommand {
                        target: target.to_string(),
                        path: own(&tail[..tail.len() - 1]),
                    },
                    Some(leaf) if is_baremetal_stream_leaf(leaf) => ParsedTopic::BareMetalStream {
                        target: target.to_string(),
                        path: own(&tail[..tail.len() - 1]),
                        leaf: leaf.to_string(),
                    },
                    _ => ParsedTopic::BareMetal { target: target.to_string(), path: own(&tail) },
                }
            }
            _ => unknown,
        },
        _ => unknown,
    }
}

pub fn is_legacy(raw: &str) -> bool {
    matches!(parse(raw), ParsedTopic::Legacy(_))
}
