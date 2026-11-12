# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""IFC inside a DXF (ROAD-3598 answer 2), format apk-ifc-dxf/1.

Twin of contracts/src/ifc/dxf.ts (the format is documented there), held byte for
byte to contracts/vectors/ifc/dxf.json: the hidden layer APK_IFC (frozen, off,
never plotted), the APPID APK_IFC, and one POINT per object part on that layer
carrying the object's IFC data as XDATA (1001 APK_IFC, 1000 apk-ifc-dxf/1,
1000 <GlobalId>, 1070 part, 1070 parts, 1000 chunks of <= 250 chars of the
object's compact, pure-ASCII JSON {guid, ifc, name?, tag?, psets?, step?}).

  add_ifc_to_dxf(dxf_text, blobs) -> dxf_text   any DXF text, R12 or R2000
  read_ifc_dxf(dxf_text) -> [blob, ...]          the blobs back, parts joined

A blob is a dict: guid, ifc (the STEP entity name), name, tag, psets, step, and
"at" ({x, y}, where its POINT sits; not part of the payload). Standard library only.
Numbers in psets encode as Python's json does, which is the TS twin's spelling for
integers and ordinary reals (not for exponents: 1e-07 vs 1e-7).
"""
from __future__ import annotations

import json
import re

LAYER = "APK_IFC"
APPID = "APK_IFC"
FORMAT = "apk-ifc-dxf/1"
CHUNK = 250
PART = 15000

_ESCAPE = re.compile(r"[^\x21-\x7e]")


def encode_payload(blob: dict) -> str:
    """The canonical payload of a blob: compact JSON, printable ASCII only, no space."""
    o = {"guid": blob["guid"], "ifc": blob["ifc"]}
    for k in ("name", "tag"):
        if blob.get(k) is not None:
            o[k] = blob[k]
    if blob.get("psets"):
        o["psets"] = blob["psets"]
    if blob.get("step") is not None:
        o["step"] = blob["step"]
    text = json.dumps(o, ensure_ascii=True, separators=(",", ":"))
    return _ESCAPE.sub(lambda m: "\\u%04x" % ord(m.group(0)), text)


def decode_payload(text: str) -> dict:
    o = json.loads(text)
    if not isinstance(o, dict) or not isinstance(o.get("guid"), str) or not isinstance(o.get("ifc"), str):
        raise ValueError("apk-ifc-dxf: payload has no guid / ifc")
    return o


def _chunks(s: str, n: int) -> list:
    return [s[i:i + n] for i in range(0, len(s), n)] or [""]


def _pair(code, value) -> str:
    return f"{code}\n{value}\n"


def _fixed(v) -> str:
    return "%.6f" % (float(v) if v is not None else 0.0)


class _Style:
    def __init__(self, handle=None, subclass=False, owner=None):
        self.handle, self.subclass, self.owner = handle, subclass, owner


def _head(kind, st, *subclasses) -> str:
    out = _pair("  0", kind)
    if st.handle:
        out += _pair("  5", st.handle())
    if st.subclass and st.owner:
        out += _pair("330", st.owner)
    if st.subclass:
        for s in subclasses:
            out += _pair("100", s)
    return out


def layer_record(st=None) -> str:
    st = st or _Style()
    out = (_head("LAYER", st, "AcDbSymbolTableRecord", "AcDbLayerTableRecord") + _pair("  2", LAYER)
           + _pair(" 70", 1) + _pair(" 62", -8) + _pair("  6", "CONTINUOUS"))
    if st.subclass:
        out += _pair("290", 0) + _pair("370", -3)
    return out


def appid_record(st=None, name=APPID) -> str:
    st = st or _Style()
    return _head("APPID", st, "AcDbSymbolTableRecord", "AcDbRegAppTableRecord") + _pair("  2", name) + _pair(" 70", 0)


def carriers(blobs, st=None) -> str:
    st = st or _Style()
    out = ""
    for b in blobs:
        parts = _chunks(encode_payload(b), PART)
        at = b.get("at") or {}
        for i, part in enumerate(parts):
            out += _head("POINT", st, "AcDbEntity") + _pair("  8", LAYER)
            if st.subclass:
                out += _pair("100", "AcDbPoint")
            out += _pair(" 10", _fixed(at.get("x", 0))) + _pair(" 20", _fixed(at.get("y", 0))) + _pair(" 30", _fixed(0))
            out += (_pair("1001", APPID) + _pair("1000", FORMAT) + _pair("1000", b["guid"])
                    + _pair("1070", i + 1) + _pair("1070", len(parts)))
            for c in _chunks(part, CHUNK):
                out += _pair("1000", c)
    return out


# ------------------------------------------------------------------ tags --

def _tags_of(text: str):
    eol = "\r\n" if "\r\n" in text else "\n"
    lines = re.split(r"\r?\n", text)
    return [[lines[i], lines[i + 1]] for i in range(0, len(lines) - 1, 2)], eol


def _code(t) -> str:
    return t[0].strip() if t else ""


def _is(t, code, value=None) -> bool:
    return bool(t) and _code(t) == code and (value is None or t[1].strip().upper() == value.upper())


def _section_at(tags, name) -> int:
    for i in range(len(tags) - 1):
        if _is(tags[i], "0", "SECTION") and _is(tags[i + 1], "2", name):
            return i
    return -1


def _endsec_from(tags, i) -> int:
    for j in range(i, len(tags)):
        if _is(tags[j], "0", "ENDSEC"):
            return j
    return -1


def _table_at(tags, start, end, name):
    for i in range(start, end - 1):
        if _is(tags[i], "0", "TABLE") and _is(tags[i + 1], "2", name):
            for j in range(i + 2, end):
                if _is(tags[j], "0", "ENDTAB"):
                    return [i, j]
    return None


def _from_text(s):
    return _tags_of(s)[0]


def _records_of(tags, span, kind):
    s, e = span
    out = []
    for i in range(s + 2, e):
        if not _is(tags[i], "0", kind):
            continue
        name, handle = "", None
        j = i + 1
        while j < e and _code(tags[j]) != "0":
            if _code(tags[j]) == "2" and not name:
                name = tags[j][1].strip()
            if _code(tags[j]) == "5" and not handle:
                handle = tags[j][1].strip()
            j += 1
        out.append({"at": i, "name": name, "handle": handle})
    return out


def _table_handle(tags, start):
    j = start + 2
    while j < len(tags) and _code(tags[j]) != "0":
        if _code(tags[j]) == "5":
            return tags[j][1].strip()
        j += 1
    return None


def _bump_count(tags, start, by):
    j = start + 2
    while j < len(tags) and _code(tags[j]) != "0":
        if _code(tags[j]) == "70":
            try:
                n = int(tags[j][1].strip())
            except ValueError:
                n = 0
            tags[j] = [tags[j][0], str(n + by)]
            return
        j += 1


def _drop_carriers(tags, start, end) -> int:
    removed = 0
    i = start
    while i < end - removed:
        if _code(tags[i]) != "0":
            i += 1
            continue
        j = i + 1
        mine = False
        while j < end - removed and _code(tags[j]) != "0":
            if _code(tags[j]) == "8" and tags[j][1].strip().upper() == LAYER:
                mine = True
            j += 1
        if mine:
            del tags[i:j]
            removed += j - i
        else:
            i = j
    return removed


def add_ifc_to_dxf(dxf: str, blobs) -> str:
    """`dxf` with the IFC data of `blobs` on the hidden layer (see the module doc)."""
    tags, eol = _tags_of(dxf)
    seed_at, acadver = -1, ""
    for i in range(len(tags) - 1):
        if _is(tags[i], "9", "$HANDSEED") and _code(tags[i + 1]) == "5":
            seed_at = i + 1
        if _is(tags[i], "9", "$ACADVER"):
            acadver = tags[i + 1][1].strip().upper()
        if _is(tags[i], "0", "ENDSEC") and seed_at >= 0:
            break
    handles = seed_at >= 0
    subclass = handles and acadver != "" and acadver > "AC1009"
    nxt = [0]
    if handles:
        try:
            nxt[0] = int(tags[seed_at][1].strip(), 16) or 1
        except ValueError:
            nxt[0] = 1
        for t in tags:
            if _code(t) in ("5", "105"):
                try:
                    h = int(t[1].strip(), 16)
                except ValueError:
                    continue
                if h >= nxt[0]:
                    nxt[0] = h + 1

    def handle():
        h = "%X" % nxt[0]
        nxt[0] += 1
        return h

    hf = handle if handles else None

    def style(owner=None):
        return _Style(hf, subclass, owner)

    tables_at = _section_at(tags, "TABLES")
    if tables_at < 0:
        header_at = _section_at(tags, "HEADER")
        at = _endsec_from(tags, header_at) + 1 if header_at >= 0 else 0
        tags[at:at] = [["  0", "SECTION"], ["  2", "TABLES"], ["  0", "ENDSEC"]]
        tables_at = at
    tables_end = _endsec_from(tags, tables_at)

    layer = _table_at(tags, tables_at, tables_end, "LAYER")
    if not layer:
        t = _from_text(_pair("  0", "TABLE") + _pair("  2", "LAYER") + (_pair("  5", hf()) if hf else "")
                       + ((_pair("330", "0") + _pair("100", "AcDbSymbolTable")) if subclass else "")
                       + _pair(" 70", 0) + _pair("  0", "ENDTAB"))
        tags[tables_end:tables_end] = t
        tables_end += len(t)
        layer = _table_at(tags, tables_at, tables_end, "LAYER")
    if not any(r["name"].upper() == LAYER for r in _records_of(tags, layer, "LAYER")):
        t = _from_text(layer_record(style(_table_handle(tags, layer[0]))))
        tags[layer[1]:layer[1]] = t
        _bump_count(tags, layer[0], 1)
        tables_end += len(t)

    appid = _table_at(tags, tables_at, tables_end, "APPID")
    if appid:
        names = [r["name"].upper() for r in _records_of(tags, appid, "APPID")]
        if APPID not in names:
            t = _from_text(appid_record(style(_table_handle(tags, appid[0]))))
            tags[appid[1]:appid[1]] = t
            _bump_count(tags, appid[0], 1)
            tables_end += len(t)
    else:
        own = hf() if hf else None
        t = _from_text(_pair("  0", "TABLE") + _pair("  2", "APPID") + (_pair("  5", own) if own else "")
                       + ((_pair("330", "0") + _pair("100", "AcDbSymbolTable")) if subclass else "")
                       + _pair(" 70", 2) + appid_record(style(own), "ACAD") + appid_record(style(own))
                       + _pair("  0", "ENDTAB"))
        br = _table_at(tags, tables_at, tables_end, "BLOCK_RECORD")
        at = br[0] if br else tables_end
        tags[at:at] = t
        tables_end += len(t)

    model = None
    br = _table_at(tags, tables_at, tables_end, "BLOCK_RECORD")
    if br:
        for r in _records_of(tags, br, "BLOCK_RECORD"):
            if r["name"].upper() == "*MODEL_SPACE":
                model = r["handle"]
                break
    ent_at = _section_at(tags, "ENTITIES")
    if ent_at < 0:
        eof = next((i for i, t in enumerate(tags) if _is(t, "0", "EOF")), -1)
        if eof < 0:
            tags.append(["  0", "EOF"])
            eof = len(tags) - 1
        tags[eof:eof] = [["  0", "SECTION"], ["  2", "ENTITIES"], ["  0", "ENDSEC"]]
        ent_at = eof
    ent_end = _endsec_from(tags, ent_at)
    ent_end -= _drop_carriers(tags, ent_at + 2, ent_end)
    tags[ent_end:ent_end] = _from_text(carriers(blobs, style(model)))

    if handles:
        tags[seed_at] = [tags[seed_at][0], "%X" % (nxt[0] + 16)]
    return "".join(f"{c}{eol}{v}{eol}" for c, v in tags)


def read_ifc_dxf(dxf: str) -> list:
    """Every object a DXF carries on the hidden layer, parts joined, in first-named order."""
    tags, _ = _tags_of(dxf)
    parts = {}
    for i, t in enumerate(tags):
        if not _is(t, "1001", APPID):
            continue
        s = i
        while s > 0 and _code(tags[s]) != "0":
            s -= 1
        layer, x, y = "", 0.0, 0.0
        for k in range(s + 1, i):
            c = _code(tags[k])
            if c == "8":
                layer = tags[k][1].strip().upper()
            elif c == "10":
                x = float(tags[k][1])
            elif c == "20":
                y = float(tags[k][1])
        if layer != LAYER:
            continue
        vals = []
        k = i + 1
        while k < len(tags) and _code(tags[k]) not in ("0", "1001"):
            vals.append((_code(tags[k]), tags[k][1]))
            k += 1
        if len(vals) < 4 or vals[0][1].strip() != FORMAT or vals[1][0] != "1000":
            continue
        guid = vals[1][1].strip()
        try:
            part, count = int(vals[2][1].strip()), int(vals[3][1].strip())
        except ValueError:
            continue
        if part < 1 or count < part:
            continue
        text = "".join(v for c, v in vals[4:] if c == "1000")
        slot = parts.setdefault(guid, {"n": count, "got": {}, "at": {"x": x, "y": y}})
        slot["got"][part] = text
    out = []
    for guid, slot in parts.items():
        if len(slot["got"]) != slot["n"]:
            raise ValueError(f"apk-ifc-dxf: {guid} has {len(slot['got'])} of {slot['n']} parts")
        blob = decode_payload("".join(slot["got"][k] for k in range(1, slot["n"] + 1)))
        blob["at"] = slot["at"]
        out.append(blob)
    return out
