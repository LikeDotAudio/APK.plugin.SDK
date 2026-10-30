// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The one process-wide handle: the tracker, the session's client and the
//! book of what stands under each hunter prefix, behind one mutex, and
//! publishing outside it — on a thread of its own, never the caller's.

use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use apkaudio_contracts::device_publish::{Publication, Sighting};
use rumqttc::{Client, QoS};

use super::{Tracker, IDENTITY_LEAVES, LINK_LEAF};

/// What this process keeps retained under one hunter `Device/…` prefix: the
/// link and the identity leaves beside it. `want` is what the last sighting
/// says; `said` is what the broker was last told on this connection.
#[derive(Debug, Default)]
pub(super) struct Row {
    pub(super) want: BTreeMap<&'static str, String>,
    pub(super) said: BTreeMap<&'static str, String>,
}

impl Row {
    /// The presence topic this prefix links to.
    pub(super) fn link(&self) -> Option<&String> {
        self.want.get(LINK_LEAF)
    }
}

pub(super) struct Shared {
    pub(super) tracker: Tracker,
    pub(super) client: Option<Client>,
    pub(super) protocols: Vec<String>,
    pub(super) connected_at: Option<Instant>,
    pub(super) token: String,
    /// `{hunter detail prefix: its link and identity}`.
    pub(super) rows: BTreeMap<String, Row>,
    /// `{prefix: link}` — retained links an earlier process left, collected
    /// while a read-back window is open. See `sweep.rs`.
    pub(super) found: BTreeMap<String, String>,
    /// `{prefix: (link, adopted at ms)}` — those links kept because their
    /// document is online but unsighted; retracted when it goes offline, or
    /// when no sighting has vouched for the row itself within its grace.
    /// See `sweep.rs`.
    pub(super) orphans: BTreeMap<String, (String, i64)>,
    pub(super) warned_unstarted: bool,
}

impl Shared {
    pub(super) fn new() -> Self {
        Shared {
            tracker: Tracker::booting(None),
            client: None,
            protocols: Vec::new(),
            connected_at: None,
            token: String::new(),
            rows: BTreeMap::new(),
            found: BTreeMap::new(),
            orphans: BTreeMap::new(),
            warned_unstarted: false,
        }
    }
}

static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();

fn shared() -> &'static Mutex<Shared> {
    SHARED.get_or_init(|| Mutex::new(Shared::new()))
}

pub(super) fn with<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    let mut guard = shared().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// Publish OUTSIDE the lock, and OFF THE CALLER'S THREAD. PLAN-3369.01.
///
/// Not `try_publish`: that drops on a full queue, and a first boot says every
/// device at once. Not a blocking `publish` under the lock: that deadlocks
/// against the session thread, which takes the lock to read back. And not a
/// blocking `publish` here either, which is what this was: `seen`/`gone` are
/// called from each plugin's own context, `tokio::spawn` tasks among them
/// (PLUGIN.VISA's heartbeat daemon calls `seen` from one) — with the broker
/// away long enough for the session's queue of 256 to fill, the next
/// transition parked that runtime worker, and every other task on it with it.
///
/// So the publications go down an unbounded channel to one [`speaker`] thread
/// that does the blocking `publish` — it may wait as long as the broker is
/// gone, because nothing else runs on it. Order is kept: one channel, one
/// thread. What can pile up is transitions, not sightings (a device already
/// online writes nothing), so an outage costs memory in proportion to devices
/// that came and went during it.
pub(super) fn say(client: Option<Client>, publications: Vec<Publication>) {
    let Some(client) = client else {
        return;
    };
    if publications.is_empty() {
        return;
    }
    let _ = speaker().send((client, publications));
}

/// The one thread that waits on the session's request queue for [`say`].
/// Started on first use, and it lives as long as the process.
fn speaker() -> &'static Sender<(Client, Vec<Publication>)> {
    static SPEAKER: OnceLock<Sender<(Client, Vec<Publication>)>> = OnceLock::new();
    SPEAKER.get_or_init(|| {
        let (tx, rx) = channel::<(Client, Vec<Publication>)>();
        std::thread::Builder::new()
            .name("presence:speaker".into())
            .spawn(move || {
                for (client, publications) in rx {
                    for p in publications {
                        let _ = client.publish(p.topic, QoS::AtLeastOnce, true, p.payload);
                    }
                }
            })
            .expect("the presence speaker starts");
        tx
    })
}

/// The identity a sighting carries, as the leaves written beside its link.
/// `name` is the hunter's own row name — the last segment of its prefix —
/// because a [`Sighting`] has no name field of its own. Empty is not said.
pub(super) fn identity_of(sighting: &Sighting, detail: &str) -> BTreeMap<&'static str, String> {
    let name = detail.rsplit('/').next().unwrap_or_default();
    let values = [
        sighting.make.as_str(),
        sighting.model.as_str(),
        sighting.device_class.as_str(),
        name,
        sighting.serial.as_deref().unwrap_or_default(),
        sighting.notes.as_deref().unwrap_or_default(),
    ];
    IDENTITY_LEAVES
        .iter()
        .zip(values)
        .filter_map(|(leaf, v)| {
            let v = v.trim();
            (!v.is_empty()).then(|| (*leaf, v.to_string()))
        })
        .collect()
}

/// Every leaf whose wanted value differs from what this connection has said,
/// once the read-back is over: a changed or new leaf is written, a leaf no
/// longer wanted is retracted. Nothing while a window is open — and nothing
/// is marked said then either, so the close of the window says it all.
pub(super) fn unsaid(s: &mut Shared) -> Vec<Publication> {
    if s.tracker.reading_back() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (detail, row) in s.rows.iter_mut() {
        let gone: Vec<&'static str> = row.said.keys().filter(|k| !row.want.contains_key(*k)).copied().collect();
        for leaf in gone {
            row.said.remove(leaf);
            out.push(Publication { topic: format!("{detail}/{leaf}"), payload: String::new() });
        }
        for (leaf, value) in &row.want {
            if row.said.get(leaf) != Some(value) {
                row.said.insert(leaf, value.clone());
                out.push(Publication { topic: format!("{detail}/{leaf}"), payload: value.clone() });
            }
        }
    }
    out
}

/// A connect opened a read-back window. What this process said on the last
/// connection is no longer known to stand: a broker that lost its retained
/// store (a crash between saves, a recreated volume) holds none of it. So
/// nothing counts as said, and the window's close restates every row — a
/// broker that kept them receives the same values again, once per row per
/// connect, under `Device/`, which the capture refuses.
pub(super) fn forget_said(s: &mut Shared) {
    for row in s.rows.values_mut() {
        row.said.clear();
    }
}

pub(super) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// THE OUTAGE, for presence. PLAN-3369.01; the pumps' version is
/// `republish::outage_tests::a_dead_broker_never_parks_the_pump`.
#[cfg(test)]
mod outage_tests {
    use super::say;
    use apkaudio_contracts::device_publish::Publication;
    use std::time::{Duration, Instant};

    /// 1,000 transitions said to a broker that is not there, and the caller
    /// back in well under a second.
    ///
    /// The connection is held and never iterated — the session thread asleep in
    /// `broker_backoff` — so the queue fills at 10 and stays full. Against the
    /// blocking `publish` this does not fail, it HANGS on transition eleven, so
    /// the calls run on their own thread and the verdict is a join with a
    /// deadline. The calling thread stands in for a plugin's tokio worker.
    #[test]
    fn a_dead_broker_never_parks_the_caller() {
        let options = rumqttc::MqttOptions::new("apk-presence-outage-test", "127.0.0.1", 9);
        let (client, connection) = rumqttc::Client::new(options, 10);

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for n in 0..1_000u32 {
                say(
                    Some(client.clone()),
                    vec![Publication {
                        topic: format!("APK.audio/Discovery/test/outage-{n}"),
                        payload: "{}".into(),
                    }],
                );
            }
            let _ = done_tx.send(());
        });

        let asked = Instant::now();
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("1,000 transitions did not come back in 5 s -- the caller is parked on a full MQTT queue");
        assert!(asked.elapsed() < Duration::from_secs(1), "took {:?}", asked.elapsed());
        // Closing the channel lets the speaker's parked send return, and the
        // rest fail fast, so it does not sit on this client for ever.
        drop(connection);
    }
}
