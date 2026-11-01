# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""
DiscoveryVerified — an agent's proof that it re-verified a discovered thing.

Twin of contracts/src/discovery-verified.ts (which carries the argument) and
contracts/rust/src/discovery_verified.rs. Retained at
APK.audio/Discovery/{protocol}/{deviceId}/verified as exactly two keys:

    verifiedAt  when the agent last re-verified it (RFC 3339 UTC, 'Z')
    by          the agent's plugin token, one topic segment

The status document beside it (DiscoveryPresence) is transition-only and must
stay silent while a device stays, so the proof lives on its own leaf: the
agent restates it at its own restatement period on each successful
re-verification, and the engine's lifecycle `check` stage reads it
(contracts/DISCOVERY-LIFECYCLE.md). It ticks by design and is never captured
to SQL. The JSON Schema is contracts/schemas/DiscoveryVerified.schema.json.
ROAD-3437.
"""

import json
import re
from datetime import datetime, timedelta, timezone

from .segments import is_segment

KEYS = ("verifiedAt", "by")

# The contract's stamp: UTC with a literal Z, seconds and fraction optional —
# the same grammar zod's z.iso.datetime() admits for DiscoveryPresence.
_STAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?Z$")


def _instant(stamp):
    if not isinstance(stamp, str) or not _STAMP.match(stamp):
        return None
    head, _, frac = stamp[:-1].partition(".")
    try:
        when = datetime.fromisoformat(head + "+00:00")
    except ValueError:
        return None
    if frac:
        when = when.replace(microsecond=int((frac + "000000")[:6]))
    return when


def document(by, now_ms):
    """The JSON an agent retains after a successful re-verification at now_ms."""
    when = datetime.fromtimestamp(now_ms / 1000, tz=timezone.utc)
    stamp = when.strftime("%Y-%m-%dT%H:%M:%S.") + f"{int(now_ms) % 1000:03d}Z"
    return json.dumps({"verifiedAt": stamp, "by": by}, separators=(",", ":"))


def parse_verified(payload):
    """A retained payload as a verified dict, or None.

    None for an empty retraction, a body with any key beyond the two, a stamp
    that is not UTC 'Z', a token that is not one topic segment, and anything
    unreadable.
    """
    if isinstance(payload, (bytes, bytearray)):
        payload = payload.decode("utf-8", "replace")
    if isinstance(payload, str):
        if not payload.strip():
            return None
        try:
            payload = json.loads(payload)
        except ValueError:
            return None
    if not isinstance(payload, dict) or set(payload) != set(KEYS):
        return None
    if _instant(payload["verifiedAt"]) is None:
        return None
    if not isinstance(payload["by"], str) or not is_segment(payload["by"]):
        return None
    return payload


def verified_ms(payload):
    """The proof's instant in unix ms, or None for anything that is not one."""
    doc = parse_verified(payload)
    if doc is None:
        return None
    epoch = datetime(1970, 1, 1, tzinfo=timezone.utc)
    return (_instant(doc["verifiedAt"]) - epoch) // timedelta(milliseconds=1)
