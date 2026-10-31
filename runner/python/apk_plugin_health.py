#!/usr/bin/env python3
# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""`apk-plugin-health` for the Python plugin image — the twin of
RUNNER/src/bin/apk-plugin-health.rs, installed under the same name so
plugin-base.yml's healthcheck (`["CMD", "apk-plugin-health"]`) needs no
override (LOOSE-bluetooth-no-runtime).

Exit 0 when the runner's telemetry tick wrote its health file within
`APK_PLUGIN_HEALTH_MAX_AGE` seconds (60), 1 otherwise. What a pass means, and
no more: the plugin process is alive and its tick thread is being scheduled.
"""

import os
import sys
import time


def verdict(path: str, max_age: int, now: int):
    """(exit code, message) — split from main() so the tests need no file system tricks."""
    try:
        with open(path, encoding="utf-8") as handle:
            text = handle.read().strip()
    except OSError:
        text = None
    if text is not None and text.startswith("not built"):
        return 1, text
    try:
        then = int(text) if text is not None else None
    except ValueError:
        then = None
    if then is None:
        return 1, "missing: %s has no timestamp yet" % path
    age = max(0, now - then)
    if age <= max_age:
        return 0, ""
    return 1, "stale: %s is %ds old (limit %ds)" % (path, age, max_age)


def main() -> int:
    path = os.environ.get("APK_PLUGIN_HEALTH_FILE") or "/tmp/apk-plugin.alive"
    try:
        max_age = int(os.environ.get("APK_PLUGIN_HEALTH_MAX_AGE", "60"))
    except ValueError:
        max_age = 60
    code, message = verdict(path, max_age, int(time.time()))
    if message:
        print(message, file=sys.stderr)
    return code


if __name__ == "__main__":
    sys.exit(main())
