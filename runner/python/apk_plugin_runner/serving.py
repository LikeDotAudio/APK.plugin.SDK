# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Serving: stop hooks, serve(), load_config(), main().

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import json
import os
import re
import signal
import sys
import threading
from typing import Callable, Dict, List, Optional, Tuple

from .contract import (
    EXIT_AGENT_PANICKED, EXIT_AGENT_RETURNED, EXIT_SESSION_DIED, EXIT_STALLED, EXIT_TERMINATED,
    TICK_S, apk_plugin_api, apk_plugin_panels
)
from .report import _REPORT, elog, going_down
from .settings import (
    broker_address, broker_is_local, credentials, health_file, ini_to_json, stall_after_s
)
from .manifest import Manifest
from .loopback import (
    DISABLED_REASON, _LOOPBACK, declare, loopback_api_section, module_enabled, start_loopback
)
from .ticking import _AGENT_TICKS, _SLOT, answer_arguments, stalest_tick
from .session import Session


_TERMINATE = threading.Event()
_AT_STOP: List[Callable[[], None]] = []
_CONFIG: Dict[str, Dict[str, str]] = {}


def at_stop(hook: Callable[[], None]) -> None:
    """Run `hook` on SIGTERM/SIGINT before the process exits 143.

    For the retained word an agent owes the bus on a clean stop (BLUETOOTH's
    `Scan/State = idle`). Hooks share a 2 s budget and an exception in one is
    printed, never fatal: the stop happens whatever they do.
    """
    _AT_STOP.append(hook)


def _run_stop_hooks() -> None:
    def all_hooks():
        for hook in list(_AT_STOP):
            try:
                hook()
            except Exception as error:                  # noqa: BLE001
                elog("⚠️  stop hook %s raised: %r" % (getattr(hook, "__name__", hook), error))
    thread = threading.Thread(target=all_hooks, daemon=True)
    thread.start()
    thread.join(2.0)


def config() -> Dict[str, Dict[str, str]]:
    """The config this process runs on — what `<topic>/config` published —
    for an agent to read its own section from."""
    return _CONFIG


def _on_terminate(signum, frame):
    _TERMINATE.set()


def pod_declared(raw: Optional[str]) -> Optional[str]:
    """`APK_POD`'s value as a pod, if it is one: trimmed, one `POD.<name>`
    segment. The twin of the Rust runner's `pod_declared` (PLAN-3430.01)."""
    pod = (raw or "").strip()
    if len(pod) > len("POD.") and pod.startswith("POD.") and not re.search(r"[/\\\s]", pod):
        return pod
    return None


def resolve_pod(given: Optional[str]) -> Optional[str]:
    """The pod incoming/api reports: `APK_POD` when the container sets it to a
    pod name, else what the plugin's `main(pod=…)` said — as the Rust runner
    takes `APK_POD` before the `POD.` folder on its source path."""
    return pod_declared(os.environ.get("APK_POD")) or given


def serve(token: str, config_text: str, config_source: str, self_test: Callable[[str], str],
          run: List[Callable[[str, int], None]], plugin_crate: Optional[str] = None,
          plugin_version: Optional[str] = None, pod: Optional[str] = "POD.THICK",
          config_overlay: Optional[Callable[[Dict[str, Dict[str, str]]], Dict[str, Dict[str, str]]]] = None,
          panels: Optional[List[str]] = None, gui_dir: Optional[str] = None,
          kind: str = "plugin", manifest: Optional[str] = None) -> None:
    """Run the plugin until something says it must not keep running. Never returns.

    `manifest="<path>/agent.toml"` is `main!(…, manifest = "agent.toml")`
    (ROAD-3478): the agent declares itself and the loopback telemetry is
    served at its `[telemetry] port` before the session publishes
    `incoming/api`, so the first document names the port. A manifest that does
    not parse is a defect said at startup, and nothing is served.

    `kind="worker"` is `main!(kind = "worker", …)`: incoming/api says what this
    process is (a POD.workers job answerer), not the default `plugin`.

    `panels=["family", …]` is `main!(…, panels = [family])`: each family's
    `gui/manifest.json` + `gui/<family>/<family>.json` is read from `gui_dir`
    and every device the plugin publishes presents its panel (apk_plugin_panels).

    `config_overlay(config) -> config` lets a plugin lay its environment
    variables over the file BEFORE anything is published, so `<topic>/config`
    and `incoming/api` carry what the plugin runs on, not what the file said.
    """
    _REPORT["name"] = token
    loaded = ini_to_json(config_text)
    if config_overlay is not None:
        overlaid = config_overlay(json.loads(json.dumps(loaded)))
        if overlaid != loaded:
            config_source += " + environment"
        loaded = overlaid
    _CONFIG.clear()
    _CONFIG.update(loaded)
    for family in panels or []:
        apk_plugin_panels.register_from_dir(gui_dir or os.path.join(os.getcwd(), "gui"), family)
    host, port = broker_address()
    stall = stall_after_s()
    # `enabled = false` (ROAD-3478), as lib.rs serve_group: connect, describe,
    # tick — but start no agent, and say stub / "disabled in config".
    if not module_enabled(loaded, token):
        _LOOPBACK["disabled"] = DISABLED_REASON
        run = []
        print("💤 [%s] enabled = false in config.ini — idling, no agent started" % token)
    names = [fn.__name__ for fn in run]
    print("🚀 [%s] plugin runner (python) — broker %s:%d %s" % (
        token, host, port, "as %s" % credentials()[0] if credentials() else "anonymously"))
    api = apk_plugin_api.PluginApi(
        token, config=loaded, config_source=config_source,
        broker_host=host, broker_port=port, authenticated=credentials() is not None,
        plugin_crate=plugin_crate, plugin_version=plugin_version,
        broker_is_ours=broker_is_local(host), stall_after_s=stall, tick_s=TICK_S,
        health_file=health_file(), pod=resolve_pod(pod), kind=kind)
    if manifest:
        try:
            with open(manifest, encoding="utf-8") as handle:
                declare(Manifest.parse(handle.read()), plugin_version)
        except (OSError, ValueError) as error:
            elog("🛑 [%s] %s does not parse: %s — no loopback telemetry" % (token, manifest, error))
    start_loopback(token, loaded)
    api.loopback = loopback_api_section()
    session = Session(token, loaded, config_source, self_test, api, names)

    failures: List[Tuple[str, int]] = []

    def guarded(name: str, target: Callable[[], None], returned_code: int) -> Callable[[], None]:
        def body():
            _SLOT.agent = name
            try:
                target()
                failures.append((name, returned_code))
            except BaseException as error:              # noqa: BLE001
                elog("🛑 [%s] %s raised: %r" % (token, name, error))
                failures.append((name, EXIT_AGENT_PANICKED if returned_code == EXIT_AGENT_RETURNED
                                 else returned_code))
        return body

    def daemon(name: str, body: Callable[[], None]) -> threading.Thread:
        thread = threading.Thread(target=body, name="%s:%s" % (token, name), daemon=True)
        thread.start()
        return thread

    signal.signal(signal.SIGTERM, _on_terminate)
    signal.signal(signal.SIGINT, _on_terminate)
    daemon("session", guarded("session", session.run_forever, EXIT_SESSION_DIED))
    daemon("telemetry", session.telemetry_forever)
    for fn in run:
        _AGENT_TICKS[fn.__name__] = [0]
    for fn in run:
        daemon(fn.__name__, guarded(fn.__name__, lambda fn=fn: fn(host, port), EXIT_AGENT_RETURNED))

    while True:
        _TERMINATE.wait(1.0)
        if _TERMINATE.is_set():
            print("👋 [%s] stop requested (SIGTERM/SIGINT) — exiting; the broker sends the last will" % token)
            _run_stop_hooks()
            sys.stdout.flush()
            os._exit(EXIT_TERMINATED)
        if failures:
            name, code = failures[0]
            if code == EXIT_SESSION_DIED:
                elog("🛑 [%s] the broker session thread ended; exiting so the container restarts" % token)
                going_down(token, "the broker session thread ended (exit 69); Docker restarts it")
            elif code == EXIT_AGENT_RETURNED:
                elog("🛑 [%s] %s returned; a hunter that stops is a failure — exiting" % (token, name))
                going_down(token, "%s returned (exit 70); Docker restarts it" % name)
            else:
                elog("🛑 [%s] %s raised — exiting" % (token, name))
                going_down(token, "%s raised (exit %d); Docker restarts it" % (name, code))
            sys.stdout.flush()
            sys.stderr.flush()
            os._exit(code)
        stale = stalest_tick()
        if stale and stale[1] > stall:
            elog("🛑 [%s] %s: no tick for %ds (limit %ds) — stalled, exiting" % (token, stale[0], stale[1], stall))
            going_down(token, "%s: no tick for %ds (limit %ds) — stalled (exit 75); Docker restarts it"
                       % (stale[0], stale[1], stall))
            sys.stderr.flush()
            os._exit(EXIT_STALLED)


def load_config(path: str) -> Tuple[str, str]:
    """(text, source): `APK_PLUGIN_CONFIG` if it reads, else the plugin's own file."""
    override = os.environ.get("APK_PLUGIN_CONFIG")
    if override:
        try:
            with open(override, encoding="utf-8") as handle:
                return handle.read(), override
        except OSError as error:
            elog("🛑 APK_PLUGIN_CONFIG=%s did not read (%s); using %s" % (override, error, path))
    with open(path, encoding="utf-8") as handle:
        return handle.read(), "config.ini"


def main(token: str, config_ini: str, self_test: Callable[[str], str],
         run: List[Callable[[str, int], None]], plugin_crate: Optional[str] = None,
         plugin_version: Optional[str] = None, pod: Optional[str] = "POD.THICK",
         config_overlay=None, panels: Optional[List[str]] = None, gui_dir: Optional[str] = None,
         kind: str = "plugin", manifest: Optional[str] = None) -> None:
    """A Python plugin's whole `__main__`: the twin of `apk_plugin_runner::main!`.

    `panels=[...]` reads the templates from `gui_dir`, by default the `gui/`
    folder beside `config_ini` — where every plugin keeps it. `manifest=
    "agent.toml"` is read beside `config_ini` too (an absolute path is kept)."""
    version = "%s %s" % (plugin_crate, plugin_version) if plugin_crate else None
    code = answer_arguments(sys.argv[1:], token, [fn.__name__ for fn in run], version)
    if code is not None:
        sys.exit(code)
    text, source = load_config(config_ini)
    if panels and gui_dir is None:
        gui_dir = os.path.join(os.path.dirname(os.path.abspath(config_ini)), "gui")
    if manifest and not os.path.isabs(manifest):
        manifest = os.path.join(os.path.dirname(os.path.abspath(config_ini)), manifest)
    serve(token, text, source, self_test, run, plugin_crate, plugin_version, pod, config_overlay,
          panels=panels, gui_dir=gui_dir, kind=kind, manifest=manifest)
