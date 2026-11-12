# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The IFC vocabulary every APK.audio IFC writer shares — the Python reader for
contracts/ifc/{mapping,connectors,units}.json (byte-identical copies ship in
apkaudio_contracts/ifc/; scripts/gen.ts writes them and gen:check holds them).
Twin: contracts/src/ifc-vocab.ts, pinned by contracts/vectors/ifc_vocab.json.

    from apkaudio_contracts.ifc_vocab import ifc_type_for, resolve_connector, port_words
    ifc_type_for(part)            # {"entity": "IfcAudioVisualApplianceType", "predefined": "SPEAKER", ...}
    resolve_connector("XLR-3 F")  # the closed connector entry {"id": "XLR3-F", ...}
    port_words("IEC C14")         # {"ElectricalConnectionType": "ACPLUG", "ConnectionSubtype": ..., "ConnectionGender": "MALE"}

Census of a library's port connectors:  python -m apkaudio_contracts.ifc_vocab census <library dir>
Standard library only.
"""
from __future__ import annotations

import json
import re
import sys
import unicodedata
from collections import Counter
from functools import lru_cache
from pathlib import Path

_HERE = Path(__file__).resolve().parent


def _load(name: str) -> dict:
    for p in (_HERE / "ifc" / name, _HERE.parents[1] / "ifc" / name):
        if p.is_file():
            return json.loads(p.read_text(encoding="utf-8"))
    raise FileNotFoundError(f"contracts ifc/{name} not found beside {_HERE}")


@lru_cache(maxsize=1)
def mapping() -> dict:
    return _load("mapping.json")


@lru_cache(maxsize=1)
def connectors() -> dict:
    return _load("connectors.json")


@lru_cache(maxsize=1)
def units() -> dict:
    return _load("units.json")


@lru_cache(maxsize=1)
def signal_formats() -> dict:
    """ifc/signal-formats.json: what a port carries when it is not a connector (ROAD-3588)."""
    return _load("signal-formats.json")


_SIG_WS = re.compile(r"\s+")


def _signal_key(text: str) -> str:
    return _SIG_WS.sub("", text).upper()


@lru_cache(maxsize=1)
def _signal_index() -> tuple[dict, dict]:
    """({key of an id or alias: format}, {id: format}); a key two formats claim names neither."""
    out, by_id, clash = {}, {}, set()
    for f in signal_formats()["formats"]:
        by_id[f["id"]] = f
        for w in [f["id"], *(f.get("aliases") or [])]:
            k = _signal_key(w)
            if not k:
                continue
            if k in out and out[k]["id"] != f["id"]:
                clash.add(k)
            else:
                out.setdefault(k, f)
    for k in clash:
        out.pop(k, None)
    return out, by_id


def resolve_signal_format(text: object) -> dict | None:
    """The signal format a word names: its id or one of its aliases, whole, case and white space aside
    ('Dante / AES67' is AES67), or None. Connectors: resolve_connector."""
    if not isinstance(text, str) or not text.strip():
        return None
    return _signal_index()[0].get(_signal_key(text))


def canonical_signal_format(text: object) -> dict | None:
    """The canonical format a word names: a flow variant's `canonical` (DANTE-TX8 -> DANTE), else the
    format itself. Pset_DistributionPortTypeCable.Protocols carries this id (ROAD-3598 answer 9)."""
    f = resolve_signal_format(text)
    if not f or not f.get("canonical"):
        return f
    return _signal_index()[1].get(f["canonical"], f)


# ------------------------------------------------------------------ connectors

_DQ = re.compile("[“”″]")
_DASH = re.compile("[–—−]")
_MM = re.compile(r"(\d)\s*mm\b")
_INCH = re.compile(r"(\d)\s*inch(es)?\b")
_WS = re.compile(r"\s+")
_TAIL = re.compile(r"[\s\-,;:.]+$")
_PAREN = re.compile(r"\s*\([^()]*\)$")
_SPLIT = re.compile(r"\s+/\s+|\s*;\s*")


def normalise(text: str) -> str:
    """connectors.json `normalise`, step for step (the TS twin is identical)."""
    t = unicodedata.normalize("NFKC", str(text)).lower()
    t = _DQ.sub('"', t)
    t = _DASH.sub("-", t)
    t = _MM.sub(r"\1 mm", t)
    t = _INCH.sub(r'\1"', t)
    t = _WS.sub(" ", t).strip()
    return _TAIL.sub("", t)


def _strip_parens(t: str) -> str:
    while True:
        s = _PAREN.sub("", t)
        if s == t:
            return _TAIL.sub("", s)
        t = s


@lru_cache(maxsize=1)
def _index() -> tuple[dict, dict, dict, dict]:
    """(by id, by alias, by domain -> alias, by combo spelling -> ids)."""
    by_id: dict = {}
    by_alias: dict = {}
    by_domain: dict = {}
    by_combo: dict = {}
    doc = connectors()
    for e in doc["connectors"]:
        by_id[e["id"]] = e
        keys = [e["id"], *e["aliases"], *([e["cad"]] if e.get("cad") else [])]
        for k in keys:
            n = normalise(k)
            if n in by_alias and by_alias[n] != e["id"]:
                raise ValueError(f"connectors.json: alias {k!r} names both {by_alias[n]} and {e['id']}")
            by_alias[n] = e["id"]
    for e in doc["connectors"]:
        for dom, spellings in (e.get("domain_aliases") or {}).items():
            table = by_domain.setdefault(dom, {})
            for k in spellings:
                n = normalise(k)
                if n in table and table[n] != e["id"]:
                    raise ValueError(f"connectors.json: {dom} alias {k!r} names both {table[n]} and {e['id']}")
                table[n] = e["id"]
    for c in doc.get("combos") or []:
        n = normalise(c["spelling"])
        if n in by_alias:
            raise ValueError(f"connectors.json: combo {c['spelling']!r} is also an alias of {by_alias[n]}")
        missing = [i for i in c["ids"] if i not in by_id]
        if missing or len(c["ids"]) < 2:
            raise ValueError(f"connectors.json: combo {c['spelling']!r} needs two known ids (unknown: {missing})")
        by_combo[n] = list(c["ids"])
    return by_id, by_alias, by_domain, by_combo


def connector(id_: str) -> dict | None:
    """The entry for a canonical id."""
    return _index()[0].get(id_)


def resolve_connector(text: object, domain: str | None = None) -> dict | None:
    """The one entry a spelling names (whole text, then without trailing parentheticals), or None.
    With the port's `domain`, that domain's aliases (connectors.json `domain_aliases`) win."""
    if not isinstance(text, str) or not text.strip():
        return None
    by_id, by_alias, by_domain, _ = _index()
    n = normalise(text)
    for table in ((by_domain.get(domain) or {}) if domain else {}, by_alias):
        hit = table.get(n) or table.get(_strip_parens(n))
        if hit:
            return by_id[hit]
    return None


def resolve_connectors(text: object, domain: str | None = None) -> list[dict]:
    """Every connector a port's text offers: one entry, a declared combo's (connectors.json `combos`),
    or one per ' / ' / ';' part when the whole text is not an alias. [] when any part is unknown (an
    unknown never half-resolves). `domain` as for resolve_connector, for every part."""
    one = resolve_connector(text, domain)
    if one:
        return [one]
    if not isinstance(text, str):
        return []
    by_id, _, _, by_combo = _index()
    n = normalise(text)
    combo = by_combo.get(n) or by_combo.get(_strip_parens(n))
    if combo:
        return [by_id[i] for i in combo]
    parts = [p for p in _SPLIT.split(n) if p]
    if len(parts) < 2:
        return []
    out = [resolve_connector(p, domain) for p in parts]
    return out if all(out) else []


def port_words(text: object, domain: str | None = None) -> dict | None:
    """Pset_DistributionPortTypeCable values for a connector spelling, or None when unknown."""
    e = resolve_connector(text, domain)
    if not e:
        return None
    w = {"ElectricalConnectionType": e["ElectricalConnectionType"], "ConnectionSubtype": e["subtype"],
         "ConnectionGender": e["ConnectionGender"]}
    if e.get("ConductorFunction"):
        w["ConductorFunction"] = e["ConductorFunction"]
    return w


def connector_presentation(gender) -> str:
    """PLUG / SOCKET / NOTKNOWN for a ConnectionGender word or a connector entry (connectors.json
    `presentation`, owner ROAD-3598 answer 16). IFC files always carry the ConnectionGender word."""
    g = gender.get("ConnectionGender") if isinstance(gender, dict) else gender
    return (connectors().get("presentation") or {}).get(g, "NOTKNOWN") if isinstance(g, str) else "NOTKNOWN"


def census(values) -> dict:
    """Coverage of a list of port `connector` values (blank values are counted apart)."""
    total = blank = real = placeholder = 0
    unresolved: Counter = Counter()
    for v in values:
        total += 1
        if not isinstance(v, str) or not v.strip():
            blank += 1
            continue
        hits = resolve_connectors(v)
        if not hits:
            unresolved[v] += 1
        elif all(h.get("placeholder") for h in hits):
            placeholder += 1
        else:
            real += 1
    named = total - blank
    return {
        "rows": total, "blank": blank, "named": named, "resolved": real, "placeholder": placeholder,
        "unresolved": sum(unresolved.values()), "unresolved_spellings": len(unresolved),
        "coverage_pct": round(100.0 * (real + placeholder) / named, 2) if named else 100.0,
        "real_coverage_pct": round(100.0 * real / named, 2) if named else 100.0,
        "top_unresolved": unresolved.most_common(40),
    }


# ------------------------------------------------------------------ part -> IFC type

def _text(part: dict) -> str:
    return " ".join(str(part.get(k) or "") for k in ("name", "model", "description"))


@lru_cache(maxsize=None)
def _rx(pattern: str):
    return re.compile(pattern, re.I)


def _part_ifc(part: dict) -> dict:
    block = part.get("ifc") if isinstance(part.get("ifc"), dict) else None
    if not block:
        block = ((part.get("ext") or {}).get("ifc")) if isinstance(part.get("ext"), dict) else None
    return block if isinstance(block, dict) and block.get("entity") else {}


def ifc_type_for(part: dict) -> dict:
    """The IFC type a part is written as: {entity, predefined, element_type, occurrence, qto, source}.
    entity None = not a physical element (no type is written)."""
    m = mapping()
    cls = str(part.get("class") or "").strip()
    category = part.get("category") if part.get("category") in m["categories"] else "Other"
    if not cls or _rx(m["noise_classes"]).search(cls):
        cls_word = ""
    else:
        cls_word = cls
    own = _part_ifc(part)
    if own:
        entity = str(own["entity"])
        if not entity.endswith("Type"):
            entity += "Type"
        rule = {"entity": entity, "predefined": own.get("predefined_type") or own.get("predefined") or "NOTDEFINED",
                "element_type": own.get("element_type")}
        source = "part"
    else:
        rule, source = None, None
        text = _text(part)
        for r in m["classes"]:
            if "category" in r and r["category"] != category:
                continue
            if "class" in r and not _rx(r["class"]).search(cls):
                continue
            if "text" in r and not _rx(r["text"]).search(text):
                continue
            rule, source = r, "class"
            break
        if rule is None:
            rule, source = m["categories"][category], "category"
    entity = rule.get("entity")
    if entity is None:
        return {"entity": None, "predefined": None, "element_type": None, "occurrence": None, "qto": None, "source": source}
    pre = rule.get("predefined") or "NOTDEFINED"
    pre = m.get("legacy_predefined", {}).get(entity, {}).get(pre, pre)
    et = rule.get("element_type")
    if et:
        et = et.replace("{class}", cls_word or category)
    if pre == "USERDEFINED" and not et:
        et = cls_word or category
    info = m["entities"].get(entity, {})
    out = {"entity": entity, "predefined": pre, "element_type": et,
           "occurrence": info.get("occurrence") or (entity[:-4] if entity.endswith("Type") else entity),
           "qto": info.get("qto"), "source": source}
    for k in ("class_pset", "frame"):
        if rule.get(k):
            out[k] = rule[k]
    return out


# ------------------------------------------------------------------ CLI

def _library_connectors(root: Path):
    for p in sorted(root.glob("*/*.bim.json")):
        try:
            part = json.loads(p.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        for port in part.get("ports") or []:
            yield port.get("connector")


def main(argv: list[str]) -> int:
    if len(argv) >= 2 and argv[0] == "census":
        report = census(_library_connectors(Path(argv[1])))
        print(json.dumps(report, indent=1, ensure_ascii=False))
        return 0
    print("usage: python -m apkaudio_contracts.ifc_vocab census <library dir>", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
