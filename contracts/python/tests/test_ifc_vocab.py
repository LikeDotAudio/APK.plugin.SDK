# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""contracts/ifc/{mapping,connectors,units}.json — the shared IFC vocabulary.

The plain tests need only the standard library. The IFC4X3_ADD2 checks need IfcOpenShell
(pip install ifcopenshell; pytest for EXPRESS rules) and skip cleanly without it: every entity,
enumeration value, Pset/Qto and property named in the three files is looked up in the schema
and the buildingSMART templates, never trusted from memory.
"""
import json
import re
from pathlib import Path

import pytest

from apkaudio_contracts import ifc_vocab as v

ROOT = Path(__file__).resolve().parents[2]
VECTORS = json.loads((ROOT / "vectors" / "ifc_vocab.json").read_text(encoding="utf-8"))
CATEGORIES = ("Audio", "Microphones", "Video", "Network", "Control", "Power",
              "Infrastructure", "Test & Measurement", "Computing", "Accessories", "Concepts", "Other")


# ------------------------------------------------------------------ plain


@pytest.mark.parametrize("name", ["mapping.json", "connectors.json", "units.json", "signal-formats.json"])
def test_package_copy_is_the_source(name):
    assert (ROOT / "ifc" / name).read_bytes() == (ROOT / "python" / "apkaudio_contracts" / "ifc" / name).read_bytes()


def test_connector_vectors():
    for c in VECTORS["connectors"]:
        assert v.normalise(c["input"]) == c["normalised"], c["input"]
        dom = c.get("domain")
        one = v.resolve_connector(c["input"], dom)
        assert (one["id"] if one else None) == c["id"], c["input"]
        assert [e["id"] for e in v.resolve_connectors(c["input"], dom)] == c["ids"], c["input"]
        assert v.port_words(c["input"], dom) == c["words"], c["input"]


def test_signal_format_vectors():
    for s in VECTORS["signal_formats"]:
        f, c = v.resolve_signal_format(s["input"]), v.canonical_signal_format(s["input"])
        assert (f["id"] if f else None) == s["id"], s["input"]
        assert (c["id"] if c else None) == s["canonical"], s["input"]
        assert (f["slot"] if f else None) == s["slot"], s["input"]


def test_presentation_vectors():
    for p in VECTORS["presentation"]:
        got = v.connector_presentation(v.connector(p["connector"]) if p.get("connector") else p["gender"])
        assert got == p["presentation"], p


def test_signal_formats_one_canonical_id():
    """ROAD-3598 answer 9: aliases resolve to their own format; a variant's canonical is a plain format of its family."""
    by_id = {f["id"]: f for f in v.signal_formats()["formats"]}
    for f in by_id.values():
        for a in f.get("aliases") or []:
            assert v.resolve_signal_format(a)["id"] == f["id"], (f["id"], a)
        if f.get("canonical"):
            c = by_id[f["canonical"]]
            assert (c["family"], c["slot"], c.get("canonical")) == (f["family"], f["slot"], None), f["id"]
    for w in ("AES67", "AES-67", "Dante / AES67", "Ravenna / AES67", "AES67-TX", "DANTE-AES67"):
        assert v.canonical_signal_format(w)["id"] == "AES67", w


def test_trade_names_and_cited_entries():
    """ROAD-3598 answers 14-16: OTHER + trade name; the 34420A input and the WE microphones' own connectors."""
    assert v.port_words("Euroblock 3-pin, 3.5 mm pitch") == {
        "ElectricalConnectionType": "OTHER", "ConnectionSubtype": "Phoenix 3.5 mm 3-pin", "ConnectionGender": "NOTKNOWN"}
    assert v.port_words("NL4 F")["ConnectionSubtype"] == "speakON NL4"
    lt = v.connector("LOW-THERMAL-34420")
    assert (lt["pins"], lt["ConnectionGender"]) == (4, "NOTKNOWN") and "keysight.com" in lt["source"]["url"]
    assert v.connector_presentation(v.connector("WE-405A")) == "SOCKET"
    assert v.connector_presentation(v.connector("WE-639-PLUG")) == "PLUG"
    for e in v.connectors()["connectors"]:
        assert v.connector_presentation(e) in ("PLUG", "SOCKET", "NOTKNOWN"), e["id"]
        if e.get("source"):
            assert e["source"]["url"].startswith("https://") and e["source"]["document"], e["id"]


def test_part_vectors():
    for p in VECTORS["parts"]:
        assert v.ifc_type_for(p["part"]) == p["expect"], p["part"]


def test_every_alias_resolves_to_its_own_entry():
    for e in v.connectors()["connectors"]:
        for a in [e["id"], *e["aliases"], *([e["cad"]] if e.get("cad") else [])]:
            assert v.resolve_connector(a)["id"] == e["id"], (a, e["id"])


def test_domain_aliases_and_combos():
    """A domain alias names its own entry on that domain only; a combo names its ids in order."""
    for e in v.connectors()["connectors"]:
        for dom, spellings in (e.get("domain_aliases") or {}).items():
            for a in spellings:
                assert v.resolve_connector(a, dom)["id"] == e["id"], (dom, a)
    assert v.resolve_connector("3.5 mm male")["id"] == "TRS-3.5-M"
    assert v.resolve_connector("3.5 mm male", "audio")["id"] == "TRS-3.5-M"
    assert v.resolve_connector("3.5 mm male", "rf")["id"] == "COAX-APC-3.5-M"
    for c in v.connectors().get("combos") or []:
        assert v.resolve_connector(c["spelling"]) is None, c["spelling"]
        assert [e["id"] for e in v.resolve_connectors(c["spelling"])] == c["ids"], c["spelling"]
        assert [e["id"] for e in v.resolve_connectors(" / ".join(c["ids"]))] == c["ids"], c["spelling"]


def test_connector_entries_are_closed_and_complete():
    doc = v.connectors()
    ids = [e["id"] for e in doc["connectors"]]
    assert len(ids) == len(set(ids))
    for e in doc["connectors"]:
        assert re.fullmatch(r"[A-Z0-9][A-Z0-9.+\-]*", e["id"]), e["id"]
        assert e["subtype"] and e["family"], e["id"]
        assert e["ElectricalConnectionType"] in doc["enums"]["ElectricalConnectionType"], e["id"]
        assert e["ConnectionGender"] in doc["enums"]["ConnectionGender"], e["id"]
        if "ConductorFunction" in e:
            assert e["ConductorFunction"] in doc["enums"]["ConductorFunction"], e["id"]
        for cf in e.get("conductors") or []:
            assert cf in doc["enums"]["ConductorFunction"], e["id"]
        if e["pins"] is not None and e["pinout"] is not None:
            assert len(e["pinout"]) == e["pins"], e["id"]
        if e.get("conductors") and e["pinout"]:
            assert len(e["conductors"]) == len(e["pinout"]), e["id"]
        if e.get("placeholder"):
            assert e["ElectricalConnectionType"] == "NOTKNOWN", e["id"]


def test_required_defaults():
    """The pinouts and words the owner named."""
    assert v.connector("XLR3-M")["pinout"] == ["Gnd", "+", "-"]
    assert v.connector("TRS-6.35")["pinout_variants"]["insert"] == ["Tip send", "Ring return", "Sleeve Gnd"]
    assert v.port_words("IEC C14")["ConnectionGender"] == "MALE"
    assert v.port_words("IEC C13")["ConnectionGender"] == "FEMALE"
    assert v.connector("IEC-C19")["conductors"] == ["PHASE_L1", "NEUTRAL", "PROTECTIVEEARTH"]
    assert v.connector("NEMA-5-15P")["ConnectionGender"] == "MALE"
    assert v.connector("NEMA-5-15R")["ConnectionGender"] == "FEMALE"
    assert v.connector("POWERCON-TRUE1")["ElectricalConnectionType"] == "ACPLUG"
    assert v.connector("ETHERCON")["ElectricalConnectionType"] == "RJ"
    assert v.connector("COAX-BNC")["ElectricalConnectionType"] == "COAXIAL"
    assert v.connector("SFP")["ElectricalConnectionType"] == "OTHER"
    assert v.connector("DSUB-9")["pinout"][4] == "GND"
    assert v.connector("ADC-PJ339")["family"] == "TT"
    for pitch in ("3.5", "5.08"):
        for n in (2, 3, 4, 5, 6, 8, 10, 12):
            assert v.connector(f"EUROBLOCK-{pitch}-{n}")["pins"] == n
    assert v.connector("EUROBLOCK-3")["pinout"] == ["1 +", "1 -", "1 Gnd"]
    for n in (20, 38, 56, 90, 120):
        assert v.connector(f"EDAC-{n}")["pins"] == n
    assert v.port_words("ground lug")["ConductorFunction"] == "PROTECTIVEEARTH"


def test_cad_panel_codes_road_3599():
    a59 = v.connector("AES59-DB25-M")
    assert (a59["ElectricalConnectionType"], a59["ConnectionGender"], a59["cad"]) == ("DSUB", "MALE", "A59P")
    assert a59["pinout"][23] == "Ch1 +" and a59["pinout"][12] == "n/c"
    assert a59["pinout_variants"]["aes3"][0] == "AES3 Out 8 +"
    assert v.connector("AES72-1E")["pinout"][0] == "1 Link 1 -"
    assert v.port_words("TJ##S") == {
        "ElectricalConnectionType": "OTHER", "ConnectionSubtype": "Tajimi (pins not stated)", "ConnectionGender": "FEMALE"}
    assert v.resolve_connector("KEY-RCA")["id"] == "RCA-F"
    assert v.resolve_connector("SMPTEP-TH")["id"] == "SMPTE-304-M"


def test_unknown_never_half_resolves():
    assert v.resolve_connector("Wobble 9000") is None
    assert v.resolve_connectors("XLR / Wobble") == []
    assert v.port_words("") is None and v.port_words(None) is None


def test_census_counts():
    r = v.census(["XLR-3 M", "Jack", "", None, "Wobble", "XLR / USB-C"])
    assert (r["rows"], r["blank"], r["resolved"], r["placeholder"], r["unresolved"]) == (6, 2, 2, 1, 1)


def test_library_census_when_present():
    lib = ROOT.parent / "PODS" / "POD.database" / "DATABASE.server.BIM" / "SRC" / "library"
    if not lib.is_dir():
        pytest.skip("BIM library not beside contracts")
    r = v.census(v._library_connectors(lib))
    assert r["coverage_pct"] >= 99.0, r["top_unresolved"]


def test_mapping_covers_every_category_and_userdefined_has_a_word():
    m = v.mapping()
    assert set(m["categories"]) == set(CATEGORIES)
    for r in [*m["categories"].values(), *m["classes"]]:
        if r.get("entity") is None:
            continue
        assert r["entity"] in m["entities"], r
        if r.get("predefined") == "USERDEFINED":
            assert r.get("element_type"), r
    for c in CATEGORIES:
        t = v.ifc_type_for({"category": c, "class": ""})
        if c == "Concepts":  # not physical: no IFC type, the twin is the library + APK_Part (ROAD-3588)
            assert t["entity"] is None, c
            continue
        assert t["entity"] and (t["predefined"] != "USERDEFINED" or t["element_type"]), c


def test_apk_sets_are_never_reserved():
    m = v.mapping()
    for f in [*m["part_fields"], *m["port"]["fields"], *m["occurrence_fields"]]:
        s = f.get("set")
        if s and not s.startswith(("Pset_", "Qto_", "{")):
            assert s.startswith("APK_"), f


def test_units_rule():
    u = v.units()
    assert u["default_voltage_unit"] == "Vrms" and u["units"]["Vrms"]["default"]
    for name in ("Vpp", "Vpk", "dBm", "dBu", "dBV"):
        d = u["units"][name]
        assert d["ifc"] == "IfcContextDependentUnit" and d["Name"] == name
        assert len(d["dimensions"]) == 7
    assert u["units"]["Vpp"]["dimensions"] == [2, 1, -3, -1, 0, 0, 0]


# ------------------------------------------------------------------ IFC4X3_ADD2 (IfcOpenShell)


@pytest.fixture(scope="module")
def ifc():
    ios = pytest.importorskip("ifcopenshell")
    import ifcopenshell.util.pset as P
    schema = ios.ifcopenshell_wrapper.schema_by_name("IFC4X3_ADD2")
    return ios, schema, P.get_template("IFC4X3")


def _decl(schema, name):
    try:
        return schema.declaration_by_name(name)
    except Exception:
        return None


def _is_sub(decl, ancestor):
    while decl is not None:
        if decl.name() == ancestor:
            return True
        decl = decl.supertype()
    return False


def _predefined_enum(schema, entity):
    d = _decl(schema, entity)
    for a in d.all_attributes():
        if a.name() == "PredefinedType":
            return set(a.type_of_attribute().declared_type().enumeration_items())
    raise AssertionError(f"{entity} has no PredefinedType")


def test_entities_exist_in_ifc4x3(ifc):
    _, schema, tpl = ifc
    m = v.mapping()
    for ent, info in m["entities"].items():
        d = _decl(schema, ent)
        assert d is not None and not d.is_abstract(), ent
        assert _is_sub(d, "IfcTypeObject"), ent
        occ = _decl(schema, info["occurrence"])
        assert occ is not None and _is_sub(occ, "IfcElement"), info["occurrence"]
        for key in ("qto", "common"):
            if info.get(key):
                t = tpl.get_by_name(info[key])
                assert t is not None, info[key]
                assert info[key] in {x.Name for x in tpl.get_applicable(ent)}, (info[key], ent)
        if info.get("qto"):
            q = {p.Name: p.TemplateType for p in tpl.get_by_name(info["qto"]).HasPropertyTemplates}
            assert q.get("GrossWeight") == "Q_WEIGHT", info["qto"]
        else:
            # no standard weight quantity: confirm IFC really has none applicable
            for t in tpl.get_applicable(ent):
                if t.Name.startswith("Qto_"):
                    assert "GrossWeight" not in [p.Name for p in t.HasPropertyTemplates], (ent, t.Name)
    assert not m["weight_fallback"]["set"].startswith(("Qto_", "Pset_"))
    assert _decl(schema, m["weight_fallback"]["ifc"]) is not None


def test_every_predefined_type_is_in_its_enum(ifc):
    _, schema, tpl = ifc
    m = v.mapping()
    rules = [*m["categories"].values(), *m["classes"]]
    for r in rules:
        if r.get("entity") is None:
            continue
        assert r["predefined"] in _predefined_enum(schema, r["entity"]), r
        if r.get("frame"):
            assert r["frame"]["predefined"] in _predefined_enum(schema, r["frame"]["entity"]), r
        if r.get("class_pset"):
            t = tpl.get_by_name(r["class_pset"])
            assert t is not None, r["class_pset"]
            names = {x.Name for x in tpl.get_applicable(r["entity"], predefined_type=r["predefined"])}
            assert r["class_pset"] in names, r["class_pset"]
    for ent, rewrites in m["legacy_predefined"].items():
        for old, new in rewrites.items():
            assert old in _predefined_enum(schema, ent) and new in _predefined_enum(schema, ent)
    for p in VECTORS["parts"]:
        e = p["expect"]
        if e["entity"]:
            assert e["predefined"] in _predefined_enum(schema, e["entity"]), p


def _check_field(schema, tpl, f, qto_names):
    sets = qto_names if f["set"] == "{qto}" else [f["set"]]
    for s in sets:
        if s.startswith(("Pset_", "Qto_")):
            t = tpl.get_by_name(s)
            assert t is not None, s
            props = {p.Name: p for p in t.HasPropertyTemplates}
            assert f["property"] in props, (s, f["property"])
            p = props[f["property"]]
            assert p.TemplateType == f["template_type"], (s, f["property"], p.TemplateType)
            if s.startswith("Pset_"):
                assert p.PrimaryMeasureType == f["measure"], (s, f["property"], p.PrimaryMeasureType)
        else:
            assert s.startswith("APK_"), s
            assert f["template_type"] in ("P_SINGLEVALUE", "P_LISTVALUE", "P_BOUNDEDVALUE", "P_ENUMERATEDVALUE", "Q_WEIGHT"), f
    assert _decl(schema, f["measure"]) is not None, f["measure"]


def test_every_field_names_a_real_property(ifc):
    _, schema, tpl = ifc
    m = v.mapping()
    qtos = sorted({i["qto"] for i in m["entities"].values() if i.get("qto")})
    for f in [*m["part_fields"], *m["port"]["fields"], *m["occurrence_fields"]]:
        if f.get("set"):
            _check_field(schema, tpl, f, qtos)
    for a in m["part_attributes"]:
        assert any(x.name() == a["attribute"] for x in _decl(schema, "IfcTypeProduct").all_attributes()), a
    port = m["port"]
    assert port["predefined_type"] in _predefined_enum(schema, port["entity"])
    flows = set(_decl(schema, "IfcFlowDirectionEnum").enumeration_items())
    systems = set(_decl(schema, "IfcDistributionSystemEnum").enumeration_items())
    assert set(port["flow"].values()) <= flows and port["earthing"]["flow"] in flows
    assert {s for s in port["system"].values() if s} <= systems and port["earthing"]["system"] in systems
    for rel in m["relationships"].values():
        assert _decl(schema, rel) is not None, rel
    for e in m["systems"].values():
        assert _decl(schema, e) is not None, e


def test_connector_enums_are_the_template_enums(ifc):
    _, _, tpl = ifc
    t = tpl.get_by_name(v.connectors()["pset"])
    props = {p.Name: p for p in t.HasPropertyTemplates}
    for name, values in v.connectors()["enums"].items():
        got = [x.wrappedValue for x in props[name].Enumerators.EnumerationValues]
        assert values == got, name
    assert props["ConnectionSubtype"].PrimaryMeasureType == "IfcLabel"


def test_units_and_types_validate_with_express_rules(ifc):
    """One model with every mapped type (each with its psets, weight and a port carrying the connector
    words) and every context-dependent unit must give 0 schema + EXPRESS-rule errors."""
    ios, schema, _ = ifc
    pytest.importorskip("pytest")
    import ifcopenshell.api as A
    import ifcopenshell.validate as V
    f = A.run("project.create_file", version="IFC4X3_ADD2")
    A.run("root.create_entity", f, ifc_class="IfcProject", name="vocab")
    A.run("unit.assign_unit", f)
    units = {}
    for name, d in v.units()["units"].items():
        if d["ifc"] == "IfcContextDependentUnit":
            dims = f.createIfcDimensionalExponents(*d["dimensions"])
            assert d["UnitType"] in schema.declaration_by_name("IfcUnitEnum").enumeration_items()
            units[name] = f.createIfcContextDependentUnit(dims, d["UnitType"], d["Name"])
    m = v.mapping()
    seen = set()
    words = v.port_words("XLR-3 F")
    for r in [*m["categories"].values(), *m["classes"]]:
        if r.get("entity") is None:
            continue
        key = (r["entity"], r["predefined"], r.get("element_type"))
        if key in seen:
            continue
        seen.add(key)
        t = A.run("root.create_entity", f, ifc_class=r["entity"], name="/".join(str(k) for k in key), predefined_type=r["predefined"])
        if r["predefined"] == "USERDEFINED":
            t.ElementType = r["element_type"].replace("{class}", "Class")
        ps = A.run("pset.add_pset", f, product=t, name="Pset_ManufacturerTypeInformation")
        A.run("pset.edit_pset", f, pset=ps, properties={"Manufacturer": "APK", "ModelReference": "M1"})
        info = m["entities"][r["entity"]]
        qto = A.run("pset.add_qto", f, product=t, name=info.get("qto") or m["weight_fallback"]["set"])
        A.run("pset.edit_qto", f, qto=qto, properties={"GrossWeight": 1.5})
        port = A.run("root.create_entity", f, ifc_class="IfcDistributionPort", name="In 1")
        port.PredefinedType = m["port"]["predefined_type"]; port.FlowDirection = "SINK"; port.SystemType = "AUDIOVISUAL"
        A.run("nest.assign_object", f, related_objects=[port], relating_object=t)
        pp = A.run("pset.add_pset", f, product=port, name="Pset_DistributionPortTypeCable")
        A.run("pset.edit_pset", f, pset=pp, properties=dict(words))
        apk = A.run("pset.add_pset", f, product=port, name="APK_Terminal")
        apk.HasProperties = [
            f.createIfcPropertySingleValue("NominalLevel", None, f.createIfcReal(4.0), units["dBu"]),
            f.createIfcPropertySingleValue("InputImpedance", None, f.createIfcElectricResistanceMeasure(10000.0), None),
            f.createIfcPropertySingleValue("OutputAmplitudeMax", None, f.createIfcElectricVoltageMeasure(10.0), units["Vpp"]),
        ]
    log = V.json_logger()
    V.validate(f, log, express_rules=True)
    assert log.statements == [], log.statements[:5]
    assert len(seen) >= 40
