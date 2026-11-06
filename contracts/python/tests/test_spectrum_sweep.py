# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""SpectrumSweep against the vectors the TS and Rust twins share."""
import json
import unittest
from pathlib import Path

from apkaudio_contracts.spectrum_sweep import faults, frequency_hz, is_sweep

ROOT = Path(__file__).resolve().parents[2] / "vectors" / "payloads" / "SpectrumSweep"


class TestSpectrumSweep(unittest.TestCase):
    def test_vectors(self):
        valid = sorted((ROOT / "valid").glob("*.json"))
        invalid = sorted((ROOT / "invalid").glob("*.json"))
        self.assertTrue(valid and invalid)
        for path in valid:
            self.assertEqual(faults(json.loads(path.read_text())), [], path.name)
        for path in invalid:
            self.assertFalse(is_sweep(json.loads(path.read_text())), path.name)

    def test_axis_runs_start_to_stop_inclusive(self):
        doc = json.loads((ROOT / "valid" / "n9340b-full-span.json").read_text())
        self.assertEqual(frequency_hz(doc, 0), doc["startHz"])
        self.assertEqual(frequency_hz(doc, doc["points"] - 1), doc["stopHz"])


if __name__ == "__main__":
    unittest.main()
