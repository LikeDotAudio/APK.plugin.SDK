# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Topic builders for every declared family, read off the grammar — not restated.

The TypeScript builders (contracts/src/topics/builders.ts) and the Rust twins
(contracts/rust/src/topics.rs) are written by hand. Python had only the
BareMetal mailbox builders (baremetal_io.py), so every Python service that
published anything else — OsApi, DockTor, the BareMetal manager, the databus
agent, the Bluetooth plugin — spelled `f"APK.audio/System/Agents/{x}"` itself
(SDK/contracts audit BM-06, WB-04, RP-07).

This module builds from ``topic_grammar.json``, which contracts/scripts/gen.ts
writes from MATCH_RULES and the closed vocabularies and ``gen:check`` holds, so
a family added in tree.ts/match-rules.ts is buildable here the day it lands:

    from apkaudio_contracts import families as T
    T.topic('agents', agent='komand')                         # APK.audio/System/Agents/komand
    T.topic('protocols', proto='ptp', path=['Device', seg])
    T.topic('discovery', protocol='visa', deviceId='visa:MY1')
    T.topic('bareMetal', target='PI', path=['pi-01', 'status'])
    T.retain('agents')                                        # True: retained-state

Every variable segment is checked by the rule's class (segment / address /
deviceId / a closed vocabulary); a refused value raises ``TopicError``. Vectors:
contracts/vectors/topics.json, run by tests/test_families.py.
"""
from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Dict, Iterable, List, Optional

_GRAMMAR = json.loads((Path(__file__).with_name("topic_grammar.json")).read_text(encoding="utf-8"))

ROOT: str = _GRAMMAR["root"]
FAMILIES: Dict[str, dict] = _GRAMMAR["families"]
VOCABULARIES: Dict[str, List[str]] = _GRAMMAR["vocabularies"]
_CHARSETS = {k: re.compile(v) for k, v in _GRAMMAR["charsets"].items()}
_RULES = [r for r in _GRAMMAR["rules"] if r["kind"] != "legacy"]
_FORBIDDEN_ANY = re.compile(r"^[^/+#\s]+$")

# Declared wildcard filters (grammar.ts DECLARED_FILTERS): subscription forms
# that cut across families, such as `APK.audio/+/monitor/in`, every agent's
# mirror. A filter, not a topic; vectors: `filters` in vectors/topics.json.
FILTERS: List[str] = list(_GRAMMAR.get("filters", []))


def is_declared_filter(value: str) -> bool:
    """True when ``value`` is one of the grammar's declared wildcard filters, spelled exactly."""
    return value in FILTERS


class TopicError(ValueError):
    """A value the grammar refuses for this family."""


def _rule(family: str) -> dict:
    for r in _RULES:
        if r["kind"] == family:
            return r
    raise TopicError(f"no declared family {family!r} (contracts/src/topics/tree.ts)")


def _check(cls: str, name: str, value: object) -> str:
    if not isinstance(value, str) or value == "":
        raise TopicError(f"{name}: empty or not a string ({value!r})")
    rx = _CHARSETS.get(cls)
    ok = rx.match(value) if rx is not None else _FORBIDDEN_ANY.match(value)
    if not ok:
        raise TopicError(f"{name}: {value!r} is not a valid {cls}")
    return value


def _lead(rule: dict) -> List[str]:
    out = []
    for s in rule["segs"]:
        if "lit" not in s:
            break
        out.append(s["lit"])
    return out


def _siblings(rule: dict) -> Iterable[dict]:
    lead = _lead(rule)
    return [r for r in _RULES if r["kind"] != rule["kind"] and _lead(r)[: len(lead)] == lead]


def _refused_by_sibling(rule: dict, index: int, value: str) -> Optional[str]:
    """A variable segment may not spell what a sibling family fixes at that place:
    `CapturePoints/Sweep` is its own family, so a capture-point category named
    Sweep would build a topic two families claim."""
    for sib in _siblings(rule):
        segs = sib["segs"]
        if any("rest" in s for s in segs[:index]):
            continue
        if index < len(segs) and segs[index].get("lit") == value and len(segs) <= len(rule["segs"]):
            return sib["kind"]
    return None


def _terminators(rule: dict) -> List[str]:
    """For a family ending in a rest, the leaves a sibling family terminates the
    same head with (bareMetal vs bareMetalCommand / bareMetalStream): the retain
    class must never be a function of what a caller put last."""
    segs = rule["segs"]
    out: List[str] = []
    for sib in _siblings(rule):
        s = sib["segs"]
        if len(s) == len(segs) + 1 and s[:-1] == segs:
            last = s[-1]
            if "lit" in last:
                out.append(last["lit"])
            elif "oneOf" in last:
                out.extend(VOCABULARIES.get(last["oneOf"], []))
    return out


def _class(seg: dict) -> str:
    """The charset a variable is held to. A rule that PARSES leniently (`any`)
    still BUILDS strictly when the grammar names a charset after the variable:
    komandState's `capability` matches anything on the way in and is built only
    as a dotted capability, as builders.ts does."""
    cls = seg.get("rule") or seg.get("rest")
    return seg["name"] if cls == "any" and seg.get("name") in _CHARSETS else cls


def _normalise(rule: dict, args: dict) -> dict:
    """match-rules.ts splits a path's first element into a named head where the
    path must not be empty (repositoryManager: `head`, then `path`). A builder is
    given the path; the head is its first element."""
    segs = rule["segs"]
    out = dict(args)
    for i, seg in enumerate(segs[:-1]):
        nxt = segs[i + 1]
        if "rule" in seg and "rest" in nxt and seg["name"] not in out and nxt["name"] in out:
            rest = list(out[nxt["name"]])
            if not rest:
                raise TopicError(f"{nxt['name']}: needs at least one segment")
            out[seg["name"]] = rest[0]
            out[nxt["name"]] = rest[1:]
    return out


def topic(family: str, /, **args) -> str:
    """Build a topic of a declared family. Raises TopicError on any refused value."""
    rule = _rule(family)
    args = _normalise(rule, args)
    parts = [ROOT]
    for i, seg in enumerate(rule["segs"]):
        if "lit" in seg:
            parts.append(seg["lit"])
            continue
        name = seg["name"]
        if "rest" in seg:
            rest = args.get(name, [])
            if isinstance(rest, str):
                raise TopicError(f"{name}: a list of segments, not one string")
            for v in rest:
                parts.append(_check(_class(seg), name, v))
            if rest and rest[-1] in _terminators(rule):
                raise TopicError(f"{name}: {rest[-1]!r} is a leaf of a sibling family; use that family")
            continue
        if name not in args:
            raise TopicError(f"{family}: missing {name!r}")
        value = args[name]
        if "oneOf" in seg:
            if value not in VOCABULARIES.get(seg["oneOf"], []):
                raise TopicError(f"{name}: {value!r} is outside the closed {seg['oneOf']} vocabulary")
        else:
            _check(_class(seg), name, value)
        sib = _refused_by_sibling(rule, i, value)
        if sib:
            raise TopicError(f"{name}: {value!r} is the {sib} family's own leaf")
        parts.append(value)
    unknown = set(args) - {s.get("name") for s in rule["segs"]}
    if unknown:
        raise TopicError(f"{family}: unknown argument(s) {sorted(unknown)}")
    return "/".join(parts)


def wildcard(family: str, /, **args) -> str:
    """The `#` subscription for a family, fixed as far as the given arguments go:
    wildcard('discovery') -> APK.audio/Discovery/#, wildcard('discovery', protocol='visa')."""
    rule = _rule(family)
    parts = [ROOT]
    for seg in rule["segs"]:
        if "lit" in seg:
            parts.append(seg["lit"])
            continue
        name = seg["name"]
        if name not in args:
            break
        value = args[name]
        if "oneOf" in seg and value not in VOCABULARIES.get(seg["oneOf"], []):
            raise TopicError(f"{name}: {value!r} is outside the closed {seg['oneOf']} vocabulary")
        if "rest" in seg:
            for v in value:
                parts.append(_check(seg["rest"], name, v))
            break
        if "rule" in seg:
            _check(seg["rule"], name, value)
        parts.append(value)
    return "/".join(parts) + "/#"


def retain(family: str) -> bool:
    """True when the family is retained-state (publish with retain=True)."""
    fam = FAMILIES.get(family)
    if fam is None:
        raise TopicError(f"no declared family {family!r}")
    return fam["retain"] == "retained-state"
