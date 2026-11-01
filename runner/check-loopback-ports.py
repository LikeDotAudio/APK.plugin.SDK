#!/usr/bin/env python3
# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""No two agents claim one loopback telemetry port, and every claim is in range.

ROAD-3478. The range is `loopback-ports.ini` beside this file; the claims are
every `agent.toml` under PODS/ (`[telemetry] port = N`) and every roster entry
in NODE.BareMetal's `agents.json` (`"telemetry": {"port": N}`). Read off the
files themselves — a table here would be a second registry to forget.

    python3 check-loopback-ports.py            # the table, exit 1 on a finding
    python3 check-loopback-ports.py --next     # the lowest free port in range

`port = 0` declares no listener and is not a claim. A roster entry and its
agent's own agent.toml naming the same port under the same name is one claim.
"""

import configparser
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PODS = os.path.normpath(os.path.join(HERE, "..", "..", "..", ".."))
AGENTS_JSON = os.path.join(PODS, "POD.THICK", "NODE.BareMetal", "SRC", "agents.json")
# Directories that hold build output or vendored trees, never a declaration.
SKIP = {".git", "node_modules", "target", "dist", "__pycache__", ".venv", "venv"}


def port_range():
    parser = configparser.ConfigParser(inline_comment_prefixes=(";", "#"))
    parser.read(os.path.join(HERE, "loopback-ports.ini"), encoding="utf-8")
    return parser.getint("loopback", "first"), parser.getint("loopback", "last")


def manifest_claim(text):
    """(name, port) from an agent.toml: the `[agent] name` and `[telemetry] port`.

    The same small subset contracts' Manifest::parse reads; only the two keys.
    """
    section, name, port = None, None, None
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        head = re.fullmatch(r"\[([A-Za-z0-9_.-]+)\]", line)
        if head:
            section = head.group(1)
            continue
        pair = re.fullmatch(r"([A-Za-z0-9_]+)\s*=\s*(.+)", line)
        if not pair:
            continue
        key, value = pair.group(1), pair.group(2).strip()
        if section == "agent" and key == "name":
            name = value.strip('"')
        elif section == "telemetry" and key == "port" and re.fullmatch(r"\d+", value):
            port = int(value)
    return name, port


def claims():
    """[(port, who, where)] for every declaration under PODS/."""
    found, seen = [], set()
    for root, dirs, files in os.walk(PODS):
        dirs[:] = [d for d in dirs if d not in SKIP]
        if "agent.toml" not in files:
            continue
        path = os.path.join(root, "agent.toml")
        real = os.path.realpath(path)
        if real in seen:
            continue
        seen.add(real)
        with open(path, encoding="utf-8") as handle:
            name, port = manifest_claim(handle.read())
        if port:
            found.append((port, name or "?", os.path.relpath(path, PODS)))
    if os.path.exists(AGENTS_JSON):
        with open(AGENTS_JSON, encoding="utf-8") as handle:
            roster = json.load(handle)
        for agent in roster.get("agents", []):
            port = (agent.get("telemetry") or {}).get("port")
            if isinstance(port, int) and port:
                found.append((port, agent.get("id", "?"),
                              os.path.relpath(AGENTS_JSON, PODS) + " #" + agent.get("id", "?")))
    return sorted(found)


def main(argv):
    first, last = port_range()
    found = claims()
    if "--next" in argv:
        taken = {port for port, _, _ in found}
        free = next((p for p in range(first, last + 1) if p not in taken), None)
        if free is None:
            sys.exit("no free port left in %d-%d" % (first, last))
        print(free)
        return 0
    findings = []
    by_port = {}
    for port, who, where in found:
        by_port.setdefault(port, []).append((who, where))
        if not first <= port <= last:
            findings.append("%s (%s) claims %d, outside %d-%d" % (who, where, port, first, last))
    for port, owners in sorted(by_port.items()):
        # A roster entry naming its own agent's port (agents.json + that
        # agent's agent.toml, same name) is one agent stated twice, which the
        # roster must do — not a clash. Two NAMES on one port is.
        if len({who for who, _ in owners}) > 1:
            findings.append("%d is claimed %d times: %s" % (
                port, len(owners), ", ".join("%s (%s)" % o for o in owners)))
    print("  loopback telemetry %d-%d: %d claim(s)" % (first, last, len(found)))
    for port, who, where in found:
        print("    %d  %-16s %s" % (port, who, where))
    for finding in findings:
        print("  ✗ " + finding, file=sys.stderr)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
