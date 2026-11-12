# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The Python report twin mints what the schemas admit, on the grammar's topics,
with the same keys as TypeScript and Rust."""
import json
import unittest
from pathlib import Path

from apkaudio_contracts.identity import fnv1a64
from apkaudio_contracts.report import clear_problem, log_line, problem_key, problem_record, report_source

SCHEMAS = Path(__file__).resolve().parents[2] / "schemas"
NOW = 1_793_000_000_000


def required(name):
    return set(json.loads((SCHEMAS / f"{name}.schema.json").read_text())["required"])


def allowed(name):
    return set(json.loads((SCHEMAS / f"{name}.schema.json").read_text())["properties"])


class ReportTest(unittest.TestCase):
    def test_log_line_shape_and_topic(self):
        m = log_line("osapi", "warn", "BIM did not load", code="fetch_failed", subject="/api/bim", app="bim", now_ms=NOW)
        doc = json.loads(m["payload"])
        self.assertTrue(required("LogLine") <= set(doc) <= allowed("LogLine"))
        self.assertEqual(m["topic"], "APK.audio/System/Log/osapi/warn")
        self.assertFalse(m["retain"])
        self.assertTrue(doc["ts"].endswith("Z"))
        with self.assertRaises(ValueError):
            log_line("x", "loud", "m")

    def test_problem_record_shape_topic_and_clear(self):
        m = problem_record("os shell", "timeout", subject="HP 34401A", fix=["Power it."], ttl_s=60, now_ms=NOW)
        doc = json.loads(m["payload"])
        self.assertTrue(required("ProblemRecord") <= set(doc) <= allowed("ProblemRecord"))
        self.assertTrue(m["retain"])
        self.assertEqual(m["topic"], f"APK.audio/System/Problems/os-shell/timeout-{fnv1a64('HP 34401A')[:8]}")
        self.assertEqual(clear_problem("os shell", "timeout", "HP 34401A"), {"topic": m["topic"], "payload": "", "retain": True})
        again = problem_record("os-shell", "timeout", subject="HP 34401A", previous=doc, now_ms=NOW + 5000)["record"]
        self.assertEqual((again["count"], again["raised_at"]), (2, doc["raised_at"]))

    def test_keys_match_the_twins(self):
        self.assertEqual(problem_key("timeout"), "timeout")
        self.assertEqual(report_source("os shell/jobs"), "os-shell-jobs")
        self.assertEqual(report_source("///"), "unknown")


if __name__ == "__main__":
    unittest.main()
