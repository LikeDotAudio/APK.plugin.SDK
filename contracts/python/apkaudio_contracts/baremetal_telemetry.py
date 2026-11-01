# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The loopback agent telemetry rule (ROAD-3472) — the Python twin of
``rust/src/baremetal_telemetry.rs`` and ``src/baremetal-telemetry-rule.ts``,
held to ``vectors/baremetal_telemetry.json``.

Five words: ``running`` (alive, no traffic processed yet — draw it as idle),
``serving`` (at least one packet), ``failed`` (says it hit a fatal fault),
``unknown`` (the OBSERVER's word: nothing answered — an alive PID whose
loopback does not answer is NOT serving) and ``stub`` (declared, no
implementation).

THE COUNTER, NOT THE CLAIM: ``observed_state`` derives running/serving from
``counters.packets_received`` itself and takes only ``failed`` and ``stub``
from the document's ``state``.
"""

SCHEMA_VERSION = 1
SERVING_STATES = ("running", "serving", "failed", "unknown", "stub")
LOOPBACK_BIND = "127.0.0.1"
ROUTE_STATUS = "/status"
ROUTE_TELEMETRY = "/telemetry"


def derive_state(stub: bool, fatal: bool, packets_received: int) -> str:
    """The agent's own rule: stub, then fatal, then the packet counter."""
    if stub:
        return "stub"
    if fatal:
        return "failed"
    return "serving" if packets_received > 0 else "running"


def observed_state(document) -> str:
    """What a supervisor reads a ``GET /telemetry`` answer as (None = nothing answered)."""
    if not isinstance(document, dict):
        return "unknown"
    if document.get("state") == "stub":
        return "stub"
    counters = document.get("counters")
    packets = counters.get("packets_received") if isinstance(counters, dict) else None
    if not isinstance(packets, int) or isinstance(packets, bool) or packets < 0:
        return "unknown"
    return derive_state(False, document.get("state") == "failed", packets)
