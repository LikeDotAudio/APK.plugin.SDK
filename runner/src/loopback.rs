// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The agent's own loopback telemetry: `GET /status` and `GET /telemetry` on
//! 127.0.0.1, at the port its compiled-in `agent.toml` declares. ROAD-3472.
//!
//! WHY. A plugin whose container is healthy and whose PID is alive was read as
//! a plugin doing its job — and a SAP listener on a segment nobody announces on
//! is healthy, alive and has heard nothing. The tick proves this process is
//! scheduling threads; it does not prove the protocol has seen a packet. This
//! module counts what the agent actually processed and answers with the
//! contract's word for it (`apkaudio_contracts::baremetal_telemetry`): zero
//! packets is `running`, any is `serving`, a declared fatal is `failed`, a stub
//! manifest is `stub`. NODE.BareMetal's supervisor polls it (plugins are
//! host-networked, so their loopback is the node's) and says `unknown` when
//! nothing answers.
//!
//! OPT-IN, PER BINARY. A plugin adopts it by having an `agent.toml` and saying
//! `manifest = "agent.toml"` in [`main!`](crate::main); one without it serves
//! nothing and is `unknown` to the supervisor, which is the truth about it.
//! The COUNTERS run in every binary regardless — [`published`] is called from
//! `unkept::say` for all of them — they are only served where declared.
//!
//! WHAT IT IS NOT. Not a second bus: nothing here is published, retained or
//! reachable off the host. `incoming/api` names the port (`loopback.port`) so a
//! reader can find it; it never carries the counters, which are samples.
//! Not a framework: std only, one thread, HTTP/1.1 with `Connection: close`,
//! two routes, nothing parsed past the request line.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use apkaudio_contracts::baremetal_telemetry::{
    status_document, telemetry_document, Counters, Manifest, LOOPBACK_BIND, ROUTE_STATUS, ROUTE_TELEMETRY,
};
use serde_json::{json, Value};

static MANIFEST: OnceLock<Manifest> = OnceLock::new();
static VERSION: OnceLock<&'static str> = OnceLock::new();
static STARTED: OnceLock<Instant> = OnceLock::new();
static PORT: OnceLock<u16> = OnceLock::new();
static UNREAD: OnceLock<Option<u64>> = OnceLock::new();
static FATAL: Mutex<Option<String>> = Mutex::new(None);
static DISABLED: OnceLock<String> = OnceLock::new();

static PACKETS_RECEIVED: AtomicU64 = AtomicU64::new(0);
static MESSAGES_PUBLISHED: AtomicU64 = AtomicU64::new(0);
static PUBLISH_FAILURES: AtomicU64 = AtomicU64::new(0);

/// `APK_TELEMETRY_PORT` moves the declared port; `0` turns the listener off.
pub const PORT_ENV: &str = "APK_TELEMETRY_PORT";

/// Record which agent this process is. The first declaration wins. Called by
/// [`main!`](crate::main)'s `manifest =`; a hand-written `main` calls it before
/// [`serve`](crate::serve).
pub fn declare(manifest: Manifest, version: &'static str) {
    let _ = VERSION.set(version);
    let _ = MANIFEST.set(manifest);
    let _ = STARTED.set(Instant::now());
}

/// The declared manifest, if any.
pub fn manifest() -> Option<&'static Manifest> {
    MANIFEST.get()
}

/// One packet taken off the protocol's own wire. Cheap: one relaxed add.
pub fn packet_received() {
    PACKETS_RECEIVED.fetch_add(1, Ordering::Relaxed);
}

/// One publish attempted: `true` when the client accepted it. Every refusal is
/// COUNTED — the replacement for `let _ = client.publish(..)`.
pub fn published(accepted: bool) -> bool {
    if accepted {
        MESSAGES_PUBLISHED.fetch_add(1, Ordering::Relaxed);
    } else {
        PUBLISH_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    accepted
}

/// The agent hit a fault it cannot serve past. The first reason is kept; the
/// state reads `failed` from here on. The process usually exits right after,
/// and the supervisor then reads `unknown` — the report is for the window
/// between, and for a fault an agent chooses to survive.
pub fn fatal(reason: impl Into<String>) {
    let mut slot = FATAL.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        *slot = Some(reason.into());
    }
}

/// Everything counted so far.
pub fn counters() -> Counters {
    Counters {
        packets_received: PACKETS_RECEIVED.load(Ordering::Relaxed),
        messages_published: MESSAGES_PUBLISHED.load(Ordering::Relaxed),
        publish_failures: PUBLISH_FAILURES.load(Ordering::Relaxed),
        unread_config_keys: UNREAD.get().copied().flatten(),
    }
}

/// The plugin is switched off in its own config (`enabled = false`, read by
/// [`serve_group`](crate::serve_group)): it idles without starting an agent,
/// and every answer says `stub` with this reason instead of `running` — a
/// switched-off hunter has nothing to hear, and `running` would call it idle
/// on a quiet segment. The first reason is kept.
pub fn disable(reason: impl Into<String>) {
    let _ = DISABLED.set(reason.into());
}

/// Why this plugin is switched off, or `None` while it is on.
pub fn disabled() -> Option<&'static str> {
    DISABLED.get().map(String::as_str)
}

/// A `/status` or `/telemetry` document with the disabled word laid over it.
/// The contract's `derive_state` knows `stub` only from the manifest; a
/// config switch is the runner's to say, so it is said here, after it.
fn with_disabled(mut document: Value, disabled: Option<&str>) -> Value {
    if let (Some(reason), Some(object)) = (disabled, document.as_object_mut()) {
        object.insert("state".into(), json!("stub"));
        object.insert("reason".into(), json!(reason));
    }
    document
}

fn fatal_reason() -> Option<String> {
    FATAL.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// `GET /telemetry`, or `None` when no manifest was declared.
pub fn telemetry() -> Option<Value> {
    let manifest = MANIFEST.get()?;
    let uptime = STARTED.get().map(|s| s.elapsed().as_secs()).unwrap_or(0);
    let version = VERSION.get().copied().unwrap_or("unknown");
    Some(with_disabled(
        telemetry_document(manifest, version, &counters(), fatal_reason().as_deref(), uptime),
        disabled(),
    ))
}

/// `GET /status`, or `None` when no manifest was declared.
pub fn status() -> Option<Value> {
    let manifest = MANIFEST.get()?;
    Some(with_disabled(status_document(manifest, &counters(), fatal_reason().as_deref()), disabled()))
}

/// What `incoming/api` says about this surface: where to ask, or `null`.
pub fn api_section() -> Value {
    match PORT.get() {
        Some(port) => json!({
            "bind": LOOPBACK_BIND,
            "port": port,
            "routes": [ROUTE_STATUS, ROUTE_TELEMETRY],
        }),
        None => Value::Null,
    }
}

/// The port to bind: `APK_TELEMETRY_PORT` when it parses, else the manifest's.
fn chosen_port(declared: u16, env: Option<&str>) -> u16 {
    env.and_then(|v| v.trim().parse::<u16>().ok()).unwrap_or(declared)
}

/// Count the config keys nothing declares reading, then bind and serve on a
/// thread. Returns the bound port, or `None` when there is no manifest, the
/// port is 0, or the bind failed (said on stderr — a taken port must not stop
/// the hunter, and the supervisor will read `unknown`, which is the truth).
pub fn start(token: &str, config: &Value) -> Option<u16> {
    let manifest = MANIFEST.get()?;
    let present: Vec<String> = config
        .as_object()
        .and_then(|sections| sections.iter().find(|(name, _)| name.eq_ignore_ascii_case(token)))
        .and_then(|(_, section)| section.as_object())
        .map(|keys| keys.keys().cloned().collect())
        .unwrap_or_default();
    let _ = UNREAD.set(manifest.unread_config_keys(present.iter().map(String::as_str)));

    let port = chosen_port(manifest.telemetry_port, std::env::var(PORT_ENV).ok().as_deref());
    if port == 0 {
        return None;
    }
    match TcpListener::bind((LOOPBACK_BIND, port)) {
        Ok(listener) => {
            let _ = PORT.set(port);
            println!("📈 [{token}] loopback telemetry on http://{LOOPBACK_BIND}:{port}{ROUTE_TELEMETRY}");
            std::thread::Builder::new()
                .name(format!("{token}:loopback"))
                .spawn(move || serve_on(listener))
                .ok()?;
            Some(port)
        }
        Err(e) => {
            elog!("⚠️  [{token}] loopback telemetry NOT served — {LOOPBACK_BIND}:{port}: {e}");
            None
        }
    }
}

/// Answer connections until the listener fails. One at a time: two routes,
/// a few hundred bytes each, asked every few seconds by one supervisor.
pub fn serve_on(listener: TcpListener) {
    for stream in listener.incoming().flatten() {
        let _ = answer(stream);
    }
}

fn answer(mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut buf = [0u8; 2048];
    let mut used = 0;
    while used < buf.len() {
        let n = stream.read(&mut buf[used..])?;
        if n == 0 {
            break;
        }
        used += n;
        if buf[..used].windows(4).any(|w| w == b"\r\n\r\n") || buf[..used].contains(&b'\n') {
            break;
        }
    }
    let request = String::from_utf8_lossy(&buf[..used]);
    let (code, body) = route(request.lines().next().unwrap_or(""));
    let reason = match code {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Service Unavailable",
    };
    let body = body.to_string();
    write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

/// The request line to `(status, body)`.
fn route(request_line: &str) -> (u16, Value) {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("").split('?').next().unwrap_or("");
    if method != "GET" {
        return (405, json!({"error": "GET only", "routes": [ROUTE_STATUS, ROUTE_TELEMETRY]}));
    }
    let document = match path {
        ROUTE_STATUS => status(),
        ROUTE_TELEMETRY => telemetry(),
        _ => return (404, json!({"error": "no such route", "routes": [ROUTE_STATUS, ROUTE_TELEMETRY]})),
    };
    match document {
        Some(doc) => (200, doc),
        None => (503, json!({"error": "no manifest declared"})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST_TOML: &str = "[agent]\nname = \"probe\"\nkind = \"protocol\"\ntitle = \"Probe\"\n\
        status = \"discovery-only\"\nsummary = \"A test agent.\"\n[telemetry]\nport = 0\n\
        [config]\nreads = [\"raise\"]\n";

    fn get(port: u16, path: &str) -> (u16, Value) {
        let mut s = TcpStream::connect((LOOPBACK_BIND, port)).unwrap();
        write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut text = String::new();
        s.read_to_string(&mut text).unwrap();
        let code = text.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap();
        (code, serde_json::from_str(body).unwrap())
    }

    /// `enabled = false` reads `stub` with the reason, whatever the counters
    /// say; switched on, the document is the contract's, untouched.
    #[test]
    fn a_plugin_disabled_in_config_says_stub_with_the_reason() {
        let manifest = Manifest::parse(MANIFEST_TOML).unwrap();
        let counters = Counters { packets_received: 3, messages_published: 1, publish_failures: 0, unread_config_keys: None };
        let doc = telemetry_document(&manifest, "1", &counters, None, 5);
        assert_eq!(with_disabled(doc.clone(), None), doc);
        let off = with_disabled(doc, Some("disabled in config"));
        assert_eq!(off["state"], "stub");
        assert_eq!(off["reason"], "disabled in config");
        assert_eq!(apkaudio_contracts::baremetal_telemetry::observed_state(Some(&off)).as_str(), "stub");
        let status = with_disabled(status_document(&manifest, &counters, None), Some("disabled in config"));
        assert_eq!((status["state"].as_str(), status["reason"].as_str()), (Some("stub"), Some("disabled in config")));
    }

    /// One test, because the counters are process-wide: running until a
    /// packet, serving after, publish refusals counted, unread keys measured,
    /// and the two routes plus the refusals served over a real socket.
    #[test]
    fn running_until_a_packet_then_serving_over_the_loopback() {
        declare(Manifest::parse(MANIFEST_TOML).unwrap(), "9.9.9");
        let config = json!({"probe": {"raise": "always-on", "multicast": "x"}});
        assert_eq!(start("probe", &config), None, "port 0 binds nothing");
        assert_eq!(api_section(), Value::Null);

        let listener = TcpListener::bind((LOOPBACK_BIND, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || serve_on(listener));

        let (code, doc) = get(port, "/telemetry");
        assert_eq!(code, 200);
        assert_eq!(doc["state"], "running");
        assert_eq!(doc["agent"], "probe");
        assert_eq!(doc["version"], "9.9.9");
        assert_eq!(doc["counters"]["unread_config_keys"], 1, "`multicast` is read by nothing declared");

        // Deltas, not totals: other tests in this binary may publish too.
        let before = counters();
        assert!(!published(false));
        assert!(published(true));
        let (_, doc) = get(port, "/telemetry");
        assert_eq!(doc["state"], "running", "publishing is not traffic");
        assert!(doc["counters"]["messages_published"].as_u64().unwrap() > before.messages_published);
        assert!(doc["counters"]["publish_failures"].as_u64().unwrap() > before.publish_failures);

        packet_received();
        let (code, status) = get(port, "/status?x=1");
        assert_eq!(code, 200);
        assert_eq!(status["state"], "serving");
        assert_eq!(get(port, "/telemetry").1["counters"]["packets_received"], 1);

        assert_eq!(get(port, "/nope").0, 404);
        let mut s = TcpStream::connect((LOOPBACK_BIND, port)).unwrap();
        write!(s, "POST /status HTTP/1.1\r\n\r\n").unwrap();
        let mut text = String::new();
        s.read_to_string(&mut text).unwrap();
        assert!(text.starts_with("HTTP/1.1 405"));

        fatal("socket closed");
        fatal("a second reason is not kept");
        let (_, doc) = get(port, "/telemetry");
        assert_eq!(doc["state"], "failed");
        assert_eq!(doc["reason"], "socket closed");
    }

    #[test]
    fn the_environment_moves_or_disables_the_port() {
        assert_eq!(chosen_port(8190, None), 8190);
        assert_eq!(chosen_port(8190, Some(" 8191 ")), 8191);
        assert_eq!(chosen_port(8190, Some("0")), 0);
        assert_eq!(chosen_port(8190, Some("nope")), 8190);
    }
}
