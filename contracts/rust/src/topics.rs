// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Hand-written mirror of `contracts/src/topics/` — build + parse for the
//! whole APK.audio namespace. Behavior is pinned to the TypeScript side by
//! `contracts/vectors/topics.json`; change a vector, not just this file.
//! Zero dependencies beyond serde (no regex crate — validation is manual).

use serde::{Deserialize, Serialize};
use std::fmt;

pub const ROOT: &str = "APK.audio";
pub const GUI_ROOT: &str = "APK.audio/Gui";

/// Default MQTT broker TCP port (mosquitto's conventional listener).
///
/// Previously spelled six different ways across the tree: bare `1883`
/// literals in the orchestrator, komand and the DataMigration importers, plus
/// `POKE_BROKER_PORT` and `MQTT_BROKER_PORT` on the Python side. One value,
/// one name.
pub const MQTT_DEFAULT_PORT: u16 = 1883;

/// Broker port for MQTT-over-WebSockets, which is what browsers must use.
pub const MQTT_WEBSOCKET_PORT: u16 = 9001;

/// Loopback host for a broker running alongside the agent on the same box.
pub const MQTT_LOOPBACK_HOST: &str = "127.0.0.1";

pub const KOMAND_VERBS: [&str; 4] = ["set", "rig", "nab", "do"];
pub const MONITOR_DIRS: [&str; 2] = ["in", "out"];
pub const LOG_LEVELS: [&str; 5] = ["trace", "debug", "info", "warn", "error"];

/// The two halves of a WebRTC signalling room — a CLOSED set of exactly two,
/// because the roles are fixed by DIRECTION and not chosen. TS twin:
/// `topics/grammar.ts` WEBRTC_SIGNAL_ROLES. `Soundcard_to_webrtc` has the
/// media, so it always OFFERS; `Webrtc_to_Soundcard` always ANSWERS. Each
/// publishes its own role and subscribes to the other's, which is what pairs
/// exactly two agents and leaves a stray third inert.
pub const WEBRTC_SIGNAL_ROLES: [&str; 2] = ["offerer", "answerer"];

/// What the shell's sound watchdog says about each framed engine — a CLOSED
/// set. TS twin: `topics/grammar.ts` AUDIO_ENGINE_LEAVES. Both retained: the
/// context's own state word, and how many times the watchdog replaced it.
/// PLAN-3315.01.
pub const AUDIO_ENGINE_LEAVES: [&str; 2] = ["state", "restarts"];

/// YoControl's two speech leaves — a CLOSED set. TS twin: `topics/grammar.ts`
/// YO_SPEECH_LEAVES, read off `SPEECH_SAY_TOPIC` and `SPEECH_AUDIO_TOPIC`.
pub const YO_SPEECH_LEAVES: [&str; 2] = ["say", "audio"];

/// The sixteen MIDI channels as the wire spells them. TS twin:
/// `topics/grammar.ts` MIDI_CHANNELS. CLOSED by somebody else's standard
/// rather than by a count of this bench — the MIDI 1.0 status byte carries the
/// channel in four bits, so there is no seventeenth and nothing mints one.
pub const MIDI_CHANNELS: [&str; 16] = [
    "ch0", "ch1", "ch2", "ch3", "ch4", "ch5", "ch6", "ch7", "ch8", "ch9", "ch10", "ch11", "ch12",
    "ch13", "ch14", "ch15",
];

/// Leaves under `System/CapturePoints/` that name a STREAM about the set of
/// points rather than a point. Reserved as a category for that reason: a point
/// called `Sweep` would be indistinguishable from the stream reporting on it.
/// TS twin: `topics/builders.ts` CAPTURE_POINT_RESERVED.
pub const CAPTURE_POINT_RESERVED: [&str; 2] = ["Sweep", "Poke"];

/// The bare-metal targets that publish under `System/BareMetal/` — a CLOSED
/// set. TS twin: `topics/grammar.ts` BAREMETAL_TARGETS. Measured, not guessed:
/// `ESP32` and `PI` are the manifest trees under `APK:BareMetal/`, `HID` and
/// `MonitorProbes` are the two live probe roots, and `ESP32_Beskar2d2` /
/// `HAL_Fader` are the two single devices `Storage:MQTT/mqtt_agent.py`
/// addresses directly, with no host segment.
pub const BAREMETAL_TARGETS: [&str; 6] =
    ["ESP32", "PI", "HID", "MonitorProbes", "ESP32_Beskar2d2", "HAL_Fader"];

/// The one-shot leaf — a `_GuiActuator`; a retained command re-fires on every
/// reconnect. `enable` is NOT one: it is a sticky `_SmartToggle` setpoint.
pub const BAREMETAL_COMMAND_LEAF: &str = "command";

/// The leaves the publishers send WITHOUT `retain=` — a retained sample is a
/// stale one, and `control` is ping/poll/zero.
pub const BAREMETAL_STREAM_LEAVES: [&str; 2] = ["telemetry", "control"];

/// The roots under `Gui/` whose leaves are SETTINGS and not gestures — a CLOSED
/// set. TS twin: `topics/grammar.ts` GUI_STATE_ROOTS, where each root carries
/// the file and line of the `retain: true` that put it there. `gui` is
/// live-event because a widget publish is an event; a fader position is not.
/// The leaves under `Gui/` that name a CLAIM keyed by the client that made it —
/// a CLOSED set, and the retained half of the shell's own traffic. TS twin:
/// `topics/grammar.ts` SHELL_SEAT_LEAVES, where each leaf carries the file and
/// line of the `retain: true` that put it there.
///
/// These arrived with the `ApkOs/` root the shell published under until it was
/// folded in here. `SOS` is not a seat and the name is a compromise: what puts
/// all three in one family is the shape and the retain class.
pub const SHELL_SEAT_LEAVES: [&str; 3] = ["CheckIn", "Presence", "SOS"];

pub const GUI_STATE_ROOTS: [&str; 6] = [
    "DrumKit",
    "DrumSets",
    "Mixer",
    "Sequencer",
    "Simulation",
    "SoundFavorites",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicError {
    pub what: &'static str,
    pub value: String,
}

impl fmt::Display for TopicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid topic {}: {:?}", self.what, self.value)
    }
}

impl std::error::Error for TopicError {}

fn err(what: &'static str, value: &str) -> TopicError {
    TopicError { what, value: value.to_string() }
}

/// Plain segment: `[A-Za-z0-9_-]+` (guidelines T4 — no spaces, no wildcards).
pub fn is_segment(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Device identity (guidelines D2): `{protocol}:{stableKey}`, where the key
/// may carry VISA-resource characters (`.`, `:`) but never `/ + #` or spaces.
pub fn is_device_id(s: &str) -> bool {
    match s.split_once(':') {
        Some((proto, key)) => {
            !proto.is_empty()
                && proto.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                && !key.is_empty()
                && key.chars().all(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-')
                })
        }
        None => false,
    }
}

/// Capability id (guidelines Y3): dotted path of plain segments.
pub fn is_capability(s: &str) -> bool {
    !s.is_empty() && s.split('.').all(is_segment)
}

/// An ADDRESS segment: `[A-Za-z0-9._:-]+`. A thing's location on the bench
/// rather than its name — a `{host}` may be an FQDN carrying dots, a capture
/// point's key is an `IP_port` pair. Never `/`, `+`, `#` or a space.
pub fn is_address(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

pub fn is_baremetal_target(s: &str) -> bool {
    BAREMETAL_TARGETS.contains(&s)
}

pub fn is_baremetal_stream_leaf(s: &str) -> bool {
    BAREMETAL_STREAM_LEAVES.contains(&s)
}

/// Coerce arbitrary free text into a single topic level that satisfies
/// [`is_segment`] (guidelines T4: `[A-Za-z0-9_-]+`).
///
/// Discovery agents name topics after values they do not control: mDNS
/// instance names, Cast friendly names, printer descriptions. Those carry
/// spaces, dots, colons and — occasionally — the MQTT separator `/` and the
/// wildcards `+` and `#`, any of which would fork or widen the topic tree.
///
/// Every character outside the T4 alphabet is replaced with `_`, so the
/// result is always a legal single level. Empty or wholly-illegal input
/// yields `"_"` rather than an empty level, because an empty level silently
/// collapses a topic path.
///
/// Trailing underscores are dropped so a trailing `.` (ubiquitous in mDNS
/// fully-qualified names) does not leave a dangling `_`. Leading underscores
/// are KEPT: they are significant in DNS-SD service types such as
/// `_ipp._tcp`, and trimming them would corrupt the identifier.
///
/// This is the one sanitiser for the whole namespace: it previously existed
/// as a private `seg()` in ten protocol crates with four different
/// behaviours, nine of which could emit segments that `is_segment` rejects.
pub fn sanitize_segment(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_end_matches('_');
    if trimmed.is_empty() { "_".to_string() } else { trimmed.to_string() }
}

/// Validate one topic level, or report which field was malformed.
fn require_segment(what: &'static str, s: &str) -> Result<(), TopicError> {
    if is_segment(s) { Ok(()) } else { Err(err(what, s)) }
}

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

/// `APK.audio/Tests/{suite}` — one prefix per TestOrchestrator module.
pub fn tests_prefix(suite: &str) -> Result<String, TopicError> {
    require_segment("suite", suite)?;
    Ok(format!("{ROOT}/Tests/{suite}"))
}

pub fn tests(suite: &str, path: &[&str]) -> Result<String, TopicError> {
    let mut topic = tests_prefix(suite)?;
    for s in path {
        require_segment("path segment", s)?;
        topic.push('/');
        topic.push_str(s);
    }
    Ok(topic)
}

pub fn tests_wildcard(suite: Option<&str>) -> Result<String, TopicError> {
    match suite {
        None => Ok(format!("{ROOT}/Tests/#")),
        Some(s) => Ok(format!("{}/#", tests_prefix(s)?)),
    }
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
    require_segment("capture", capture)?;
    Ok(format!("{ROOT}/System/Captures/{kind}/{capture}"))
}

pub fn captures_wildcard() -> String {
    format!("{ROOT}/System/Captures/#")
}

/// A protocol agent's config/status tree: `System/Protocols/{proto}/{...path}`.
/// The tail belongs to the protocol, as it does for `bareMetal`. An EMPTY tail
/// builds a topic that `parse` classifies as the legacy `protocolStatus`
/// family, which is deliberate: that spelling predates this one.
pub fn protocols(proto: &str, path: &[&str]) -> Result<String, TopicError> {
    require_segment("proto", proto)?;
    let mut topic = format!("{ROOT}/System/Protocols/{proto}");
    for s in path {
        require_segment("path segment", s)?;
        topic.push('/');
        topic.push_str(s);
    }
    Ok(topic)
}

pub fn protocols_wildcard() -> String {
    format!("{ROOT}/System/Protocols/#")
}

/// The leaf that separates a device's ADDRESS from the verb pressed on it:
/// `.../Device/{category}/{device}/Control/{verb}`.
///
/// THIS STRING WAS AGREED BY FOUR LITERALS AND HELD BY NOTHING. Fourteen device
/// consoles publish onto that shape; `Discovery:ChromeCast/src/control.rs` and
/// `Discovery:AppleTV/src/control.rs` each arrived at `["Control", verb]`
/// independently, and `instruments/panel_template.rs::bind_control` writes
/// `{base}/Control/{verb}` a third time. `protocols()` builds the ROOT and takes
/// a free `&[&str]` tail, so the contract had no opinion at all about the part
/// every one of them depends on. PLAN-914.01.
pub const DEVICE_CONTROL_LEAF: &str = "Control";

/// A press on one device's own agent:
/// `System/Protocols/{proto}/Device/{category}/{device}/Control/{verb}`.
///
/// Built on `protocols()` rather than beside it, so the root stays declared in
/// one place and a segment carrying `/`, `+` or `#` is a `TopicError` HERE —
/// at the moment the topic is composed — rather than a publish into a level
/// nobody subscribed to. That is the whole gain over the free tail: a category
/// with a slash in it used to build a longer topic and fail silently.
pub fn device_control(
    proto: &str,
    category: &str,
    device: &str,
    verb: &str,
) -> Result<String, TopicError> {
    protocols(proto, &["Device", category, device, DEVICE_CONTROL_LEAF, verb])
}

/// What an agent subscribes to for ONE device: every verb, and nothing else.
///
/// `#` and not `+`, because a verb is the last segment today and a deeper one
/// tomorrow should still reach the agent that owns the device rather than
/// falling into the silent `_ => {}` arm PLAN-857.01 is about.
pub fn device_control_wildcard(
    proto: &str,
    category: &str,
    device: &str,
) -> Result<String, TopicError> {
    Ok(format!(
        "{}/#",
        protocols(proto, &["Device", category, device, DEVICE_CONTROL_LEAF])?
    ))
}

/// The leaf the VISA agent answers a query on:
/// `System/Protocols/visa/Device/{family}/{model}/{dev}/Reply`.
/// `apk-visa`'s write daemon publishes it; KOMAND subscribes to it and turns
/// each reply into a named reading.
pub const DEVICE_REPLY_LEAF: &str = "Reply";

/// Every VISA instrument's reply, and nothing else:
/// `System/Protocols/visa/Device/+/+/+/Reply`.
///
/// Three `+`, because a VISA device address is three levels (family, model,
/// dev), not the two of `device_control`. Built on `protocols()` so the root is
/// declared once; KOMAND held this filter as an inline literal until it was
/// moved here.
pub fn visa_reply_wildcard() -> String {
    let base = protocols("visa", &["Device"]).expect("constant segments are valid");
    format!("{base}/+/+/+/{DEVICE_REPLY_LEAF}")
}

/// A scan's narration leaf: `System/Protocols/{proto}/Scan/Log`.
///
/// Its own builder, and its own family, for its retain class alone. `Scan/State`
/// beside it is the scan's conclusion and goes out retained under `protocols`;
/// this one is a narration line published `retain=false`, and a retained
/// narration outlives the scan it describes. Third instance of the shape that
/// split `bareMetal` into `baremetal_command`.
pub fn protocols_scan_log(proto: &str) -> Result<String, TopicError> {
    require_segment("proto", proto)?;
    Ok(format!("{ROOT}/System/Protocols/{proto}/Scan/Log"))
}

/// The Folder Navigator's selected path / trigger — browser-only, no agent
/// serves it, which `tree.ts` states rather than leaves implied.
pub fn file_browser(key: &str) -> Result<String, TopicError> {
    require_segment("key", key)?;
    Ok(format!("{ROOT}/System/FileBrowser/{key}"))
}

/// `APK.audio/System/BareMetal/{target}/{...path}` plus a validated tail.
/// The tail is address-class, not segment-class: `{host}` may be an FQDN.
fn baremetal_head(target: &str, path: &[&str]) -> Result<String, TopicError> {
    if !is_baremetal_target(target) {
        return Err(err("bareMetal target", target));
    }
    let mut topic = format!("{ROOT}/System/BareMetal/{target}");
    for s in path {
        if !is_address(s) {
            return Err(err("path segment", s));
        }
        topic.push('/');
        topic.push_str(s);
    }
    Ok(topic)
}

/// The RETAINED half. `command`, `telemetry` and `control` are refused as the
/// last segment and minted by [`baremetal_command`] / [`baremetal_stream`], so
/// the retain class is never a function of what a caller put in last position.
pub fn baremetal(target: &str, path: &[&str]) -> Result<String, TopicError> {
    if let Some(last) = path.last() {
        if *last == BAREMETAL_COMMAND_LEAF || is_baremetal_stream_leaf(last) {
            return Err(err("bareMetal path", last));
        }
    }
    baremetal_head(target, path)
}

pub fn baremetal_command(target: &str, path: &[&str]) -> Result<String, TopicError> {
    Ok(format!("{}/{BAREMETAL_COMMAND_LEAF}", baremetal_head(target, path)?))
}

pub fn baremetal_stream(target: &str, path: &[&str], leaf: &str) -> Result<String, TopicError> {
    if !is_baremetal_stream_leaf(leaf) {
        return Err(err("bareMetal stream leaf", leaf));
    }
    Ok(format!("{}/{leaf}", baremetal_head(target, path)?))
}

pub fn baremetal_wildcard(target: Option<&str>) -> Result<String, TopicError> {
    match target {
        None => Ok(format!("{ROOT}/System/BareMetal/#")),
        Some(t) if is_baremetal_target(t) => Ok(format!("{ROOT}/System/BareMetal/{t}/#")),
        Some(t) => Err(err("bareMetal target", t)),
    }
}

pub fn log(source: &str, level: &str) -> Result<String, TopicError> {
    require_segment("source", source)?;
    if !LOG_LEVELS.contains(&level) {
        return Err(err("log level", level));
    }
    Ok(format!("{ROOT}/System/Log/{source}/{level}"))
}

// ------------------------------------------------------------------ parse

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ParsedTopic {
    #[serde(rename = "webrtcSignal")]
    WebrtcSignal { room: String, role: String },
    #[serde(rename = "audioCue")]
    AudioCue { name: String },
    #[serde(rename = "audioEngine")]
    AudioEngine { app: String, leaf: String },
    #[serde(rename = "yoSpeech")]
    YoSpeech { leaf: String },
    #[serde(rename = "yoAlarm")]
    YoAlarm,
    #[serde(rename = "yoStatus")]
    YoStatus,
    #[serde(rename = "yoAnnounce")]
    YoAnnounce,
    #[serde(rename = "yoEcho")]
    YoEcho,
    #[serde(rename = "yoChatInput")]
    YoChatInput { room: String },
    #[serde(rename = "yoVoiceCapture")]
    YoVoiceCapture,
    #[serde(rename = "midi")]
    Midi { device: String, channel: String, control: String },
    #[serde(rename = "timeMark")]
    TimeMark,
    #[serde(rename = "discovery")]
    Discovery {
        protocol: String,
        #[serde(rename = "deviceId")]
        device_id: String,
    },
    #[serde(rename = "gui")]
    Gui { segments: Vec<String> },
    #[serde(rename = "guiState")]
    GuiState {
        #[serde(rename = "stateRoot")]
        state_root: String,
        path: Vec<String>,
    },
    #[serde(rename = "shellSeat")]
    ShellSeat {
        path: Vec<String>,
        leaf: String,
        #[serde(rename = "clientId")]
        client_id: String,
    },
    #[serde(rename = "komandCmd")]
    KomandCmd {
        verb: String,
        #[serde(rename = "deviceClass")]
        device_class: String,
        model: String,
    },
    #[serde(rename = "komandState")]
    KomandState {
        #[serde(rename = "deviceClass")]
        device_class: String,
        model: String,
        capability: String,
    },
    #[serde(rename = "komandMonitor")]
    KomandMonitor { dir: String },
    #[serde(rename = "komandClockArmed")]
    KomandClockArmed,
    #[serde(rename = "tests")]
    Tests { suite: String, path: Vec<String> },
    #[serde(rename = "agents")]
    Agents { agent: String },
    #[serde(rename = "config")]
    Config { agent: String },
    #[serde(rename = "log")]
    Log { source: String, level: String },
    #[serde(rename = "dataMigration")]
    DataMigration { source: String, key: String },
    #[serde(rename = "repositoryManager")]
    RepositoryManager { path: Vec<String> },
    #[serde(rename = "capturePoint")]
    CapturePoint { category: String, key: String, terminal: String },
    #[serde(rename = "capturePointSweep")]
    CapturePointSweep,
    #[serde(rename = "capturePointPoke")]
    CapturePointPoke,
    #[serde(rename = "capture")]
    Capture {
        #[serde(rename = "captureKind")]
        capture_kind: String,
        capture: String,
    },
    #[serde(rename = "protocols")]
    Protocols { proto: String, path: Vec<String> },
    #[serde(rename = "protocolsScanLog")]
    ProtocolsScanLog { proto: String },
    #[serde(rename = "fileBrowser")]
    FileBrowser { key: String },
    #[serde(rename = "agentCounters")]
    AgentCounters { agent: String },
    #[serde(rename = "bareMetal")]
    BareMetal { target: String, path: Vec<String> },
    #[serde(rename = "bareMetalCommand")]
    BareMetalCommand { target: String, path: Vec<String> },
    #[serde(rename = "bareMetalStream")]
    BareMetalStream { target: String, path: Vec<String>, leaf: String },
    #[serde(rename = "legacy")]
    Legacy(LegacyTopic),
    #[serde(rename = "unknown")]
    Unknown { raw: String },
}

/// The v40 namespace map (guidelines T7) — same families as topics/legacy.ts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyTopic {
    pub family: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<String>>,
}

impl LegacyTopic {
    fn new(family: &str) -> Self {
        LegacyTopic { family: family.to_string(), protocol: None, channel: None, guid: None, segments: None }
    }
}

fn classify_legacy(segs: &[&str]) -> Option<LegacyTopic> {
    let s1 = segs.get(1).copied();
    let s2 = segs.get(2).copied();
    let s3 = segs.get(3).copied();
    let rest: Vec<&str> = segs.iter().skip(4).copied().collect();

    if s1 == Some("Protocol") {
        if let Some(channel) = s2 {
            let mut t = LegacyTopic::new("wsSideBus");
            t.channel = Some(channel.to_string());
            t.segments = Some(segs.iter().skip(3).map(|s| s.to_string()).collect());
            return Some(t);
        }
        return None;
    }
    if s1 != Some("System") {
        return None;
    }
    if s2 == Some("Failover") && s3 == Some("WEB") && rest.len() == 2 && rest[0] == "Heartbeat" {
        let mut t = LegacyTopic::new("failoverWebHeartbeat");
        t.guid = Some(rest[1].to_string());
        return Some(t);
    }
    if s2 != Some("Protocols") {
        return None;
    }
    let proto = s3?;
    if proto == "visa" && rest.first() == Some(&"Device") {
        let mut t = LegacyTopic::new("visaDeviceTree");
        t.segments = Some(rest.iter().skip(1).map(|s| s.to_string()).collect());
        return Some(t);
    }
    if proto == "midi" && rest.first() == Some(&"Device") {
        let mut t = LegacyTopic::new("midiDeviceTree");
        t.segments = Some(rest.iter().skip(1).map(|s| s.to_string()).collect());
        return Some(t);
    }
    if proto == "komand" {
        let mut t = LegacyTopic::new("komandAgent");
        t.channel = Some(rest.join("/"));
        return Some(t);
    }
    if rest.is_empty() || rest == ["status"] {
        let mut t = LegacyTopic::new("protocolStatus");
        t.protocol = Some(proto.to_string());
        return Some(t);
    }
    if rest == ["config"] {
        let mut t = LegacyTopic::new("protocolConfig");
        t.protocol = Some(proto.to_string());
        return Some(t);
    }
    None
}

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
        Some("Time") => match (s2, s3) {
            (Some("Mark"), None) => ParsedTopic::TimeMark,
            _ => unknown,
        },
        Some("Discovery") => match (s2, s3, s4) {
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
        // `APK.audio/Tests/{suite}` alone is the orchestrator's prefix and parses
        // with an empty path; everything below it is suite-private structure.
        Some("Tests") => match s2 {
            Some(suite) if is_segment(suite) && segs.iter().skip(3).all(|s| is_segment(s)) => ParsedTopic::Tests {
                suite: suite.to_string(),
                path: segs.iter().skip(3).map(|s| s.to_string()).collect(),
            },
            _ => unknown,
        },
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
                if is_segment(kind) && is_segment(capture) =>
            {
                ParsedTopic::Capture {
                    capture_kind: kind.to_string(),
                    capture: capture.to_string(),
                }
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

// --------------------------------------------- panel path → GUI topic

/// Canonized `topicMaker.jsx` semantics — the TS twin is
/// `contracts/src/topics/gui-path.ts`; the vector suite pins both.
pub fn gui_prefix_from_panel_path(file_path: &str) -> String {
    let segs = gui_segments_from_panel_path(file_path);
    if segs.is_empty() {
        GUI_ROOT.to_string()
    } else {
        format!("{GUI_ROOT}/{}", segs.join("/"))
    }
}

pub fn gui_segments_from_panel_path(file_path: &str) -> Vec<String> {
    let mut parts: Vec<&str> = file_path.split('/').filter(|p| !p.is_empty()).collect();
    if let Some(last) = parts.last() {
        if has_file_extension(last) {
            parts.pop();
        }
    }
    parts.iter().filter_map(|p| {
        let n = normalize_part(p);
        if n.is_empty() { None } else { Some(n) }
    }).collect()
}

const SKIP_TOKENS: [&str; 6] = ["display", "window", "left", "right", "top", "bottom"];

fn has_file_extension(s: &str) -> bool {
    match s.rfind('.') {
        Some(i) => {
            let ext = &s[i + 1..];
            !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric())
        }
        None => false,
    }
}

fn normalize_part(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    if raw.eq_ignore_ascii_case("oagui") {
        return "GUI".to_string();
    }
    if raw.chars().all(|c| c.is_ascii_digit()) {
        return String::new();
    }
    // strip a leading "<n>_" / "<n>-" ordering prefix
    let digits_end = raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len());
    let mut clean = &raw[digits_end..];
    if digits_end > 0 {
        clean = clean.strip_prefix(['_', '-']).unwrap_or(clean);
    }
    if clean.is_empty() {
        return String::new();
    }
    // base = clean with a trailing "[_-]?<digits>" suffix removed, lowercased
    let bytes = clean.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    let mut base_end = bytes.len();
    if i < bytes.len() {
        base_end = i;
        if i > 0 && (bytes[i - 1] == b'_' || bytes[i - 1] == b'-') {
            base_end = i - 1;
        }
    }
    let base = clean[..base_end].to_lowercase();
    if SKIP_TOKENS.contains(&base.as_str()) {
        return String::new();
    }
    // whitespace runs → single '_'
    let mut out = String::with_capacity(clean.len());
    let mut in_ws = false;
    for c in clean.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push('_');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

#[cfg(test)]
mod device_control_tests {
    use super::*;

    /// The shape fourteen consoles and two agents already depend on, now
    /// composed in one place instead of spelled in four. PLAN-914.01.
    #[test]
    fn a_press_addresses_one_device_own_agent() {
        assert_eq!(
            device_control("chromecast", "Video_Cast", "Living_Room", "Volume").unwrap(),
            "APK.audio/System/Protocols/chromecast/Device/Video_Cast/Living_Room/Control/Volume"
        );
        assert_eq!(
            device_control("appletv", "Video", "Apple_TV_HD__4th_gen__44-44-44-150", "Play").unwrap(),
            "APK.audio/System/Protocols/appletv/Device/Video/Apple_TV_HD__4th_gen__44-44-44-150/Control/Play"
        );
    }

    /// THE GAIN OVER THE FREE TAIL. `protocols()` takes `&[&str]` and validates
    /// each segment, but nothing made the caller pass four of them in this
    /// order — a category carrying a separator used to build a longer topic and
    /// fail by being subscribed to by nobody. It is a `TopicError` now, at the
    /// moment the topic is composed.
    #[test]
    fn a_segment_carrying_a_separator_is_refused_rather_than_published() {
        for bad in ["Video/Cast", "Video+Cast", "Video#Cast", ""] {
            assert!(device_control("chromecast", bad, "Living_Room", "Volume").is_err(),
                    "category {bad:?} was allowed through");
            assert!(device_control("chromecast", "Video_Cast", bad, "Volume").is_err(),
                    "device {bad:?} was allowed through");
            assert!(device_control("chromecast", "Video_Cast", "Living_Room", bad).is_err(),
                    "verb {bad:?} was allowed through");
        }
    }

    /// One device, every verb — and `#` rather than `+`, so a deeper verb
    /// tomorrow still reaches the agent that owns the device.
    #[test]
    fn the_wildcard_covers_one_device_and_stops_there() {
        let sub = device_control_wildcard("chromecast", "Video_Cast", "Living_Room").unwrap();
        assert_eq!(sub, "APK.audio/System/Protocols/chromecast/Device/Video_Cast/Living_Room/Control/#");
        let press = device_control("chromecast", "Video_Cast", "Living_Room", "Mute").unwrap();
        assert!(press.starts_with(sub.trim_end_matches('#')),
                "the wildcard does not cover the topic the builder writes");
    }

    /// The leaf is the string the two vendor crates match on and the console
    /// generator writes. It is asserted rather than assumed, because changing it
    /// here without changing them is exactly the drift this constant exists to
    /// stop.
    /// The filter covers what the VISA agent publishes for one instrument:
    /// `{prefix}/Reply`, the prefix being three levels below `Device`.
    #[test]
    fn the_visa_reply_wildcard_covers_one_reply() {
        let sub = visa_reply_wildcard();
        assert_eq!(sub, "APK.audio/System/Protocols/visa/Device/+/+/+/Reply");
        let reply = protocols("visa", &["Device", "Spectrum", "N9340B", "Dev0", DEVICE_REPLY_LEAF]).unwrap();
        let (s, r): (Vec<&str>, Vec<&str>) = (sub.split('/').collect(), reply.split('/').collect());
        assert_eq!(s.len(), r.len());
        assert!(s.iter().zip(&r).all(|(a, b)| *a == "+" || a == b));
    }

    #[test]
    fn the_leaf_is_the_word_the_agents_match_on() {
        assert_eq!(DEVICE_CONTROL_LEAF, "Control");
    }
}
