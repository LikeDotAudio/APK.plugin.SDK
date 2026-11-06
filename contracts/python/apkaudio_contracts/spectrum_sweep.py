# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""
SpectrumSweep — one complete spectrum-analyzer sweep, axis and trace together.

Twin of contracts/src/spectrum-sweep.ts (which carries the argument) and
contracts/rust/src/spectrum_sweep.rs. Retained by AUDIO.komand's sweep recorder
at APK.audio/System/Protocols/visa/Device/{family}/{model}/{dev}/Reading/Sweep:

    {schemaVersion: 1, device, model?, startHz, stopHz, centerHz, spanHz,
     rbwHz?, points, unit, trace: [...], sweep, ts, sweepMs, sweepTimeS?}

trace[i] was measured at frequency_hz(doc, i): evenly spaced from startHz to
stopHz inclusive. The JSON Schema is contracts/schemas/SpectrumSweep.schema.json.
"""

import math
import re

SWEEP_LEAF = "Sweep"

REQUIRED = ("schemaVersion", "device", "startHz", "stopHz", "centerHz", "spanHz",
            "points", "unit", "trace", "sweep", "ts", "sweepMs")

# zod's z.iso.datetime(): UTC with a literal Z, seconds and fraction optional.
_STAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?Z$")


def _number(v):
    return isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)


def _int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def faults(doc):
    """Every way `doc` is not a SpectrumSweep — empty when it is one."""
    if not isinstance(doc, dict):
        return ["not an object"]
    out = [f"missing {k}" for k in REQUIRED if k not in doc]
    if out:
        return out
    if doc["schemaVersion"] != 1 or not _int(doc["schemaVersion"]):
        out.append("schemaVersion is not 1")
    if not isinstance(doc["device"], str) or not doc["device"]:
        out.append("device is not a topic")
    if "model" in doc and (not isinstance(doc["model"], str) or not doc["model"]):
        out.append("model is empty")
    for k in ("startHz", "stopHz", "centerHz", "spanHz", "sweepMs"):
        if not _number(doc[k]) or doc[k] < 0:
            out.append(f"{k} is not a non-negative number")
    if "rbwHz" in doc and (not _number(doc["rbwHz"]) or doc["rbwHz"] <= 0):
        out.append("rbwHz is not positive")
    if "sweepTimeS" in doc and (not _number(doc["sweepTimeS"]) or doc["sweepTimeS"] < 0):
        out.append("sweepTimeS is not a non-negative number")
    if not _int(doc["points"]) or doc["points"] < 2:
        out.append("points is not a whole number of at least 2")
    if not isinstance(doc["unit"], str) or not doc["unit"]:
        out.append("unit is empty")
    trace = doc["trace"]
    if not isinstance(trace, list) or not all(_number(v) for v in trace):
        out.append("trace is not a list of numbers")
    elif _int(doc["points"]) and len(trace) != doc["points"]:
        out.append("points must count the trace")
    if not _int(doc["sweep"]) or doc["sweep"] < 1:
        out.append("sweep is not a counter from 1")
    if not isinstance(doc["ts"], str) or not _STAMP.match(doc["ts"]):
        out.append("ts is not an ISO-8601 UTC instant")
    if not out and doc["stopHz"] < doc["startHz"]:
        out.append("a sweep runs from start up to stop")
    return out


def is_sweep(doc):
    return not faults(doc)


def frequency_hz(doc, i):
    """The frequency, Hz, trace[i] was measured at."""
    n = doc["points"]
    if n < 2:
        return doc["startHz"]
    return doc["startHz"] + i * (doc["stopHz"] - doc["startHz"]) / (n - 1)
