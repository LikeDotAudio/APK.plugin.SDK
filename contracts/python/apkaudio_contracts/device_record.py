# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""DeviceRecord typed extras, Python side (contracts 0.7.0).

The DeviceRecord itself is defined in src/device-record.ts (zod), emitted as
schemas/DeviceRecord.schema.json and generated into Rust; Python carried no
model of it until now. This module reads the one extra a Python writer or
reader needed first: ``extra.nmos`` (an IS-04 node's senders and receivers,
for SPOG). Held to vectors/payloads/DeviceRecord/{valid,invalid}/nmos-*.json.
"""
from __future__ import annotations

from typing import Any, Dict, List, Optional

_SENDER_OPTIONAL = ("format", "transport", "flowId")
_RECEIVER_OPTIONAL = ("format", "transport", "subscribedSender")


def _items(v: Any, what: str, optional: tuple) -> List[Dict[str, str]]:
    if not isinstance(v, list):
        raise ValueError(f"nmos.{what} is not an array")
    out = []
    for i, item in enumerate(v):
        if not isinstance(item, dict):
            raise ValueError(f"nmos.{what}[{i}] is not an object")
        row: Dict[str, str] = {}
        for key in ("id", "label"):
            if not isinstance(item.get(key), str):
                raise ValueError(f"nmos.{what}[{i}].{key} is not a string")
            row[key] = item[key]
        for key in optional:
            if key in item:
                if not isinstance(item[key], str):
                    raise ValueError(f"nmos.{what}[{i}].{key} is not a string")
                row[key] = item[key]
        out.append(row)
    return out


def nmos_extra(record: Any) -> Optional[Dict[str, Any]]:
    """The ``extra.nmos`` of a DeviceRecord dict: ``{nodeId?, senders[], receivers[]}``,
    or None when the record carries none. Raises ValueError on a malformed one."""
    extra = record.get("extra") if isinstance(record, dict) else None
    nmos = extra.get("nmos") if isinstance(extra, dict) else None
    if nmos is None:
        return None
    if not isinstance(nmos, dict):
        raise ValueError("extra.nmos is not an object")
    out: Dict[str, Any] = {}
    if "nodeId" in nmos:
        if not isinstance(nmos["nodeId"], str):
            raise ValueError("nmos.nodeId is not a string")
        out["nodeId"] = nmos["nodeId"]
    out["senders"] = _items(nmos.get("senders"), "senders", _SENDER_OPTIONAL)
    out["receivers"] = _items(nmos.get("receivers"), "receivers", _RECEIVER_OPTIONAL)
    return out
