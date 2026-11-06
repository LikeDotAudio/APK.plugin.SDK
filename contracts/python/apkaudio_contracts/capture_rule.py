# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The capture SLOTS — Python twin of contracts/src/capture-rule.ts.

Nothing is restated here: the slots are read from ``topic_grammar.json``
(``captureSlots``), which contracts/scripts/gen.ts writes from capture-rule.ts.
Every producer of ``System/Captures/{kind}/{capture}`` publishes through
``publish_slot`` (or checks ``capture_problems`` first), so a document that does
not fill its slot never reaches the bus.

    from apkaudio_contracts import capture_rule
    capture_rule.publish_slot(client.publish, 'CatchUp', {'device': ..., ...})
"""
import json
from pathlib import Path
import re
from typing import Callable, Dict, List, Optional

from . import families, segments

_GRAMMAR = json.loads((Path(__file__).with_name("topic_grammar.json")).read_text(encoding="utf-8"))
CAPTURE_SLOTS: Dict[str, dict] = _GRAMMAR["captureSlots"]
CAPTURE_KINDS: List[str] = list(_GRAMMAR["vocabularies"]["captureKind"])
_PLAIN = re.compile(r"^[A-Za-z0-9._-]+$")


class SlotError(ValueError):
    """A document that may not fill the slot it was published to."""


def capture_segment(device: str) -> str:
    """A device key as one topic level: ``visa/DMM/34401A/DevttyUSB0`` -> ``visa-DMM-34401A-DevttyUSB0``."""
    plain = "-".join(str(device).split("/"))
    return plain if _PLAIN.match(plain) else segments.sanitize_segment(plain)


def _holds(kind: str, v) -> bool:
    if v is None:
        return kind.endswith("|null")
    base = kind.replace("|null", "")
    if base == "string":
        return isinstance(v, str)
    if base == "number":
        return isinstance(v, (int, float)) and not isinstance(v, bool) and v == v and v not in (float("inf"), float("-inf"))
    if base == "array":
        return isinstance(v, list)
    if base == "object":
        return isinstance(v, dict)
    return False


def capture_problems(kind: str, doc, capture: Optional[str] = None) -> List[str]:
    """Why ``doc`` may not fill the ``kind`` slot (filed under ``capture``); empty when it may."""
    slot = CAPTURE_SLOTS.get(kind)
    if slot is None:
        return [f"{kind!r} is not a capture kind ({', '.join(CAPTURE_SLOTS)})"]
    if not isinstance(doc, dict):
        return ["not an object"]
    out = []
    for name, t in slot["fields"].items():
        if name not in doc:
            out.append(f"{kind}: missing {name}")
        elif not _holds(t, doc[name]):
            out.append(f"{kind}: {name} must be {t}")
    for name, t in slot["optional"].items():
        if name in doc and not _holds(t, doc[name]):
            out.append(f"{kind}: {name} must be {t}")
    if capture is not None and isinstance(doc.get("device"), str):
        want = "-".join(doc["device"].split("/"))
        if _PLAIN.match(want) and capture != want:
            out.append(f"{kind}: filed under {capture}, but its device says {want}")
    return out


def slot_topic(kind: str, device: str) -> str:
    return families.topic("capture", captureKind=kind, capture=capture_segment(device))


def publish_slot(publish: Callable[[str, str, bool], object], kind: str, doc: dict) -> str:
    """Fill one slot: check the document against its rule, then publish it retained.

    ``publish(topic, payload, retain)``. Returns the topic. Raises SlotError when
    the document does not fill the slot, TopicError when the device cannot be a
    topic level — the caller decides whether a lost broadcast matters."""
    problems = capture_problems(kind, doc)
    if problems:
        raise SlotError("; ".join(problems))
    topic = slot_topic(kind, doc["device"])
    publish(topic, json.dumps(doc, separators=(",", ":")), families.retain("capture"))
    return topic
