# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The trunk vectors (ROAD-3440), shared with the Rust and TypeScript twins."""
import json
import unittest
from pathlib import Path

from apkaudio_contracts import trunk as t

VECTORS = json.loads((Path(__file__).resolve().parents[2] / "vectors" / "trunk.json").read_text(encoding="utf-8"))


class TestTrunk(unittest.TestCase):
    def test_lines(self):
        for case in VECTORS["lines"]["valid"]:
            line = t.parse_line(case["text"])
            self.assertEqual(line, case["line"], case["why"])
            self.assertEqual(t.render_line(line), case["render"], case["why"])
            self.assertEqual(t.parse_line(t.render_line(line)), line, case["why"])
            self.assertEqual(t.is_local(line), case["local"], case["why"])
            self.assertEqual(t.is_checkin(line), case["checkin"], case["why"])
            self.assertEqual(t.flipped(t.flipped(line)), line)
        for case in VECTORS["lines"]["invalid"]:
            with self.assertRaises(ValueError, msg=case["why"]):
                t.parse_line(case["text"])
        for case in VECTORS["checkins"]:
            self.assertEqual(t.render_line(t.checkin_line(case["site"], case["frame"], case["when"], case["closed"])), case["text"])
        for case in VECTORS["rejects"]:
            self.assertEqual(t.render_reject(case["reason"], case["line"]), case["payload"])
            reason, line = t.parse_reject(case["payload"])
            self.assertEqual(line, case["line"])
            self.assertNotIn("|", reason)

    def test_batches(self):
        b = VECTORS["batches"]
        self.assertEqual(t.BATCH_MAX, b["max"])
        for case in b["valid"]:
            render, parse = (t.render_line_batch, t.parse_line_batch) if case["leaf"] == "line" else (t.render_ack_batch, t.parse_ack_batch)
            if "payload" in case:
                self.assertEqual(render(case["entries"]), case["payload"], case["why"])
            self.assertEqual(parse(case.get("read", case.get("payload"))), case["entries"], case["why"])
        for case in b["invalid"]:
            parse = t.parse_line_batch if case["leaf"] == "line" else t.parse_ack_batch
            with self.assertRaises(ValueError, msg=case["why"]):
                parse(case["payload"])
        many = [f"HQ,f,U{n:02},A,CLOSE,B,U01,f,HQ" for n in range(t.BATCH_MAX + 1)]
        with self.assertRaises(ValueError):
            t.parse_line_batch(t.render_line_batch(many))

    def test_protocol_batches_and_states(self):
        b = VECTORS["protocol_batches"]
        self.assertEqual(t.BATCH_MAX, b["max"])
        for case in b["valid"]:
            line = case["leaf"] == "line"
            render = t.render_protocol_line_batch if line else t.render_protocol_ack_batch
            parse = t.parse_protocol_line_batch if line else t.parse_protocol_ack_batch
            if "payload" in case:
                self.assertEqual(render(case["frames"]), case["payload"], case["why"])
            self.assertEqual(parse(case.get("read", case.get("payload"))), case["frames"], case["why"])
        for case in b["invalid"]:
            parse = t.parse_protocol_line_batch if case["leaf"] == "line" else t.parse_protocol_ack_batch
            with self.assertRaises(ValueError, msg=case["why"]):
                parse(case["payload"])
        half = [f"HQ,f,U{n:02},A,CLOSE,B,U01,f,HQ" for n in range(t.BATCH_MAX // 2)]
        self.assertEqual(sorted(t.parse_protocol_line_batch(t.render_protocol_line_batch({"f-0": half, "f-1": half}))), ["f-0", "f-1"])
        with self.assertRaises(ValueError):
            t.parse_protocol_line_batch(t.render_protocol_line_batch({"f-0": half, "f-1": half, "f-9": ["x"]}))
        for case in VECTORS["protocol_states"]["valid"]:
            want = (case["protocol"], case["frames"])
            self.assertEqual(t.parse_protocol_state(case["payload"]), want, case["why"])
            self.assertEqual(t.parse_protocol_state(t.render_protocol_state(case["protocol"], case["frames"])), want, case["why"])
        for case in VECTORS["protocol_states"]["invalid"]:
            with self.assertRaises(ValueError, msg=case["why"]):
                t.parse_protocol_state(case["payload"])
        for case in VECTORS["sim_protocol_topics"]:
            want = tuple(case["parts"]) if case["parts"] else None
            self.assertEqual(t.parse_sim_protocol_topic(case["topic"]), want, case["topic"])

    def test_frame_docs(self):
        for case in VECTORS["frame_docs"]["valid"]:
            self.assertEqual(t.parse_frame_doc(case["payload"]), case["want"], case["why"])
            self.assertEqual(t.parse_frame_doc(t.render_frame_doc(case["want"])), case["want"], case["why"])
        for case in VECTORS["frame_docs"]["invalid"]:
            with self.assertRaises(ValueError, msg=case["why"]):
                t.parse_frame_doc(case["payload"])
        for case in VECTORS["frame_doc_topics"]:
            self.assertEqual(t.parse_frame_doc_topic(case["topic"]), case["frame"], case["topic"])

    def test_labels(self):
        for case in VECTORS["labels"]:
            got = t.label(case["full"])
            for k in ("l6", "l8", "l16", "chars", "over6", "over8", "over16"):
                self.assertEqual(got[k], case[k], f"{case['full']} {k}")
        for case in VECTORS["label_overrides"]:
            got = t.label(case["full"], case["l6"], case["l8"], case["l16"])
            for k in ("l6", "l8", "l16"):
                self.assertEqual(got[k], case["want"][k])
        for case in VECTORS["widths"]:
            self.assertEqual(t.width(case["s"]), case["width"], case["why"])

    def test_identities(self):
        for n, id in VECTORS["ports"]["user"]:
            self.assertEqual(t.user_port(n), id)
            self.assertEqual(t.parse_port(id), ("user", n))
        for n, id in VECTORS["ports"]["caster"]:
            self.assertEqual(t.caster_port(n), id)
            self.assertEqual(t.parse_port(id), ("caster", n))
        for bad in VECTORS["ports"]["invalid"]:
            self.assertIsNone(t.parse_port(bad), bad)
        for prefix, n, want in VECTORS["frames"]:
            self.assertEqual(t.frame_id(prefix, n), want)
        for n, want in VECTORS["trunks"]:
            self.assertEqual(t.trunk_id(n), want)
        for raw, want in VECTORS["slugs"]:
            self.assertEqual(t.slugify(raw), want)

    def test_commands(self):
        for case in VECTORS["commands"]["valid"]:
            c = t.parse_command(json.dumps(case["doc"]))
            self.assertEqual(t.parse_command(t.command_to_json(c)), c, case["why"])
        for case in VECTORS["commands"]["invalid"]:
            with self.assertRaises(t.Refused, msg=case["why"]) as ctx:
                t.parse_command(case["body"] if "body" in case else json.dumps(case["doc"]))
            self.assertEqual(ctx.exception.id, case["refused_id"], case["why"])
        for case in VECTORS["results"]:
            self.assertEqual(json.loads(t.result(*case["args"])), case["want"])

    def test_topics(self):
        for case in VECTORS["topics"]:
            got = getattr(t, case["builder"])(*case["args"])
            self.assertEqual(got, (case["topic"], case["retain"]), case["builder"])
        for case in VECTORS["topic_refusals"]:
            with self.assertRaises(ValueError, msg=case["why"]):
                getattr(t, case["builder"])(*case["args"])
        self.assertEqual(t.caster_wildcard(), VECTORS["wildcards"]["caster"])
        self.assertEqual(t.sim_wildcard(), VECTORS["wildcards"]["sim"])
        self.assertEqual(t.sim_wildcard("dante"), VECTORS["wildcards"]["sim_dante"])
        for case in VECTORS["sim_topics"]:
            want = tuple(case["parts"]) if case["parts"] else None
            self.assertEqual(t.parse_sim_topic(case["topic"]), want)

    def test_every_verb_is_one_route(self):
        routes = [v[0] for v in t.VERBS.values()]
        self.assertEqual(len(routes), len(set(routes)))
        self.assertEqual(len(routes), 21)


if __name__ == "__main__":
    unittest.main()
