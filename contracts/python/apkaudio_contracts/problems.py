# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Every error this bench can name, and the way back from it — the Python reader
for contracts/problems/problems.json (a byte-identical copy ships beside this
file; scripts/gen.ts writes it and gen:check holds it).

    from apkaudio_contracts.problems import problem, fix_for, problem_text
    p = problem("device_unreachable", detail="no reply from 10.0.0.7:5025", subject="HP 34401A")
    log.warning(problem_text(p))

A plugin sends the CODE (ControlError.code); every reader looks the fix up
here. An unknown code is kept and still gets the generic way forward.
Standard library only.
"""
from __future__ import annotations

import json
from functools import lru_cache
from pathlib import Path

_HERE = Path(__file__).resolve().parent


@lru_cache(maxsize=1)
def catalogue() -> dict:
    """problems.json's `problems` map, by code."""
    return json.loads((_HERE / "problems.json").read_text(encoding="utf-8"))["problems"]


def is_known(code: object) -> bool:
    return isinstance(code, str) and code in catalogue()


def code_for_status(status: int) -> str:
    """The catalogue code for an HTTP status — the twin of `codeForStatus` in
    src/problems.ts (a server's error reply carries it; the reader looks it up)."""
    if status in (404, 410):
        return "http_not_found"
    if status in (401, 403):
        return "http_forbidden"
    if status in (408, 504):
        return "timeout"
    if status == 413:
        return "bad_input"
    if status >= 500:
        return "http_server_error"
    if status >= 400:
        return "bad_input"
    return "script_error"


def fix_for(code: object) -> list[str]:
    """The fix steps for a code, or [] when the catalogue does not know it."""
    return list(catalogue()[code]["fix"]) if is_known(code) else []


def problem(code: str, detail: str | None = None, subject: str | None = None, fix: list[str] | None = None) -> dict:
    """The Problem for a code: {code, area, title, cause, fix, retry[, detail, subject]}.
    A caller's own `fix` steps go first."""
    known = is_known(code)
    entry = catalogue()[code if known else "script_error"]
    out = {
        "code": code,
        "area": entry["area"],
        "title": entry["title"] if known else (detail or entry["title"]),
        "cause": entry["cause"],
        "fix": list(fix or []) + list(entry["fix"]),
        "retry": entry["retry"],
    }
    if detail is not None and known:
        out["detail"] = detail
    if subject is not None:
        out["subject"] = subject
    return out


def problem_text(p: dict) -> str:
    """A Problem as numbered plain text, for a log or a terminal."""
    head = p["title"] + (f" — {p['subject']}" if p.get("subject") else "") + f" [{p['code']}]"
    lines = [head]
    if p.get("detail"):
        lines.append(f"Details: {p['detail']}")
    lines.append(f"Why: {p['cause']}")
    lines += [f"{i}. {step}" for i, step in enumerate(p["fix"], 1)]
    return "\n".join(lines)
