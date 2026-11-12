# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The self test: Check, Report, run_as_json, agent_state_publications.

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import json
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple

from .contract import (
    AGENT_STATES, DEPTHS, LEAF_REPORT, LEAF_RESULT, OUTCOMES, agent_topic, module_incoming
)


# ------------------------------------------------------------- self test

def parse_engaged(raw: str) -> str:
    """`Engaged::parse` — generous about "on", and anything else is `off`."""
    word = raw.strip().strip('"').lower()
    return "engaged" if word in ("engaged", "engage", "on", "true", "1", "yes", "test") else "off"


def parse_depth(raw: str) -> Optional[str]:
    """`Depth::parse` — the depth word, or None for a word outside the set."""
    word = raw.strip().strip('"').lower()
    return {"self": "self", "own": "self", "bus": "bus", "broker": "bus",
            "device": "device", "hardware": "device", "all": "all"}.get(word)


def covers(depth: str, other: str) -> bool:
    return depth == "all" or depth == other


class Check:
    """One named thing a run proved, or did not (`self_test::Check`)."""

    def __init__(self, name: str, depth: str, outcome: str, detail: str):
        if depth not in DEPTHS or outcome not in OUTCOMES:
            raise ValueError("check %s: depth %r / outcome %r outside the vocabulary" % (name, depth, outcome))
        self.name, self.depth, self.outcome, self.detail = name, depth, outcome, detail

    @staticmethod
    def pass_(name: str, depth: str, detail: str) -> "Check":
        return Check(name, depth, "pass", detail)

    @staticmethod
    def fail(name: str, depth: str, detail: str) -> "Check":
        return Check(name, depth, "fail", detail)

    @staticmethod
    def skipped(name: str, depth: str, detail: str) -> "Check":
        return Check(name, depth, "skipped", detail)

    @staticmethod
    def of(name: str, depth: str, pass_detail: str, test: Callable[[], Any]) -> "Check":
        """`Check::from_result`: `test()` returning passes; raising fails with its message."""
        try:
            test()
        except Exception as error:                      # noqa: BLE001
            return Check.fail(name, depth, str(error) or type(error).__name__)
        return Check.pass_(name, depth, pass_detail)


class Report:
    """Everything one run concluded about one module (`self_test::Report`)."""

    def __init__(self, module: str, depth: str, refused: Optional[str] = None):
        if depth not in DEPTHS:
            raise ValueError("depth %r outside the vocabulary" % depth)
        self.module, self.depth, self.refused = module, depth, refused
        self.checks: List[Check] = []

    @staticmethod
    def refuse(module: str, depth: str, why: str) -> "Report":
        return Report(module, depth, refused=why)

    def record(self, check: Check) -> "Report":
        """A check outside this run's depth is recorded `skipped`, whatever it said."""
        if not covers(self.depth, check.depth):
            check = Check.skipped(check.name, check.depth, "not in this run's depth")
        self.checks.append(check)
        return self

    with_ = record

    def counts(self) -> Tuple[int, int, int]:
        return tuple(sum(1 for c in self.checks if c.outcome == o) for o in OUTCOMES)  # type: ignore[return-value]

    def verdict(self) -> str:
        if self.refused is not None:
            return "refused"
        passed, failed, _ = self.counts()
        return "fail" if failed else ("pass" if passed else "idle")

    def to_json(self) -> Dict[str, Any]:
        passed, failed, skipped = self.counts()
        out = {
            "module": self.module,
            "depth": self.depth,
            "verdict": self.verdict(),
            "passed": passed,
            "failed": failed,
            "skipped": skipped,
            "checks": [{"name": c.name, "depth": c.depth, "outcome": c.outcome, "detail": c.detail}
                       for c in self.checks],
        }
        if self.refused is not None:
            out["refused"] = self.refused
        return out

    @staticmethod
    def from_json(raw: str) -> "Report":
        value = json.loads(raw)
        depth = parse_depth(str(value.get("depth", "")))
        if depth is None:
            raise ValueError("depth %r is outside the vocabulary" % value.get("depth"))
        report = Report(str(value.get("module", "")), depth, refused=value.get("refused"))
        for check in value.get("checks") or []:
            check_depth = parse_depth(str(check.get("depth", "")))
            if check_depth is None:
                raise ValueError("check %s has no readable depth" % check.get("name"))
            # Appended, not recorded: the producer already filed the depths.
            report.checks.append(Check(str(check.get("name", "")), check_depth,
                                       str(check.get("outcome")), str(check.get("detail", ""))))
        return report

    def headline(self) -> str:
        if self.refused is not None:
            return "%s %s — refused: %s" % (self.module, self.depth, self.refused)
        return "%s %s — %d passed, %d failed, %d skipped" % ((self.module, self.depth) + self.counts())

    def publications(self) -> List[Tuple[str, str, bool]]:
        return [(module_incoming(self.module, LEAF_RESULT), self.verdict(), True),
                (module_incoming(self.module, LEAF_REPORT), json.dumps(self.to_json()), True)]


def run_as_json(module: str, depth_word: str, run: Callable[[str], Report]) -> str:
    """`self_test::run_as_json` — the door every plugin's self test exposes."""
    depth = parse_depth(depth_word)
    if depth is None:
        report = Report.refuse(module, "self", "`%s` is not a depth — use self, bus, device or all"
                               % depth_word.strip())
    else:
        report = run(depth)
    return json.dumps(report.to_json())


def agent_state_publications(module: str, state: str, detail: str,
                             interfaces: Iterable[str] = ()) -> List[Tuple[str, str, bool]]:
    """`AgentReport::publications` — `Agent/{state,detail,interface}` and the
    same three under `incoming/`, all retained."""
    if state not in AGENT_STATES:
        raise ValueError("agent state %r outside the vocabulary" % state)
    interface = ",".join(i.strip() for i in interfaces if i.strip())
    out = []
    for leaf, payload in (("state", state), ("detail", detail), ("interface", interface)):
        out.append((agent_topic(module, leaf), payload, True))
    for leaf, payload in (("state", state), ("detail", detail), ("interface", interface)):
        out.append((module_incoming(module, leaf), payload, True))
    report_agent_state(module, state, detail)
    return out


# Imported last: these modules import this one (a cycle the one-file runner never had to name).
from .report import report_agent_state  # noqa: E402
