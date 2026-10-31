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

The exit codes, the topics, the vocabularies and the report's JSON are the
Rust crate's and contracts/rust/src/self_test.rs's, SPELLED HERE rather than
imported, for the reason apk_plugin_api.py gives: the APK.plugin.SDK carries
runner/ and not contracts/python, so a plugin that builds from its own
repository must not need the monorepo to start. `test_apk_plugin_runner.py`
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
        try:
            self.client.publish(topic, payload, qos=qos, retain=retain)
        except Exception as error:                      # noqa: BLE001
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
            sample = {"uptime_s": int(time.monotonic() - self.started), "pid": os.getpid(),
                      "last_tick_age_s": stale[1] if stale else None,
                      "image_rev": os.environ.get("APK_IMAGE_REV")}
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
          kind: str = "plugin") -> None:
    """Run the plugin until something says it must not keep running. Never returns.

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
    names = [fn.__name__ for fn in run]
    print("🚀 [%s] plugin runner (python) — broker %s:%d %s" % (
        token, host, port, "as %s" % credentials()[0] if credentials() else "anonymously"))
    api = apk_plugin_api.PluginApi(
        token, config=loaded, config_source=config_source,
        broker_host=host, broker_port=port, authenticated=credentials() is not None,
        plugin_crate=plugin_crate, plugin_version=plugin_version,
        broker_is_ours=broker_is_local(host), stall_after_s=stall, tick_s=TICK_S,
        health_file=health_file(), pod=resolve_pod(pod), kind=kind)
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
         kind: str = "plugin") -> None:
    """A Python plugin's whole `__main__`: the twin of `apk_plugin_runner::main!`.

    `panels=[...]` reads the templates from `gui_dir`, by default the `gui/`
    folder beside `config_ini` — where every plugin keeps it."""
    version = "%s %s" % (plugin_crate, plugin_version) if plugin_crate else None
    code = answer_arguments(sys.argv[1:], token, [fn.__name__ for fn in run], version)
    if code is not None:
        sys.exit(code)
    text, source = load_config(config_ini)
    if panels and gui_dir is None:
        gui_dir = os.path.join(os.path.dirname(os.path.abspath(config_ini)), "gui")
    serve(token, text, source, self_test, run, plugin_crate, plugin_version, pod, config_overlay,
          panels=panels, gui_dir=gui_dir, kind=kind)
