# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Agent State definitions and LWT Heartbeat contract constructors."""

import time
from enum import Enum
from typing import Dict, Any


class AgentState(str, Enum):
    STARTING = "STARTING"
    RUNNING = "RUNNING"
    DEGRADED = "DEGRADED"
    STOPPING = "STOPPING"
    STOPPED = "STOPPED"
    FAILED = "FAILED"


def create_heartbeat(agent_id: str, state: AgentState = AgentState.RUNNING, details: str = "") -> Dict[str, Any]:
    """Build a compliant AgentHeartbeat payload dictionary."""
    return {
        "agent_id": agent_id,
        "state": state.value,
        "timestamp_ms": int(time.time() * 1000),
        "details": details,
    }
