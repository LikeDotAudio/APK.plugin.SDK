// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! One way to open an agent's own MQTT session (audit 2026-10-30, W02).
//!
//! WHY. Seventy `rumqttc::MqttOptions::new` calls in the tree each re-decided
//! the same five things — the client id, the keep-alive, the credentials, the
//! packet ceiling and the last will — and got them different ways. The worst
//! of it was the client id: `"apkaudio-dnssd"` is the same string on every
//! node, and MQTT silently drops the OLDER of two sessions with one id, so two
//! benches on one broker kicked each other off for ever. `{token}-{pid}` is no
//! better in a container, where every process is pid 1.
//!
//! ```ignore
//! use apk_plugin_runner::session::Session;
//!
//! // the agent's own session, drained on a thread, paced by broker_backoff:
//! let client = Session::new("dnssd", "browse").to(host, port).capacity(64).spawn_drained();
//!
//! // or keep the connection and read it yourself, paced the same way:
//! let (client, mut connection) = Session::new("ptp", "agent").to(host, port).connect();
//! for event in apk_plugin_runner::session::paced("ptp", &mut connection) { … }
//! ```
//!
//! WHAT IT DECIDES, so a plugin does not:
//!   · the client id — [`client_id`]: `apk-<token>-<role>-<node>-<pid>-<nonce>`,
//!     unique per node AND per process, even for pid 1 in two containers;
//!   · the credentials — `broker_auth::credentials()` (`APK_MQTT_USERNAME` /
//!     `APK_MQTT_PASSWORD`), the only place they are read;
//!   · keep-alive 30 s, the request-queue capacity (64 unless told), and
//!     rumqttc's packet ceiling unless [`Session::max_packet`] raises it;
//!   · the reconnect pacing — `broker_backoff`, through [`paced`] and
//!     [`Session::spawn_drained`], never a hot loop;
//!   · the last will, when one is given — QoS 1, and retained unless told not.
//!
//! The broker address is still the agent's argument (`run_*_agent(host, port)`
//! gets it from the runner), with [`crate::broker_address`] as the default.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rumqttc::{Client, Connection, ConnectionError, Event, LastWill, MqttOptions, QoS};

/// Keep-alive every hand-built session already used.
pub const KEEP_ALIVE: Duration = Duration::from_secs(30);

/// The request-queue capacity unless [`Session::capacity`] says otherwise.
pub const CAPACITY: usize = 64;

/// The longest node name kept in a client id. Mosquitto takes 65535 bytes,
/// but a log line that wraps on the id is a log line nobody reads.
const NODE_CHARS: usize = 24;

/// An agent's own broker session, before it connects.
#[derive(Clone, Debug)]
pub struct Session {
    token: String,
    role: String,
    host: String,
    port: u16,
    keep_alive: Duration,
    capacity: usize,
    max_packet: Option<usize>,
    will: Option<LastWill>,
    client_id: String,
    /// `None` = `broker_auth::credentials()`; `Some(None)` = anonymous;
    /// `Some(Some(pair))` = this identity.
    credentials: Option<Option<(String, String)>>,
}

impl Session {
    /// A session for agent `role` of module `token` (`"dnssd", "browse"`), to
    /// the broker [`crate::broker_address`] names.
    pub fn new(token: &str, role: &str) -> Self {
        let (host, port) = crate::broker_address();
        Session {
            token: token.to_string(),
            role: role.to_string(),
            host,
            port,
            keep_alive: KEEP_ALIVE,
            capacity: CAPACITY,
            max_packet: None,
            will: None,
            client_id: client_id(token, role),
            credentials: None,
        }
    }

    /// The broker this session goes to — what the runner handed the agent.
    pub fn to(mut self, host: &str, port: u16) -> Self {
        self.host = host.to_string();
        self.port = port;
        self
    }

    /// Requests that may wait in the queue before `try_publish` refuses.
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    pub fn keep_alive(mut self, keep_alive: Duration) -> Self {
        self.keep_alive = keep_alive;
        self
    }

    /// Raise the packet ceiling (both directions) above rumqttc's 10 KiB: a
    /// panel, a job carrying a file. An over-size packet is not refused, it
    /// kills the session — so size this for the largest thing ever sent.
    pub fn max_packet(mut self, bytes: usize) -> Self {
        self.max_packet = Some(bytes);
        self
    }

    /// The broker says this for the process when it dies. QoS 1, RETAINED —
    /// a will is a fact a later reader must still find.
    pub fn last_will(self, topic: impl Into<String>, payload: impl Into<Vec<u8>>) -> Self {
        self.last_will_with(topic, payload, true)
    }

    /// [`Session::last_will`] with the retain flag chosen.
    pub fn last_will_with(mut self, topic: impl Into<String>, payload: impl Into<Vec<u8>>, retain: bool) -> Self {
        let payload: Vec<u8> = payload.into();
        self.will = Some(LastWill::new(topic.into(), payload, QoS::AtLeastOnce, retain));
        self
    }

    /// Use exactly this client id instead of [`client_id`]'s. For the rare
    /// session another program addresses by id; everything else must not.
    pub fn with_client_id(mut self, id: impl Into<String>) -> Self {
        self.client_id = id.into();
        self
    }

    /// A second identity instead of `broker_auth::credentials()` — an
    /// ACL-scoped agent (PLUGIN.MACHINE's ear as `apkaudio-machine`, the
    /// engine's DockTor transport). `None` connects anonymously.
    pub fn credentials(mut self, pair: Option<(String, String)>) -> Self {
        self.credentials = Some(pair);
        self
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// The label log lines and the backoff speak under: `<token>:<role>`.
    pub fn label(&self) -> String {
        format!("{}:{}", self.token, self.role)
    }

    /// The `MqttOptions` this session connects with, for a caller that must
    /// set something this builder does not.
    pub fn options(&self) -> MqttOptions {
        let mut options = MqttOptions::new(self.client_id.clone(), self.host.clone(), self.port);
        options.set_keep_alive(self.keep_alive);
        if let Some(bytes) = self.max_packet {
            options.set_max_packet_size(bytes, bytes);
        }
        let pair = match &self.credentials {
            Some(chosen) => chosen.clone(),
            None => apkaudio_contracts::broker_auth::credentials(),
        };
        if let Some((username, password)) = pair {
            options.set_credentials(username, password);
        }
        if let Some(will) = &self.will {
            options.set_last_will(will.clone());
        }
        options
    }

    /// The client and its connection. The caller MUST poll the connection —
    /// rumqttc sends nothing, pings included, while it is not polled. Read it
    /// through [`paced`] for the backoff.
    pub fn connect(self) -> (Client, Connection) {
        Client::new(self.options(), self.capacity)
    }

    /// The client, with its connection drained on a named thread of its own
    /// and paced by `broker_backoff` — the shape of a publish-only agent.
    pub fn spawn_drained(self) -> Client {
        self.spawn_drained_with(|_| {})
    }

    /// [`Session::spawn_drained`], with every event handed to `on_event` on the
    /// drain thread (a ConnAck to resubscribe on, an incoming publish).
    pub fn spawn_drained_with(self, mut on_event: impl FnMut(Event) + Send + 'static) -> Client {
        let label = self.label();
        let (client, mut connection) = self.connect();
        std::thread::Builder::new()
            .name(format!("{label}:mqtt"))
            .spawn(move || {
                for event in paced(&label, &mut connection) {
                    on_event(event);
                }
            })
            .expect("the MQTT drain thread starts");
        client
    }
}

/// The connection's events, with every error turned into a `broker_backoff`
/// wait (and a throttled log line) instead of an item. Never ends on its own:
/// rumqttc reconnects on the next poll after an error.
pub fn paced<'a>(label: &'a str, connection: &'a mut Connection) -> impl Iterator<Item = Event> + 'a {
    let mut backoff = apkaudio_contracts::broker_backoff::Backoff::new();
    connection.iter().filter_map(move |event| {
        apkaudio_contracts::broker_backoff::step(label, &mut backoff, event, is_refusal)
    })
}

/// A CONNACK that refused the session (bad credentials, ACL): the slow ladder.
pub fn is_refusal(error: &ConnectionError) -> bool {
    matches!(error, ConnectionError::ConnectionRefused(_))
}

/// `apk-<token>-<role>-<node>-<pid>-<nonce>`.
///
/// NODE because two benches share a broker; PID because two processes share a
/// node; NONCE because in a container every process is pid 1 and two
/// containers on one host (host network) share the node name. Built once per
/// session: a reconnect keeps its id, as MQTT expects.
pub fn client_id(token: &str, role: &str) -> String {
    let node = node_name();
    format!("apk-{}-{}-{node}-{}-{:04x}", id_part(token), id_part(role), std::process::id(), nonce())
}

/// The node's name as one id segment: the kernel hostname, else `node`.
pub fn node_name() -> String {
    let raw = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_default();
    let part = id_part(raw.trim());
    let part: String = part.chars().take(NODE_CHARS).collect();
    if part.is_empty() || part == "_" { "node".to_string() } else { part }
}

/// Anything outside `[A-Za-z0-9_-]` becomes `_`, so an id is one plain token.
fn id_part(raw: &str) -> String {
    raw.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// 16 bits that differ between two processes started in the same instant.
fn nonce() -> u16 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let local = &nanos as *const u32 as usize as u32; // stack address: ASLR differs per process
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mixed = nanos ^ local.rotate_left(13) ^ n.wrapping_mul(0x9E37_79B9);
    (mixed ^ (mixed >> 16)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_id_names_module_role_node_and_process_and_differs_each_time() {
        let a = client_id("dnssd", "browse");
        let b = client_id("dnssd", "browse");
        assert!(a.starts_with("apk-dnssd-browse-"), "{a}");
        assert!(a.contains(&format!("-{}-", std::process::id())), "{a}");
        assert_ne!(a, b, "two sessions of one agent in one process must not share an id");
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{a}");
    }

    #[test]
    fn an_id_part_is_one_plain_token() {
        assert_eq!(id_part("my host.local/x"), "my_host_local_x");
        assert!(!node_name().is_empty());
        assert!(node_name().len() <= NODE_CHARS);
    }

    #[test]
    fn the_options_carry_what_the_builder_was_told() {
        let s = Session::new("ptp", "agent")
            .to("10.0.0.5", 1884)
            .keep_alive(Duration::from_secs(10))
            .max_packet(1 << 20)
            .last_will("APK.audio/System/Protocols/ptp/incoming/state", "unavailable");
        let o = s.options();
        assert_eq!(o.broker_address(), ("10.0.0.5".to_string(), 1884));
        assert_eq!(o.keep_alive(), Duration::from_secs(10));
        assert_eq!(o.max_packet_size(), 1 << 20);
        let will = o.last_will().expect("a will");
        assert_eq!(will.topic, "APK.audio/System/Protocols/ptp/incoming/state");
        assert!(will.retain);
        assert_eq!(o.client_id(), s.client_id());
        assert_eq!(s.label(), "ptp:agent");
        let ear = Session::new("machine", "ear").credentials(Some(("apkaudio-machine".into(), "pw".into()))).options();
        assert_eq!(ear.credentials(), Some(("apkaudio-machine".to_string(), "pw".to_string())));
        assert_eq!(Session::new("a", "b").credentials(None).options().credentials(), None);
        let fixed = Session::new("x", "y").with_client_id("exact");
        assert_eq!(fixed.options().client_id(), "exact");
    }
}
