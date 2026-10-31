# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""One topic level — the Python twin of ``topics::is_segment`` / ``topics::sanitize_segment``.

Python services that name a topic level after a value they do not control (a
container name, an mDNS instance, a printer description) used to carry their
own sanitiser (OsApi ``bus_readings.py``, DockTor ``report.py``). This is the
one rule; ``vectors/sanitize_segment.json`` holds it to the Rust function.

    from apkaudio_contracts.segments import sanitize_segment, is_segment
    sanitize_segment("Living Room TV.")   # 'Living_Room_TV'
"""
from __future__ import annotations


def _ok(c: str) -> bool:
    return c.isascii() and (c.isalnum() or c in "_-")


def is_segment(s: str) -> bool:
    """T4: ``[A-Za-z0-9_-]+``."""
    return bool(s) and all(_ok(c) for c in s)


def sanitize_segment(raw: str) -> str:
    """Coerce free text into a legal single level (never empty, never a wildcard)."""
    cleaned = "".join(c if (c.isascii() and (c.isalnum() or c == "-")) else "_" for c in raw.strip())
    trimmed = cleaned.rstrip("_")
    return trimmed or "_"
