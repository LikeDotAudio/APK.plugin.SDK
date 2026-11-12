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

/// What is filed under `System/Captures/{kind}/{capture}` — a CLOSED set; each
/// kind is a slot whose payload `capture-rule.ts` holds (`CAPTURE_SLOTS`, also in
/// `topic_grammar.json`). TS twin: `topics/grammar.ts` CAPTURE_KINDS.
pub const CAPTURE_KINDS: [&str; 7] = ["Rig", "Attention", "RedFlag", "Landing", "CatchUp", "Setup", "Qualified"];

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
/// addresses directly, with no host segment. `NETWORK` and `NTP` are the
/// node-addressed manifest trees of `DATA.NETWORK` and `DATA.NTP`, whose
/// readers validate through [`baremetal`] (DA-03).
pub const BAREMETAL_TARGETS: [&str; 8] = [
    "ESP32",
    "PI",
    "HID",
    "MonitorProbes",
    "ESP32_Beskar2d2",
    "HAL_Fader",
    "NETWORK",
    "NTP",
];

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

// Kept in this file, not topics/builders_protocols.rs: the plugin runner's
// Python tests (RUNNER/python/test_apk_plugin_panels.py) read it off this text.
/// `APK.audio/System/Gui/Panels` — where apk-plugin-runner's panels live.
/// TS twin: `Topics.guiPanels`.
pub const GUI_PANELS_ROOT: &str = "APK.audio/System/Gui/Panels";

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
                && proto.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
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

mod filters;
mod builders;
mod builders_protocols;
mod builders_gui;
mod builders_system;
mod parsed;
mod legacy;
mod parser;
mod panel_path;

pub use filters::*;
pub use builders::*;
pub use builders_protocols::*;
pub use builders_gui::*;
pub use builders_system::*;
pub use parsed::*;
use legacy::classify_legacy;
pub use parser::*;
pub use panel_path::*;

#[cfg(test)]
mod device_control_tests;
