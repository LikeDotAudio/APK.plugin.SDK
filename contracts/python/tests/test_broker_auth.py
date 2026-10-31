# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
import re
import unittest
from pathlib import Path

from apkaudio_contracts import broker_auth

RUST = Path(__file__).resolve().parents[2] / "rust/src/broker_auth.rs"


class Client:
    def __init__(self):
        self.set = None

    def username_pw_set(self, u, p):
        self.set = (u, p)


class TestBrokerAuth(unittest.TestCase):
    def test_names_are_the_rust_constants(self):
        src = RUST.read_text()
        self.assertIn(f'USERNAME_VARIABLE: &str = "{broker_auth.USERNAME_VARIABLE}"', src)
        self.assertIn(f'PASSWORD_VARIABLE: &str = "{broker_auth.PASSWORD_VARIABLE}"', src)

    def test_both_set(self):
        env = {"APK_MQTT_USERNAME": "bench", "APK_MQTT_PASSWORD": "s3"}
        self.assertEqual(broker_auth.credentials(env), ("bench", "s3"))
        c = Client()
        self.assertEqual(broker_auth.apply(c, env), "as bench")
        self.assertEqual(c.set, ("bench", "s3"))
        self.assertEqual(broker_auth.paho_auth(env), {"username": "bench", "password": "s3"})

    def test_anonymous_and_half_set(self):
        for env in ({}, {"APK_MQTT_USERNAME": "  ", "APK_MQTT_PASSWORD": "x"},
                    {"APK_MQTT_USERNAME": "bench", "APK_MQTT_PASSWORD": ""},
                    {"APK_MQTT_USERNAME": "bench"}):
            c = Client()
            self.assertIsNone(broker_auth.credentials(env))
            self.assertIsNone(broker_auth.paho_auth(env))
            broker_auth.apply(c, env)
            self.assertIsNone(c.set)
        self.assertEqual(broker_auth.describe({}), "anonymously")
        self.assertTrue(broker_auth.describe({"APK_MQTT_USERNAME": "b"}).startswith("ANONYMOUSLY"))
        self.assertNotIn("s3", broker_auth.describe({"APK_MQTT_USERNAME": "b", "APK_MQTT_PASSWORD": "s3"}))


if __name__ == "__main__":
    unittest.main()
