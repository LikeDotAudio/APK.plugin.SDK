// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `topics.rs` — see that file for the whole picture.

use super::*;

// ---------------------------------------------------------------- builders

/// One half of a WebRTC signalling room. Both halves are built the same way
/// and differ only in the role, which is why the pair is one builder with a
/// closed `role` rather than two.
pub fn webrtc_signal(room: &str, role: &str) -> Result<String, TopicError> {
    require_segment("room", room)?;
    if !WEBRTC_SIGNAL_ROLES.contains(&role) {
        return Err(err("webrtc signal role", role));
    }
    Ok(format!("{ROOT}/Audio/WebRTC/{room}/{role}"))
}

/// Both halves of one room — what a monitor watching a call subscribes to.
pub fn webrtc_signal_wildcard(room: Option<&str>) -> Result<String, TopicError> {
    match room {
        None => Ok(format!("{ROOT}/Audio/WebRTC/#")),
        Some(r) => {
            require_segment("room", r)?;
            Ok(format!("{ROOT}/Audio/WebRTC/{r}/+"))
        }
    }
}

/// `chN` for a raw MIDI channel number, refusing anything outside 0..=15.
///
/// This is the refusal `Protocol-midi/src/mapper.rs` did not have: it takes the
/// channel as a `u8` and wrote `format!("ch{}", channel)`, so `ch200` was a
/// topic it would publish into a namespace nothing binds, with no error at
/// either end.
pub fn midi_channel(channel: u8) -> Result<String, TopicError> {
    let leaf = format!("ch{channel}");
    if !MIDI_CHANNELS.contains(&leaf.as_str()) {
        return Err(err("midi channel", &leaf));
    }
    Ok(leaf)
}

/// One mapped MIDI event's topic: `MIDI/{device}/{channel}/{control}`.
///
/// `control` is OPEN — `cc{n}` and `note{n}` are minted by the mapper and its
/// fourth arm passes the driver's message type through verbatim — and is still
/// held to the segment charset, which is the part that was missing.
pub fn midi_control(device: &str, channel: &str, control: &str) -> Result<String, TopicError> {
    require_segment("device", device)?;
    if !MIDI_CHANNELS.contains(&channel) {
        return Err(err("midi channel", channel));
    }
    require_segment("control", control)?;
    Ok(format!("{ROOT}/MIDI/{device}/{channel}/{control}"))
}

/// Every channel and control of one surface, or of every surface.
pub fn midi_wildcard(device: Option<&str>) -> Result<String, TopicError> {
    match device {
        None => Ok(format!("{ROOT}/MIDI/#")),
        Some(d) => {
            require_segment("device", d)?;
            Ok(format!("{ROOT}/MIDI/{d}/#"))
        }
    }
}

/// The bench's clock of record — the whole family, which is one exact topic.
/// One publisher (`APK:Softapps/TIME/TIME.SynchNTP-NRCtimeSignal`), one subscriber
/// (`APK:audio:komand`), and both name this string.
pub fn time_mark() -> String {
    format!("{ROOT}/Time/Mark")
}

/// THE HOUR — the one retained document saying where the bench is in its
/// hourly boom / reconnect / self-test (`TimeHour`, `src/time-hour.ts`).
/// Published by `AUDIO.komand` (`the_hour.rs`); obeyed by every recorder.
pub fn time_hour() -> String {
    format!("{ROOT}/Time/Hour")
}

/// A sound asked for by name — played by the APK:OS sound host. PLAN-3315.01.
pub fn audio_cue(name: &str) -> Result<String, TopicError> {
    require_segment("cue name", name)?;
    Ok(format!("{ROOT}/Audio/Cue/{name}"))
}

/// One leaf of the sound watchdog's report on one framed engine.
pub fn audio_engine(app: &str, leaf: &str) -> Result<String, TopicError> {
    require_segment("app", app)?;
    if !AUDIO_ENGINE_LEAVES.contains(&leaf) {
        return Err(err("audio engine leaf", leaf));
    }
    Ok(format!("{ROOT}/Audio/Engine/{app}/{leaf}"))
}

pub fn audio_engine_wildcard(app: Option<&str>) -> Result<String, TopicError> {
    match app {
        None => Ok(format!("{ROOT}/Audio/Engine/#")),
        Some(a) => {
            require_segment("app", a)?;
            Ok(format!("{ROOT}/Audio/Engine/{a}/+"))
        }
    }
}

/// YoControl's voice.
pub fn yo_speech(leaf: &str) -> Result<String, TopicError> {
    if !YO_SPEECH_LEAVES.contains(&leaf) {
        return Err(err("yo speech leaf", leaf));
    }
    Ok(format!("{ROOT}/Yo/speech/{leaf}"))
}

/// The Big Alarm siren — the whole family, which is one exact topic.
pub fn yo_alarm() -> String {
    format!("{ROOT}/Yo/alarm")
}

/// YoControl's ten-second heartbeat. LOOSE-yocontrol-topics.
pub fn yo_status() -> String {
    format!("{ROOT}/Yo/status")
}

/// A reply YoControl wants spoken aloud in the room.
pub fn yo_announce() -> String {
    format!("{ROOT}/Yo/announce")
}

/// The ECHO broadcast.
pub fn yo_echo() -> String {
    format!("{ROOT}/Yo/echo")
}

/// A chat line addressed to Yo in `room`.
pub fn yo_chat_input(room: &str) -> Result<String, TopicError> {
    require_segment("room", room)?;
    Ok(format!("{ROOT}/Yo/chat/input/{room}"))
}

/// A voice capture buffer for Yo to transcribe.
pub fn yo_voice_capture() -> String {
    format!("{ROOT}/Yo/voice/capture")
}

pub fn discovery(protocol: &str, device_id: &str) -> Result<String, TopicError> {
    require_segment("protocol", protocol)?;
    if !is_device_id(device_id) {
        return Err(err("deviceId", device_id));
    }
    Ok(format!("{ROOT}/Discovery/{protocol}/{device_id}"))
}

/// `APK.audio/Discovery/{protocol}/{deviceId}/verified` — the agent's retained
/// proof it re-verified the thing (`DiscoveryVerified`), beside the silent
/// status document. Same validation as [`discovery`]. TS twin:
/// `Topics.discoveryVerified`. ROAD-3437.
pub fn discovery_verified(protocol: &str, device_id: &str) -> Result<String, TopicError> {
    Ok(format!("{}/verified", discovery(protocol, device_id)?))
}

/// `APK.audio/Discovery/{protocol}/{deviceId}/record` — the thing's retained
/// identity (`DeviceRecord`), beside the silent status document. Same
/// validation as [`discovery`]. TS twin: `Topics.discoveryRecord`. ROAD-3437.
pub fn discovery_record(protocol: &str, device_id: &str) -> Result<String, TopicError> {
    Ok(format!("{}/record", discovery(protocol, device_id)?))
}

/// `APK.audio/Discovery/_engine/{leaf}/{...path}` — the discovery engine's own
/// retained facts (`status`, `_Counter`, `_Counter/{field}`). TS twin:
/// `Topics.discoveryEngine`.
pub fn discovery_engine(leaf: &str, path: &[&str]) -> Result<String, TopicError> {
    require_segment("leaf", leaf)?;
    let mut out = format!("{ROOT}/Discovery/_engine/{leaf}");
    for p in path {
        require_segment("path", p)?;
        out.push('/');
        out.push_str(p);
    }
    Ok(out)
}

/// `APK.audio/Discovery/_lifecycle` — a plugin launched or stopped. Live event.
pub fn discovery_lifecycle() -> String {
    format!("{ROOT}/Discovery/_lifecycle")
}

/// `APK.audio/Discovery/_death` — a device declared dead. Live event.
pub fn discovery_death() -> String {
    format!("{ROOT}/Discovery/_death")
}

pub fn discovery_wildcard(protocol: Option<&str>) -> Result<String, TopicError> {
    match protocol {
        None => Ok(format!("{ROOT}/Discovery/#")),
        Some(p) => {
            require_segment("protocol", p)?;
            Ok(format!("{ROOT}/Discovery/{p}/+"))
        }
    }
}

pub fn gui_wildcard() -> String {
    format!("{GUI_ROOT}/#")
}

/// A retained SETTING under `Gui/`. Minted from its own builder so the retain
/// class is reachable from the kind alone — the reason `bare_metal_command`
/// exists rather than a path ending in `command`. `path` may be empty:
/// `APK.audio/Gui/DrumSets` is a whole topic.
pub fn gui_state(state_root: &str, path: &[&str]) -> Result<String, TopicError> {
    if !GUI_STATE_ROOTS.contains(&state_root) {
        return Err(err("gui state root", state_root));
    }
    for seg in path {
        require_segment("path segment", seg)?;
    }
    let mut out = format!("{GUI_ROOT}/{state_root}");
    for seg in path {
        out.push('/');
        out.push_str(seg);
    }
    Ok(out)
}

/// Every retained setting under one root — the `#` subscription a shell makes
/// to catch up on reconnect.
pub fn gui_state_wildcard(state_root: &str) -> Result<String, TopicError> {
    if !GUI_STATE_ROOTS.contains(&state_root) {
        return Err(err("gui state root", state_root));
    }
    Ok(format!("{GUI_ROOT}/{state_root}/#"))
}

/// A retained CLAIM under `Gui/`, keyed by the client that made it — a display
/// seat, a chat roster seat, a standing SOS. Minted from its own builder for
/// the same reason `gui_state` is: the retain class has to be reachable from
/// the kind alone. `path` may be empty.
pub fn gui_seat(path: &[&str], leaf: &str, client_id: &str) -> Result<String, TopicError> {
    if !SHELL_SEAT_LEAVES.contains(&leaf) {
        return Err(err("shell seat leaf", leaf));
    }
    for seg in path {
        require_segment("path segment", seg)?;
    }
    require_segment("clientId", client_id)?;
    let mut out = String::from(GUI_ROOT);
    for seg in path {
        out.push('/');
        out.push_str(seg);
    }
    out.push('/');
    out.push_str(leaf);
    out.push('/');
    out.push_str(client_id);
    Ok(out)
}

pub fn komand_cmd(verb: &str, device_class: &str, model: &str) -> Result<String, TopicError> {
    if !KOMAND_VERBS.contains(&verb) {
        return Err(err("verb", verb));
    }
    require_segment("deviceClass", device_class)?;
    require_segment("model", model)?;
    Ok(format!("{ROOT}/Komand/cmd/{verb}/{device_class}/{model}"))
}

pub fn komand_state(device_class: &str, model: &str, capability: &str) -> Result<String, TopicError> {
    require_segment("deviceClass", device_class)?;
    require_segment("model", model)?;
    if !is_capability(capability) {
        return Err(err("capability", capability));
    }
    Ok(format!("{ROOT}/Komand/state/{device_class}/{model}/{capability}"))
}

/// What KOMAND is doing, so a restart does not forget it — the whole family,
/// which is one exact topic. Retained, and cleared with a zero-length publish
/// when nothing is armed; `komand_clock` owns the document that goes on it and
/// says why the arm could not be read back off the `gui` topic that set it.
pub fn komand_clock_armed() -> String {
    format!("{ROOT}/Komand/Clock/Armed")
}

pub fn komand_monitor(dir: &str) -> Result<String, TopicError> {
    if !MONITOR_DIRS.contains(&dir) {
        return Err(err("monitor dir", dir));
    }
    Ok(format!("{ROOT}/Komand/monitor/{dir}"))
}

pub fn agents(agent: &str) -> Result<String, TopicError> {
    require_segment("agent", agent)?;
    Ok(format!("{ROOT}/System/Agents/{agent}"))
}

pub fn agents_wildcard() -> String {
    format!("{ROOT}/System/Agents/+")
}

pub fn config(agent: &str) -> Result<String, TopicError> {
    require_segment("agent", agent)?;
    Ok(format!("{ROOT}/System/Config/{agent}"))
}

/// `APK.audio/System/DataMigration/{source}/{key}` — fixed depth two.
pub fn data_migration(source: &str, key: &str) -> Result<String, TopicError> {
    require_segment("source", source)?;
    require_segment("key", key)?;
    Ok(format!("{ROOT}/System/DataMigration/{source}/{key}"))
}

pub fn data_migration_wildcard() -> String {
    format!("{ROOT}/System/DataMigration/#")
}

/// `APK.audio/System/RepositoryManager/{...path}` — declared, not yet served.
pub fn repository_manager(path: &[&str]) -> Result<String, TopicError> {
    if path.is_empty() {
        return Err(err("repositoryManager path", ""));
    }
    let mut topic = format!("{ROOT}/System/RepositoryManager");
    for s in path {
        require_segment("path segment", s)?;
        topic.push('/');
        topic.push_str(s);
    }
    Ok(topic)
}

pub fn repository_manager_wildcard() -> String {
    format!("{ROOT}/System/RepositoryManager/#")
}

/// A capture point's own identity: `System/CapturePoints/{category}/{key}/{terminal}`.
/// `key` and `terminal` are ADDRESSES — an `IP_port` pair, a MAC, an mDNS name —
/// not plain segments. `Sweep` and `Poke` are refused as a category because they
/// are the two exact leaves naming a stream about the set of points.
pub fn capture_point(category: &str, key: &str, terminal: &str) -> Result<String, TopicError> {
    require_segment("category", category)?;
    if CAPTURE_POINT_RESERVED.contains(&category) {
        return Err(err("category", category));
    }
    if !is_address(key) {
        return Err(err("key", key));
    }
    if !is_address(terminal) {
        return Err(err("terminal", terminal));
    }
    Ok(format!("{ROOT}/System/CapturePoints/{category}/{key}/{terminal}"))
}

pub fn capture_point_sweep() -> String {
    format!("{ROOT}/System/CapturePoints/Sweep")
}

pub fn capture_point_poke() -> String {
    format!("{ROOT}/System/CapturePoints/Poke")
}

pub fn capture_points_wildcard() -> String {
    format!("{ROOT}/System/CapturePoints/#")
}

/// A taken capture, filed by kind: `System/Captures/{kind}/{capture}`.
pub fn capture(kind: &str, capture: &str) -> Result<String, TopicError> {
    require_segment("kind", kind)?;
    if !CAPTURE_KINDS.contains(&kind) {
        return Err(err("capture kind", kind));
    }
    require_segment("capture", capture)?;
    Ok(format!("{ROOT}/System/Captures/{kind}/{capture}"))
}

pub fn captures_wildcard() -> String {
    format!("{ROOT}/System/Captures/#")
}
