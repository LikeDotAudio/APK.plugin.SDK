# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""
DiscoveryPresence — a discovered device's STATUS, and nothing else.

Twin of contracts/src/discovery-presence.ts (which carries the argument) and
contracts/rust/src/discovery_presence.rs. Retained at
APK.audio/Discovery/{protocol}/{deviceId} as exactly three keys:

    lastOnline      when it went inactive -> active (RFC 3339, or None)
    lastOffline     when it went active -> inactive (RFC 3339, or None)
    lastDiscovered  the real last sighting, stamped only with a transition

No identity, no protocol/deviceId (they are the topic path), no published
state. Online <=> lastOnline is later than lastOffline. The JSON Schema is
contracts/schemas/DiscoveryPresence.schema.json. PLAN-3318.01.
"""

import json
from datetime import datetime

KEYS = ("lastOnline", "lastOffline", "lastDiscovered")


def _instant(stamp):
    if not isinstance(stamp, str):
        return None
    try:
        return datetime.fromisoformat(stamp.replace("Z", "+00:00"))
    except ValueError:
        return None


def parse_presence(payload):
    """A retained payload as a presence dict, or None.

    None for an empty retraction, a schemaVersion 1 DeviceRecord (MIDI and VISA
    still write those on the same family), a body carrying any key beyond the
    three, and anything unreadable — a reader must not guess status from those.
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
    for key in KEYS:
        value = payload[key]
        if value is not None and _instant(value) is None:
            return None
    return payload


def is_active(payload):
    """True/False for a presence document, None for anything else."""
    doc = parse_presence(payload)
    if doc is None:
        return None
    online, offline = _instant(doc["lastOnline"]), _instant(doc["lastOffline"])
    if online is None:
        return False
    return offline is None or online > offline
