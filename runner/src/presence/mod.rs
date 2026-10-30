// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Presence — every discovery plugin's devices as a STATUS, retained at
//! `APK.audio/Discovery/{protocol}/{deviceId}`. PLAN-3318.01.
//!
//! Owns: the per-device state, the one expiry tick for the whole process, the
//! read-back of this plugin's own retained documents on connect, and the rule
//! about when a sighting is worth a write. Must not: know how a hunter finds
//! anything. A hunter calls [`seen`] on every sighting and [`gone`] when its
//! protocol says a device left, and that is all it knows about this file.
//!
//! THE OWNER'S RULE, revised 2026-09-17: none of the discovery information goes
//! in the database, only its status. The document is exactly three keys —
//!
//! ```text
//!   { lastOnline, lastOffline, lastDiscovered }      RFC 3339 or null
//!
//!   new, or back after inactive   → lastOnline = now,  lastDiscovered = now
//!   not seen for inactive_after   → lastOffline = last sighting + inactive_after,
//!                                   lastDiscovered = the last sighting
//!   the protocol says it departed → lastOffline = now, lastDiscovered = the last sighting
//!   seen again while active       → nothing
//!   an identity field changed     → nothing — identity is not on this topic
//! ```
//!
//! — so the bus, and the SQL capture of the bus, hold exactly the moments a
//! device appeared or vanished. `lastDiscovered` is the real last sighting and
//! it is still silent: it is stamped only inside a transition write. `active` /
//! `inactive` lives in this file's memory and never on the wire; a reader
//! derives it (online ⇔ `lastOnline` later than `lastOffline`), which is
//! [`discovery_presence::is_online`](apkaudio_contracts::discovery_presence::is_online). Make, model and the rest of the identity are
//! not on this topic: the runner keeps them under the hunter's own
//! `System/Protocols/{proto}/Device/…` prefix, beside the link (below).
//!
//! ONE TIMER PER PROCESS, NOT PER DEVICE. [`start`] spawns one thread that
//! wakes every [`TICK`] (≥ 5 s) and walks the table. A bench with 150 devices
//! is one wake-up every five seconds, not 150 timers, and a device's expiry is
//! stamped at `last sighting + inactive_after` rather than at the tick that
//! noticed it — so the tick's coarseness is in WHEN it is said, never in WHAT.
//!
//! TWO KINDS OF PLUGIN. One re-sights a live device on a clock — ADP every few
//! seconds, a PTP flush, a registry poll, a console sweep — and declares
//! `presence_inactive_after_s`. The other hears a device once and then only
//! hears it LEAVE: mdns-sd emits `ServiceResolved` when a record is new or
//! changed and `ServiceRemoved` on a goodbye or a lapsed TTL, never on a steady
//! refresh. A clock would expire every steady mDNS device, so those plugins
//! declare `0` (departures only) and call [`gone`].
//!
//! RESTART KEEPS THE TIMESTAMPS. On every connect the tracker subscribes to its
//! own protocols' retained documents and holds every write for [`READBACK`].
//! When the window closes it reconciles: a device the broker already holds as
//! online and the hunter has sighted again keeps every stamp and writes nothing;
//! one the broker holds as offline is a real return and says so; and a retained
//! online device nobody has sighted is adopted with the connect as its clock —
//! it expires on `inactive_after` (or [`ADOPT_GRACE`] for a departures-only
//! plugin, which will never hear it leave) if it really has gone, instead of
//! every device flapping offline→online on boot. The same pass on a RECONNECT
//! refills a broker that lost its retained store and writes nothing to one that
//! did not.
//!
//! A SEPARATE SESSION. The tracker has its own client rather than borrowing the
//! runner's config session, so `apkaudio-playstation` — which is not built on
//! [`crate::serve`] — starts it with the same one call, and a retained replay
//! of a few hundred documents never sits in front of a `selftest_run`.
//!
//! THE JOIN FOR THE DISCOVERED MIRROR, AND THE IDENTITY BESIDE IT. A presence
//! document has no identity, so nothing in it says which table row it is
//! about. A hunter that still writes a `System/Protocols/{proto}/Device/…`
//! subtree passes that prefix to [`seen`], and the runner keeps it retained:
//! `{prefix}/presence_topic` = the Discovery topic, and beside it the
//! sighting's identity, one leaf each — [`IDENTITY_LEAVES`]. The runner keeps
//! identity because most hunters write theirs unretained, restated on a
//! clock, and a reader of the retained tree alone (NetBox's drain, the
//! manager's `discover`) would otherwise find a link and nothing to register.
//! Every leaf is written when it changes, never per sighting, and all of it
//! is under `Device/`, which the capture refuses, so it costs the database
//! nothing. Each connect's read-back window restates every row once when it
//! closes, so a broker that came back without its retained store is refilled
//! with the links as well as the documents. A hunter that retracts a row
//! calls [`unlink`], which retracts the link and the identity with it.
//!
//! A LINK NOBODY HERE SET. The same window reads back every retained
//! `presence_topic` an earlier process left under this plugin's protocols. One
//! no sighting accounts for is retracted, link and identity, unless its
//! document is online — then it goes when that document goes offline, or when
//! the row itself is not sighted within the plugin's clock (else
//! [`ADOPT_GRACE`]): a sibling row can keep a shared document online for ever,
//! so the document alone never vouches for a row. A device
//! that departed while the plugin was down no longer leaves a leaf-only ghost
//! under `Device/+` for ever (`sweep.rs`).
//!
//! AN ID THAT WAS NEVER A DEVICE. A hunter keyed MAC-then-name says a box under
//! its name until the neighbour cache answers, then under its MAC. The name id
//! did not go offline — it was the same box — so [`retire`] retracts its
//! document instead of leaving it online for ever, and re-links its rows.

use std::time::Duration;

mod config;
mod document;
mod hunter;
mod session;
mod shared;
mod sweep;
mod tracker;

pub use config::{settings_from_config, KEY_INACTIVE_AFTER, KEY_PROTOCOLS};
pub use hunter::{gone, retire, seen, unlink};
pub use session::start;
pub use tracker::Tracker;

/// How often the one expiry thread wakes. The plan's floor is five seconds;
/// nothing a person reads is sharper than that, and nothing expires faster.
pub const TICK: Duration = Duration::from_secs(5);

/// How long writes are held after a connect while the broker replays this
/// plugin's retained documents. A replay is one burst on a local broker —
/// milliseconds for hundreds of topics — so three seconds is generous, and the
/// first tick after it (≤ 5 s later) is when the reconciliation is said.
pub const READBACK: Duration = Duration::from_secs(3);

/// How long a retained-online device nobody has sighted since a boot is given
/// before it is said offline, for a plugin with no clock of its own. An mDNS
/// browse resolves everything answering within seconds of starting; two
/// minutes is the margin for a slow responder, not a guess at a lease.
pub const ADOPT_GRACE: Duration = Duration::from_secs(120);

/// The leaf under a hunter's `Device/…` prefix that names its presence topic.
pub const LINK_LEAF: &str = "presence_topic";

/// The identity leaves written retained beside [`LINK_LEAF`], in the order
/// `shared::identity_of` fills them: the sighting's make, model, class, the
/// hunter's row name, serial and notes. The spellings are the ones
/// `Baremetal:Manager/discovered.py` reads.
pub const IDENTITY_LEAVES: [&str; 6] = ["manufacturer", "model", "device_class", "name", "serial", "notes"];
