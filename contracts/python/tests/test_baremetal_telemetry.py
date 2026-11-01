# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Loopback agent telemetry — the same vectors/baremetal_telemetry.json the Rust and TS tests read."""

import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))

from apkaudio_contracts import baremetal_telemetry  # noqa: E402

with open(os.path.join(HERE, "..", "..", "vectors", "baremetal_telemetry.json"), encoding="utf-8") as handle:
    VECTORS = json.load(handle)


class BareMetalTelemetryVectors(unittest.TestCase):
    def test_states(self):
        self.assertEqual(list(baremetal_telemetry.SERVING_STATES), VECTORS["states"])

    def test_observed(self):
        for v in VECTORS["observed"]:
            self.assertEqual(baremetal_telemetry.observed_state(v["document"]), v["state"], v["why"])

    def test_derived(self):
        for v in VECTORS["derived"]:
            self.assertEqual(
                baremetal_telemetry.derive_state(v["stub"], v["fatal"], v["packets_received"]), v["state"], v)


if __name__ == "__main__":
    unittest.main()
