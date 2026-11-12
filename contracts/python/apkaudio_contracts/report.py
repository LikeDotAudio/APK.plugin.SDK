# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Logs and problems — the Python twin of contracts/src/report.ts and
rust/src/report.rs: one log line and one problem record, so a Python plugin,
worker or service (WEB.OsApi, DockTor) reports the way every other one does.

    from apkaudio_contracts.report import log_line, problem_record, clear_problem
    m = log_line("osapi", "warn", "BIM library did not load", code="fetch_failed")
    client.publish(m["topic"], m["payload"], retain=m["retain"])

A record carries the catalogue CODE, never the words (problems.json is looked
up by the reader). An empty retained publish on the problem's topic clears it.
The payloads are held to vectors/payloads/{LogLine,ProblemRecord} by
tests/test_report.py.
"""
from __future__ import annotations

import json
import re
import time
from datetime import datetime, timezone

from .identity import fnv1a64
from .topics import ROOT

LOG_LEVELS = ("trace", "debug", "info", "warn", "error")
PROBLEM_DEFAULT_TTL_S = 300
LOG_MESSAGE_MAX = 500
REPORT_DETAIL_MAX = 2000


def _clip(s: str, limit: int) -> str:
    return s if len(s) <= limit else s[: limit - 1] + "…"


def _iso(now_ms: float | None) -> str:
    ms = time.time() * 1000 if now_ms is None else now_ms
    d = datetime.fromtimestamp(ms / 1000, tz=timezone.utc)
    return d.strftime("%Y-%m-%dT%H:%M:%S.") + f"{int(ms) % 1000:03d}Z"


def report_source(name: str) -> str:
    """Any name as a topic segment (twin of reportSource)."""
    s = re.sub(r"[^A-Za-z0-9_-]+", "-", str(name or ""))
    s = re.sub(r"-{2,}", "-", s).strip("-")
    return s or "unknown"


def problem_key(code: str, subject: str | None = None) -> str:
    c = report_source(code)
    return f"{c}-{fnv1a64(subject)[:8]}" if subject else c


def log_topic(source: str, level: str) -> str:
    return f"{ROOT}/System/Log/{report_source(source)}/{level}"


def problem_topic(source: str, key: str) -> str:
    return f"{ROOT}/System/Problems/{report_source(source)}/{report_source(key)}"


def log_line(source: str, level: str, message: str, *, code=None, subject=None,
             detail=None, app=None, now_ms=None) -> dict:
    """One log line: {topic, payload, retain: False, line}."""
    if level not in LOG_LEVELS:
        raise ValueError(f"not a log level: {level!r}")
    src = report_source(source)
    line = {"schemaVersion": 1, "ts": _iso(now_ms), "source": src, "level": level,
            "message": _clip(str(message or code or "(empty)"), LOG_MESSAGE_MAX)}
    if code:
        line["code"] = code
    if subject:
        line["subject"] = subject
    if detail:
        line["detail"] = _clip(str(detail), REPORT_DETAIL_MAX)
    if app:
        line["app"] = app
    return {"topic": log_topic(src, level), "payload": json.dumps(line), "retain": False, "line": line}


def problem_record(source: str, code: str, *, subject=None, detail=None, fix=None,
                   app=None, ttl_s=None, previous=None, now_ms=None) -> dict:
    """One open problem: {topic, payload, retain: True, record}."""
    src = report_source(source)
    key = problem_key(code, subject)
    at = _iso(now_ms)
    prev = previous if previous and previous.get("code") == code else None
    record = {"schemaVersion": 1, "code": code, "source": src, "key": key,
              "raised_at": prev["raised_at"] if prev else at, "last_at": at,
              "count": int(prev["count"]) + 1 if prev else 1}
    if ttl_s is not None:
        if not isinstance(ttl_s, int) or ttl_s <= 0:
            raise ValueError(f"ttl_s must be a positive integer: {ttl_s!r}")
        record["ttl_s"] = ttl_s
    if subject:
        record["subject"] = subject
    if detail:
        record["detail"] = _clip(str(detail), REPORT_DETAIL_MAX)
    if fix:
        record["fix"] = list(fix)
    if app:
        record["app"] = app
    return {"topic": problem_topic(src, key), "payload": json.dumps(record), "retain": True, "record": record}


def clear_problem(source: str, code: str, subject: str | None = None) -> dict:
    """The tombstone: an empty retained publish on the problem's topic."""
    return {"topic": problem_topic(source, problem_key(code, subject)), "payload": "", "retain": True}
