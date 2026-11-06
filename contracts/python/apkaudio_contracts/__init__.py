# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""
apkaudio_contracts — The Python binding of @apkaudio/contracts.

Provides topic building, mailbox validation, retain-flag rules, and LWT heartbeat payload generation.
"""

# Which edition of these contracts this process was built against
# (PLAN-3407.01). The Rust mirror reads it off Cargo.toml as
# `apkaudio_contracts::CONTRACT_VERSION`; tests/test_contract_version.py fails
# when this string and that version differ.
CONTRACT_VERSION = "2.2.0"

from .topics import (
    ROOT,
    DIR_INCOMING,
    DIR_OUTGOING,
    INCOMING_RETAINED,
    INCOMING_SAMPLED,
    OUTGOING_ONESHOT,
    OUTGOING_SETPOINT,
    retains,
)
from .baremetal_io import (
    module_incoming,
    module_outgoing,
    device_incoming,
    device_outgoing,
    module_inbox,
    module_wildcard,
)
from .agent_state import AgentState
from .heartbeat import create_heartbeat
from .discovery_presence import parse_presence, is_active as presence_is_active
from .discovery_verified import parse_verified, verified_ms, document as verified_document

__all__ = [
    "CONTRACT_VERSION",
    "ROOT",
    "DIR_INCOMING",
    "DIR_OUTGOING",
    "INCOMING_RETAINED",
    "INCOMING_SAMPLED",
    "OUTGOING_ONESHOT",
    "OUTGOING_SETPOINT",
    "retains",
    "module_incoming",
    "module_outgoing",
    "device_incoming",
    "device_outgoing",
    "module_inbox",
    "module_wildcard",
    "AgentState",
    "create_heartbeat",
    "parse_presence",
    "presence_is_active",
    "parse_verified",
    "verified_ms",
    "verified_document",
]
