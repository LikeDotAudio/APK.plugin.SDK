# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""IFC inside a DXF (apk-ifc-dxf/1): the Python twin writes the bytes the TS one does
(contracts/vectors/ifc/dxf.json, from scripts/ifc_dxf_vectors.ts), reads its blobs back,
and — when ezdxf is installed — the DXF still opens with the APK_IFC layer frozen and
off, survives an ezdxf audit and re-save with every blob intact."""
import io
import json
import unittest
from pathlib import Path

from apkaudio_contracts import ifc_dxf as X

ROOT = Path(__file__).resolve().parents[3]
VEC = ROOT / "contracts" / "vectors" / "ifc"
DOC = json.loads((VEC / "dxf.json").read_text(encoding="utf-8"))

try:
    import ezdxf  # noqa: F401
    from ezdxf import recover
except ImportError:  # the twin itself is stdlib only
    ezdxf = None


def _payload(blob):
    return {k: v for k, v in blob.items() if k != "at"}


class TestIfcDxf(unittest.TestCase):
    def test_constants(self):
        self.assertEqual(DOC["format"], X.FORMAT)
        self.assertEqual((X.LAYER, X.APPID, X.CHUNK, X.PART), ("APK_IFC", "APK_IFC", 250, 15000))

    def test_payloads(self):
        for case in DOC["payloads"]:
            p = X.encode_payload(case["blob"])
            self.assertEqual(p, case["payload"])
            self.assertTrue(all(0x21 <= ord(c) <= 0x7E for c in p), "pure printable ASCII, no space")
            self.assertEqual(X.decode_payload(p), _payload(case["blob"]))

    def test_drawings_byte_identical(self):
        blobs = [c["blob"] for c in DOC["payloads"]]
        self.assertGreaterEqual(len(DOC["drawings"]), 4)
        for d in DOC["drawings"]:
            src = (VEC / "dxf" / d["name"]).read_text(encoding="utf-8")
            self.assertEqual(X.add_ifc_to_dxf(src, blobs), d["out"], d["name"])

    def test_round_trip(self):
        blobs = [c["blob"] for c in DOC["payloads"]]
        for d in DOC["drawings"]:
            back = X.read_ifc_dxf(d["out"])
            self.assertEqual([_payload(b) for b in back], [_payload(b) for b in blobs], d["name"])
            self.assertEqual(back[0]["at"], {"x": 120.5, "y": -40.0})
            # written twice: the first carriers are replaced, not doubled
            again = X.add_ifc_to_dxf(d["out"], blobs[:1])
            self.assertEqual([b["guid"] for b in X.read_ifc_dxf(again)], [blobs[0]["guid"]])
            self.assertEqual(again.count("  2\nAPK_IFC\n"), 2, d["name"])  # one LAYER, one APPID record

    def test_two_parts(self):
        long = DOC["payloads"][2]
        self.assertGreater(len(long["payload"]), X.PART)
        out = X.carriers([long["blob"]])
        self.assertEqual(out.count("POINT\n"), 2)

    @unittest.skipIf(ezdxf is None, "ezdxf not installed")
    def test_ezdxf_opens_with_layer_hidden(self):
        for d in DOC["drawings"]:
            doc, auditor = recover.read(io.BytesIO(d["out"].encode("utf-8")))
            self.assertFalse(auditor.has_errors, (d["name"], [str(e.message) for e in auditor.errors]))
            layer = doc.layers.get(X.LAYER)
            self.assertTrue(layer.is_frozen() and layer.is_off(), d["name"])
            mine = [e for e in doc.modelspace() if e.dxf.layer == X.LAYER]
            self.assertEqual(len(mine), 4, d["name"])  # 3 objects, one in 2 parts
            self.assertTrue(all(e.has_xdata(X.APPID) for e in mine))
            # what a reader that ignores the layer sees: the drawing, unchanged
            src = (VEC / "dxf" / d["name"]).read_text(encoding="utf-8")
            orig, _ = recover.read(io.BytesIO(src.encode("utf-8")))
            self.assertEqual(sorted(e.dxftype() for e in doc.modelspace() if e.dxf.layer != X.LAYER),
                             sorted(e.dxftype() for e in orig.modelspace()))
            # re-saved by a program that does not know the format: the blobs survive
            s = io.StringIO()
            doc.write(s)
            back = X.read_ifc_dxf(s.getvalue())
            self.assertEqual([_payload(b) for b in back], [_payload(c["blob"]) for c in DOC["payloads"]], d["name"])


if __name__ == "__main__":
    unittest.main()
