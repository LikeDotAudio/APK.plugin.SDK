# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""APK_SERIAL_SETTINGS — the same vectors/serial_settings.json the Rust tests read."""

import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))

from apkaudio_contracts import serial_settings  # noqa: E402

with open(os.path.join(HERE, "..", "..", "vectors", "serial_settings.json"), encoding="utf-8") as handle:
    VECTORS = json.load(handle)


def label(line):
    return None if line is None else line.label()


class SerialSettingsVectors(unittest.TestCase):
    def test_parse(self):
        for v in VECTORS["parse"]:
            self.assertEqual(label(serial_settings.parse_settings(v["text"])), v["label"], v)

    def test_pins(self):
        for v in VECTORS["pins"]:
            self.assertEqual(label(serial_settings.pinned(v["spec"], v["tty"], v["serial"])), v["label"], v)

    def test_rejected(self):
        for v in VECTORS["rejected"]:
            self.assertEqual(serial_settings.SerialPins.parse(v["spec"]).rejected, v["rejected"], v)

    def test_from_env(self):
        pins = serial_settings.SerialPins.from_env({"APK_SERIAL_SETTINGS": "*=9600 7E2"})
        self.assertEqual(pins.for_port("ttyUSB3").label(), "9600 7E2 none")
        self.assertTrue(serial_settings.SerialPins.from_env({}).pins == [])


if __name__ == "__main__":
    unittest.main()
