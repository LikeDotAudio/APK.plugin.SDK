# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The IFC GlobalId vectors (viewers-plumbing-1), shared with the TS twin and
generated from DATABASE.server.BIM bim/ifc.py guid()."""
import importlib.util
import json
import sys
import unittest
import uuid
from pathlib import Path

from apkaudio_contracts import ifc_guid as g

ROOT = Path(__file__).resolve().parents[3]
VECTORS = json.loads((ROOT / "contracts" / "vectors" / "ifc-guid.json").read_text(encoding="utf-8"))
IFC_PY = ROOT / "PODS" / "POD.database" / "DATABASE.server.BIM" / "SRC" / "bim" / "ifc.py"


class TestIfcGuid(unittest.TestCase):
    def test_constants(self):
        self.assertEqual(str(g.NAMESPACE), VECTORS["namespace"])
        self.assertEqual(g.ALPHABET, VECTORS["alphabet"])

    def test_vectors(self):
        self.assertGreaterEqual(len(VECTORS["cases"]), 200)
        for case in VECTORS["cases"]:
            gid = g.ifc_guid(*case["roles"])
            self.assertEqual(gid, case["guid"], case["roles"])
            self.assertEqual(g.compress(g.expand(gid)), gid)
            self.assertEqual(g.expand(gid), uuid.uuid5(g.NAMESPACE, "/".join(case["roles"])))
        with self.assertRaises(ValueError):
            g.expand("too-short")

    def test_same_as_server(self):
        if not IFC_PY.exists():
            self.skipTest("DATABASE.server.BIM not checked out")
        # ifc.py is a module of the bim package (relative imports), so import it as bim.ifc
        src = str(IFC_PY.parents[1])
        sys.path.insert(0, src)
        try:
            ifc = importlib.import_module("bim.ifc")
        finally:
            sys.path.remove(src)
        self.assertEqual(ifc._NS, g.NAMESPACE)
        for case in VECTORS["cases"]:
            self.assertEqual(ifc.guid(*case["roles"]), g.ifc_guid(*case["roles"]))

    def test_same_as_ifcopenshell(self):
        try:
            import ifcopenshell.guid as og
        except ImportError:
            self.skipTest("ifcopenshell not importable")
        for case in VECTORS["cases"]:
            u = uuid.uuid5(g.NAMESPACE, "/".join(case["roles"]))
            self.assertEqual(og.compress(u.hex), case["guid"])
            self.assertEqual(og.expand(case["guid"]), u.hex)


if __name__ == "__main__":
    unittest.main()
