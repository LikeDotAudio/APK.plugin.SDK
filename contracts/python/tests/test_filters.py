# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""families.is_declared_filter() against the `filters` vectors TS and Rust answer."""
import json
import unittest
from pathlib import Path

from apkaudio_contracts import families as T

CONTRACTS = Path(__file__).resolve().parents[2]
VECTORS = json.loads((CONTRACTS / "vectors/topics.json").read_text())


class TestDeclaredFilters(unittest.TestCase):
    def test_filter_vectors(self):
        for case in VECTORS["filters"]:
            with self.subTest(case=case):
                self.assertEqual(T.is_declared_filter(case["filter"]), case["declared"], case["why"])

    def test_every_declared_filter_has_a_vector(self):
        self.assertTrue(T.FILTERS)
        declared = {c["filter"] for c in VECTORS["filters"] if c["declared"]}
        self.assertEqual(set(T.FILTERS), declared)


if __name__ == "__main__":
    unittest.main()
