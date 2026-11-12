// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Trunk-Caster on the bus: the Ward-Beck line, the identities, the panel
//! labels, the caster's command envelope and every topic (ROAD-3440).
//!
//! ```text
//!   APK.audio/System/Protocols/trunk/…                       the caster (PLUGIN.TRUNK_CASTER)
//!   APK.audio/System/Protocols/trunk_sim/{protocol}/…        one frame simulator per protocol (PLUGIN.TRUNK_SIM)
//! ```
//!
//! The full topic table, retain classes and payload shapes are
//! `contracts/TRUNK.md`; the cases every twin is held to are
//! `contracts/vectors/trunk.json`. Ported from the Trunk-Caster SDK
//! (`PODS/POD.THICK/INTERCOM.Caster/SRC/TRUNK-CASTER-SDK/src/wardbeck.rs`,
//! `labels.rs`, `sites.rs`, `types.rs`) — only what crosses the bus.
//!
//! THE WARD-BECK LINE IS THE UNIFYING WORD. Every crosspoint on every frame of
//! every protocol is one line, `SITE,FRAME,INPUT,INPUT_NAME,CLOSE/OPEN,
//! OUTPUT_NAME,OUTPUT,FRAME,SITE`; a check-in is the line whose INPUT is
//! `CHECKIN`. It travels on TCP 7879 and, twinned, in batches: since 0.7.0 one
//! message per PROTOCOL per flush on `trunk_sim/{protocol}/Frames/line`, each
//! entry under its frame (0.6.0 was one per frame on `…/Frame/{frame}/line`,
//! still read for one release).
//!
//! THE COMMAND REPLACES THE HTTP API. Each of the runtime's 21 POST routes is a
//! verb on `trunk/outgoing/command`, answered on `trunk/incoming/result`.
//! A body with a key this module does not know is refused whole.
use serde_json::{json, Map, Value};

use crate::topics::{protocols, TopicError};

/// The caster's bus token.
pub const TOKEN: &str = "trunk";
/// The frame simulators' bus token; the protocol is the next level.
pub const SIM_TOKEN: &str = "trunk_sim";
/// The `wire_version` every trunk document carries.
pub const WIRE_VERSION: u32 = 1;
/// The INPUT that marks a check-in line.
pub const CHECKIN: &str = "CHECKIN";
/// The two voice cuts, in seconds.
pub const VOICE_CUTS: [u32; 2] = [1, 5];
/// The non-retained event streams under `trunk/Event/{kind}`.
pub const EVENT_KINDS: [&str; 5] = ["Crosspoint", "Gpio", "Label", "Gain", "Relay"];
/// The caster's read-only views, one retained document each (`trunk/View/{name}`) — what the
/// imported runtime served as GET routes (ROAD-3453).
pub const VIEW_NAMES: [&str; 9] = ["Status", "Report", "Regions", "PhoneBook", "Bandwidth", "Captures", "Presets", "Productions", "Globe"];
/// The keyed views, one retained document per key (`trunk/View/{name}/{key}`): a frame's tap
/// strip, one capture analysed.
pub const VIEW_ITEM_NAMES: [&str; 2] = ["Taps", "Capture"];
/// Panel widths a label is cut for.
pub const LABEL_WIDTHS: [usize; 3] = [6, 8, 16];
/// Where a site sits in the fleet.
pub const TIERS: [&str; 3] = ["capital", "province", "city"];
/// A protocol's broad family.
pub const FAMILIES: [&str; 7] =
    ["analog-audio", "audio-over-ip", "telephony", "control", "wireless", "media-transport", "other"];
/// A booking's life.
pub const BOOKING_STATUSES: [&str; 5] = ["booked", "on-air", "done", "failed", "missed"];
/// What a booked port is.
pub const PORT_KINDS: [&str; 2] = ["panel", "input"];
/// The GPIO pin functions, pin 1 first.
pub const GPIO_FUNCTIONS: [&str; 8] =
    ["blink-panel", "check-lights", "set-latch", "set-listen", "unset-latch", "clear", "last-caller", "ping"];

/// The answer's `status`.
pub const STATUS_DONE: &str = "done";
pub const STATUS_REFUSED: &str = "refused";
pub const STATUS_ERROR: &str = "error";
pub const STATUSES: [&str; 3] = [STATUS_DONE, STATUS_REFUSED, STATUS_ERROR];

/// Every key a command envelope may carry.
pub const COMMAND_KEYS: [&str; 4] = ["id", "verb", "args", "issued_at_ms"];

mod line;
mod batch;
mod protocol;
mod frame_doc;
mod ids;
mod labels;
mod command;
mod wire_topics;

pub use line::*;
pub use batch::*;
pub use protocol::*;
pub use frame_doc::*;
pub use ids::*;
pub use labels::*;
pub use command::*;
pub use wire_topics::*;

#[cfg(test)]
mod tests;
