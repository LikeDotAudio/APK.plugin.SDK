# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Trunk-Caster on the bus — the Python twin of ``contracts/rust/src/trunk.rs`` (ROAD-3440).

The Ward-Beck line, frame / port / trunk identities, panel labels, the caster's
command envelope and every ``trunk`` / ``trunk_sim`` topic. The topic table and
payload shapes are ``contracts/TRUNK.md``; all three twins are held to
``contracts/vectors/trunk.json``.

    APK.audio/System/Protocols/trunk/…                  the caster (PLUGIN.TRUNK_CASTER)
    APK.audio/System/Protocols/trunk_sim/{protocol}/…   one frame simulator per protocol (PLUGIN.TRUNK_SIM)
"""
from __future__ import annotations

import json
import math
import re
from typing import Any, Dict, List, Optional, Tuple

from .segments import is_segment
from .topics import ROOT

TOKEN = "trunk"
SIM_TOKEN = "trunk_sim"
WIRE_VERSION = 1
CHECKIN = "CHECKIN"
VOICE_CUTS = (1, 5)
EVENT_KINDS = ("Crosspoint", "Gpio", "Label", "Gain", "Relay")
VIEW_NAMES = ("Status", "Report", "Regions", "PhoneBook", "Bandwidth", "Captures", "Presets", "Productions", "Globe")
VIEW_ITEM_NAMES = ("Taps", "Capture")
LABEL_WIDTHS = (6, 8, 16)
TIERS = ("capital", "province", "city")
FAMILIES = ("analog-audio", "audio-over-ip", "telephony", "control", "wireless", "media-transport", "other")
BOOKING_STATUSES = ("booked", "on-air", "done", "failed", "missed")
PORT_KINDS = ("panel", "input")
GPIO_FUNCTIONS = ("blink-panel", "check-lights", "set-latch", "set-listen", "unset-latch", "clear",
                  "last-caller", "ping")

STATUS_DONE = "done"
STATUS_REFUSED = "refused"
STATUS_ERROR = "error"
STATUSES = (STATUS_DONE, STATUS_REFUSED, STATUS_ERROR)

COMMAND_KEYS = ("id", "verb", "args", "issued_at_ms")

# ---------------------------------------------------------------------------
# The Ward-Beck line
# ---------------------------------------------------------------------------

LINE_FIELDS = ("site", "frame", "input", "input_name", "switch", "output_name", "output", "out_frame", "out_site")


def _split_csv(s: str) -> List[str]:
    out, cur, in_q, i = [], [], False, 0
    while i < len(s):
        c = s[i]
        if c == '"' and in_q and i + 1 < len(s) and s[i + 1] == '"':
            cur.append('"')
            i += 2
            continue
        if c == '"':
            in_q = not in_q
        elif c == "," and not in_q:
            out.append("".join(cur).strip())
            cur = []
        else:
            cur.append(c)
        i += 1
    out.append("".join(cur).strip())
    return out


def _q(s: str) -> str:
    if "," in s or '"' in s:
        return '"' + s.replace('"', '""') + '"'
    return s


def parse_line(text: str) -> Dict[str, str]:
    """One line as a dict with the nine fields; ``switch`` is ``CLOSE`` or ``OPEN``. Raises ValueError."""
    f = _split_csv(text.strip())
    if len(f) != 9:
        raise ValueError(f"expected 9 fields, got {len(f)}")
    switch = f[4].upper()
    if switch not in ("CLOSE", "OPEN"):
        raise ValueError(f"field 5 must be CLOSE or OPEN, got {f[4]}")
    f[4] = switch
    return dict(zip(LINE_FIELDS, f))


def render_line(line: Dict[str, str]) -> str:
    return ",".join(line[k] if k == "switch" else _q(line[k]) for k in LINE_FIELDS)


def checkin_line(site: str, frame: str, when: str, closed: int) -> Dict[str, str]:
    return {"site": site, "frame": frame, "input": CHECKIN, "input_name": when,
            "switch": "CLOSE" if closed > 0 else "OPEN", "output_name": f"{closed} crosspoints",
            "output": str(closed), "out_frame": frame, "out_site": site}


def is_checkin(line: Dict[str, str]) -> bool:
    return line["input"] == CHECKIN


def is_local(line: Dict[str, str]) -> bool:
    return line["frame"] == line["out_frame"] and line["site"] == line["out_site"]


def flipped(line: Dict[str, str]) -> Dict[str, str]:
    return {**line, "switch": "OPEN" if line["switch"] == "CLOSE" else "CLOSE"}


def render_reject(reason: str, line: str) -> str:
    return f"{reason.replace('|', '/')}|{line}"


def parse_reject(payload: str) -> Optional[Tuple[str, str]]:
    if "|" not in payload:
        return None
    reason, line = payload.split("|", 1)
    return reason, line

# ---------------------------------------------------------------------------
# Batches: the line twin and the ack, many lines a message (0.6.0, ROAD-3457)
# ---------------------------------------------------------------------------

BATCH_MAX = 500
LINE_BATCH_KEY = "lines"
ACK_BATCH_KEY = "acks"


def _render_batch(key: str, entries: List[str]) -> str:
    return json.dumps({key: list(entries)}, separators=(",", ":"), ensure_ascii=False)


def _parse_batch(key: str, payload: str) -> List[str]:
    text = payload.strip()
    if not text:
        raise ValueError("an empty payload is no batch")
    if not text.startswith("{"):
        return [text]  # a 0.5.0 writer's single bare line: a batch of one
    try:
        doc = json.loads(text)
    except ValueError as e:
        raise ValueError(f"not JSON: {e}") from None
    entries = doc.get(key) if isinstance(doc, dict) else None
    if not isinstance(entries, list):
        raise ValueError(f"no `{key}` array")
    if not entries:
        raise ValueError(f"`{key}` is empty: an empty batch is never sent")
    if len(entries) > BATCH_MAX:
        raise ValueError(f"`{key}` holds {len(entries)} entries, more than {BATCH_MAX}")
    out = []
    for e in entries:
        if not isinstance(e, str) or not e.strip():
            raise ValueError(f"a `{key}` entry is not a non-empty string")
        out.append(e.strip())
    return out


def render_line_batch(lines: List[str]) -> str:
    """The ``…/line`` payload ``{"lines": [...]}``: one frame's twins of one flush, in TCP order."""
    return _render_batch(LINE_BATCH_KEY, lines)


def render_ack_batch(acks: List[str]) -> str:
    """The ``…/ack`` payload ``{"acks": [...]}``: one frame's applied lines of one flush, in order."""
    return _render_batch(ACK_BATCH_KEY, acks)


def parse_line_batch(payload: str) -> List[str]:
    """The lines of a ``…/line`` payload (a bare 0.5.0 line reads as one). Raises ValueError."""
    return _parse_batch(LINE_BATCH_KEY, payload)


def parse_ack_batch(payload: str) -> List[str]:
    """The applied lines of a ``…/ack`` payload (a bare 0.5.0 line reads as one). Raises ValueError."""
    return _parse_batch(ACK_BATCH_KEY, payload)

# ---------------------------------------------------------------------------
# Protocol batches: one message per PROTOCOL per flush (0.7.0, ROAD-3467)
# ---------------------------------------------------------------------------

PROTOCOL_LEVEL = "Frames"
PROTOCOL_LEAVES = ("line", "ack", "state")


def _render_frames_batch(key: str, frames: Dict[str, List[str]]) -> str:
    by = {f: list(frames[f]) for f in sorted(frames) if frames[f]}
    return json.dumps({key: by}, separators=(",", ":"), ensure_ascii=False)


def _parse_frames_batch(key: str, payload: str) -> Dict[str, List[str]]:
    text = payload.strip()
    if not text:
        raise ValueError("an empty payload is no batch")
    try:
        doc = json.loads(text)
    except ValueError as e:
        raise ValueError(f"not JSON: {e}") from None
    by = doc.get(key) if isinstance(doc, dict) else None
    if not isinstance(by, dict):
        raise ValueError(f"no `{key}` object of frames")
    if not by:
        raise ValueError(f"`{key}` names no frame: an empty batch is never sent")
    out: Dict[str, List[str]] = {}
    total = 0
    for frame in sorted(by):
        if not is_segment(frame):
            raise ValueError(f"`{frame}` is not a frame id (one topic segment)")
        entries = by[frame]
        if not isinstance(entries, list) or not entries:
            raise ValueError(f"`{key}.{frame}` is not a non-empty array")
        total += len(entries)
        if total > BATCH_MAX:
            raise ValueError(f"`{key}` holds more than {BATCH_MAX} entries")
        lines = []
        for e in entries:
            if not isinstance(e, str) or not e.strip():
                raise ValueError(f"a `{key}.{frame}` entry is not a non-empty string")
            lines.append(e.strip())
        out[frame] = lines
    return out


def render_protocol_line_batch(frames: Dict[str, List[str]]) -> str:
    """The ``…/Frames/line`` payload ``{"lines": {"<frame>": [...]}}``: a protocol's twins of one flush, per frame in TCP order."""
    return _render_frames_batch(LINE_BATCH_KEY, frames)


def render_protocol_ack_batch(frames: Dict[str, List[str]]) -> str:
    """The ``…/Frames/ack`` payload ``{"acks": {"<frame>": [...]}}``: a protocol's applied lines of one flush."""
    return _render_frames_batch(ACK_BATCH_KEY, frames)


def parse_protocol_line_batch(payload: str) -> Dict[str, List[str]]:
    """Frame -> its twins of a ``…/Frames/line`` payload. Raises ValueError."""
    return _parse_frames_batch(LINE_BATCH_KEY, payload)


def parse_protocol_ack_batch(payload: str) -> Dict[str, List[str]]:
    """Frame -> its applied lines of a ``…/Frames/ack`` payload. Raises ValueError."""
    return _parse_frames_batch(ACK_BATCH_KEY, payload)


def render_protocol_state(protocol: str, frames: Dict[str, Any]) -> str:
    """The RETAINED ``…/Frames/state`` document: every hosted frame's whole SimFrame, keyed by frame."""
    by = {f: frames[f] for f in sorted(frames)}
    return json.dumps({"wire_version": WIRE_VERSION, "protocol": protocol, "frames": by}, separators=(",", ":"), ensure_ascii=False)


def parse_protocol_state(payload: str) -> Tuple[str, Dict[str, Dict[str, Any]]]:
    """(protocol, frame -> SimFrame) of a ``…/Frames/state`` document. Raises ValueError."""
    try:
        doc = json.loads(payload.strip())
    except ValueError as e:
        raise ValueError(f"not JSON: {e}") from None
    protocol = doc.get("protocol") if isinstance(doc, dict) else None
    if not isinstance(protocol, str) or not is_segment(protocol):
        raise ValueError("no `protocol` segment")
    by = doc.get("frames")
    if not isinstance(by, dict) or not by:
        raise ValueError("no `frames`: a protocol hosting none is erased")
    out: Dict[str, Dict[str, Any]] = {}
    for frame in sorted(by):
        if not is_segment(frame):
            raise ValueError(f"`{frame}` is not a frame id")
        sf = by[frame]
        if not isinstance(sf, dict):
            raise ValueError(f"frames.{frame} is not a SimFrame object")
        if ("frame" in sf and sf["frame"] != frame) or ("protocol" in sf and sf["protocol"] != protocol):
            raise ValueError(f"frames.{frame} names another frame or protocol")
        out[frame] = sf
    return protocol, out

# ---------------------------------------------------------------------------
# The per-frame document: one retained word per frame (0.8.0, ROAD-3467)
# ---------------------------------------------------------------------------

FRAME_DOC_LEVEL = "Frame"


def render_frame_doc(doc: Dict[str, Any]) -> str:
    """The ``trunk/Frame/{frame}`` payload from ``{frame, device?, trunks, connections, crosspoints?}``.

    ``device`` / ``crosspoints`` are left out when None; ``trunks`` / ``connections`` are always written, sorted by id.
    """
    out: Dict[str, Any] = {"wire_version": WIRE_VERSION, "frame": doc["frame"]}
    if doc.get("device") is not None:
        out["device"] = doc["device"]
    trunks = doc.get("trunks") or {}
    conns = doc.get("connections") or {}
    out["trunks"] = {k: trunks[k] for k in sorted(trunks)}
    out["connections"] = {k: conns[k] for k in sorted(conns)}
    if doc.get("crosspoints") is not None:
        out["crosspoints"] = doc["crosspoints"]
    return json.dumps(out, separators=(",", ":"), ensure_ascii=False)


def _frame_doc_entries(doc: Dict[str, Any], key: str, owner: str, frame: str) -> Dict[str, Dict[str, Any]]:
    v = doc.get(key)
    if v is None:
        return {}
    if not isinstance(v, dict):
        raise ValueError(f"`{key}` is not an object keyed by id")
    out: Dict[str, Dict[str, Any]] = {}
    for i in sorted(v):
        if not is_segment(i):
            raise ValueError(f"`{key}.{i}` is not an id (one topic segment)")
        e = v[i]
        if not isinstance(e, dict):
            raise ValueError(f"`{key}.{i}` is not an object")
        if "id" in e and e["id"] != i:
            raise ValueError(f"`{key}.{i}` names another id")
        if owner in e and e[owner] != frame:
            raise ValueError(f"`{key}.{i}` belongs in its `{owner}`'s document, not `{frame}`'s")
        out[i] = e
    return out


def parse_frame_doc(payload: str) -> Dict[str, Any]:
    """A ``trunk/Frame/{frame}`` document read back: ``{frame, device, trunks, connections, crosspoints}``
    (missing maps empty, missing device / crosspoints None). Raises ValueError on the refusals TRUNK.md lists."""
    text = payload.strip()
    if not text:
        raise ValueError("an empty payload is the frame's erase, not a document")
    try:
        doc = json.loads(text)
    except ValueError as e:
        raise ValueError(f"not JSON: {e}") from None
    if not isinstance(doc, dict):
        raise ValueError("not a JSON object")
    frame = doc.get("frame")
    if not isinstance(frame, str) or not is_segment(frame):
        raise ValueError("no `frame` segment")
    device = doc.get("device")
    if device is not None:
        if not isinstance(device, dict):
            raise ValueError("`device` is not a Frame object")
        if "id" in device and device["id"] != frame:
            raise ValueError("`device` names another frame")
    trunks = _frame_doc_entries(doc, "trunks", "a_frame", frame)
    conns = _frame_doc_entries(doc, "connections", "from_frame", frame)
    xp = doc.get("crosspoints")
    if xp is not None:
        if not isinstance(xp, dict):
            raise ValueError("`crosspoints` is not a Crosspoints object")
        if "frame" in xp and xp["frame"] != frame:
            raise ValueError("`crosspoints` names another frame")
        if "closed" in xp and not isinstance(xp["closed"], list):
            raise ValueError("`crosspoints.closed` is not an array")
    if device is None and xp is None and not trunks and not conns:
        raise ValueError("a frame with nothing to say is erased, not stated")
    return {"frame": frame, "device": device, "trunks": trunks, "connections": conns, "crosspoints": xp}

# ---------------------------------------------------------------------------
# Identities
# ---------------------------------------------------------------------------


def user_port(n: int) -> str:
    return f"U{n:02d}"


def caster_port(n: int) -> str:
    return f"C{n}"


def parse_port(s: str) -> Optional[Tuple[str, int]]:
    """``U05`` -> ("user", 5); ``C3`` -> ("caster", 3). Only the canonical spelling."""
    m = re.fullmatch(r"([UC])([0-9]+)", s or "")
    if not m:
        return None
    kind, n = ("user" if m.group(1) == "U" else "caster"), int(m.group(2))
    canonical = user_port(n) if kind == "user" else caster_port(n)
    return (kind, n) if canonical == s else None


def frame_id(prefix: str, n: int) -> str:
    return f"{prefix}-{n:02d}"


def trunk_id(n: int) -> str:
    return f"trunk-{n:05d}"


def slugify(name: str) -> str:
    chars = "".join(c.lower() if (c.isascii() and c.isalnum()) else "-" for c in name)
    return "-".join(p for p in chars.split("-") if p)


def is_channel(index: int, capacity: int) -> bool:
    return 0 <= index < capacity

# ---------------------------------------------------------------------------
# Panel labels
# ---------------------------------------------------------------------------


def _is_combining(c: str) -> bool:
    u = ord(c)
    return (0x0300 <= u <= 0x036F or 0x1AB0 <= u <= 0x1AFF or 0x1DC0 <= u <= 0x1DFF
            or 0x20D0 <= u <= 0x20FF or 0xFE00 <= u <= 0xFE0F or 0xFE20 <= u <= 0xFE2F
            or 0x1F3FB <= u <= 0x1F3FF or 0xE0020 <= u <= 0xE007F or u in (0x200C, 0x200D))


def _is_ri(c: str) -> bool:
    return 0x1F1E6 <= ord(c) <= 0x1F1FF


def graphemes(s: str) -> List[str]:
    out: List[str] = []
    join_next = pending_flag = False
    for c in s:
        glue = join_next or _is_combining(c) or (pending_flag and _is_ri(c)) or (c == "\n" and bool(out) and out[-1] == "\r")
        if glue and out:
            out[-1] += c
        else:
            out.append(c)
        join_next = c == "‍"
        pending_flag = _is_ri(c) and not glue
    return out


def width(s: str) -> int:
    return len(graphemes(s))


def fit(s: str, n: int) -> str:
    return "".join(graphemes(s.strip())[:n])


def label(full: str, l6: Optional[str] = None, l8: Optional[str] = None, l16: Optional[str] = None) -> Dict[str, Any]:
    full = full.strip()
    g = graphemes(full)
    out = {"full": full, "l6": "".join(g[:6]), "l8": "".join(g[:8]), "l16": "".join(g[:16]), "chars": len(g),
           "over6": len(g) > 6, "over8": len(g) > 8, "over16": len(g) > 16}
    for key, value, n in (("l6", l6, 6), ("l8", l8, 8), ("l16", l16, 16)):
        if value is not None:
            out[key] = fit(value, n)
    return out

# ---------------------------------------------------------------------------
# The command envelope
# ---------------------------------------------------------------------------

# verb -> (route, required [(name, type)], optional [(name, type)], free)
# types: s non-empty string, i non-negative integer, n number, b bool, a array, o object
VERBS: Dict[str, Tuple[str, Tuple[Tuple[str, str], ...], Tuple[Tuple[str, str], ...], bool]] = {
    "connect": ("/api/request", (("who", "s"), ("from", "s"), ("to", "s")),
                (("from_port", "i"), ("to_port", "i"), ("why", "s"), ("where", "s"), ("hold_seconds", "n"), ("gpio", "a")), False),
    "release": ("/api/release", (("connection", "s"),), (), False),
    "rename": ("/api/rename", (("frame", "s"), ("port", "i"), ("name", "s")), (("l6", "s"), ("l8", "s"), ("l16", "s")), False),
    "book": ("/api/bookings", (("frame", "s"), ("port", "i"), ("program", "s"), ("start_hour", "n"), ("minutes", "n")),
             (("kind", "s"), ("who", "s"), ("to", "s"), ("daily", "b")), False),
    "cancel_booking": ("/api/bookings/cancel", (("booking", "s"),), (), False),
    "crosspoint_reset": ("/api/crosspoints/reset", (), (), False),
    "crosspoint_settings": ("/api/crosspoints/settings", (), (), True),
    "fader": ("/api/fader", (), (), True),
    "fader_preset": ("/api/fader/preset", (("preset", "s"),), (), False),
    "fader_quick": ("/api/fader/quick", (), (("speed", "n"), ("running", "b")), False),
    "clock": ("/api/clock", (("hour", "n"),), (), False),
    "gain": ("/api/gain", (("db", "n"),), (("frame", "s"), ("input", "s"), ("output", "s"), ("trunk", "s"), ("end", "s"),
                                            ("connection", "s"), ("hop", "i"), ("who", "s")), False),
    "taps_unity": ("/api/taps/unity", (), (("frame", "s"), ("who", "s")), False),
    "relay_bind": ("/api/relay/bind", (("matrix", "s"), ("port", "i"), ("device_id", "s"), ("device_label", "s"), ("kind", "s")), (), False),
    "relay_unbind": ("/api/relay/unbind", (("matrix", "s"), ("port", "i")), (), False),
    "relay_event": ("/api/relay/event", (("kind", "s"), ("device_label", "s")),
                    (("matrix", "s"), ("port", "i"), ("peak_dbfs", "n"), ("threshold_dbfs", "n"), ("note", "s")), False),
    "gpio": ("/api/gpio", (("pin", "i"), ("state", "b")), (("connection", "s"), ("function", "s"), ("note", "s")), False),
    "panel_seek": ("/api/panel-seek", (("who", "s"), ("from", "s"), ("panel_frame", "s")), (("panel_port", "i"), ("why", "s")), False),
    "capture_start": ("/api/capture/start", (), (("note", "s"),), False),
    "capture_stop": ("/api/capture/stop", (), (), False),
    "globe_config": ("/api/globe/config", (), (), True),
}

_ID = re.compile(r"[A-Za-z0-9._:-]{1,80}")
_TYPE_WORD = {"s": "a non-empty string", "i": "a non-negative integer", "n": "a number", "b": "true or false",
              "a": "an array", "o": "an object"}
_GAIN_POINTS = (("frame", "input", "output"), ("trunk", "end"), ("connection", "hop"))


class Refused(ValueError):
    """A body that is not a command; ``id`` is the id to answer against, if one could be read."""

    def __init__(self, id: Optional[str], reason: str):
        super().__init__(reason)
        self.id = id
        self.reason = reason


def is_command_id(s: Any) -> bool:
    return isinstance(s, str) and bool(_ID.fullmatch(s))


def _number(v: Any) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)


def _int(v: Any) -> bool:
    return _number(v) and v >= 0 and float(v).is_integer() and v <= 9007199254740991


def _type_ok(v: Any, t: str) -> bool:
    return {
        "s": lambda: isinstance(v, str) and v.strip() != "",
        "i": lambda: _int(v),
        "n": lambda: _number(v),
        "b": lambda: isinstance(v, bool),
        "a": lambda: isinstance(v, list),
        "o": lambda: isinstance(v, dict),
    }[t]()


def _rules(verb: str, a: Dict[str, Any]) -> Optional[str]:
    if verb == "book":
        if not (0 <= a["start_hour"] < 24):
            return "`start_hour` is 0 ≤ h < 24, the frame's local time of day"
        if a["minutes"] <= 0:
            return "`minutes` must be more than 0"
        if "kind" in a and a["kind"] not in PORT_KINDS:
            return f"`kind` is one of {PORT_KINDS}"
    elif verb == "clock":
        if not (0 <= a["hour"] < 24):
            return "`hour` is 0 ≤ h < 24"
    elif verb == "fader_quick":
        if "speed" not in a and "running" not in a:
            return "`fader_quick` needs `speed`, `running` or both"
        if "speed" in a and a["speed"] < 0:
            return "`speed` cannot be negative"
    elif verb == "gpio":
        if not (1 <= a["pin"] <= 8):
            return "`pin` is 1–8"
        if "function" in a and a["function"] not in GPIO_FUNCTIONS:
            return f"`function` is one of {GPIO_FUNCTIONS}"
    elif verb == "gain":
        given = sum(1 for g in _GAIN_POINTS for k in g if k in a)
        whole = [g for g in _GAIN_POINTS if all(k in a for k in g)]
        if len(whole) != 1 or len(whole[0]) != given:
            return "`gain` names exactly one point: frame+input+output, trunk+end, or connection+hop"
        if "end" in a and a["end"] not in ("a", "b"):
            return "`end` is `a` or `b`"
    return None


def parse_command(payload: Any) -> Dict[str, Any]:
    """Read a ``trunk/outgoing/command`` payload into ``{id, verb, args, issued_at_ms?}``. Raises Refused."""
    try:
        doc = json.loads(payload)
    except (ValueError, TypeError):
        raise Refused(None, "not JSON")
    if not isinstance(doc, dict):
        raise Refused(None, "not a JSON object")
    id = doc.get("id") if is_command_id(doc.get("id")) else None
    for k in doc:
        if k not in COMMAND_KEYS:
            raise Refused(id, f"unknown key `{k}`")
    if id is None:
        raise Refused(None, "no `id` (1–80 of A-Z a-z 0-9 . _ : -)")
    verb = doc.get("verb")
    if not isinstance(verb, str) or verb not in VERBS:
        raise Refused(id, f"unknown verb `{verb}`")
    args = doc.get("args", {})
    if not isinstance(args, dict):
        raise Refused(id, "`args` is not an object")
    issued = doc.get("issued_at_ms")
    if issued is not None and not _int(issued):
        raise Refused(id, "`issued_at_ms` is not unix ms")
    _, required, optional, free = VERBS[verb]
    if not free:
        names = {n for n, _ in required + optional}
        for k in args:
            if k not in names:
                raise Refused(id, f"`{verb}` takes no argument `{k}`")
        for name, t in required:
            if name not in args:
                raise Refused(id, f"`{verb}` needs `{name}`")
            if not _type_ok(args[name], t):
                raise Refused(id, f"`{name}` is not {_TYPE_WORD[t]}")
        for name, t in optional:
            if name in args and not _type_ok(args[name], t):
                raise Refused(id, f"`{name}` is not {_TYPE_WORD[t]}")
        why = _rules(verb, args)
        if why:
            raise Refused(id, why)
    out: Dict[str, Any] = {"id": id, "verb": verb, "args": args}
    if issued is not None:
        out["issued_at_ms"] = int(issued)
    return out


def command_to_json(command: Dict[str, Any]) -> str:
    out = {"id": command["id"], "verb": command["verb"], "args": command.get("args", {})}
    if command.get("issued_at_ms") is not None:
        out["issued_at_ms"] = command["issued_at_ms"]
    return json.dumps(out, ensure_ascii=False)


def result(id: Optional[str], verb: Optional[str], status: str, detail: str, result: Any = None) -> str:
    """The answer on ``trunk/incoming/result``."""
    out: Dict[str, Any] = {"id": id, "verb": verb, "status": status, "ok": status == STATUS_DONE, "detail": detail}
    if result is not None:
        out["result"] = result
    return json.dumps(out, ensure_ascii=False)

# ---------------------------------------------------------------------------
# Topics — (topic, retained)
# ---------------------------------------------------------------------------


def _topic(token: str, path: List[str], retain: bool) -> Tuple[str, bool]:
    for s in [token, *path]:
        if not is_segment(s):
            raise ValueError(f"not a topic segment: {s!r}")
    return "/".join([ROOT, "System", "Protocols", token, *path]), retain


def frame_doc_topic(frame: str) -> Tuple[str, bool]:
    """``trunk/Frame/{frame}``: the frame's whole state folded, RETAINED (0.8.0)."""
    return _topic(TOKEN, [FRAME_DOC_LEVEL, frame], True)


def frame_topic(frame: str) -> Tuple[str, bool]:
    """0.7.0 ``trunk/Device/Frame/{frame}``; folded into frame_doc_topic since 0.8.0, read for one release."""
    return _topic(TOKEN, ["Device", "Frame", frame], True)


def trunk_topic(trunk: str) -> Tuple[str, bool]:
    return _topic(TOKEN, ["Trunk", trunk], True)


def connection_topic(connection: str) -> Tuple[str, bool]:
    return _topic(TOKEN, ["Connection", connection], True)


def crosspoints_topic(frame: str) -> Tuple[str, bool]:
    return _topic(TOKEN, ["Crosspoint", frame], True)


def booking_topic(booking: str) -> Tuple[str, bool]:
    return _topic(TOKEN, ["Booking", booking], True)


def site_topic(slug: str) -> Tuple[str, bool]:
    return _topic(TOKEN, ["Site", slug], True)


def relay_binding_topic(matrix: str, port: int) -> Tuple[str, bool]:
    return _topic(TOKEN, ["Relay", matrix, str(int(port))], True)


def event_topic(kind: str) -> Tuple[str, bool]:
    if kind not in EVENT_KINDS:
        raise ValueError(f"trunk event kind: {kind!r}")
    return _topic(TOKEN, ["Event", kind], False)


def view_topic(name: str) -> Tuple[str, bool]:
    if name not in VIEW_NAMES:
        raise ValueError(f"trunk view: {name!r}")
    return _topic(TOKEN, ["View", name], True)


def view_item_topic(name: str, key: str) -> Tuple[str, bool]:
    if name not in VIEW_ITEM_NAMES:
        raise ValueError(f"trunk keyed view: {name!r}")
    return _topic(TOKEN, ["View", name, key], True)


def command_topic() -> Tuple[str, bool]:
    return _topic(TOKEN, ["outgoing", "command"], False)


def result_topic() -> Tuple[str, bool]:
    return _topic(TOKEN, ["incoming", "result"], False)


def sim_frame_topic(protocol: str, frame: str) -> Tuple[str, bool]:
    return _topic(SIM_TOKEN, [protocol, "Frame", frame, "state"], True)


def line_topic(protocol: str, frame: str) -> Tuple[str, bool]:
    return _topic(SIM_TOKEN, [protocol, "Frame", frame, "line"], False)


def ack_topic(protocol: str, frame: str) -> Tuple[str, bool]:
    return _topic(SIM_TOKEN, [protocol, "Frame", frame, "ack"], False)


def reject_topic(protocol: str, frame: str) -> Tuple[str, bool]:
    return _topic(SIM_TOKEN, [protocol, "Frame", frame, "reject"], False)


def voice_topic(protocol: str, frame: str, cut: int) -> Tuple[str, bool]:
    if cut not in VOICE_CUTS:
        raise ValueError(f"voice cut: {cut!r}")
    return _topic(SIM_TOKEN, [protocol, "Frame", frame, f"voice{cut}"], False)


def protocol_line_topic(protocol: str) -> Tuple[str, bool]:
    return _topic(SIM_TOKEN, [protocol, PROTOCOL_LEVEL, "line"], False)


def protocol_ack_topic(protocol: str) -> Tuple[str, bool]:
    return _topic(SIM_TOKEN, [protocol, PROTOCOL_LEVEL, "ack"], False)


def protocol_state_topic(protocol: str) -> Tuple[str, bool]:
    return _topic(SIM_TOKEN, [protocol, PROTOCOL_LEVEL, "state"], True)


def caster_wildcard() -> str:
    return _topic(TOKEN, [], False)[0] + "/#"


def sim_wildcard(protocol: Optional[str] = None) -> str:
    return _topic(SIM_TOKEN, [protocol] if protocol else [], False)[0] + "/#"


def parse_sim_topic(topic: str) -> Optional[Tuple[str, str, str]]:
    head = _topic(SIM_TOKEN, [], False)[0] + "/"
    if not topic.startswith(head):
        return None
    parts = topic[len(head):].split("/")
    if len(parts) == 4 and parts[1] == "Frame" and all(parts):
        return parts[0], parts[2], parts[3]
    return None


def parse_sim_protocol_topic(topic: str) -> Optional[Tuple[str, str]]:
    """Split ``…/trunk_sim/{protocol}/Frames/{line|ack|state}`` into (protocol, leaf); anything else is None."""
    head = _topic(SIM_TOKEN, [], False)[0] + "/"
    if not topic.startswith(head):
        return None
    parts = topic[len(head):].split("/")
    if len(parts) == 3 and parts[0] and parts[1] == PROTOCOL_LEVEL and parts[2] in PROTOCOL_LEAVES:
        return parts[0], parts[2]
    return None


def parse_frame_doc_topic(topic: str) -> Optional[str]:
    """The frame of a ``trunk/Frame/{frame}`` topic; anything else is None."""
    head = _topic(TOKEN, [], False)[0] + "/" + FRAME_DOC_LEVEL + "/"
    if not topic.startswith(head):
        return None
    frame = topic[len(head):]
    return frame if is_segment(frame) else None
