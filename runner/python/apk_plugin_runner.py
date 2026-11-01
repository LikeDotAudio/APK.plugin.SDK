# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""`apk_plugin_runner` — the Python twin of the `apk-plugin-runner` crate.

LOOSE-template-python-twin / LOOSE-bluetooth-no-runtime. A plugin written in
Python does the same five things a Rust plugin's `main!` does, through this
module, so the bus cannot tell which language a plugin was written in:

  1. Broker    `MQTT_HOST` / `MQTT_PORT` (127.0.0.1:1883), `APK_MQTT_USERNAME`
               / `APK_MQTT_PASSWORD` — the names `broker_auth` reads — and a
               reconnect paced by paho's backoff (1 s .. 30 s), never a hot loop.
  2. Config    the plugin's own `config.ini` (or the file `APK_PLUGIN_CONFIG`
               names), published retained to `<topic>/config` as
               `{"value": {section: {key: value}}, "path", "source": "plugin"}`,
               `<topic>/status = online`, and `<topic>/host` on a broker the
               bench owns — `publish_config` in RUNNER/src/lib.rs.
  3. Liveness  the session's LAST WILL is `incoming/state = unavailable`,
               retained; `incoming/telemetry` ticks every 15 s while it lives,
               and the tick rewrites the health file `apk-plugin-health` reads.
  4. Selftest  `outgoing/selftest` (engage) and `outgoing/selftest_run` (a
               depth, never retained) for THIS module, answered through the
               plugin's `self_test(depth_word) -> report JSON` door, with the
               verdict `running` published before the run, and the same
               `the_broker_session_carried_this_press` check appended.
               `outgoing/api_get` republishes `incoming/api` (apk_plugin_api).
  5. Watchdog  every agent runs on its own thread. A thread that returns exits
               70, one that raises exits 101, an agent that has called
               `tick()` and stops calling it for `APK_PLUGIN_STALL_SECS`
               (120) exits 75, the session thread dying exits 69, and
               SIGTERM/SIGINT exit 143 within a second.
  6. Loopback  `main(..., manifest="agent.toml")` serves `GET /status` and
               `GET /telemetry` on 127.0.0.1 at the declared port, the same
               documents as RUNNER/src/loopback.rs, the state from contracts'
               rule; `packet_received()` is what moves `running` to `serving`,
               and every runner publish is counted (ROAD-3478).

The exit codes, the topics, the vocabularies and the report's JSON are the
Rust crate's and contracts/rust/src/self_test.rs's, SPELLED HERE rather than
imported, for the reason apk_plugin_api.py gives: a plugin that builds from
its own repository must not need the monorepo to start, and only SDK tags from
sdk-v0.9 on carry contracts/python beside runner/ (a plugin pinned to an older
tag gets the runner alone). `test_apk_plugin_runner.py`
reads the Rust sources and fails when the two disagree.

    import apk_plugin_runner as runner

    def run_hello_agent(host, port):
        while True:
            ...                       # the work
            runner.tick()             # arms the stall watchdog for this agent

    def self_test(depth_word):
        return runner.run_as_json("hello", depth_word, lambda depth:
            runner.Report("hello", depth).with_(runner.Check.pass_(...)))

    if __name__ == "__main__":
        runner.main(token="hello", config_ini=os.path.join(HERE, "config.ini"),
                    self_test=self_test, run=[run_hello_agent],
                    plugin_crate="apkaudio-hello", plugin_version="0.1.0")
"""

from __future__ import annotations

import ipaddress
import json
import os
import re
import signal
import sys
import threading
import time
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import apk_plugin_api  # noqa: E402  (beside this file, in the SDK and the image)
import apk_plugin_panels  # noqa: E402  (the panels.rs twin, beside this file)

# ------------------------------------------------------------ the contract

ROOT = "APK.audio"
MODULE_ROOT = ROOT + "/System/Protocols"

# contracts/rust/src/self_test.rs
LEAF_ENGAGE = "selftest"
LEAF_RUN = "selftest_run"
LEAF_RESULT = "selftest_result"
LEAF_REPORT = "selftest_report"
ENGAGEMENTS = ("engaged", "off")
VERDICTS = ("pass", "running", "idle", "refused", "fail")
DEPTHS = ("self", "bus", "device", "all")
OUTCOMES = ("pass", "fail", "skipped")

# contracts/rust/src/agent_state.rs
AGENT_STATES = ("listening", "partial", "unavailable", "under_test")

# contracts/rust/src/presence.rs
ONLINE_NOW = "-1"

# RUNNER/src/lib.rs
TICK_S = 15
EXIT_AGENT_RETURNED = 70
EXIT_AGENT_PANICKED = 101
EXIT_SESSION_DIED = 69
EXIT_STALLED = 75
EXIT_TERMINATED = 143
DEFAULT_STALL_SECS = 120
DEFAULT_HEALTH_FILE = "/tmp/apk-plugin.alive"
# rumqttc's packet ceiling the Rust session sets; paho has none of its own.
MAX_PACKET = 4 * 1024 * 1024


def module_incoming(module: str, leaf: str) -> str:
    return "%s/%s/incoming/%s" % (MODULE_ROOT, module, leaf)


def module_outgoing(module: str, leaf: str) -> str:
    return "%s/%s/outgoing/%s" % (MODULE_ROOT, module, leaf)


def module_inbox(module: str) -> str:
    return "%s/%s/outgoing/#" % (MODULE_ROOT, module)


def agent_topic(module: str, leaf: str) -> str:
    return "%s/%s/Agent/%s" % (MODULE_ROOT, module, leaf)


def is_online_now(raw: str) -> bool:
    """`presence::is_online_now`: a negative number is the present tense."""
    try:
        return float(raw.strip()) < 0.0
    except ValueError:
        return False


# ------------------------------------------------------------- self test

def parse_engaged(raw: str) -> str:
    """`Engaged::parse` — generous about "on", and anything else is `off`."""
    word = raw.strip().strip('"').lower()
    return "engaged" if word in ("engaged", "engage", "on", "true", "1", "yes", "test") else "off"


def parse_depth(raw: str) -> Optional[str]:
    """`Depth::parse` — the depth word, or None for a word outside the set."""
    word = raw.strip().strip('"').lower()
    return {"self": "self", "own": "self", "bus": "bus", "broker": "bus",
            "device": "device", "hardware": "device", "all": "all"}.get(word)


def covers(depth: str, other: str) -> bool:
    return depth == "all" or depth == other


class Check:
    """One named thing a run proved, or did not (`self_test::Check`)."""

    def __init__(self, name: str, depth: str, outcome: str, detail: str):
        if depth not in DEPTHS or outcome not in OUTCOMES:
            raise ValueError("check %s: depth %r / outcome %r outside the vocabulary" % (name, depth, outcome))
        self.name, self.depth, self.outcome, self.detail = name, depth, outcome, detail

    @staticmethod
    def pass_(name: str, depth: str, detail: str) -> "Check":
        return Check(name, depth, "pass", detail)

    @staticmethod
    def fail(name: str, depth: str, detail: str) -> "Check":
        return Check(name, depth, "fail", detail)

    @staticmethod
    def skipped(name: str, depth: str, detail: str) -> "Check":
        return Check(name, depth, "skipped", detail)

    @staticmethod
    def of(name: str, depth: str, pass_detail: str, test: Callable[[], Any]) -> "Check":
        """`Check::from_result`: `test()` returning passes; raising fails with its message."""
        try:
            test()
        except Exception as error:                      # noqa: BLE001
            return Check.fail(name, depth, str(error) or type(error).__name__)
        return Check.pass_(name, depth, pass_detail)


class Report:
    """Everything one run concluded about one module (`self_test::Report`)."""

    def __init__(self, module: str, depth: str, refused: Optional[str] = None):
        if depth not in DEPTHS:
            raise ValueError("depth %r outside the vocabulary" % depth)
        self.module, self.depth, self.refused = module, depth, refused
        self.checks: List[Check] = []

    @staticmethod
    def refuse(module: str, depth: str, why: str) -> "Report":
        return Report(module, depth, refused=why)

    def record(self, check: Check) -> "Report":
        """A check outside this run's depth is recorded `skipped`, whatever it said."""
        if not covers(self.depth, check.depth):
            check = Check.skipped(check.name, check.depth, "not in this run's depth")
        self.checks.append(check)
        return self

    with_ = record

    def counts(self) -> Tuple[int, int, int]:
        return tuple(sum(1 for c in self.checks if c.outcome == o) for o in OUTCOMES)  # type: ignore[return-value]

    def verdict(self) -> str:
        if self.refused is not None:
            return "refused"
        passed, failed, _ = self.counts()
        return "fail" if failed else ("pass" if passed else "idle")

    def to_json(self) -> Dict[str, Any]:
        passed, failed, skipped = self.counts()
        out = {
            "module": self.module,
            "depth": self.depth,
            "verdict": self.verdict(),
            "passed": passed,
            "failed": failed,
            "skipped": skipped,
            "checks": [{"name": c.name, "depth": c.depth, "outcome": c.outcome, "detail": c.detail}
                       for c in self.checks],
        }
        if self.refused is not None:
            out["refused"] = self.refused
        return out

    @staticmethod
    def from_json(raw: str) -> "Report":
        value = json.loads(raw)
        depth = parse_depth(str(value.get("depth", "")))
        if depth is None:
            raise ValueError("depth %r is outside the vocabulary" % value.get("depth"))
        report = Report(str(value.get("module", "")), depth, refused=value.get("refused"))
        for check in value.get("checks") or []:
            check_depth = parse_depth(str(check.get("depth", "")))
            if check_depth is None:
                raise ValueError("check %s has no readable depth" % check.get("name"))
            # Appended, not recorded: the producer already filed the depths.
            report.checks.append(Check(str(check.get("name", "")), check_depth,
                                       str(check.get("outcome")), str(check.get("detail", ""))))
        return report

    def headline(self) -> str:
        if self.refused is not None:
            return "%s %s — refused: %s" % (self.module, self.depth, self.refused)
        return "%s %s — %d passed, %d failed, %d skipped" % ((self.module, self.depth) + self.counts())

    def publications(self) -> List[Tuple[str, str, bool]]:
        return [(module_incoming(self.module, LEAF_RESULT), self.verdict(), True),
                (module_incoming(self.module, LEAF_REPORT), json.dumps(self.to_json()), True)]


def run_as_json(module: str, depth_word: str, run: Callable[[str], Report]) -> str:
    """`self_test::run_as_json` — the door every plugin's self test exposes."""
    depth = parse_depth(depth_word)
    if depth is None:
        report = Report.refuse(module, "self", "`%s` is not a depth — use self, bus, device or all"
                               % depth_word.strip())
    else:
        report = run(depth)
    return json.dumps(report.to_json())


def agent_state_publications(module: str, state: str, detail: str,
                             interfaces: Iterable[str] = ()) -> List[Tuple[str, str, bool]]:
    """`AgentReport::publications` — `Agent/{state,detail,interface}` and the
    same three under `incoming/`, all retained."""
    if state not in AGENT_STATES:
        raise ValueError("agent state %r outside the vocabulary" % state)
    interface = ",".join(i.strip() for i in interfaces if i.strip())
    out = []
    for leaf, payload in (("state", state), ("detail", detail), ("interface", interface)):
        out.append((agent_topic(module, leaf), payload, True))
    for leaf, payload in (("state", state), ("detail", detail), ("interface", interface)):
        out.append((module_incoming(module, leaf), payload, True))
    return out


# ---------------------------------------------------------------- config

def ini_to_json(text: str) -> Dict[str, Dict[str, str]]:
    """`ini_to_json` — `{section: {key: value}}`, every value a string."""
    root: Dict[str, Dict[str, str]] = {}
    section = ""
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or line.startswith(";"):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1].strip()
            root.setdefault(section, {})
            continue
        if "=" in line:
            key, value = line.split("=", 1)
        elif ":" in line:
            key, value = line.split(":", 1)
        else:
            continue
        root.setdefault(section, {})[key.strip()] = value.strip()
    return root


def module_topic(config: Dict[str, Dict[str, str]], token: str) -> str:
    """The section's `topic` key (section matched without case), else the default."""
    for name, section in config.items():
        if name.lower() == token.lower() and isinstance(section.get("topic"), str):
            return section["topic"]
    return "%s/%s" % (MODULE_ROOT, token)


def broker_is_local(host: str) -> bool:
    """`broker_is_local` — syntactic, no DNS; anything not obviously ours is public."""
    host = host.strip().lstrip("[").rstrip("]")
    lower = host.lower()
    if lower in ("localhost", "broker") or lower.endswith(".local") or lower.endswith(".localhost"):
        return True
    try:
        ip = ipaddress.ip_address(host)
    except ValueError:
        return False
    if ip.version == 4:
        return ip.is_loopback or ip.is_private or ip.is_link_local
    return ip.is_loopback or ip.is_link_local or ((int(ip) >> 112) & 0xFE00) == 0xFC00


def broker_address() -> Tuple[str, int]:
    host = (os.environ.get("MQTT_HOST") or "").strip() or "127.0.0.1"
    try:
        port = int((os.environ.get("MQTT_PORT") or "").strip())
    except ValueError:
        port = 1883
    return host, port


def credentials() -> Optional[Tuple[str, str]]:
    """`broker_auth::credentials` — both set, the username not blank, or None."""
    user = os.environ.get("APK_MQTT_USERNAME")
    password = os.environ.get("APK_MQTT_PASSWORD")
    if user is None or password is None or not user.strip() or not password:
        return None
    return user, password


def health_file() -> str:
    return os.environ.get("APK_PLUGIN_HEALTH_FILE") or DEFAULT_HEALTH_FILE


def stall_after_s() -> int:
    try:
        return int(os.environ.get("APK_PLUGIN_STALL_SECS", ""))
    except ValueError:
        return DEFAULT_STALL_SECS


def machine_name() -> Optional[str]:
    try:
        with open("/proc/sys/kernel/hostname", encoding="utf-8") as handle:
            return handle.read().strip() or None
    except OSError:
        return None


# ------------------------------------------------------------------- MQTT

def mqtt_client(client_id: str):
    """A paho client on either API generation (Debian ships 1.6, pip 2.x).

    LOOSE-bluetooth-paho-v1: on 2.x the client is asked for callback API
    VERSION2 — VERSION1 is deprecated there, and a release that drops it would
    raise here and leave a plugin silently standalone. 1.x has no
    CallbackAPIVersion and speaks its only API. Every callback in the runner
    and the plugins is written to fit both: `on_connect(client, userdata,
    flags, rc, *rest)` (rc an int on 1.x, a ReasonCode on 2.x — read it with
    `connect_failed`) and `on_message(client, userdata, message)`, the same in
    both.
    """
    import paho.mqtt.client as mqtt
    version = getattr(mqtt, "CallbackAPIVersion", None)
    if version is not None:
        client = mqtt.Client(version.VERSION2, client_id=client_id)
    else:
        client = mqtt.Client(client_id=client_id)
    pair = credentials()
    if pair:
        client.username_pw_set(*pair)
    client.reconnect_delay_set(min_delay=1, max_delay=30)
    return client


def connect_failed(rc) -> bool:
    """True when a CONNACK refused the session: rc is an int on paho 1.x, a
    ReasonCode (with `is_failure`) on the 2.x VERSION2 API."""
    failure = getattr(rc, "is_failure", None)
    return bool(failure) if failure is not None else rc != 0


def agent_client(token: str, name: str):
    """A session of the agent's own, as the Rust template's agent opens one.

    Not the runner's session: that one carries the last will and answers the
    self test, and an agent that publishes through it could wedge the press
    that tells an operator the agent is wedged. Connected asynchronously, so a
    broker that is not up yet is retried, not fatal. The caller may set a will
    and callbacks before calling `start_agent_client`.
    """
    return mqtt_client("apkaudio-%s-%s-%d" % (token, name, os.getpid()))


def start_agent_client(client) -> None:
    host, port = broker_address()
    client.connect_async(host, port, keepalive=30)
    client.loop_start()


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
        print("⚠️  [%s] loopback telemetry NOT served — 127.0.0.1:%d: %s" % (token, port, error), file=sys.stderr)
        return None
    _LOOPBACK["port"] = port
    print("📈 [%s] loopback telemetry on http://127.0.0.1:%d/telemetry" % (token, port))
    threading.Thread(target=server.serve_forever, name="%s:loopback" % token, daemon=True).start()
    return port


# --------------------------------------------------------------- ticking

_LAST_TICK = [0]                     # a tick from a thread no agent owns
_AGENT_TICKS: Dict[str, List[int]] = {}
_SLOT = threading.local()


def tick() -> None:
    """An agent says "still making progress". Arms ITS stall clock, once called."""
    clock = _AGENT_TICKS.get(getattr(_SLOT, "agent", None) or "")
    (clock if clock is not None else _LAST_TICK)[0] = int(time.time())


def stalest(clocks: Iterable[Tuple[str, int]], now: int) -> Optional[Tuple[str, int]]:
    aged = [(name, max(0, now - last)) for name, last in clocks if last != 0]
    return max(aged, key=lambda pair: pair[1]) if aged else None


def stalest_tick() -> Optional[Tuple[str, int]]:
    clocks = [(name, clock[0]) for name, clock in _AGENT_TICKS.items()]
    clocks.append(("process", _LAST_TICK[0]))
    return stalest(clocks, int(time.time()))


def write_health_file() -> None:
    try:
        with open(health_file(), "w", encoding="utf-8") as handle:
            handle.write(str(int(time.time())))
    except OSError:
        pass


# ------------------------------------------------------------- arguments

def answer_arguments(args: List[str], token: str, agents: List[str],
                     version: Optional[str] = None) -> Optional[int]:
    """`answer_arguments`: None to start, else the exit code (help 0, version 0, else 2)."""
    if not args:
        return None
    exe = os.path.basename(sys.argv[0] or "apk-plugin")
    usage = (
        "%s — APK.audio plugin runner (Python) for [%s] (agents: %s)\n\n"
        "Usage: %s            run until stopped; settings come from the environment\n"
        "       %s --help     this text, and exit (touches no broker)\n"
        "       %s --version  the build, and exit\n\n"
        "Environment:\n"
        "  MQTT_HOST              broker host (default 127.0.0.1)\n"
        "  MQTT_PORT              broker port (default 1883)\n"
        "  APK_PLUGIN_CONFIG      a config.ini to use instead of the plugin's own\n"
        "  APK_PLUGIN_STALL_SECS  seconds without a tick before the agent counts as stalled (default 120)\n"
        % (exe, token, ", ".join(agents), exe, exe, exe))
    first = args[0]
    if first in ("-h", "--help", "help"):
        sys.stdout.write(usage)
        return 0
    if first in ("-V", "--version"):
        print(version or "%s (unstamped build)" % exe)
        return 0
    sys.stderr.write("%s: takes no arguments (got %r); nothing was started.\n\n%s" % (exe, first, usage))
    return 2


# ---------------------------------------------------------------- serving

def tick_sample(uptime_s: int) -> Dict[str, Any]:
    """The `incoming/telemetry` tick: RUNNER/src/lib.rs's `tick_sample`, key
    for key — `counters` is the loopback's block (ROAD-3478), so a bus reader
    applies contracts' rule to the tick as the supervisor does to the loopback."""
    stale = stalest_tick()
    sample = {"uptime_s": uptime_s, "pid": os.getpid(),
              "last_tick_age_s": stale[1] if stale else None,
              "image_rev": os.environ.get("APK_IMAGE_REV"),
              "counters": counters()}
    if _LOOPBACK["disabled"]:
        sample["state"], sample["reason"] = "stub", _LOOPBACK["disabled"]
    return sample


class Session:
    """The runner's one broker session: config, api, will, self test, telemetry."""

    def __init__(self, token: str, config: Dict[str, Dict[str, str]], config_source: str,
                 door: Callable[[str], str], api: "apk_plugin_api.PluginApi",
                 agents: List[str]):
        self.token = token
        self.config = config
        self.config_source = config_source
        self.topic = module_topic(self.config, token)
        self.door = door
        self.api = api
        self.agents = agents
        self.engaged = "off"
        self.presence: set = set()
        self.lock = threading.Lock()
        self.host, self.port = broker_address()
        self.ours = broker_is_local(self.host)
        self.started = time.monotonic()
        self.client = mqtt_client("apk-plugin-%s-%d" % (token, os.getpid()))
        try:
            self.client.max_packet_size = MAX_PACKET   # paho 2.x reads it; 1.x ignores
        except Exception:                               # noqa: BLE001
            pass
        # THE LAST WILL IS THE LIVENESS: retained, so a reader after the crash reads it.
        self.client.will_set(module_incoming(token, "state"), "unavailable", qos=1, retain=True)
        self.client.on_connect = self.on_connect
        self.client.on_message = self.on_message
        # DEVICE PANELS (panels.rs): only for a plugin that registered a gui/
        # template; for every other plugin the book is inert and subscribes nothing.
        self.book = apk_plugin_panels.Book(token, apk_plugin_panels.templates(),
                                           apk_plugin_panels.expire_after_from_config(config, token))
        self.book_lock = threading.Lock()

    # -- the publishes

    def publish(self, topic: str, payload, retain: bool, qos: int = 1) -> None:
        # COUNTED (ROAD-3478): paho answers a refused publish (no session, a
        # full queue) with a non-zero `rc`, not an exception, and both are a
        # `publish_failures` in the loopback telemetry and the tick.
        try:
            info = self.client.publish(topic, payload, qos=qos, retain=retain)
            published(getattr(info, "rc", 0) == 0)
        except Exception as error:                      # noqa: BLE001
            published(False)
            print("⚠️  [%s] publish %s failed: %s" % (self.token, topic, error), file=sys.stderr)

    def publish_config(self) -> None:
        payload = {"value": self.config, "path": "%s/config.ini" % self.token, "source": "plugin"}
        self.publish(self.topic + "/config", json.dumps(payload), True)
        self.publish(self.topic + "/status", "online", True)
        if self.ours:
            machine = machine_name()
            if machine:
                self.publish(self.topic + "/host", machine, True)
        print("📡 [%s] config published retained to %s/config (+ /status=online)" % (self.token, self.topic))

    def api_fields(self) -> Dict[str, Any]:
        stale = stalest_tick()
        with self.lock:
            return {"agents": list(self.agents), "presence_online": len(self.presence),
                    "engaged": self.engaged == "engaged",
                    "last_tick_age_s": stale[1] if stale else None}

    def publish_api(self) -> None:
        self.api.publish(self.client, **self.api_fields())

    # -- the callbacks (signatures both paho 1.x and VERSION2 call)

    def on_connect(self, client, userdata, flags, rc, *rest):
        if connect_failed(rc):
            print("⚠️  [%s] broker refused the session (rc=%s); paho retries" % (self.token, rc), file=sys.stderr)
            return
        # Every (re)connect: the broker may have lost its retained store, and a
        # subscription does not survive a clean session.
        self.publish_config()
        self.publish_api()
        client.subscribe(module_inbox(self.token), qos=1)
        client.subscribe("%s/%s/Device/+/last_online" % (MODULE_ROOT, self.token), qos=1)
        if self.book.is_active():
            client.subscribe(self.book.filter(), qos=1)
            # Read back what an earlier process left, so a panel no device
            # claims is emptied, not kept.
            client.subscribe(self.book.panels_filter(), qos=1)
            with self.book_lock:
                restated = self.book.restate()
            self.publish_panels(restated)

    def on_message(self, client, userdata, message):
        topic = message.topic
        payload = message.payload.decode("utf-8", "replace") if message.payload else ""
        retained = bool(getattr(message, "retain", False))
        # Every device leaf feeds the panel book first; the presence set below
        # still sees its own `last_online`.
        if self.book.is_active():
            with self.book_lock:
                out = self.book.observe(topic, payload, time.monotonic())
            self.publish_panels(out)
            if topic.startswith(apk_plugin_panels.PANELS_TOPIC_ROOT + "/"):
                return
        if topic.endswith("/last_online"):
            with self.lock:
                (self.presence.add if is_online_now(payload) else self.presence.discard)(topic)
            return
        if topic.endswith("/" + apk_plugin_api.LEAF_API_GET):
            if not retained:            # a retained press is ignored, as selftest_run's is
                self.publish_api()
            return
        leaf = topic.rsplit("/", 1)[-1]
        if leaf == LEAF_ENGAGE:
            self.engage(payload)
        elif leaf == LEAF_RUN and not retained:
            self.perform(payload)

    # -- the self test

    def engage(self, payload: str) -> None:
        want = parse_engaged(payload)
        with self.lock:
            if want == self.engaged:
                return
            self.engaged = want
            topics = sorted(self.presence)
        self.publish(module_incoming(self.token, LEAF_ENGAGE), want, True)
        if want == "engaged":
            self.publish(module_incoming(self.token, LEAF_RESULT), "idle", True)
            for topic, body, retain in agent_state_publications(
                    self.token, "under_test", "withdrawn onto the workbench — findings are not live"):
                self.publish(topic, body, retain)
            stamp = str(int(time.time()))
            for topic in topics:
                self.publish(topic, stamp, True)
            print("🧪 [%s] withdrawn — %d device(s) taken offline" % (self.token, len(topics)))
        else:
            print("🧪 [%s] released — its own next announcement restores it" % self.token)

    def perform(self, payload: str) -> None:
        depth = parse_depth(payload)
        if depth is None:
            report = Report.refuse(self.token, "self",
                                   "`%s` is not a depth — use self, bus, device or all" % payload.strip())
            self.publish_report(report)
            return
        self.publish(module_incoming(self.token, LEAF_RESULT), "running", True)
        with self.lock:
            engaged = self.engaged == "engaged"
        if not engaged:
            report = Report.refuse(self.token, depth,
                                   "not under test — publish `engaged` to outgoing/selftest first")
        else:
            try:
                report = Report.from_json(self.door(depth))
            except Exception as error:                  # noqa: BLE001
                report = Report.refuse(self.token, depth, "its report did not parse: %s" % error)
        report.record(Check.pass_("the_broker_session_carried_this_press", "bus",
                                  "the run request arrived over the session that answers it"))
        print("🧪 [%s] %s" % (self.token, report.headline()))
        self.publish_report(report)

    def publish_panels(self, out) -> None:
        """Device panels, retained; an empty payload clears one."""
        for topic, payload in out:
            self.publish(topic, payload, True)

    def publish_report(self, report: Report) -> None:
        for topic, payload, retain in report.publications():
            self.publish(topic, payload, retain)

    # -- the threads

    def telemetry_forever(self) -> None:
        while True:
            # A silent device's panel leaves on the tick (`panels_expire_after_s`).
            if self.book.is_active():
                with self.book_lock:
                    expired = self.book.expire(time.monotonic())
                self.publish_panels(expired)
            stale = stalest_tick()
            sample = tick_sample(int(time.monotonic() - self.started))
            self.publish(module_incoming(self.token, "telemetry"), json.dumps(sample), False, qos=0)
            write_health_file()
            time.sleep(TICK_S)

    def run_forever(self) -> None:
        """The session thread. Returning (or raising) is a dead session: exit 69."""
        self.client.connect_async(self.host, self.port, keepalive=30)
        self.client.loop_forever(retry_first_connection=True)


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
                print("⚠️  stop hook %s raised: %r" % (getattr(hook, "__name__", hook), error), file=sys.stderr)
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
            print("🛑 [%s] %s does not parse: %s — no loopback telemetry" % (token, manifest, error), file=sys.stderr)
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
                print("🛑 [%s] %s raised: %r" % (token, name, error), file=sys.stderr)
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
                print("🛑 [%s] the broker session thread ended; exiting so the container restarts" % token,
                      file=sys.stderr)
            elif code == EXIT_AGENT_RETURNED:
                print("🛑 [%s] %s returned; a hunter that stops is a failure — exiting" % (token, name),
                      file=sys.stderr)
            else:
                print("🛑 [%s] %s raised — exiting" % (token, name), file=sys.stderr)
            sys.stdout.flush()
            sys.stderr.flush()
            os._exit(code)
        stale = stalest_tick()
        if stale and stale[1] > stall:
            print("🛑 [%s] %s: no tick for %ds (limit %ds) — stalled, exiting" % (token, stale[0], stale[1], stall),
                  file=sys.stderr)
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
            print("🛑 APK_PLUGIN_CONFIG=%s did not read (%s); using %s" % (override, error, path), file=sys.stderr)
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
