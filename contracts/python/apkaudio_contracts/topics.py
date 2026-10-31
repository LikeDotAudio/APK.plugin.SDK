# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Topic constants and retain rules for APK.audio bus framing."""

ROOT = "APK.audio"

DIR_INCOMING = "incoming"
DIR_OUTGOING = "outgoing"
DIRECTIONS = (DIR_INCOMING, DIR_OUTGOING)

# `api` and `api_get` — see the Rust mirror in contracts/rust/src/baremetal_io.rs
# for why the description retains and the press does not. THESE TWO TABLES MUST
# AGREE: a leaf legal in one language and not the other is a plugin publishing
# to a topic the node refuses to name.
LEAF_API = "api"
LEAF_API_GET = "api_get"

# `job` and `result` — the worker pair (PLAN-3423.01, PODS/POD.workers). A job is
# a gesture and a result answers one job, so neither retains. Same tables as the
# Rust mirror.
LEAF_JOB = "job"
LEAF_RESULT = "result"

# Retained conclusions (Item -> Bus)
INCOMING_RETAINED = (
    "state",
    "detail",
    "interface",
    "selftest",
    "selftest_result",
    "selftest_report",
    LEAF_API,
    # PLUGIN.SERIAL_COM (2026-10-30 audit, RP-07): the inventory and one
    # port's document (`port/{identity}`, two levels — see baremetal_io.py).
    "ports",
    "port",
)

# Un-retained samples (Item -> Bus)
INCOMING_SAMPLED = ("telemetry", "discovered", LEAF_RESULT, "exchange")


# Un-retained gestures (Bus -> Item)
OUTGOING_ONESHOT = ("command", "selftest_run", LEAF_API_GET, LEAF_JOB, "open")

# Retained setpoints (Bus -> Item)
OUTGOING_SETPOINT = ("enable", "selftest")


def retains(direction: str, leaf: str) -> bool:
    """Return True if leaf in direction MUST be published with MQTT retain=True."""
    if direction == DIR_INCOMING:
        if leaf in INCOMING_RETAINED:
            return True
        if leaf in INCOMING_SAMPLED:
            return False
        raise ValueError(f"Unknown incoming leaf: {leaf}")
    elif direction == DIR_OUTGOING:
        if leaf in OUTGOING_SETPOINT:
            return True
        if leaf in OUTGOING_ONESHOT:
            return False
        raise ValueError(f"Unknown outgoing leaf: {leaf}")
    else:
        raise ValueError(f"Unknown direction: {direction}")
