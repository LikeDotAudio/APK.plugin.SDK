# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""DeviceRecord.extra.nmos against the vectors the TS and Rust twins share."""
import json
import unittest
from pathlib import Path

from apkaudio_contracts.device_record import nmos_extra

ROOT = Path(__file__).resolve().parents[2] / "vectors" / "payloads" / "DeviceRecord"


class TestNmosExtra(unittest.TestCase):
    def test_vectors(self):
        valid = sorted((ROOT / "valid").glob("nmos-*.json"))
        invalid = sorted((ROOT / "invalid").glob("nmos-*.json"))
        self.assertTrue(valid and invalid)
        for path in valid:
            doc = json.loads(path.read_text())
            self.assertEqual(nmos_extra(doc), doc["extra"]["nmos"], path.name)
        for path in invalid:
            with self.assertRaises(ValueError, msg=path.name):
                nmos_extra(json.loads(path.read_text()))
        ravenna = json.loads((ROOT / "valid" / "ravenna-node-streams.json").read_text())
        self.assertIsNone(nmos_extra(ravenna))


if __name__ == "__main__":
    unittest.main()
