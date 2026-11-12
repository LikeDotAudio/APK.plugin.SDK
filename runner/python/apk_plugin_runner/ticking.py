# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Ticking (the per-agent stall clocks), the command line, and the telemetry tick.

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import os
import sys
import threading
import time
from typing import Any, Dict, Iterable, List, Optional, Tuple

from .settings import health_file
from .loopback import _LOOPBACK, counters


# --------------------------------------------------------------- ticking

_LAST_TICK = [0]                     # a tick from a thread no agent owns
_AGENT_TICKS: Dict[str, List[int]] = {}
_SLOT = threading.local()


def tick() -> None:
    """An agent says "still making progress". Arms ITS stall clock, once called."""
    clock = _AGENT_TICKS.get(getattr(_SLOT, "agent", None) or "")
    (clock if clock is not None else _LAST_TICK)[0] = int(time.time())


def stalest(clocks: Iterable[Tuple[str, int]], now: int) -> Optional[Tuple[str, int]]:
    aged = [(name, max(0, now - last)) for name, last in clocks if last != 0]
    return max(aged, key=lambda pair: pair[1]) if aged else None


def stalest_tick() -> Optional[Tuple[str, int]]:
    clocks = [(name, clock[0]) for name, clock in _AGENT_TICKS.items()]
    clocks.append(("process", _LAST_TICK[0]))
    return stalest(clocks, int(time.time()))


def write_health_file() -> None:
    try:
        with open(health_file(), "w", encoding="utf-8") as handle:
            handle.write(str(int(time.time())))
    except OSError:
        pass


# ------------------------------------------------------------- arguments

def answer_arguments(args: List[str], token: str, agents: List[str],
                     version: Optional[str] = None) -> Optional[int]:
    """`answer_arguments`: None to start, else the exit code (help 0, version 0, else 2)."""
    if not args:
        return None
    exe = os.path.basename(sys.argv[0] or "apk-plugin")
    usage = (
        "%s — APK.audio plugin runner (Python) for [%s] (agents: %s)\n\n"
        "Usage: %s            run until stopped; settings come from the environment\n"
        "       %s --help     this text, and exit (touches no broker)\n"
        "       %s --version  the build, and exit\n\n"
        "Environment:\n"
        "  MQTT_HOST              broker host (default 127.0.0.1)\n"
        "  MQTT_PORT              broker port (default 1883)\n"
        "  APK_PLUGIN_CONFIG      a config.ini to use instead of the plugin's own\n"
        "  APK_PLUGIN_STALL_SECS  seconds without a tick before the agent counts as stalled (default 120)\n"
        % (exe, token, ", ".join(agents), exe, exe, exe))
    first = args[0]
    if first in ("-h", "--help", "help"):
        sys.stdout.write(usage)
        return 0
    if first in ("-V", "--version"):
        print(version or "%s (unstamped build)" % exe)
        return 0
    sys.stderr.write("%s: takes no arguments (got %r); nothing was started.\n\n%s" % (exe, first, usage))
    return 2


# ---------------------------------------------------------------- serving

def tick_sample(uptime_s: int) -> Dict[str, Any]:
    """The `incoming/telemetry` tick: RUNNER/src/lib.rs's `tick_sample`, key
    for key — `counters` is the loopback's block (ROAD-3478), so a bus reader
    applies contracts' rule to the tick as the supervisor does to the loopback."""
    stale = stalest_tick()
    sample = {"uptime_s": uptime_s, "pid": os.getpid(),
              "last_tick_age_s": stale[1] if stale else None,
              "image_rev": os.environ.get("APK_IMAGE_REV"),
              "counters": counters()}
    if _LOOPBACK["disabled"]:
        sample["state"], sample["reason"] = "stub", _LOOPBACK["disabled"]
    return sample
