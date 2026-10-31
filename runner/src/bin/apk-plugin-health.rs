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
    std::process::exit(apk_plugin_runner::health_check())
}
