// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The health file the telemetry tick writes and `apk-plugin-health` reads.
//! Split out of lib.rs (ROAD-3749); re-exported at the crate root.

use super::*;

/// Where the tick leaves its timestamp for `apk-plugin-health`.
///
/// /tmp, which the compose file mounts as a tmpfs because the container root is
/// read-only. `APK_PLUGIN_HEALTH_FILE` moves it.
pub fn health_file() -> String {
    std::env::var("APK_PLUGIN_HEALTH_FILE").unwrap_or_else(|_| "/tmp/apk-plugin.alive".to_string())
}

/// The `apk-plugin-health` probe as a function: 0 when the runner's telemetry
/// tick wrote the health file within `APK_PLUGIN_HEALTH_MAX_AGE` (60 s), 1
/// otherwise, with the reason on stderr. A worker in its own workspace ships a
/// one-line `fn main() { std::process::exit(apk_plugin_runner::health_check()) }`
/// instead of compiling this crate's bin file by path (P08 handoff).
///
/// A file that says `not built …` is the image's stub for a crate that failed
/// to compile (Dockerfile, PLAN-3398.01): reported as such, not as "missing".
pub fn health_check() -> i32 {
    let path = health_file();
    let max_age: u64 = std::env::var("APK_PLUGIN_HEALTH_MAX_AGE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let contents = std::fs::read_to_string(&path).ok();
    if let Some(text) = contents.as_deref().map(str::trim) {
        if text.starts_with("not built") {
            eprintln!("{text}");
            return 1;
        }
    }
    match contents.and_then(|s| s.trim().parse::<u64>().ok()) {
        Some(then) if unix_now().saturating_sub(then) <= max_age => 0,
        Some(then) => {
            eprintln!("stale: {path} is {}s old (limit {max_age}s)", unix_now().saturating_sub(then));
            1
        }
        None => {
            eprintln!("missing: {path} has no timestamp yet");
            1
        }
    }
}

pub(crate) fn write_health_file() {
    let _ = std::fs::write(health_file(), unix_now().to_string());
}

pub(crate) fn machine_name() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}
