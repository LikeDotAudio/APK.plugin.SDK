# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""`apk_plugin_panels` — the Python twin of RUNNER/src/panels.rs (audit 2026-10-30).

Every device a plugin finds presents its own panel, retained at
`APK.audio/System/Gui/Panels/<key>`. A Rust plugin gets that from
`main!(…, panels = [family])`; a Python plugin gets it from

    runner.main(token="bluetooth", …, panels=["bluetooth"])        # gui/ beside config.ini

and the runner's session then does what the Rust session does: subscribe to
`…/Protocols/<token>/Device/#` and `Gui/Panels/+`, feed every message to a
`Book`, publish what it returns RETAINED, restate the book on every connect,
and expire it on the telemetry tick. The plugin publishes its device leaves
(`model`, `name`, `serial`, `last_online`, …) and nothing else.

ONE IMPLEMENTATION PER LANGUAGE, ONE BEHAVIOUR. Same manifest keys
(`tree`, `device_depth`, `device_path`, `device_document`, `key_by`), same binding
(`${model}` `${family}` `${slug}` `${resource}` `${name}`, `device_state`,
`device_control`, `device_doc`), same document shape, same rules for when a
panel leaves (offline at once; silent after `panels_expire_after_s`; an
earlier process's panel no device claims after ADOPT_GRACE). Both halves run
RUNNER/vectors/panels.json (`test_apk_plugin_panels.py` and panels.rs's
`the_shared_vectors_hold`), so the two cannot drift without a test failing.
"""

from __future__ import annotations

import json
import os
import sys
import threading
from typing import Any, Dict, List, Optional, Tuple

def _contract_families():
    """`apkaudio_contracts.families` when importable (the image, the monorepo),
    else None — SDK tags before sdk-v0.9 carry runner/ and not contracts/python,
    and a probe must start either way (the rule apk_plugin_api.py states)."""
    try:
        from apkaudio_contracts import families
        return families
    except ImportError:
        candidate = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                  *[".."] * 6, "contracts", "python"))
        if os.path.isdir(candidate) and candidate not in sys.path:
            sys.path.append(candidate)
            try:
                from apkaudio_contracts import families
                return families
            except ImportError:
                pass
    return None


_FAMILIES = _contract_families()

# The contract's guiPanels / protocols families when they are importable; the
# same strings spelled out otherwise (test_apk_plugin_panels.py pins them to
# the Rust contract either way).
PANELS_TOPIC_ROOT = (_FAMILIES.topic("guiPanels", key="_").rsplit("/", 1)[0] if _FAMILIES
                     else "APK.audio/System/Gui/Panels")
MODULE_ROOT = (_FAMILIES.topic("protocols", proto="_", path=[]).rsplit("/", 1)[0] if _FAMILIES
               else "APK.audio/System/Protocols")


def panel_topic(key: str) -> str:
    """`APK.audio/System/Gui/Panels/<key>` through the contract builder."""
    if _FAMILIES:
        try:
            return _FAMILIES.topic("guiPanels", key=key)
        except Exception:                                # noqa: BLE001 — never stop a hunter
            pass
    return "%s/%s" % (PANELS_TOPIC_ROOT, key)
DEVICE_CONTROL_LEAF = "Control"
# presence::ADOPT_GRACE in RUNNER/src/presence/mod.rs (the tests read it there).
ADOPT_GRACE_S = 120
ANY = "+"
# The device leaves the book reads, and so the ones a panel may be keyed by.
KEY_LEAVES = ("model", "name", "serial")

Publication = Tuple[str, str]


def sanitize_segment(raw: str) -> str:
    """`topics::sanitize_segment`: one legal topic level, `[A-Za-z0-9_-]+`."""
    cleaned = "".join(c if (c.isascii() and c.isalnum()) or c == "-" else "_" for c in raw.strip())
    trimmed = cleaned.rstrip("_")
    return trimmed or "_"


def is_online_now(raw: str) -> bool:
    """`presence::is_online_now`: a negative number is the present tense."""
    try:
        return float(raw.strip()) < 0.0
    except ValueError:
        return False


def placement_of(tree: str) -> str:
    """`7_Device_Consoles` -> `Device_Consoles`."""
    n, sep, rest = tree.partition("_")
    return rest if sep and n and n.isdigit() else tree


class Template:
    """One family's template, parsed once."""

    def __init__(self, family: str, at: str, path: List[str], document: bool, body: Any,
                 key_by: Optional[str] = None, bim: Optional[Dict[str, Any]] = None):
        self.family, self.at, self.path, self.document, self.body = family, at, path, document, body
        self.key_by = key_by
        # The BIM part a presented panel names (ROAD-3565): the manifest entry's generated
        # `bim` key — the protocol's part, and the model parts that speak it, by folded model.
        bim = bim if isinstance(bim, dict) else {}
        self.bim_part = bim.get("part") if isinstance(bim.get("part"), str) else None
        models = bim.get("models") if isinstance(bim.get("models"), dict) else {}
        self.bim_models = {k: v for k, v in models.items() if isinstance(v, str)}

    def part_for(self, model: str) -> Optional[str]:
        """The part a device of this template is: its model's own part, else the protocol's."""
        return self.bim_models.get(fold_model(model)) or self.bim_part


def fold_model(model: str) -> str:
    """A model folded the way the BIM generator keys it: lower case, letters and digits only."""
    return "".join(c.lower() for c in str(model or "") if c.isascii() and c.isalnum())


def parse_template(family: str, manifest: str, template: str) -> Template:
    """`manifest.json` + `<family>.json` -> a Template; ValueError says what is wrong."""
    try:
        manifest_doc = json.loads(manifest)
    except ValueError as e:
        raise ValueError("gui/manifest.json: %s" % e)
    entry = manifest_doc.get(family) if isinstance(manifest_doc, dict) else None
    if entry is None:
        raise ValueError("gui/manifest.json declares no `%s`" % family)
    tree = entry.get("tree")
    at = placement_of(tree) if isinstance(tree, str) else "Instruments"
    depth = entry.get("device_depth")
    if depth is None:
        depth = 2
    elif isinstance(depth, bool) or not isinstance(depth, int) or depth not in (1, 2):
        raise ValueError("gui/manifest.json `%s.device_depth` is %s; it is 1 or 2" % (family, json.dumps(depth)))
    if "device_path" in entry:
        text = entry["device_path"]
        if not isinstance(text, str):
            raise ValueError("gui/manifest.json `%s.device_path` is %s; it is a string" % (family, json.dumps(text)))
        path = text.split("/")
        if any(s in ("", "#", DEVICE_CONTROL_LEAF) for s in path) or ANY not in path:
            raise ValueError("gui/manifest.json `%s.device_path` is %s; it is segments with at least one `+`"
                             % (family, json.dumps(text)))
    else:
        path = [ANY] * depth
    document = entry.get("device_document", False)
    if not isinstance(document, bool):
        raise ValueError("gui/manifest.json `%s.device_document` is %s; it is true or false"
                         % (family, json.dumps(document)))
    key_by = entry.get("key_by")
    if key_by is not None and key_by not in KEY_LEAVES:
        raise ValueError("gui/manifest.json `%s.key_by` is %s; it is one of %s"
                         % (family, json.dumps(key_by), list(KEY_LEAVES)))
    try:
        body = json.loads(template)
    except ValueError as e:
        raise ValueError("gui/%s/%s.json: %s" % (family, family, e))
    return Template(family, at, path, document, body, key_by, entry.get("bim"))


_TEMPLATES: List[Template] = []
_TEMPLATES_LOCK = threading.Lock()


def register(family: str, manifest: str, template: str) -> None:
    """Register one family. A template that does not parse is reported and
    skipped: a broken panel must not stop the hunter that finds the devices."""
    try:
        t = parse_template(family, manifest, template)
    except ValueError as why:
        print("⚠️  [panels] %s: %s — no panel will be presented for it" % (family, why), file=sys.stderr)
        return
    with _TEMPLATES_LOCK:
        _TEMPLATES.append(t)


def register_from_dir(gui_dir: str, family: str) -> None:
    """`panel!(family)` for Python: reads `gui/manifest.json` and
    `gui/<family>/<family>.json` (Rust compiles the same two files in)."""
    try:
        with open(os.path.join(gui_dir, "manifest.json"), encoding="utf-8") as f:
            manifest = f.read()
        with open(os.path.join(gui_dir, family, "%s.json" % family), encoding="utf-8") as f:
            template = f.read()
    except OSError as why:
        print("⚠️  [panels] %s: %s — no panel will be presented for it" % (family, why), file=sys.stderr)
        return
    register(family, manifest, template)


def templates() -> List[Template]:
    with _TEMPLATES_LOCK:
        return list(_TEMPLATES)


def expire_after_from_config(config: Dict[str, Dict[str, str]], token: str) -> Optional[float]:
    """`panels_expire_after_s` from the module's own section, if declared (> 0)."""
    for name, section in (config or {}).items():
        if name.lower() == token.lower() and isinstance(section, dict):
            raw = section.get("panels_expire_after_s")
            if not isinstance(raw, str):
                return None
            try:
                secs = int(raw.strip())
            except ValueError:
                return None
            return float(secs) if secs > 0 else None
    return None


class _Shape:
    def __init__(self, path: List[str], document: bool):
        self.path, self.document, self.templates = path, document, []

    def find(self, segments: List[str]) -> Optional[Tuple[List[str], str]]:
        n = len(self.path)
        if len(segments) < n or any(s == "" or (p != ANY and p != s) for s, p in zip(segments[:n], self.path)):
            return None
        if self.document:
            return (segments[:n], "") if len(segments) == n else None
        if len(segments) == n or segments[n] == DEVICE_CONTROL_LEAF:
            return None
        return segments[:n], "/".join(segments[n:])

    def name_of(self, segments: List[str]) -> str:
        if len(self.path) <= 2:
            return segments[-1] if segments else ""
        return "_".join(s for s, p in zip(segments, self.path) if p == ANY)

    def key_by(self) -> Optional[str]:
        return next((t.key_by for t in self.templates if t.key_by), None)


class _Device:
    def __init__(self, shape: int, segments: List[str], name: str, now: float):
        self.shape, self.segments, self.name = shape, segments, name
        self.fields: Dict[str, str] = {}
        self.online: Optional[bool] = None
        self.seen = now
        self.presented: Optional[str] = None


def _text(v: Any) -> str:
    return v if isinstance(v, str) else json.dumps(v, separators=(",", ":"))


def _substitute(node: Any, marks: Dict[str, str]) -> Any:
    if isinstance(node, str):
        if "${" in node:
            for mark in sorted(marks):
                node = node.replace("${%s}" % mark, marks[mark])
        return node
    if isinstance(node, list):
        return [_substitute(v, marks) for v in node]
    if isinstance(node, dict):
        return {k: _substitute(v, marks) for k, v in node.items()}
    return node


def _bind(node: Any, base: str) -> None:
    if isinstance(node, dict):
        if isinstance(node.get("device_state"), str):
            node["topic"] = "%s/%s" % (base, node["device_state"])
        if isinstance(node.get("device_control"), str):
            node["topic"] = "%s/%s/%s" % (base, DEVICE_CONTROL_LEAF, node["device_control"])
        if node.get("device_doc") is True:
            node["topic"] = base
        for v in list(node.values()):
            _bind(v, base)
    elif isinstance(node, list):
        for v in node:
            _bind(v, base)


def fingerprint(document: Any) -> str:
    """The payload text: sorted keys, no spaces — what serde_json writes."""
    return json.dumps(document, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


class Book:
    """The per-module state: what devices exist and which panels are on the broker.

    `observe`, `expire` and `restate` return `[(topic, payload)]` to publish
    RETAINED; an empty payload is a clear. `now` is `time.monotonic()` seconds.
    """

    def __init__(self, token: str, templates_: List[Template], expire_after_s: Optional[float] = None):
        self.token = token
        self.shapes: List[_Shape] = []
        for t in templates_:
            shape = next((s for s in self.shapes if s.path == t.path and s.document == t.document), None)
            if shape is None:
                shape = _Shape(list(t.path), t.document)
                self.shapes.append(shape)
            shape.templates.append(t)
        self.expire_after_s = expire_after_s
        self.devices: Dict[str, _Device] = {}
        self.ledger: Dict[str, str] = {}
        self.inherited: Dict[str, float] = {}

    def is_active(self) -> bool:
        return bool(self.shapes)

    def device_root(self) -> str:
        if _FAMILIES:
            try:
                return _FAMILIES.topic("protocols", proto=self.token, path=["Device"]) + "/"
            except Exception:                            # noqa: BLE001 — a bad token is the plugin's bug
                pass
        return "%s/%s/Device/" % (MODULE_ROOT, self.token)

    def filter(self) -> str:
        return self.device_root() + "#"

    def panels_filter(self) -> str:
        return PANELS_TOPIC_ROOT + "/+"

    def observe(self, topic: str, payload: str, now: float) -> List[Publication]:
        if topic.startswith(PANELS_TOPIC_ROOT + "/"):
            self._read_back(topic[len(PANELS_TOPIC_ROOT) + 1:], payload, now)
            return []
        root = self.device_root()
        if not topic.startswith(root):
            return []
        segments = topic[len(root):].split("/")
        found = []
        for index, shape in enumerate(self.shapes):
            hit = shape.find(segments)
            if hit is not None:
                found.append((index, shape.document, hit[0], hit[1], shape.name_of(hit[0])))
        out: List[Publication] = []
        for index, document, at, field, name in found:
            base = root + "/".join(at)
            device = self.devices.get(base)
            if device is None:
                device = self.devices[base] = _Device(index, list(at), name, now)
            device.seen = now
            if document:
                try:
                    doc = json.loads(payload)
                except ValueError:
                    doc = None
                if isinstance(doc, dict):
                    if "last_online" in doc:
                        device.online = is_online_now(_text(doc["last_online"]))
                    for leaf in KEY_LEAVES:
                        if leaf in doc:
                            device.fields[leaf] = _text(doc[leaf]).strip()
                elif payload.strip() == "":
                    device.online = False
            elif field == "last_online":
                device.online = is_online_now(payload)
            elif field in KEY_LEAVES:
                device.fields[field] = payload.strip()
            out.extend(self._reconcile(base))
        return out

    def _read_back(self, key: str, payload: str, now: float) -> None:
        if payload.strip() == "":
            self.inherited.pop(key, None)
            return
        if key in self.ledger:
            return
        try:
            doc = json.loads(payload)
        except ValueError:
            return
        source = doc.get("source") if isinstance(doc, dict) else None
        if source == "plugin:%s" % self.token:
            self.inherited.setdefault(key, now)

    def expire(self, now: float) -> List[Publication]:
        out: List[Publication] = []
        for key in sorted(k for k, since in self.inherited.items() if now - since > ADOPT_GRACE_S):
            out.extend(self._clear(key))
        if self.expire_after_s is None:
            return out
        for base in sorted(b for b, d in self.devices.items() if now - d.seen > self.expire_after_s):
            device = self.devices.pop(base)
            key = device.presented or self._key_of(device)
            if key is not None and not self._held_elsewhere(key, base):
                out.extend(self._clear(key))
        return out

    def restate(self) -> List[Publication]:
        return [(panel_topic(k), t) for k, t in sorted(self.ledger.items())]

    def _key_of(self, device: _Device) -> Optional[str]:
        leaf = self.shapes[device.shape].key_by()
        if leaf is None:
            return sanitize_segment("%s-%s" % (self.token, "-".join(device.segments)))
        value = (device.fields.get(leaf) or "").strip()
        return sanitize_segment("%s-%s" % (self.token, value)) if value else None

    def _held_elsewhere(self, key: str, base: str) -> bool:
        return any(b != base and d.presented == key for b, d in self.devices.items())

    def _clear(self, key: str) -> List[Publication]:
        ours = self.ledger.pop(key, None) is not None
        inherited = self.inherited.pop(key, None) is not None
        return [(panel_topic(key), "")] if ours or inherited else []

    def _reconcile(self, base: str) -> List[Publication]:
        device = self.devices.get(base)
        if device is None:
            return []
        key = self._key_of(device)
        out: List[Publication] = []
        prev = device.presented
        if prev is not None and prev != key:
            device.presented = None
            if not self._held_elsewhere(prev, base):
                out.extend(self._clear(prev))
        if key is None:
            return out
        if device.online is False:
            device.presented = None
            if not self._held_elsewhere(key, base):
                out.extend(self._clear(key))
            return out
        text = fingerprint(self.document(base, device, key))
        device.presented = key
        if self.ledger.get(key) == text:
            return out
        self.inherited.pop(key, None)
        self.ledger[key] = text
        out.append((panel_topic(key), text))
        return out

    def document(self, base: str, device: _Device, key: str) -> Dict[str, Any]:
        """The `present.rs` shape: `{key, folder, device, source, placements}`."""
        model = device.fields.get("model", device.name)
        name = device.fields.get("name", device.name)
        templates_ = self.shapes[device.shape].templates
        placements = []
        for t in templates_:
            marks = {"model": model, "name": name, "family": t.family, "slug": key, "resource": base}
            content = _substitute(t.body, marks)
            _bind(content, base)
            placements.append({"at": t.at, "frames": [{"path": "%s.json" % t.family, "content": content}]})
        section = {
            "type": templates_[0].family if templates_ else "",
            "model": model,
            "resource": base,
            "write_topic": "%s/%s" % (base, DEVICE_CONTROL_LEAF),
            "serial": device.fields.get("serial"),
            "name": name,
        }
        part = templates_[0].part_for(model) if templates_ else None
        if part:
            section["part"] = part   # where a reader fetches its protocols and roles (ROAD-3565)
        return {
            "key": key,
            "folder": "%s_%s" % (device.name, self.token),
            "device": section,
            "source": "plugin:%s" % self.token,
            "placements": placements,
        }
