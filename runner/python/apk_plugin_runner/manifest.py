# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The agent manifest (agent.toml, a TOML subset) and the telemetry rule it is judged by.

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import os
import re
import sys
from typing import Any, Dict, Iterable, Optional, Tuple

from .contract import HERE


# ------------------------------------------------- loopback telemetry (ROAD-3478)
#
# THE TWIN OF RUNNER/src/loopback.rs: `GET /status` and `GET /telemetry` on
# 127.0.0.1, at the port the plugin's `agent.toml` declares, with the same
# documents (contracts 0.9.0) and THE SAME RULE — taken from contracts' Python
# half (`apkaudio_contracts.baremetal_telemetry.derive_state`), never spelled
# here, because the word is the one thing the two runners must not disagree on.
#
# OPT-IN, as in Rust: `main(..., manifest="agent.toml")`. Without one nothing
# is served and the supervisor reads `unknown`, which is the truth. The
# COUNTERS run regardless (every runner publish is counted, and the
# `incoming/telemetry` tick carries them); only the HTTP surface is declared.
# A tree without contracts/python (an SDK tag before sdk-v0.9) serves nothing
# and says so: a loopback that cannot apply the rule must not invent one.
#
# NOT A SECOND BUS: nothing here is published; `incoming/api` names the port
# (`loopback`), never the counters. Stdlib only, one thread, GET only.

TELEMETRY_PORT_ENV = "APK_TELEMETRY_PORT"
# contracts/rust/src/baremetal_telemetry.rs (test_apk_plugin_runner reads them)
AGENT_KINDS = ("protocol", "vendor", "service", "worker")
IMPLEMENTATION_STATUSES = ("live", "discovery-only", "stub")


def telemetry_rule():
    """`apkaudio_contracts.baremetal_telemetry`, or None where it is not importable.

    The installed package first (the image stages it beside the runner since
    sdk-v0.9), then the monorepo's own copy, as `apk_plugin_api.contract_version`
    finds it.
    """
    try:
        from apkaudio_contracts import baremetal_telemetry
        return baremetal_telemetry
    except Exception:                                   # noqa: BLE001
        pass
    candidate = os.path.normpath(os.path.join(HERE, *[".."] * 6, "contracts", "python"))
    if os.path.isdir(candidate) and candidate not in sys.path:
        sys.path.append(candidate)
        try:
            from apkaudio_contracts import baremetal_telemetry
            return baremetal_telemetry
        except Exception:                               # noqa: BLE001
            return None
    return None


def _toml_subset(text: str) -> Dict[Tuple[str, str], Any]:
    """{(section, key): value} for the subset contracts' Manifest::parse reads:
    sections, "strings", integers, booleans and arrays of strings (one line or
    several). Python 3.11's tomllib is used when present; this is the fallback."""
    try:
        import tomllib                                  # Python 3.11+
        table = tomllib.loads(text)
        return {(section, key): value for section, body in table.items() if isinstance(body, dict)
                for key, value in body.items()}
    except ImportError:
        pass
    out: Dict[Tuple[str, str], Any] = {}
    section, pending = "", None
    for raw in text.splitlines():
        line = re.sub(r'\s*#[^"]*$', "", raw).strip()   # a comment, not a # inside a string
        if pending is not None:
            pending[1] += " " + line
            if "]" in line:
                out[(section, pending[0])] = re.findall(r'"([^"]*)"', pending[1])
                pending = None
            continue
        if not line or line.startswith("#"):
            continue
        head = re.fullmatch(r"\[([A-Za-z0-9_.-]+)\]", line)
        if head:
            section = head.group(1)
            continue
        pair = re.fullmatch(r"([A-Za-z0-9_]+)\s*=\s*(.*)", line)
        if not pair:
            raise ValueError("not a key = value line: %r" % raw)
        key, value = pair.group(1), pair.group(2).strip()
        if value.startswith("["):
            if "]" in value:
                out[(section, key)] = re.findall(r'"([^"]*)"', value)
            else:
                pending = [key, value]
        elif value.startswith('"'):
            out[(section, key)] = value[1:value.index('"', 1)]
        elif value in ("true", "false"):
            out[(section, key)] = value == "true"
        else:
            out[(section, key)] = int(value)
    return out


class Manifest:
    """An agent's `agent.toml`, parsed and validated as contracts'
    `Manifest::parse` does it — every refusal names the key."""

    def __init__(self, name, kind, title, status, summary, telemetry_port, config_reads):
        self.name, self.kind, self.title = name, kind, title
        self.status, self.summary = status, summary
        self.telemetry_port, self.config_reads = telemetry_port, config_reads

    @staticmethod
    def parse(text: str) -> "Manifest":
        table = _toml_subset(text)

        def text_of(section, key):
            if (section, key) not in table:
                raise ValueError("[%s] %s is required" % (section, key))
            value = table[(section, key)]
            if not isinstance(value, str) or not value.strip():
                raise ValueError("[%s] %s must be a non-empty string" % (section, key))
            return value

        name, kind = text_of("agent", "name"), text_of("agent", "kind")
        if kind not in AGENT_KINDS:
            raise ValueError("[agent] kind %r is not one of %s" % (kind, AGENT_KINDS))
        status = text_of("agent", "status")
        if status not in IMPLEMENTATION_STATUSES:
            raise ValueError("[agent] status %r is not one of %s" % (status, IMPLEMENTATION_STATUSES))
        title, summary = text_of("agent", "title"), text_of("agent", "summary")
        port = table.get(("telemetry", "port"), 0)
        if isinstance(port, bool) or not isinstance(port, int) or not 0 <= port <= 65535:
            raise ValueError("[telemetry] port must be an integer 0-65535")
        reads = table.get(("config", "reads"))
        if reads is not None and (not isinstance(reads, list) or not all(isinstance(r, str) for r in reads)):
            raise ValueError("[config] reads must be an array of strings")
        return Manifest(name, kind, title, status, summary, port, reads)

    def is_stub(self) -> bool:
        return self.status == "stub"

    def unread_config_keys(self, present: Iterable[str]) -> Optional[int]:
        """How many of `present` the manifest does not list as read; None when
        it lists nothing. Case-insensitive, as the ini reader is."""
        if self.config_reads is None:
            return None
        reads = [r.lower() for r in self.config_reads]
        return sum(1 for key in present if key.lower() not in reads)
