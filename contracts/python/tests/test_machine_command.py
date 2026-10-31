# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The machine command vectors (PLAN-3394.01), shared with the Rust mirror."""
import json
import unittest
from pathlib import Path

from apkaudio_contracts import machine_command as mc

VECTORS = json.loads((Path(__file__).resolve().parents[2] / "vectors" / "machine_command.json").read_text(encoding="utf-8"))


class TestMachineCommand(unittest.TestCase):
    def test_valid_parse_and_round_trip(self):
        for case in VECTORS["valid"]:
            c = mc.parse(json.dumps(case["doc"]))
            self.assertEqual(mc.parse(mc.to_json(c)), c, case["why"])

    def test_invalid_refused_against_their_id(self):
        for case in VECTORS["invalid"]:
            with self.assertRaises(mc.Refused, msg=case["why"]) as ctx:
                mc.parse(case["body"] if "body" in case else json.dumps(case["doc"]))
            self.assertEqual(ctx.exception.id, case["refused_id"], case["why"])

    def test_acl_list_is_the_vectors_list(self):
        self.assertEqual(mc.acl_deny_patterns(), VECTORS["acl_deny"])

    def test_topics(self):
        self.assertEqual(mc.command_topic("LinuxDesktop", "bench1"),
                         ("APK.audio/System/BareMetal/MonitorProbes/LinuxDesktop/bench1/outgoing/command", False))
        self.assertEqual(mc.state_topic("LinuxDesktop", "bench1"),
                         ("APK.audio/System/BareMetal/MonitorProbes/LinuxDesktop/bench1/incoming/state", True))
        self.assertTrue(mc.detail_topic("LinuxDesktop", "bench1")[1])
        with self.assertRaises(ValueError):
            mc.command_topic("LinuxDesktop", "a/b")
        with self.assertRaises(ValueError):
            mc.command_topic("+", "bench1")

    def test_answer(self):
        c = mc.parse(json.dumps(VECTORS["valid"][0]["doc"]))
        a = json.loads(mc.answer(c, None, mc.STATUS_DONE, "ok", "bench1", True, 5))
        self.assertEqual((a["id"], a["session"], a["status"], a["dry_run"]),
                         ("cmd-0001-abcd", "mlog-20261030-0001", "done", True))


if __name__ == "__main__":
    unittest.main()
