# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Reports: elog, raise_problem / clear_problem — the twin of RUNNER/src/report.rs.

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import json
import os
import sys
import threading
import time
from typing import Any, Dict, Optional

from .contract import HERE


# ---------------------------------------------------------------- reports
# ROAD-3695 step 4 — the twin of RUNNER/src/report.rs. Every line `elog`
# prints to stderr is ALSO said as a LogLine on System/Log/{token}/{level};
# every fault the runner can name is raised as a retained ProblemRecord on
# System/Problems/{token}/{key} and cleared when it is over — through
# `apkaudio_contracts.report` (the image stages it beside the runner; where it
# is not importable, nothing is sent and the stderr line stands). Never blocks,
# never raises. Lines said before the session exists are held (at most 32).

REPORT_CHECK_FAILED = "plugin_check_failed"
REPORT_JOB_FAILED = "job_failed"
REPORT_UNAVAILABLE = "plugin_unavailable"
REPORT_DOWN = "plugin_down"
REPORT_EARLY_LINES = 32
REPORT_TTL_S = 300

_REPORT: Dict[str, Any] = {"session": None, "name": None, "early": [], "raised": {}}
_REPORT_LOCK = threading.Lock()


def _report_contract():
    """`apkaudio_contracts.report`, or None — found the way `telemetry_rule` finds its module."""
    try:
        from apkaudio_contracts import report
        return report
    except Exception:                                   # noqa: BLE001
        pass
    candidate = os.path.normpath(os.path.join(HERE, *[".."] * 6, "contracts", "python"))
    if os.path.isdir(candidate) and candidate not in sys.path:
        sys.path.append(candidate)
        try:
            from apkaudio_contracts import report
            return report
        except Exception:                               # noqa: BLE001
            return None
    return None


def report_source() -> str:
    session = _REPORT["session"]
    name = session.token if session is not None else (_REPORT["name"] or os.path.basename(sys.argv[0] or "plugin"))
    contract = _report_contract()
    return contract.report_source(name) if contract else name


def report_install(session: "Session") -> None:
    """Give the reports a session (the first wins); what was held is said."""
    with _REPORT_LOCK:
        if _REPORT["session"] is not None:
            return
        _REPORT["session"] = session
        held, _REPORT["early"] = _REPORT["early"], []
    for message in held:
        _report_send(message)


def _report_send(message: Dict[str, Any]) -> bool:
    session = _REPORT["session"]
    if session is None:
        with _REPORT_LOCK:
            if len(_REPORT["early"]) < REPORT_EARLY_LINES:
                _REPORT["early"].append(message)
        return False
    try:
        info = session.client.publish(message["topic"], message["payload"], qos=0, retain=message["retain"])
        return published(getattr(info, "rc", 0) == 0)
    except Exception:                                   # noqa: BLE001
        return published(False)


def level_of(line: str) -> str:
    """The level a stderr line is said at, read off its own mark: 🛑/❌ error, ⚠️ warn, else info."""
    head = line.lstrip()
    if head.startswith("🛑") or "❌" in head:
        return "error"
    if head.startswith("⚠"):
        return "warn"
    return "info"


def report_log(level: str, message: str, code: Optional[str] = None, subject: Optional[str] = None,
               detail: Optional[str] = None) -> bool:
    contract = _report_contract()
    if contract is None:
        return False
    try:
        m = contract.log_line(report_source(), level, message.strip(), code=code, subject=subject, detail=detail)
    except Exception:                                   # noqa: BLE001
        return False
    return _report_send(m)


def elog(line: str, level: Optional[str] = None) -> None:
    """`print(line, file=sys.stderr)` with a LogLine twin — `elog!` in the Rust runner."""
    print(line, file=sys.stderr)
    report_log(level or level_of(line), line)


def raise_problem(code: str, subject: Optional[str] = None, detail: str = "") -> bool:
    """Raise (or raise again: count + first raise kept) a retained problem, and say it as an error line."""
    contract = _report_contract()
    if contract is None:
        return False
    source = report_source()
    topic = contract.problem_topic(source, contract.problem_key(code, subject))
    with _REPORT_LOCK:
        previous = _REPORT["raised"].get(topic)
    try:
        m = contract.problem_record(source, code, subject=subject or None, detail=detail or None,
                                    ttl_s=REPORT_TTL_S, previous=previous)
    except Exception:                                   # noqa: BLE001
        return False
    with _REPORT_LOCK:
        _REPORT["raised"][topic] = json.loads(m["payload"])
    sent = _report_send(m)
    report_log("error", "problem %s%s" % (code, " — %s" % subject if subject else ""), code=code,
               subject=subject or None, detail=detail or None)
    return sent


def clear_problem(code: str, subject: Optional[str] = None, any_process: bool = False) -> bool:
    """Clear a problem this process raised — or, `any_process`, whoever raised it."""
    contract = _report_contract()
    if contract is None:
        return False
    m = contract.clear_problem(report_source(), code, subject or None)
    with _REPORT_LOCK:
        was = _REPORT["raised"].pop(m["topic"], None) is not None
    if was or any_process:
        _report_send(m)
    return was


def report_checks(report: "Report") -> None:
    """One problem per self-test check: a failure raises plugin_check_failed about
    `<token>/<check>`, a pass clears it."""
    for check in report.checks:
        subject = "%s/%s" % (report.module, check.name)
        if check.outcome == "fail":
            raise_problem(getattr(check, "problem", None) or REPORT_CHECK_FAILED, subject, check.detail)
        elif check.outcome == "pass":
            clear_problem(REPORT_CHECK_FAILED, subject, any_process=True)


def report_agent_state(module: str, state: str, detail: str) -> None:
    """`unavailable` raises plugin_unavailable about the module; `listening` clears it."""
    if state == "unavailable":
        raise_problem(REPORT_UNAVAILABLE, module, detail)
    elif state == "listening":
        clear_problem(REPORT_UNAVAILABLE, module, any_process=True)


def going_down(token: str, why: str) -> None:
    """plugin_down before an exit the runner chose; the next process's connect clears it."""
    raise_problem(REPORT_DOWN, token, why)
    if _REPORT["session"] is not None:
        time.sleep(0.3)


# Imported last: these modules import this one (a cycle the one-file runner never had to name).
from .loopback import published  # noqa: E402
