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


if __name__ == "__main__":
    unittest.main()
