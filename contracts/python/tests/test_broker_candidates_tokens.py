# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""broker_candidates (moved from NODE.BareMetal), tokens as package data, broker_auth prefix."""
import json
import os
import sys
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from apkaudio_contracts import broker_auth, broker_candidates as B, tokens  # noqa: E402

CONTRACTS = HERE.parents[1]


class BrokerCandidates(unittest.TestCase):
    def test_local_first_and_no_public_without_both_opt_ins(self):
        with mock.patch.dict(os.environ, {B.ALLOW_PUBLIC_ENVIRONMENT: ""}):
            self.assertEqual(B.candidates(), [B.LOOPBACK_BROKER])
            self.assertEqual(B.candidates(allow_public=True), [B.LOOPBACK_BROKER])
        with mock.patch.dict(os.environ, {B.ALLOW_PUBLIC_ENVIRONMENT: "1"}):
            self.assertEqual(B.candidates(), [B.LOOPBACK_BROKER], "the caller's opt-in is required too")
            self.assertEqual(B.candidates(allow_public=True), [B.LOOPBACK_BROKER] + B.PUBLIC_MQTT_BROKERS)
            self.assertEqual(B.candidates(allow_public=True, public_first=True), B.PUBLIC_MQTT_BROKERS + [B.LOOPBACK_BROKER])

    def test_configured_first_and_container_loopback(self):
        self.assertEqual(B.candidates(("broker", "1883"))[0], ("broker", 1883, "Configured Broker"))
        self.assertEqual(B.candidates(("broker", 1883), include_loopback=False), [("broker", 1883, "Configured Broker")])
        self.assertEqual(B.candidates(("127.0.0.1", 1883), include_loopback=False), [B.LOOPBACK_BROKER])

    def test_posture(self):
        with mock.patch.dict(os.environ, {B.ALLOW_PUBLIC_ENVIRONMENT: ""}):
            self.assertIn("local only", B.describe_posture())


class Tokens(unittest.TestCase):
    def test_package_copies_are_the_tokens(self):
        for name in ("brand.json", "state.json"):
            self.assertEqual((Path(tokens.__file__).with_name(name)).read_text(), (CONTRACTS / "tokens" / name).read_text())

    def test_values(self):
        brand = json.loads((CONTRACTS / "tokens/brand.json").read_text())
        self.assertEqual(tokens.BRAND_ACCENT_HEX, brand["accent"]["hex"])
        self.assertEqual(tokens.state_tone("ONLINE "), tokens.state()["words"].get("online", tokens.state()["default"]))
        self.assertEqual(tokens.state_tone("no-such-word-xyz"), tokens.state()["default"])


class BrokerAuthPrefix(unittest.TestCase):
    def test_prefix(self):
        env = {"APK_MQTT_USERNAME": "u", "APK_MQTT_PASSWORD": "p", "OTHER_USERNAME": "x", "OTHER_PASSWORD": "y"}
        self.assertEqual(broker_auth.credentials(env), ("u", "p"))
        self.assertEqual(broker_auth.credentials(env, prefix="OTHER"), ("x", "y"))
        self.assertIsNone(broker_auth.credentials({"OTHER_USERNAME": "x"}, prefix="OTHER"))


if __name__ == "__main__":
    unittest.main()
