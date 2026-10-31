# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""sanitize_segment — the same vectors/sanitize_segment.json the Rust tests read."""
import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))

from apkaudio_contracts.segments import is_segment, sanitize_segment  # noqa: E402

with open(os.path.join(HERE, "..", "..", "vectors", "sanitize_segment.json"), encoding="utf-8") as handle:
    CASES = json.load(handle)["cases"]


class Segments(unittest.TestCase):
    def test_vectors(self):
        for case in CASES:
            got = sanitize_segment(case["raw"])
            self.assertEqual(got, case["segment"], case)
            self.assertTrue(is_segment(got), case)

    def test_is_segment(self):
        self.assertTrue(is_segment("ttyUSB0"))
        for bad in ("", "a b", "a/b", "+", "#", "a.b", "é"):
            self.assertFalse(is_segment(bad), bad)


if __name__ == "__main__":
    unittest.main()
