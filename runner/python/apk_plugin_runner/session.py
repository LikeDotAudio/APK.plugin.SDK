# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Session: the runner's one broker session.

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import json
import os
import sys
import threading
import time
from typing import Any, Callable, Dict, List

from .contract import (
    LEAF_ENGAGE, LEAF_RESULT, LEAF_RUN, MAX_PACKET, MODULE_ROOT, TICK_S, apk_plugin_api,
    apk_plugin_panels, is_online_now, module_inbox, module_incoming
)
from .selftest import Check, Report, agent_state_publications, parse_depth, parse_engaged
from .report import REPORT_DOWN, clear_problem, report_checks, report_install
from .settings import broker_address, broker_is_local, machine_name, module_topic
from .mqtt import connect_failed, mqtt_client
from .loopback import published
from .ticking import stalest_tick, tick_sample, write_health_file


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
        report_install(self)

    # -- the publishes

    def publish(self, topic: str, payload, retain: bool, qos: int = 1) -> None:
        # COUNTED (ROAD-3478): paho answers a refused publish (no session, a
        # full queue) with a non-zero `rc`, not an exception, and both are a
        # `publish_failures` in the loopback telemetry and the tick.
        try:
            info = self.client.publish(topic, payload, qos=qos, retain=retain)
            return published(getattr(info, "rc", 0) == 0)
        except Exception as error:                      # noqa: BLE001
            published(False)
            print("⚠️  [%s] publish %s failed: %s" % (self.token, topic, error), file=sys.stderr)
            return False

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
        # Up again: what an earlier process said on its way down is over.
        clear_problem(REPORT_DOWN, self.token, any_process=True)
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
        report_checks(report)
        self.publish_report(report)

    def publish_panels(self, out) -> None:
        """Device panels, retained; an empty payload clears one.

        A panel change is said ONCE (a repaint, a clear), so one the client refuses (no
        session, a full queue) waits in `pending_panels`, newest per topic, and is retried
        on the next tick; one that goes out drops anything older still waiting for its
        topic, so a retry never puts an older panel over a newer one. The Rust runner's
        rule, the same (ROAD-3566)."""
        pending = getattr(self, "pending_panels", None)
        if pending is None:
            pending = self.pending_panels = []
        for topic, payload in out:
            pending[:] = [(t, p) for t, p in pending if t != topic]
            if not self.publish(topic, payload, True):
                pending.append((topic, payload))

    def retry_panels(self) -> None:
        """What a refused publish left waiting goes out first, on the tick."""
        waiting, self.pending_panels = list(getattr(self, "pending_panels", None) or []), []
        if waiting:
            self.publish_panels(waiting)

    def publish_report(self, report: Report) -> None:
        for topic, payload, retain in report.publications():
            self.publish(topic, payload, retain)

    # -- the threads

    def telemetry_forever(self) -> None:
        while True:
            # A silent device's panel leaves on the tick (`panels_expire_after_s`).
            if self.book.is_active():
                self.retry_panels()
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
