# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Loopback telemetry (ROAD-3478): counters, documents and the local HTTP server.

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import json
import os
import sys
import threading
import time
from typing import Any, Dict, Optional, Tuple

from .report import elog
from .manifest import Manifest, TELEMETRY_PORT_ENV, telemetry_rule


_LOOPBACK: Dict[str, Any] = {"manifest": None, "version": "unknown", "started": None,
                             "port": None, "unread": None, "fatal": None, "rule": None,
                             "disabled": None}
_COUNTERS = {"packets_received": 0, "messages_published": 0, "publish_failures": 0}
_COUNTERS_LOCK = threading.Lock()


def declare(manifest: Manifest, version: Optional[str]) -> None:
    """Record which agent this process is. The first declaration wins."""
    if _LOOPBACK["manifest"] is None:
        _LOOPBACK.update(manifest=manifest, version=version or "unknown", started=time.monotonic())


def packet_received() -> None:
    """One packet taken off the protocol's own wire — the one fact that moves
    an agent from `running` to `serving`. Never a publish, never a timer."""
    with _COUNTERS_LOCK:
        _COUNTERS["packets_received"] += 1


def published(accepted: bool) -> bool:
    """One publish attempted: `True` when the client accepted it. Every refusal
    is COUNTED — the replacement for an ignored publish result."""
    with _COUNTERS_LOCK:
        _COUNTERS["messages_published" if accepted else "publish_failures"] += 1
    return accepted


def fatal(reason: str) -> None:
    """A fault the agent cannot serve past. The first reason is kept and the
    state reads `failed` from here on."""
    if _LOOPBACK["fatal"] is None:
        _LOOPBACK["fatal"] = str(reason)


def counters() -> Dict[str, Any]:
    with _COUNTERS_LOCK:
        snapshot = dict(_COUNTERS)
    snapshot["unread_config_keys"] = _LOOPBACK["unread"]
    return snapshot


DISABLED_REASON = "disabled in config"


def module_enabled(config: Dict[str, Dict[str, str]], token: str) -> bool:
    """lib.rs `module_enabled`: only an explicit no (`false`/`no`/`off`/`0`, any
    case) in the token's own section switches the plugin off; absent is on."""
    section = next((v for k, v in config.items() if k.lower() == token.lower()), None) or {}
    value = section.get("enabled")
    return value is None or str(value).strip().lower() not in ("false", "no", "off", "0")


def _with_disabled(document: Dict[str, Any]) -> Dict[str, Any]:
    """lib.rs/loopback.rs `with_disabled`: switched off in config reads `stub`
    with the reason, whatever the counters say."""
    if _LOOPBACK["disabled"]:
        document["state"], document["reason"] = "stub", _LOOPBACK["disabled"]
    return document


def _state(rule, manifest: Manifest, counted: Dict[str, Any]) -> str:
    return rule.derive_state(manifest.is_stub(), _LOOPBACK["fatal"] is not None, counted["packets_received"])


def telemetry_document() -> Optional[Dict[str, Any]]:
    """`GET /telemetry` — contracts' `telemetry_document`, key for key; None
    without a manifest or without the rule."""
    manifest, rule = _LOOPBACK["manifest"], _LOOPBACK["rule"] or telemetry_rule()
    if manifest is None or rule is None:
        return None
    counted = counters()
    started = _LOOPBACK["started"]
    return _with_disabled({
        "schemaVersion": rule.SCHEMA_VERSION, "agent": manifest.name, "kind": manifest.kind,
        "status": manifest.status, "title": manifest.title, "version": _LOOPBACK["version"],
        "state": _state(rule, manifest, counted), "reason": _LOOPBACK["fatal"],
        "uptime_s": int(time.monotonic() - started) if started is not None else 0,
        "counters": counted})


def status_document() -> Optional[Dict[str, Any]]:
    """`GET /status`: the word and who says it."""
    manifest, rule = _LOOPBACK["manifest"], _LOOPBACK["rule"] or telemetry_rule()
    if manifest is None or rule is None:
        return None
    return _with_disabled({"schemaVersion": rule.SCHEMA_VERSION, "agent": manifest.name,
                           "state": _state(rule, manifest, counters()), "reason": _LOOPBACK["fatal"]})


def loopback_api_section() -> Optional[Dict[str, Any]]:
    """What `incoming/api` says about this surface (`loopback`): where to ask, or None."""
    port = _LOOPBACK["port"]
    if port is None:
        return None
    rule = _LOOPBACK["rule"]
    return {"bind": rule.LOOPBACK_BIND, "port": port, "routes": [rule.ROUTE_STATUS, rule.ROUTE_TELEMETRY]}


def chosen_port(declared: int, env: Optional[str]) -> int:
    """`APK_TELEMETRY_PORT` when it parses, else the manifest's; 0 is off."""
    try:
        value = int((env or "").strip())
        return value if 0 <= value <= 65535 else declared
    except ValueError:
        return declared


def loopback_route(method: str, path: str) -> Tuple[int, Dict[str, Any]]:
    """The request to `(status, body)` — loopback.rs's `route`, answer for answer."""
    routes = ["/status", "/telemetry"]
    if method != "GET":
        return 405, {"error": "GET only", "routes": routes}
    path = path.split("?", 1)[0]
    if path == "/status":
        document = status_document()
    elif path == "/telemetry":
        document = telemetry_document()
    else:
        return 404, {"error": "no such route", "routes": routes}
    if document is None:
        return 503, {"error": "no manifest declared"}
    return 200, document


def make_loopback_server(port: int):
    """An HTTP server on 127.0.0.1:`port` answering `loopback_route`. Not started."""
    from http.server import BaseHTTPRequestHandler, HTTPServer

    class Handler(BaseHTTPRequestHandler):
        def _answer(self):
            code, body = loopback_route(self.command, self.path)
            data = json.dumps(body).encode("utf-8")
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(data)

        do_GET = do_POST = do_PUT = do_DELETE = do_PATCH = do_HEAD = _answer

        def log_message(self, *args):                 # one supervisor asks every few seconds
            pass

    return HTTPServer(("127.0.0.1", port), Handler)


def start_loopback(token: str, config: Dict[str, Dict[str, str]]) -> Optional[int]:
    """Count the config keys nothing declares reading, then bind and serve on a
    daemon thread. The bound port, or None (no manifest, port 0, no rule, or a
    failed bind — said on stderr; a taken port must not stop the agent)."""
    manifest = _LOOPBACK["manifest"]
    if manifest is None:
        return None
    present = next((list(keys) for name, keys in config.items() if name.lower() == token.lower()), [])
    _LOOPBACK["unread"] = manifest.unread_config_keys(present)
    rule = telemetry_rule()
    if rule is None:
        print("⚠️  [%s] loopback telemetry NOT served — apkaudio_contracts is not importable "
              "(an SDK tag before sdk-v0.9?)" % token, file=sys.stderr)
        return None
    _LOOPBACK["rule"] = rule
    port = chosen_port(manifest.telemetry_port, os.environ.get(TELEMETRY_PORT_ENV))
    if port == 0:
        return None
    try:
        server = make_loopback_server(port)
    except OSError as error:
        elog("⚠️  [%s] loopback telemetry NOT served — 127.0.0.1:%d: %s" % (token, port, error))
        return None
    _LOOPBACK["port"] = port
    print("📈 [%s] loopback telemetry on http://127.0.0.1:%d/telemetry" % (token, port))
    threading.Thread(target=server.serve_forever, name="%s:loopback" % token, daemon=True).start()
    return port
