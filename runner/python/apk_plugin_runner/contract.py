# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The contract: the names, topics, vocabularies and exit codes the Rust crate spells.

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import os
import sys

# The folder that holds this package (was the module's own folder before
# ROAD-3749 made it a package): apk_plugin_api and apk_plugin_panels sit there.
HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import apk_plugin_api  # noqa: E402  (beside this file, in the SDK and the image)
import apk_plugin_panels  # noqa: E402  (the panels.rs twin, beside this file)

# ------------------------------------------------------------ the contract

ROOT = "APK.audio"
MODULE_ROOT = ROOT + "/System/Protocols"

# contracts/rust/src/self_test.rs
LEAF_ENGAGE = "selftest"
LEAF_RUN = "selftest_run"
LEAF_RESULT = "selftest_result"
LEAF_REPORT = "selftest_report"
ENGAGEMENTS = ("engaged", "off")
VERDICTS = ("pass", "running", "idle", "refused", "fail")
DEPTHS = ("self", "bus", "device", "all")
OUTCOMES = ("pass", "fail", "skipped")

# contracts/rust/src/agent_state.rs
AGENT_STATES = ("listening", "partial", "unavailable", "under_test")

# contracts/rust/src/presence.rs
ONLINE_NOW = "-1"

# RUNNER/src/lib.rs
TICK_S = 15
EXIT_AGENT_RETURNED = 70
EXIT_AGENT_PANICKED = 101
EXIT_SESSION_DIED = 69
EXIT_STALLED = 75
EXIT_TERMINATED = 143
DEFAULT_STALL_SECS = 120
DEFAULT_HEALTH_FILE = "/tmp/apk-plugin.alive"
# rumqttc's packet ceiling the Rust session sets; paho has none of its own.
MAX_PACKET = 4 * 1024 * 1024


def module_incoming(module: str, leaf: str) -> str:
    return "%s/%s/incoming/%s" % (MODULE_ROOT, module, leaf)


def module_outgoing(module: str, leaf: str) -> str:
    return "%s/%s/outgoing/%s" % (MODULE_ROOT, module, leaf)


def module_inbox(module: str) -> str:
    return "%s/%s/outgoing/#" % (MODULE_ROOT, module)


def agent_topic(module: str, leaf: str) -> str:
    return "%s/%s/Agent/%s" % (MODULE_ROOT, module, leaf)


def is_online_now(raw: str) -> bool:
    """`presence::is_online_now`: a negative number is the present tense."""
    try:
        return float(raw.strip()) < 0.0
    except ValueError:
        return False
