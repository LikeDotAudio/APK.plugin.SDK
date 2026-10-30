// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The links an EARLIER process left: read back on every connect, and
//! retracted unless something here can still vouch for the device.
//!
//! A row's link and identity are retained, and only [`unlink`](super::unlink)
//! ever retracted them. A device that departed while its plugin was down, or
//! before a build that unlinked on departure, therefore kept its
//! `{prefix}/presence_topic` for ever: a leaf-only ghost under `Device/+`
//! whose own leaves (written unretained by the hunter) were long gone.
//! Measured: `nmos/Device/WBS-32ME-RIGHT_local_80/presence_topic`, still
//! retained six days after its document said offline.
//!
//! So the read-back window also collects every retained `presence_topic`
//! under `System/Protocols/+/Device/…` whose link names one of THIS plugin's
//! protocols (another plugin's rows are not ours to judge). When the window
//! closes, a link with no row this process has sighted is:
//!
//! ```text
//!   linked document offline, or absent   → link + identity retracted now
//!   linked document online (adopted)     → kept, and retracted when the
//!                                          tracker says that document offline
//!   sighted again before either          → an ordinary row; nothing special
//! ```
//!
//! Nothing unverified is trusted: an adopted-online document expires on its
//! own clock (or `ADOPT_GRACE`), and the link goes with it.

use apkaudio_contracts::device_publish::Publication;
use apkaudio_contracts::topics::ROOT;

use super::shared::Shared;
use super::{IDENTITY_LEAVES, LINK_LEAF};

/// How deep under `Device/` a hunter's row prefix goes. Every hunter today
/// is one level (`Device/{id}`) or two (`Device/{category}/{id}`,
/// `Device/Input/Dev0`); four leaves room without a `#` replay of every leaf.
const MAX_DEPTH: usize = 4;

/// The subscriptions that replay every retained link, one per depth — MQTT
/// allows `#` only last, so `Device/+/…/presence_topic` is spelt out per level.
pub(super) fn link_filters() -> Vec<String> {
    (1..=MAX_DEPTH)
        .map(|depth| format!("{ROOT}/System/Protocols/+/Device/{}{LINK_LEAF}", "+/".repeat(depth)))
        .collect()
}

/// The prefix a retained link leaf stands under, when `topic` is one.
pub(super) fn link_prefix(topic: &str) -> Option<&str> {
    topic.strip_suffix(LINK_LEAF)?.strip_suffix('/')
}

/// One retained link off the broker, while a read-back window is open.
/// Kept only when it links to one of this plugin's protocols.
pub(super) fn found(s: &mut Shared, topic: &str, payload: &[u8]) {
    if !s.tracker.reading_back() {
        return;
    }
    let (Some(prefix), Ok(link)) = (link_prefix(topic), std::str::from_utf8(payload)) else {
        return;
    };
    let link = link.trim();
    if link.is_empty() {
        s.found.remove(prefix);
        return;
    }
    let ours = s.protocols.iter().any(|p| {
        apkaudio_contracts::topics::discovery_wildcard(Some(p))
            .ok()
            .and_then(|w| w.strip_suffix('+').map(str::to_owned))
            .is_some_and(|stem| link.starts_with(&stem))
    });
    if ours {
        s.found.insert(prefix.to_string(), link.to_string());
    }
}

/// The window closed (the tracker has already reconciled its documents):
/// judge every link it found that no row of this process accounts for.
pub(super) fn swept(s: &mut Shared) -> Vec<Publication> {
    let mut out = Vec::new();
    for (prefix, link) in std::mem::take(&mut s.found) {
        if s.rows.contains_key(&prefix) {
            s.orphans.remove(&prefix);
            continue;
        }
        if s.tracker.is_online(&link) {
            s.orphans.insert(prefix, link);
        } else {
            println!("🧹 [{}] {prefix} unlinked — {link} is not online", s.token);
            s.orphans.remove(&prefix);
            out.extend(retraction(&prefix));
        }
    }
    out
}

/// The tick said these documents offline: an orphan linked to one goes too.
pub(super) fn expired(s: &mut Shared, offline: &[Publication]) -> Vec<Publication> {
    let topics: Vec<&str> = offline.iter().map(|p| p.topic.as_str()).collect();
    let gone: Vec<String> =
        s.orphans.iter().filter(|(_, link)| topics.contains(&link.as_str())).map(|(p, _)| p.clone()).collect();
    let mut out = Vec::new();
    for prefix in gone {
        s.orphans.remove(&prefix);
        println!("🧹 [{}] {prefix} unlinked — its device went offline", s.token);
        out.extend(retraction(&prefix));
    }
    out
}

/// Every leaf the runner ever keeps under `prefix`, emptied.
fn retraction(prefix: &str) -> impl Iterator<Item = Publication> + '_ {
    std::iter::once(LINK_LEAF)
        .chain(IDENTITY_LEAVES)
        .map(move |leaf| Publication { topic: format!("{prefix}/{leaf}"), payload: String::new() })
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
