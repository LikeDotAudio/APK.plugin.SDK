# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""A machine command — power, a named command, a deploy (PLAN-3394.01).

Twin of contracts/rust/src/machine_command.rs, which carries the argument, and
of contracts/src/machine-command.ts; the three are pinned to the same vectors
(contracts/vectors/machine_command.json).

    APK.audio/System/BareMetal/MonitorProbes/{probe}/{host}/outgoing/command   NOT retained
    APK.audio/System/BareMetal/MonitorProbes/{probe}/{host}/incoming/state     RETAINED answer
    APK.audio/System/BareMetal/MonitorProbes/{probe}/{host}/incoming/detail    RETAINED: what it can do

Only the broker user OPERATOR_ROLE may publish a command, and the shell holds
that credential: it publishes only after a person confirmed the action and the
confirmation was logged. `session` is that log line's id. `run` and `deploy`
name an allow-list entry in the agent's config.ini — there is no field for a
command line, and a body with an unknown key is refused whole.
"""

import json
import re
from typing import Any, Dict, List, Optional, Tuple

from .baremetal_io import device_incoming, device_outgoing
from .topics import ROOT

OPERATOR_ROLE = "apkaudio-operator"
MACHINE_ROLE = "apkaudio-machine"
TARGET = "MonitorProbes"

VERB_POWER = "power"
VERB_RUN = "run"
VERB_DEPLOY = "deploy"
VERBS = (VERB_POWER, VERB_RUN, VERB_DEPLOY)
POWER_ACTIONS = ("on", "off", "cycle")

STATUS_ACCEPTED = "accepted"
STATUS_DONE = "done"
STATUS_REFUSED = "refused"
STATUS_ERROR = "error"
STATUSES = (STATUS_ACCEPTED, STATUS_DONE, STATUS_REFUSED, STATUS_ERROR)

KEYS = ("id", "verb", "action", "name", "session", "operator", "issued_at_ms", "mac", "dry_run")

_TOKEN = re.compile(r"[A-Za-z0-9._:-]{8,80}")
_NAME = re.compile(r"[A-Za-z0-9_][A-Za-z0-9._-]{0,63}")
_MAC = re.compile(r"[0-9A-Fa-f]{2}(:[0-9A-Fa-f]{2}){5}")


class Refused(ValueError):
    """Why a body is not a command; `id` is what the refusal is answered against."""

    def __init__(self, reason: str, id: Optional[str] = None):
        super().__init__(reason)
        self.reason = reason
        self.id = id


def is_token(s: Any) -> bool:
    return isinstance(s, str) and bool(_TOKEN.fullmatch(s))


def is_name(s: Any) -> bool:
    return isinstance(s, str) and bool(_NAME.fullmatch(s))


def is_mac(s: Any) -> bool:
    return isinstance(s, str) and bool(_MAC.fullmatch(s))


def parse(payload: Any) -> Dict[str, Any]:
    """Read a command body (bytes, str or dict). Raises Refused. Strict, as the Rust twin."""
    if isinstance(payload, (bytes, bytearray, str)):
        try:
            doc = json.loads(payload)
        except ValueError:
            raise Refused("not JSON")
    else:
        doc = payload
    if not isinstance(doc, dict):
        raise Refused("not a JSON object")
    id_ = doc.get("id") if is_token(doc.get("id")) else None
    for key in doc:
        if key not in KEYS:
            raise Refused(f"unknown key `{key}` — a command names an allow-list entry, never a command line", id_)
    if id_ is None:
        raise Refused("no `id` (8–80 of A-Z a-z 0-9 . _ : -)")
    for key in ("session", "operator", "verb", "action", "name", "mac"):
        if doc.get(key) is not None and not isinstance(doc[key], str):
            raise Refused(f"`{key}` is not a string", id_)
    if not is_token(doc.get("session")):
        raise Refused("no `session` — the shell's confirmation log id; nothing unconfirmed is obeyed", id_)
    operator = doc.get("operator") or ""
    # 80 UTF-8 bytes, as the Rust and TypeScript twins count it — not 80 characters.
    if not operator.strip() or len(operator.encode("utf-8")) > 80:
        raise Refused("no `operator` — who confirmed it", id_)
    issued = doc.get("issued_at_ms")
    if not isinstance(issued, int) or isinstance(issued, bool) or issued < 0:
        raise Refused("no `issued_at_ms` (unix ms)", id_)
    dry_run = doc.get("dry_run")
    if dry_run is not None and not isinstance(dry_run, bool):
        raise Refused("`dry_run` is not a boolean", id_)
    verb, action, name, mac = doc.get("verb") or "", doc.get("action"), doc.get("name"), doc.get("mac")
    if verb == VERB_POWER:
        if action not in POWER_ACTIONS:
            raise Refused(f"power `action` must be one of {list(POWER_ACTIONS)}", id_)
        if name is not None:
            raise Refused("power takes no `name`", id_)
        if mac is not None and (action != "on" or not is_mac(mac)):
            raise Refused("`mac` is for power on only, as aa:bb:cc:dd:ee:ff", id_)
    elif verb in (VERB_RUN, VERB_DEPLOY):
        if action is not None or mac is not None:
            raise Refused(f"{verb} takes only a `name`", id_)
        if not is_name(name):
            raise Refused(f"{verb} needs a `name` from the agent's allow-list", id_)
    else:
        raise Refused(f"unknown verb `{verb}` — one of {list(VERBS)}", id_)
    out = {"id": id_, "verb": verb, "session": doc["session"], "operator": operator, "issued_at_ms": issued}
    if action is not None:
        out["action"] = action
    if name is not None:
        out["name"] = name
    if mac is not None:
        out["mac"] = mac
    if dry_run:
        out["dry_run"] = True
    return out


def to_json(command: Dict[str, Any]) -> str:
    """The wire body of a parsed command."""
    return json.dumps(parse(command), separators=(",", ":"))


def describe(command: Dict[str, Any]) -> str:
    return f"{command['verb']} {command.get('action') or command.get('name') or ''}".strip()


def answer(command: Optional[Dict[str, Any]], id: Optional[str], status: str, detail: str,
           by: str, dry_run: bool, at_ms: int) -> str:
    """The retained `incoming/state` answer to one command."""
    if status not in STATUSES:
        raise ValueError(f"unknown status {status!r}")
    doc: Dict[str, Any] = {"id": id or (command or {}).get("id"), "status": status, "detail": detail,
                           "by": by, "dry_run": dry_run, "at_ms": at_ms}
    if command:
        doc["verb"] = command["verb"]
        for key in ("action", "name"):
            if command.get(key) is not None:
                doc[key] = command[key]
        doc["session"] = command["session"]
        doc["operator"] = command["operator"]
    return json.dumps(doc, separators=(",", ":"))


def _segment(s: str, what: str) -> str:
    if not isinstance(s, str) or not re.fullmatch(r"[A-Za-z0-9._:-]+", s) or s in ("incoming", "outgoing"):
        raise ValueError(f"bad {what}: {s!r}")
    return s


def command_topic(probe: str, host: str) -> Tuple[str, bool]:
    return device_outgoing(TARGET, [_segment(probe, "probe"), _segment(host, "host")], "command")


def state_topic(probe: str, host: str) -> Tuple[str, bool]:
    return device_incoming(TARGET, [_segment(probe, "probe"), _segment(host, "host")], "state")


def detail_topic(probe: str, host: str) -> Tuple[str, bool]:
    return device_incoming(TARGET, [_segment(probe, "probe"), _segment(host, "host")], "detail")


def acl_deny_patterns() -> List[str]:
    """The filters the broker ACL denies every client but the operator."""
    return [f"{ROOT}/System/BareMetal/{'/'.join(['+'] * d)}/outgoing/command" for d in range(1, 5)]
