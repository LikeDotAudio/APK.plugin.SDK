# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""families.topic() against the golden vectors the TS and Rust builders answer."""
import json
import unittest
from pathlib import Path

from apkaudio_contracts import families as T

CONTRACTS = Path(__file__).resolve().parents[2]
VECTORS = json.loads((CONTRACTS / "vectors/topics.json").read_text())


def _runnable(case):
    """A vector whose family is a grammar rule and whose argument names are the rule's."""
    try:
        rule = T._rule(case["family"])
    except T.TopicError:
        return False
    names = {s.get("name") for s in rule["segs"]}
    return set(case["args"]) <= names


class TestFamilies(unittest.TestCase):
    # A monorepo drift check: APK.plugin.SDK ships contracts/python without
    # contracts/generated (publish_sdk.py), where the copy IS the grammar.
    @unittest.skipUnless((CONTRACTS / "generated/topic-grammar.json").exists(), "no contracts/generated (published SDK)")
    def test_grammar_copy_is_the_generated_one(self):
        self.assertEqual(
            (Path(T.__file__).with_name("topic_grammar.json")).read_text(),
            (CONTRACTS / "generated/topic-grammar.json").read_text(),
        )

    def test_build_vectors(self):
        n = 0
        for case in VECTORS["build"]:
            if not _runnable(case):
                continue
            with self.subTest(case=case):
                self.assertEqual(T.topic(case["family"], **case["args"]), case["topic"])
                n += 1
        self.assertGreaterEqual(n, 40)

    def test_build_invalid_vectors(self):
        n = 0
        for case in VECTORS["buildInvalid"]:
            if not _runnable(case):
                continue
            with self.subTest(case=case):
                with self.assertRaises(T.TopicError):
                    T.topic(case["family"], **case["args"])
                n += 1
        self.assertGreaterEqual(n, 25)

    def test_wildcards_and_retain(self):
        self.assertEqual(T.wildcard("discovery"), "APK.audio/Discovery/#")
        self.assertEqual(T.wildcard("discovery", protocol="visa"), "APK.audio/Discovery/visa/#")
        self.assertEqual(T.wildcard("agents"), "APK.audio/System/Agents/#")
        self.assertTrue(T.retain("agents"))
        self.assertFalse(T.retain("log"))
        with self.assertRaises(T.TopicError):
            T.topic("noSuchFamily")
        with self.assertRaises(T.TopicError):
            T.topic("agents", agent="komand", extra="x")


if __name__ == "__main__":
    unittest.main()
