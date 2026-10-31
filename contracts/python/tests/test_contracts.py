# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
import unittest
from apkaudio_contracts import (
    ROOT,
    DIR_INCOMING,
    DIR_OUTGOING,
    module_incoming,
    module_outgoing,
    device_incoming,
    retains,
    AgentState,
    create_heartbeat,
    parse_presence,
    presence_is_active,
)
from apkaudio_contracts.identity import device_id_for, fnv1a64


class TestContracts(unittest.TestCase):
    def test_topic_shapes(self):
        topic, retain = module_incoming("ptp", "state")
        self.assertEqual(topic, "APK.audio/System/Protocols/ptp/incoming/state")
        self.assertTrue(retain)

        topic, retain = module_outgoing("ptp", "command")
        self.assertEqual(topic, "APK.audio/System/Protocols/ptp/outgoing/command")
        self.assertFalse(retain)

    def test_device_topic_shapes(self):
        topic, retain = device_incoming("PI", ["bench1"], "telemetry")
        self.assertEqual(topic, "APK.audio/System/BareMetal/PI/bench1/incoming/telemetry")
        self.assertFalse(retain)

    def test_retain_rules(self):
        self.assertTrue(retains(DIR_INCOMING, "state"))
        self.assertTrue(retains(DIR_INCOMING, "interface"))
        self.assertFalse(retains(DIR_INCOMING, "telemetry"))
        self.assertFalse(retains(DIR_OUTGOING, "command"))
        self.assertTrue(retains(DIR_OUTGOING, "enable"))

    def test_job_and_result_leaves(self):
        """The worker pair: unretained, and each legal in one direction only."""
        topic, retain = module_outgoing("wireless_csv", "job")
        self.assertEqual(topic, "APK.audio/System/Protocols/wireless_csv/outgoing/job")
        self.assertFalse(retain)
        topic, retain = module_incoming("wireless_csv", "result")
        self.assertEqual(topic, "APK.audio/System/Protocols/wireless_csv/incoming/result")
        self.assertFalse(retain)
        with self.assertRaises(ValueError):
            retains(DIR_INCOMING, "job")
        with self.assertRaises(ValueError):
            retains(DIR_OUTGOING, "result")

    def test_heartbeat_payload(self):
        hb = create_heartbeat("agent-123", AgentState.RUNNING, "OK")
        self.assertEqual(hb["agent_id"], "agent-123")
        self.assertEqual(hb["state"], "RUNNING")
        self.assertIn("timestamp_ms", hb)

    def test_discovery_presence_vectors(self):
        import json
        from pathlib import Path
        root = Path(__file__).resolve().parents[2] / "vectors" / "payloads" / "DiscoveryPresence"
        for path in sorted((root / "valid").glob("*.json")):
            doc = json.loads(path.read_text())
            self.assertIsNotNone(parse_presence(doc), path.name)
            self.assertIn(presence_is_active(doc), (True, False), path.name)
        for path in sorted((root / "invalid").glob("*.json")):
            self.assertIsNone(parse_presence(path.read_text()), path.name)
        self.assertIsNone(presence_is_active(b""))
        self.assertTrue(presence_is_active((root / "valid" / "back-online.json").read_text()))
        self.assertFalse(presence_is_active((root / "valid" / "offline-after-expiry.json").read_text()))


    def test_identity_vectors(self):
        """The D2 rule, against the vectors the TS and Rust twins are pinned by.

        Two agents deriving different IDs for one instrument is the
        duplicate-34401A bug; this file is why Python can no longer be the
        agent that gets it wrong.
        """
        import json
        from pathlib import Path

        vectors = json.loads(
            (Path(__file__).resolve().parents[2] / "vectors" / "identity.json").read_text()
        )
        for case in vectors["deviceId"]:
            given = case["input"]
            self.assertEqual(
                device_id_for(
                    given.get("protocol"),
                    given.get("serial"),
                    given.get("address"),
                    given.get("make"),
                    given.get("model"),
                ),
                case["deviceId"],
                case["why"],
            )
        for case in vectors["fnv1a64"]:
            self.assertEqual(fnv1a64(case["input"]), case["hex"])

    def test_serial_beats_every_other_spelling_of_an_instrument(self):
        """The 33220A that started this: one box, one id, whoever derives it."""
        serial_only = device_id_for("visa", "MY44044028")
        with_everything = device_id_for(
            "visa",
            "MY44044028",
            "TCPIP0::44.44.44.33::5025::SOCKET",
            "Agilent Technologies",
            "33220A",
        )
        self.assertEqual(serial_only, "visa:MY44044028")
        self.assertEqual(serial_only, with_everything)
        # The container-name spelling is a NAME, never the identity.
        self.assertNotEqual(serial_only, "visa:33220a-my44044028")

if __name__ == "__main__":
    unittest.main()


class TestSerialComLeaves(unittest.TestCase):
    def test_leaves(self):
        from apkaudio_contracts.baremetal_io import module_port
        self.assertEqual(module_incoming("serial_com", "ports"), ("APK.audio/System/Protocols/serial_com/incoming/ports", True))
        self.assertEqual(module_port("serial_com", "dev:_dev_ttyS0"), ("APK.audio/System/Protocols/serial_com/incoming/port/dev:_dev_ttyS0", True))
        self.assertFalse(module_incoming("serial_com", "exchange")[1])
        self.assertEqual(module_outgoing("serial_com", "open"), ("APK.audio/System/Protocols/serial_com/outgoing/open", False))
        with self.assertRaises(ValueError):
            module_port("serial_com", "a/b")
