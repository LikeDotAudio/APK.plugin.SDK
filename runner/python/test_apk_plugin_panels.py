# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The Python panel book says what panels.rs says, or this fails.

    python3 -m unittest discover -s PODS/POD.THICK/DISCOVERY.Engine/SRC/RUNNER/python -p 'test_*.py'

RUNNER/vectors/panels.json is run here AND by panels.rs's
`the_shared_vectors_hold`, so one implementation cannot change a rule alone.
"""

import json
import os
import re
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import apk_plugin_panels as panels  # noqa: E402
import apk_plugin_runner as runner  # noqa: E402

VECTORS = os.path.join(HERE, "..", "vectors", "panels.json")
PANELS_RS = os.path.join(HERE, "..", "src", "panels.rs")
PRESENCE_RS = os.path.join(HERE, "..", "src", "presence", "mod.rs")


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def parsed(out):
    return [[t, json.loads(p) if p else ""] for t, p in out]


class TheVectorsHold(unittest.TestCase):
    def test_every_case(self):
        doc = json.loads(read(VECTORS))
        for case in doc["cases"]:
            manifest = json.dumps(case["manifest"])
            templates = [panels.parse_template(f, manifest, json.dumps(body)) for f, body in case["families"].items()]
            book = panels.Book(case["token"], templates, case["expire_after_s"])
            for i, step in enumerate(case["steps"]):
                if "observe" in step:
                    out = book.observe(step["observe"][0], step["observe"][1], step["at"])
                elif "expire" in step:
                    out = book.expire(step["at"])
                else:
                    out = book.restate()
                self.assertEqual(parsed(out), step["expect"], "%s, step %d" % (case["name"], i))
        template = json.dumps(doc["template"])
        for bad in doc["rejected_manifests"]:
            with self.assertRaises(ValueError, msg=bad):
                panels.parse_template(bad["family"], json.dumps(bad["manifest"]), template)


class TheConstantsAreTheRustOnes(unittest.TestCase):
    @unittest.skipIf(not os.path.isfile(PANELS_RS), "no RUNNER/src beside this tree (the image)")
    def test_root_and_grace(self):
        self.assertIn("PANELS_TOPIC_ROOT: &str = apkaudio_contracts::topics::GUI_PANELS_ROOT", read(PANELS_RS))
        topics_rs = next((p for p in (os.path.join(HERE, *[".."] * 6, "contracts", "rust", "src", "topics.rs"),
                                      os.path.join(HERE, "..", "..", "contracts", "rust", "src", "topics.rs"))
                          if os.path.isfile(p)), None)
        if topics_rs:
            self.assertIn('GUI_PANELS_ROOT: &str = "%s"' % panels.PANELS_TOPIC_ROOT, read(topics_rs))
        grace = re.search(r"ADOPT_GRACE: Duration = Duration::from_secs\((\d+)\)", read(PRESENCE_RS))
        self.assertEqual(int(grace.group(1)), panels.ADOPT_GRACE_S)

    def test_helpers(self):
        self.assertEqual(panels.placement_of("7_Device_Consoles"), "Device_Consoles")
        self.assertEqual(panels.placement_of("Instruments"), "Instruments")
        self.assertEqual(panels.sanitize_segment(" a.b/c. "), "a_b_c")
        self.assertEqual(panels.sanitize_segment("..."), "_")
        self.assertEqual(panels.expire_after_from_config({"Cast": {"panels_expire_after_s": "120"}}, "cast"), 120)
        self.assertIsNone(panels.expire_after_from_config({"cast": {"panels_expire_after_s": "0"}}, "cast"))


class FakeClient:
    def __init__(self):
        self.sent = []

    def publish(self, topic, payload, qos=0, retain=False):
        self.sent.append((topic, payload, retain))

    def subscribe(self, topic, qos=0):
        self.sent.append(("SUBSCRIBE", topic, False))


class FakeMessage:
    def __init__(self, topic, payload, retain=False):
        self.topic, self.payload, self.retain = topic, payload.encode(), retain


class TheRunnerSessionPresentsPanels(unittest.TestCase):
    """`runner.main(..., panels=[family])` wires the book into the session."""

    def setUp(self):
        self._saved = list(panels._TEMPLATES)
        panels._TEMPLATES.clear()

    def tearDown(self):
        panels._TEMPLATES[:] = self._saved

    def test_register_from_a_gui_dir_and_present_on_the_session(self):
        with tempfile.TemporaryDirectory() as gui:
            os.makedirs(os.path.join(gui, "bt"))
            with open(os.path.join(gui, "manifest.json"), "w") as f:
                json.dump({"bt": {"tree": "7_Device_Consoles", "device_depth": 1}}, f)
            with open(os.path.join(gui, "bt", "bt.json"), "w") as f:
                json.dump({"Console": {"d": "${model}", "f": {"s": {"device_state": "status"}}}}, f)
            panels.register_from_dir(gui, "bt")
            panels.register_from_dir(gui, "missing")       # reported, skipped
        self.assertEqual([t.family for t in panels.templates()], ["bt"])

        api = runner.apk_plugin_api.PluginApi("bt")
        session = runner.Session("bt", runner.ini_to_json("[bt]\n"), "config.ini", lambda d: "{}", api, ["run"])
        client = FakeClient()
        session.client = client
        session.on_connect(client, None, {}, 0)
        subs = [t for k, t, _ in client.sent if k == "SUBSCRIBE"]
        self.assertIn("APK.audio/System/Protocols/bt/Device/#", subs)
        self.assertIn("APK.audio/System/Gui/Panels/+", subs)

        client.sent.clear()
        session.on_message(client, None, FakeMessage("APK.audio/System/Protocols/bt/Device/hci0/model", "HCI"))
        (topic, payload, retain), = client.sent
        self.assertEqual(topic, "APK.audio/System/Gui/Panels/bt-hci0")
        self.assertTrue(retain)
        doc = json.loads(payload)
        content = doc["placements"][0]["frames"][0]["content"]["Console"]
        self.assertEqual(content["d"], "HCI")
        self.assertEqual(content["f"]["s"]["topic"], "APK.audio/System/Protocols/bt/Device/hci0/status")
        self.assertEqual(doc["source"], "plugin:bt")

        client.sent.clear()
        session.on_message(client, None, FakeMessage("APK.audio/System/Protocols/bt/Device/hci0/last_online", "1761830000"))
        self.assertIn(("APK.audio/System/Gui/Panels/bt-hci0", "", True), client.sent)

    def test_a_plugin_without_panels_subscribes_nothing_more(self):
        api = runner.apk_plugin_api.PluginApi("zz")
        session = runner.Session("zz", runner.ini_to_json("[zz]\n"), "config.ini", lambda d: "{}", api, ["run"])
        client = FakeClient()
        session.client = client
        session.on_connect(client, None, {}, 0)
        subs = [t for k, t, _ in client.sent if k == "SUBSCRIBE"]
        self.assertNotIn("APK.audio/System/Gui/Panels/+", subs)


if __name__ == "__main__":
    unittest.main()
