# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The Python runner says what the Rust runner says, or this fails.

    python3 -m unittest discover -s PODS/POD.THICK/DISCOVERY.Engine/SRC/RUNNER/python -p 'test_*.py'

READ OFF THE RUST SOURCE, not off a third list: the vocabularies and exit codes
are pulled out of RUNNER/src/lib.rs and contracts/rust/src/*.rs. In the SDK the
contracts sit at `../contracts/rust`, in the monorepo at `<repo>/contracts/rust`;
a tree with neither skips those checks and says so.
"""

import json
import os
import re
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import apk_plugin_runner as runner  # noqa: E402
import apk_plugin_health as health  # noqa: E402

RUNNER_LIB = os.path.join(HERE, "..", "src", "lib.rs")
CONTRACT_DIRS = [os.path.normpath(os.path.join(HERE, *[".."] * 6, "contracts", "rust", "src")),   # monorepo
                 os.path.normpath(os.path.join(HERE, "..", "..", "contracts", "rust", "src"))]     # SDK
CONTRACTS = next((d for d in CONTRACT_DIRS if os.path.isdir(d)), None)


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def rust_const(source, name):
    m = re.search(r'const %s: [^=]+= ([^;]+);' % name, source)
    if not m:
        raise AssertionError("no const %s in the Rust source" % name)
    value = m.group(1).strip()
    if value.startswith('"'):
        return value.strip('"')
    if value.startswith('['):
        return tuple(re.findall(r'"([^"]*)"', value))
    if value.startswith("Duration::from_secs("):
        return int(value[len("Duration::from_secs("):-1])
    return int(value)


class FakeMessage:
    def __init__(self, topic, payload, retain=False):
        self.topic, self.payload, self.retain = topic, payload.encode(), retain


class FakeClient:
    def __init__(self):
        self.sent = []

    def publish(self, topic, payload, qos=0, retain=False):
        self.sent.append((topic, payload, retain))

    def subscribe(self, topic, qos=0):
        self.sent.append(("SUBSCRIBE", topic, False))

    def last(self, topic):
        hits = [p for t, p, _ in self.sent if t == topic]
        return hits[-1] if hits else None


class TheRunnerConstantsAreTheRustOnes(unittest.TestCase):
    @unittest.skipIf(not os.path.isfile(RUNNER_LIB), "no RUNNER/src beside this tree (the image)")
    def test_exit_codes_tick_and_packet_ceiling(self):
        lib = read(RUNNER_LIB)
        for name in ("EXIT_AGENT_RETURNED", "EXIT_AGENT_PANICKED", "EXIT_SESSION_DIED",
                     "EXIT_STALLED", "EXIT_TERMINATED"):
            self.assertEqual(getattr(runner, name), rust_const(lib, name), name)
        self.assertEqual(runner.TICK_S, rust_const(lib, "TICK"))
        self.assertIn("unwrap_or(%d)" % runner.DEFAULT_STALL_SECS, lib)
        self.assertIn('"%s"' % runner.DEFAULT_HEALTH_FILE, lib)

    @unittest.skipIf(CONTRACTS is None, "no contracts/rust beside this tree")
    def test_self_test_vocabulary(self):
        source = read(os.path.join(CONTRACTS, "self_test.rs"))
        for name in ("LEAF_ENGAGE", "LEAF_RUN", "LEAF_RESULT", "LEAF_REPORT",
                     "ENGAGEMENTS", "VERDICTS", "DEPTHS"):
            self.assertEqual(getattr(runner, name), rust_const(source, name), name)
        self.assertIn("refused: {why}", source)   # the headline shape mirrored below

    @unittest.skipIf(CONTRACTS is None, "no contracts/rust beside this tree")
    def test_agent_states_and_presence_sentinel(self):
        states = re.findall(r'AgentState::\w+ => "([a-z_]+)"', read(os.path.join(CONTRACTS, "agent_state.rs")))
        self.assertEqual(tuple(dict.fromkeys(states)), runner.AGENT_STATES)
        self.assertEqual(runner.ONLINE_NOW, rust_const(read(os.path.join(CONTRACTS, "presence.rs")), "ONLINE_NOW"))

    @unittest.skipIf(CONTRACTS is None, "no contracts/rust beside this tree")
    def test_the_leaves_it_publishes_are_legal_and_retained_as_the_contract_says(self):
        python_contracts = os.path.normpath(os.path.join(CONTRACTS, "..", "..", "python"))
        if not os.path.isdir(python_contracts):
            self.skipTest("no contracts/python")
        sys.path.insert(0, python_contracts)
        from apkaudio_contracts import retains, module_incoming, module_outgoing
        for leaf, retained in (("state", True), ("telemetry", False), (runner.LEAF_ENGAGE, True),
                               (runner.LEAF_RESULT, True), (runner.LEAF_REPORT, True), ("api", True)):
            self.assertEqual(retains("incoming", leaf), retained, leaf)
            self.assertEqual(module_incoming("zz", leaf)[0], runner.module_incoming("zz", leaf))
        for leaf, retained in ((runner.LEAF_ENGAGE, True), (runner.LEAF_RUN, False), ("api_get", False)):
            self.assertEqual(retains("outgoing", leaf), retained, leaf)
            self.assertEqual(module_outgoing("zz", leaf)[0], runner.module_outgoing("zz", leaf))


class ConfigAsTheRustRunnerReadsIt(unittest.TestCase):
    def test_a_config_ini_becomes_sections_of_strings(self):
        v = runner.ini_to_json("# c\n[dante]\nenabled = true\ndomain = local.\n; x\n[other]\nk=v\n")
        self.assertEqual(v, {"dante": {"enabled": "true", "domain": "local."}, "other": {"k": "v"}})

    def test_the_topic_is_the_sections_own_or_the_tokens_default(self):
        declared = runner.ini_to_json("[PTP]\ntopic = APK.audio/System/Protocols/ptp\n")
        self.assertEqual(runner.module_topic(declared, "ptp"), "APK.audio/System/Protocols/ptp")
        bare = runner.ini_to_json("[dante]\nenabled = true\n")
        self.assertEqual(runner.module_topic(bare, "dante"), "APK.audio/System/Protocols/dante")

    def test_a_public_broker_is_not_ours(self):
        self.assertTrue(runner.broker_is_local("127.0.0.1"))
        self.assertTrue(runner.broker_is_local("broker"))
        self.assertTrue(runner.broker_is_local("fd12::1"))
        self.assertFalse(runner.broker_is_local("test.mosquitto.org"))
        self.assertFalse(runner.broker_is_local("8.8.8.8"))

    def test_apk_pod_names_the_pod_when_it_is_one(self):
        # PLAN-3430.01: the twin of the Rust runner's pod_declared.
        self.assertEqual(runner.pod_declared("POD.workers"), "POD.workers")
        self.assertEqual(runner.pod_declared("  POD.workers\n"), "POD.workers")
        for raw in (None, "", "POD.", "workers", "POD.a/POD.b", "POD.a b"):
            self.assertIsNone(runner.pod_declared(raw), raw)
        saved = os.environ.pop("APK_POD", None)
        try:
            self.assertEqual(runner.resolve_pod("POD.THICK"), "POD.THICK")
            os.environ["APK_POD"] = "POD.workers"
            self.assertEqual(runner.resolve_pod("POD.THICK"), "POD.workers")
            os.environ["APK_POD"] = "nonsense"
            self.assertEqual(runner.resolve_pod("POD.THICK"), "POD.THICK")
        finally:
            os.environ.pop("APK_POD", None)
            if saved is not None:
                os.environ["APK_POD"] = saved

    def test_arguments_stop_before_the_broker(self):
        self.assertIsNone(runner.answer_arguments([], "x", ["a"]))
        self.assertEqual(runner.answer_arguments(["--help"], "x", ["a"]), 0)
        self.assertEqual(runner.answer_arguments(["--version"], "x", ["a"], "apkaudio-x 0.1.0"), 0)
        self.assertEqual(runner.answer_arguments(["--port=1"], "x", ["a"]), 2)


class TheReportIsTheRustReport(unittest.TestCase):
    def test_verdict_is_derived_and_depth_filters(self):
        report = runner.Report("ptp", "self")
        self.assertEqual(report.verdict(), "idle")
        report.record(runner.Check.pass_("a", "self", "ok"))
        report.record(runner.Check.pass_("hw", "device", "would touch hardware"))
        self.assertEqual(report.counts(), (1, 0, 1))
        self.assertEqual(report.verdict(), "pass")
        report.record(runner.Check.of("b", "self", "ok", lambda: int("x")))
        self.assertEqual(report.verdict(), "fail")
        self.assertEqual(report.headline(), "ptp self — 1 passed, 1 failed, 1 skipped")

    def test_json_round_trips_with_the_rust_keys(self):
        report = runner.Report("ptp", "all").with_(runner.Check.pass_("a", "bus", "ok"))
        doc = report.to_json()
        self.assertEqual(list(doc), ["module", "depth", "verdict", "passed", "failed", "skipped", "checks"])
        again = runner.Report.from_json(json.dumps(doc))
        self.assertEqual(again.to_json(), doc)
        refused = runner.Report.refuse("ptp", "bus", "not under test")
        self.assertEqual(refused.to_json()["refused"], "not under test")
        self.assertEqual(refused.verdict(), "refused")

    def test_a_word_outside_the_depths_is_refused_not_guessed(self):
        doc = json.loads(runner.run_as_json("ptp", "deep", lambda d: runner.Report("ptp", d)))
        self.assertEqual(doc["verdict"], "refused")
        self.assertEqual(runner.parse_depth("own"), "self")
        self.assertEqual(runner.parse_engaged("ON"), "engaged")
        self.assertEqual(runner.parse_engaged("garbled"), "off")


class TheSessionAnswersTheBus(unittest.TestCase):
    def setUp(self):
        def door(depth):
            return runner.run_as_json("zz", depth, lambda d: runner.Report("zz", d).with_(
                runner.Check.pass_("own_config", "self", "parses")))
        api = runner.apk_plugin_api.PluginApi("zz")
        self.session = runner.Session("zz", runner.ini_to_json("[zz]\nrestate_s = 30\n"), "config.ini", door, api, ["run_zz"])
        self.client = FakeClient()
        self.session.client = self.client

    def test_connect_publishes_config_status_api_and_subscribes_the_inbox(self):
        self.session.on_connect(self.client, None, {}, 0)
        config = json.loads(self.client.last("APK.audio/System/Protocols/zz/config"))
        self.assertEqual(config, {"value": {"zz": {"restate_s": "30"}}, "path": "zz/config.ini", "source": "plugin"})
        self.assertEqual(self.client.last("APK.audio/System/Protocols/zz/status"), "online")
        api = json.loads(self.client.last("APK.audio/System/Protocols/zz/incoming/api"))
        self.assertEqual(api["process"]["agents"], ["run_zz"])
        self.assertIn(("SUBSCRIBE", "APK.audio/System/Protocols/zz/outgoing/#", False), self.client.sent)

    def test_a_run_before_engaging_is_refused_and_after_it_runs(self):
        result = "APK.audio/System/Protocols/zz/incoming/selftest_result"
        report = "APK.audio/System/Protocols/zz/incoming/selftest_report"
        self.session.on_message(self.client, None, FakeMessage("APK.audio/System/Protocols/zz/outgoing/selftest_run", "self"))
        self.assertEqual(self.client.last(result), "refused")
        self.session.on_message(self.client, None, FakeMessage("APK.audio/System/Protocols/zz/outgoing/selftest", "engaged", True))
        self.assertEqual(self.client.last("APK.audio/System/Protocols/zz/incoming/state"), "under_test")
        self.assertEqual(self.client.last(result), "idle")
        self.session.on_message(self.client, None, FakeMessage("APK.audio/System/Protocols/zz/outgoing/selftest_run", "all"))
        self.assertEqual(self.client.last(result), "pass")
        checks = [c["name"] for c in json.loads(self.client.last(report))["checks"]]
        self.assertEqual(checks, ["own_config", "the_broker_session_carried_this_press"])

    def test_a_retained_run_or_api_press_is_ignored(self):
        self.session.on_message(self.client, None, FakeMessage("APK.audio/System/Protocols/zz/outgoing/selftest_run", "self", True))
        self.session.on_message(self.client, None, FakeMessage("APK.audio/System/Protocols/zz/outgoing/api_get", "", True))
        self.assertEqual(self.client.sent, [])
        self.session.on_message(self.client, None, FakeMessage("APK.audio/System/Protocols/zz/outgoing/api_get", ""))
        self.assertIsNotNone(self.client.last("APK.audio/System/Protocols/zz/incoming/api"))

    def test_presence_is_counted_and_moved_off_the_sentinel_when_engaged(self):
        device = "APK.audio/System/Protocols/zz/Device/a/last_online"
        self.session.on_message(self.client, None, FakeMessage(device, "-1", True))
        self.assertEqual(self.session.api_fields()["presence_online"], 1)
        self.session.on_message(self.client, None, FakeMessage("APK.audio/System/Protocols/zz/outgoing/selftest", "on"))
        self.assertNotEqual(self.client.last(device), "-1")


class ThePahoSessionIsCallbackApiTwo(unittest.TestCase):
    """LOOSE-bluetooth-paho-v1: 2.x is asked for VERSION2, never the deprecated
    VERSION1, and a VERSION2 connect (ReasonCode + properties) still drives
    the session as 1.x's int rc does."""

    def setUp(self):
        try:
            import paho.mqtt.client as mqtt
        except ImportError:
            self.skipTest("no paho on this host")
        self.mqtt = mqtt

    def test_the_client_is_version2_and_warns_of_nothing(self):
        import warnings
        if not hasattr(self.mqtt, "CallbackAPIVersion"):
            self.skipTest("paho 1.x has a single callback API")
        with warnings.catch_warnings():
            warnings.simplefilter("error", DeprecationWarning)
            client = runner.mqtt_client("apk-test-%d" % os.getpid())
        self.assertEqual(client._callback_api_version, self.mqtt.CallbackAPIVersion.VERSION2)

    def test_a_reason_code_or_an_int_says_whether_the_broker_refused(self):
        self.assertFalse(runner.connect_failed(0))
        self.assertTrue(runner.connect_failed(5))
        if not hasattr(self.mqtt, "CallbackAPIVersion"):
            return
        from paho.mqtt.packettypes import PacketTypes
        from paho.mqtt.reasoncodes import ReasonCode
        self.assertFalse(runner.connect_failed(ReasonCode(PacketTypes.CONNACK, "Success")))
        self.assertTrue(runner.connect_failed(ReasonCode(PacketTypes.CONNACK, "Not authorized")))

    def test_a_version2_connect_publishes_config_like_a_version1_one(self):
        if not hasattr(self.mqtt, "CallbackAPIVersion"):
            self.skipTest("paho 1.x has a single callback API")
        from paho.mqtt.packettypes import PacketTypes
        from paho.mqtt.reasoncodes import ReasonCode
        api = runner.apk_plugin_api.PluginApi("zz")
        session = runner.Session("zz", runner.ini_to_json("[zz]\n"), "config.ini",
                                 lambda d: "{}", api, [])
        client = FakeClient()
        session.client = client
        session.on_connect(client, None, self.mqtt.ConnectFlags(False),
                           ReasonCode(PacketTypes.CONNACK, "Not authorized"), None)
        self.assertEqual(client.sent, [])
        session.on_connect(client, None, self.mqtt.ConnectFlags(False),
                           ReasonCode(PacketTypes.CONNACK, "Success"), None)
        self.assertEqual(client.last("APK.audio/System/Protocols/zz/status"), "online")


class TheWatchdogClocks(unittest.TestCase):
    def test_the_stalest_armed_clock_is_judged_and_unarmed_ones_are_not(self):
        self.assertEqual(runner.stalest([("browse", 995), ("control", 800), ("idle", 0)], 1000), ("control", 200))
        self.assertIsNone(runner.stalest([("idle", 0)], 1000))


class TheHealthCheck(unittest.TestCase):
    def test_fresh_stale_missing_and_stub(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "alive")
            self.assertEqual(health.verdict(path, 60, 1000)[0], 1)
            with open(path, "w") as f:
                f.write("990")
            self.assertEqual(health.verdict(path, 60, 1000), (0, ""))
            self.assertEqual(health.verdict(path, 60, 2000)[0], 1)
            with open(path, "w") as f:
                f.write("not built: x")
            self.assertEqual(health.verdict(path, 60, 1000), (1, "not built: x"))



class APanelChangeIsNeverLost(unittest.TestCase):
    """ROAD-3566, the Python twin: a refused panel publish waits and is retried, newest first."""
    def test_refused_waits_a_newer_one_supersedes_and_the_retry_sends_it(self):
        class Refusing:
            def __init__(self):
                self.ok, self.sent = False, []

            def publish(self, topic, payload, qos=0, retain=False):
                self.sent.append((topic, payload))
                return type("Info", (), {"rc": 0 if self.ok else 4})()
        s = runner.Session.__new__(runner.Session)
        s.client, s.token = Refusing(), "t"
        s.publish_panels([("Panels/a", "old"), ("Panels/b", "")])
        self.assertEqual(s.pending_panels, [("Panels/a", "old"), ("Panels/b", "")])
        s.client.ok = True
        s.publish_panels([("Panels/a", "newer")])
        self.assertEqual(s.pending_panels, [("Panels/b", "")], "a newer panel that went out drops the old one")
        s.client.sent.clear()
        s.retry_panels()
        self.assertEqual(s.client.sent, [("Panels/b", "")], "the clear goes out on the retry")
        self.assertEqual(s.pending_panels, [])

if __name__ == "__main__":
    unittest.main()


class ServeForwardsKind(unittest.TestCase):
    """`kind="worker"` (main!'s kind) reaches the incoming/api document."""

    def test_serve_passes_kind_to_the_api_document(self):
        import apk_plugin_runner as runner
        seen = {}

        class Stop(Exception):
            pass

        real = runner.apk_plugin_api.PluginApi

        def capture(*args, **kwargs):
            seen["kind"] = kwargs.get("kind")
            raise Stop()

        runner.apk_plugin_api.PluginApi = capture
        try:
            for kind in ("worker", "plugin"):
                with self.assertRaises(Stop):
                    runner.serve("zz", "[zz]\ntopic = APK.audio/System/Protocols/zz\n", "config.ini",
                                 lambda depth: "{}", [], kind=kind)
                self.assertEqual(seen["kind"], kind)
        finally:
            runner.apk_plugin_api.PluginApi = real


class LoopbackTelemetry(unittest.TestCase):
    """The Python runner serves what loopback.rs serves (ROAD-3478): the same
    routes, the same refusals, the same documents, contracts' rule."""

    MANIFEST = ('[agent]\nname = "probe"   # the token\nkind = "protocol"\ntitle = "Probe"\n'
                'status = "discovery-only"\nsummary = "A test agent."\n[telemetry]\nport = 0\n'
                '[config]\nreads = [\n  "raise",\n]\n')

    def get(self, port, path, method="GET"):
        import http.client
        connection = http.client.HTTPConnection("127.0.0.1", port, timeout=3)
        connection.request(method, path)
        answer = connection.getresponse()
        body = answer.read()
        connection.close()
        return answer.status, json.loads(body)

    def test_the_manifest_parses_as_contracts_does_and_refuses_by_key(self):
        manifest = runner.Manifest.parse(self.MANIFEST)
        self.assertEqual((manifest.name, manifest.telemetry_port, manifest.config_reads), ("probe", 0, ["raise"]))
        self.assertEqual(manifest.unread_config_keys(["RAISE", "multicast"]), 1)
        for broken, key in ((self.MANIFEST.replace('kind = "protocol"', 'kind = "daemon"'), "kind"),
                            (self.MANIFEST.replace('status = "discovery-only"', 'status = "maybe"'), "status"),
                            (self.MANIFEST.replace('title = "Probe"\n', ''), "title"),
                            (self.MANIFEST.replace("port = 0", "port = 70000"), "port")):
            with self.assertRaisesRegex(ValueError, key):
                runner.Manifest.parse(broken)

    @unittest.skipIf(CONTRACTS is None, "no contracts/rust beside this tree")
    def test_the_vocabularies_and_document_keys_are_contracts(self):
        source = read(os.path.join(CONTRACTS, "baremetal_telemetry.rs"))
        self.assertEqual(rust_const(source, "AGENT_KINDS"), runner.AGENT_KINDS)
        self.assertEqual(rust_const(source, "IMPLEMENTATION_STATUSES"), runner.IMPLEMENTATION_STATUSES)
        body = source.split("pub fn telemetry_document(", 1)[1].split("json!({", 1)[1].split("})", 1)[0]
        rust_keys = re.findall(r'^\s{8}"([A-Za-z_]+)":', body, re.M)
        rust_counters = re.findall(r'^\s{12}"([a-z_]+)":', body, re.M)
        runner._LOOPBACK.update(manifest=runner.Manifest.parse(self.MANIFEST), started=0.0)
        try:
            document = runner.telemetry_document()
        finally:
            runner._LOOPBACK.update(manifest=None, started=None)
        self.assertEqual(list(document), rust_keys)
        self.assertEqual(list(document["counters"]), rust_counters)
        tick = read(RUNNER_LIB).split("pub fn tick_sample(", 1)[1].split("\n}\n", 1)[0]
        self.assertEqual(list(runner.tick_sample(1)), re.findall(r'^\s{8}"([a-z_]+)":', tick, re.M))

    def test_running_until_a_packet_then_serving_over_the_loopback(self):
        if runner.telemetry_rule() is None:
            self.skipTest("apkaudio_contracts not importable here")
        saved = dict(runner._LOOPBACK)
        try:
            runner._LOOPBACK.update(manifest=None, fatal=None, port=None)
            self.assertIsNone(runner.start_loopback("probe", {}), "no manifest serves nothing")
            runner.declare(runner.Manifest.parse(self.MANIFEST), "9.9.9")
            self.assertIsNone(runner.start_loopback("probe", {"probe": {"raise": "x", "multicast": "y"}}),
                              "port 0 binds nothing")
            self.assertIsNone(runner.loopback_api_section())
            server = runner.make_loopback_server(0)
            port = server.server_address[1]
            threading_ = __import__("threading")
            threading_.Thread(target=server.serve_forever, daemon=True).start()

            code, doc = self.get(port, "/telemetry")
            self.assertEqual((code, doc["state"], doc["agent"], doc["version"]), (200, "running", "probe", "9.9.9"))
            self.assertEqual(doc["counters"]["unread_config_keys"], 1, "`multicast` is read by nothing declared")

            before = runner.counters()
            self.assertFalse(runner.published(False))
            self.assertTrue(runner.published(True))
            code, doc = self.get(port, "/telemetry")
            self.assertEqual(doc["state"], "running", "publishing is not traffic")
            self.assertGreater(doc["counters"]["publish_failures"], before["publish_failures"])

            runner.packet_received()
            code, status = self.get(port, "/status?x=1")
            self.assertEqual((code, status["state"]), (200, "serving"))
            self.assertEqual(runner.telemetry_rule().observed_state(self.get(port, "/telemetry")[1]), "serving")
            self.assertEqual(runner.telemetry_rule().observed_state(runner.tick_sample(0)), "serving",
                             "the tick carries the counters the rule reads")

            self.assertEqual(self.get(port, "/nope")[0], 404)
            self.assertEqual(self.get(port, "/status", "POST")[0], 405)

            runner.fatal("socket closed")
            runner.fatal("a second reason is not kept")
            code, doc = self.get(port, "/telemetry")
            self.assertEqual((doc["state"], doc["reason"]), ("failed", "socket closed"))
            server.shutdown()
            server.server_close()
        finally:
            runner._LOOPBACK.clear()
            runner._LOOPBACK.update(saved)

    def test_enabled_false_reads_stub_disabled_in_config(self):
        """lib.rs parity (ROAD-3478): the runner honours `enabled = false`."""
        for no in ("false", "FALSE", " off ", "no", "0"):
            self.assertFalse(runner.module_enabled({"probe": {"enabled": no}}, "probe"), no)
        for yes in ("true", "1", "flase", ""):
            self.assertTrue(runner.module_enabled({"probe": {"enabled": yes}}, "probe"), yes)
        self.assertTrue(runner.module_enabled({"probe": {}}, "probe"), "absent is on")
        self.assertFalse(runner.module_enabled({"PROBE": {"enabled": "false"}}, "probe"))
        saved = dict(runner._LOOPBACK)
        try:
            runner._LOOPBACK.update(manifest=runner.Manifest.parse(self.MANIFEST), started=0.0,
                                    disabled=runner.DISABLED_REASON)
            if runner.telemetry_rule() is not None:
                document = runner.telemetry_document()
                self.assertEqual((document["state"], document["reason"]), ("stub", "disabled in config"))
                self.assertEqual(runner.telemetry_rule().observed_state(document), "stub")
            tick = runner.tick_sample(1)
            self.assertEqual((tick["state"], tick["reason"]), ("stub", "disabled in config"))
            runner._LOOPBACK["disabled"] = None
            self.assertNotIn("state", runner.tick_sample(1), "switched on, the tick is lib.rs's shape")
        finally:
            runner._LOOPBACK.clear()
            runner._LOOPBACK.update(saved)

    def test_the_environment_moves_or_disables_the_port(self):
        self.assertEqual(runner.chosen_port(8190, None), 8190)
        self.assertEqual(runner.chosen_port(8190, " 8191 "), 8191)
        self.assertEqual(runner.chosen_port(8190, "0"), 0)
        self.assertEqual(runner.chosen_port(8190, "nope"), 8190)

    def test_a_refused_publish_is_counted(self):
        class Info:
            def __init__(self, rc):
                self.rc = rc

        class Client(FakeClient):
            def publish(self, topic, payload, qos=0, retain=False):
                return Info(4)                      # MQTT_ERR_NO_CONN

        session = runner.Session.__new__(runner.Session)
        session.token, session.client = "probe", Client()
        before = runner.counters()["publish_failures"]
        session.publish("t", "p", False)
        self.assertEqual(runner.counters()["publish_failures"], before + 1)


class TheRunnerReportsThroughTheContract(unittest.TestCase):
    """ROAD-3695 step 4: elog lines are LogLines, faults are retained ProblemRecords."""

    def setUp(self):
        if runner._report_contract() is None:
            self.skipTest("apkaudio_contracts is not importable here")
        self.saved = dict(runner._REPORT)
        runner._REPORT.update({"session": None, "name": "zz", "early": [], "raised": {}})
        self.client = FakeClient()
        session = runner.Session.__new__(runner.Session)
        session.token, session.client = "zz", self.client
        self.session = session

    def tearDown(self):
        runner._REPORT.clear()
        runner._REPORT.update(self.saved)

    def sent(self, family):
        return [(t, p, r) for t, p, r in self.client.sent if "/System/%s/" % family in t]

    def test_a_line_said_before_the_session_is_held_then_said(self):
        runner.elog("🛑 [zz] the config did not read")
        self.assertEqual(self.client.sent, [])
        runner.report_install(self.session)
        (topic, payload, retain), = self.sent("Log")
        self.assertEqual(topic, "APK.audio/System/Log/zz/error")
        self.assertFalse(retain)
        self.assertEqual(json.loads(payload)["message"], "🛑 [zz] the config did not read")
        self.assertEqual(runner.level_of("⚠️  [zz] late"), "warn")
        self.assertEqual(runner.level_of("📡 sent"), "info")

    def test_a_problem_is_raised_again_counted_and_cleared_once(self):
        runner.report_install(self.session)
        runner.raise_problem("timeout", "HP 34401A", "no reply")
        runner.raise_problem("timeout", "HP 34401A", "no reply")
        records = [json.loads(p) for _, p, _ in self.sent("Problems")]
        self.assertEqual([r["count"] for r in records], [1, 2])
        self.assertEqual(records[0]["raised_at"], records[1]["raised_at"])
        self.assertTrue(all(r for _, _, r in self.sent("Problems")))
        self.assertTrue(runner.clear_problem("timeout", "HP 34401A"))
        self.assertEqual(self.sent("Problems")[-1][1], "")
        self.assertFalse(runner.clear_problem("timeout", "HP 34401A"), "a second clear says nothing")

    def test_unavailable_raises_listening_clears_and_a_failed_check_raises(self):
        runner.report_install(self.session)
        runner.agent_state_publications("zz", "unavailable", "no interface came up")
        record = json.loads(self.sent("Problems")[-1][1])
        self.assertEqual((record["code"], record["subject"], record["detail"]),
                         ("plugin_unavailable", "zz", "no interface came up"))
        runner.agent_state_publications("zz", "listening", "on eth0")
        self.assertEqual(self.sent("Problems")[-1][1], "")
        report = runner.Report("zz", "self").with_(runner.Check.fail("own_config", "self", "missing key"))
        runner.report_checks(report)
        record = json.loads(self.sent("Problems")[-1][1])
        self.assertEqual((record["code"], record["subject"]), ("plugin_check_failed", "zz/own_config"))
