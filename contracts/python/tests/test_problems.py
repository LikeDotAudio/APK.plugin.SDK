# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
import json
from pathlib import Path

from apkaudio_contracts import problems

ROOT = Path(__file__).resolve().parents[2]


def test_copy_is_the_catalogue():
    assert (ROOT / "problems" / "problems.json").read_bytes() == (ROOT / "python" / "apkaudio_contracts" / "problems.json").read_bytes()


def test_every_code_has_a_fix():
    for code, entry in problems.catalogue().items():
        assert entry["fix"], code


def test_unknown_code_keeps_its_code_and_a_way_forward():
    p = problems.problem("wobble", detail="The wobble fired")
    assert p["code"] == "wobble" and p["title"] == "The wobble fired" and p["fix"]


def test_text():
    t = problems.problem_text(problems.problem("no_lock", subject="Clock A"))
    assert t.startswith("No lock — Clock A [no_lock]") and "\n1. " in t
