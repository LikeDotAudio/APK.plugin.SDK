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


def test_code_for_status_is_the_typescript_table():
    table = {404: "http_not_found", 410: "http_not_found", 401: "http_forbidden", 403: "http_forbidden",
             408: "timeout", 504: "timeout", 413: "bad_input", 400: "bad_input", 422: "bad_input",
             500: "http_server_error", 501: "http_server_error", 503: "http_server_error", 302: "script_error"}
    for status, code in table.items():
        assert problems.code_for_status(status) == code, status
        assert problems.is_known(code), code
