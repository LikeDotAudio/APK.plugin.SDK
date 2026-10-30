// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! `apk-plugin-health` — the compose healthcheck for every plugin container.
//!
//! Exit 0 when the runner's telemetry tick wrote its health file recently,
//! 1 otherwise. A binary rather than `test -f` in YAML because the plugin image
//! has no shell tools worth relying on, and because "recently" is arithmetic.
//!
//! WHAT A PASS MEANS, and no more: the plugin process is alive and its tick
//! thread is being scheduled. Docker does not restart an `unhealthy` container
//! by itself (audit §9.3) — the runner's watchdog exits the process for that.
//! This check is what `docker ps` and DockTor show a person.

fn main() {
    let path = apk_plugin_runner::health_file();
    let max_age: u64 = std::env::var("APK_PLUGIN_HEALTH_MAX_AGE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let contents = std::fs::read_to_string(&path).ok();
    // A STUB, NOT A STARTING PLUGIN. The image's stand-in for a binary whose
    // crate did not compile (Dockerfile, PLAN-3398.01) writes "not built: …"
    // here instead of a timestamp, so `docker ps` and DockTor say why.
    if let Some(text) = contents.as_deref().map(str::trim) {
        if text.starts_with("not built") {
            eprintln!("{text}");
            std::process::exit(1)
        }
    }
    let stamp = contents.and_then(|s| s.trim().parse::<u64>().ok());
    match stamp {
        Some(then) if now.saturating_sub(then) <= max_age => std::process::exit(0),
        Some(then) => {
            eprintln!("stale: {path} is {}s old (limit {max_age}s)", now.saturating_sub(then));
            std::process::exit(1)
        }
        None => {
            eprintln!("missing: {path} has no timestamp yet");
            std::process::exit(1)
        }
    }
}
