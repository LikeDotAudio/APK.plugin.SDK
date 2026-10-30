#!/usr/bin/env python3
# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The two languages describe a plugin with the SAME KEYS, or this fails.

Restored by PLAN-3402.01 (a50b469 had removed it with apk_plugin_api.py).

ONE GATEWAY READS BOTH. `GET /plugins` on the BareMetal node merges the
documents the Rust plugins publish through `api.rs` with the ones the
Python probes publish through `apk_plugin_api.py`, and a reader that must ask
which language a plugin was written in before it can find `config` is a reader
with two parsers. Nothing at run time would catch a key added on one side only:
both documents are valid JSON and the gateway would serve them both.

READ OFF THE SOURCE, not off a third list. A list here would be a third place
to forget.

    python3 check-api-shape.py
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def rust_keys() -> list:
    """The top-level keys of the `json!({…})` block in api.rs.

    Eight spaces of indent is the top level inside that macro; anything deeper
    is a key of a nested object and is not ours to compare.
    """
    with open(os.path.join(HERE, "..", "src", "api.rs"), encoding="utf-8") as handle:
        source = handle.read()
    body = source.split("json!({", 1)[1].split("\n}\n", 1)[0]
    return re.findall(r'^\s{8}"([a-z_]+)":', body, re.M)


def python_keys() -> list:
    sys.path.insert(0, HERE)
    import apk_plugin_api
    return list(apk_plugin_api.PluginApi("probe").document())


# PLAN-3407.01: the keys inside `build` a reader needs to judge compatibility.
# Checked by name in BOTH sources, because `build` is otherwise free to differ
# (`runner_version` is Rust's, `language` is Python's).
BUILD_KEYS = ("plugin_version", "contract_version")


def rust_build_keys() -> list:
    with open(os.path.join(HERE, "..", "src", "api.rs"), encoding="utf-8") as handle:
        source = handle.read()
    build = source.split('"build": {', 1)[1].split("},", 1)[0]
    return re.findall(r'^\s{12}"([a-z_]+)":', build, re.M)


def python_build_keys() -> list:
    sys.path.insert(0, HERE)
    import apk_plugin_api
    return list(apk_plugin_api.PluginApi("probe").document()["build"])


def main() -> int:
    for side, keys in (("api.rs", rust_build_keys()), ("apk_plugin_api.py", python_build_keys())):
        missing = [k for k in BUILD_KEYS if k not in keys]
        if missing:
            print("🛑 %s publishes no build.%s — a reader cannot tell whether "
                  "this plugin is compatible" % (side, ", build.".join(missing)))
            return 1
    rust, python = rust_keys(), python_keys()
    if not rust:
        print("🛑 read no keys out of api.rs — the json! block moved, and this "
              "check would pass forever on an empty set")
        return 2
    if set(rust) != set(python):
        only_rust = sorted(set(rust) - set(python))
        only_python = sorted(set(python) - set(rust))
        print("🛑 the plugin API document differs between the two languages.")
        if only_rust:
            print("   only in api.rs (Rust):          %s" % ", ".join(only_rust))
        if only_python:
            print("   only in apk_plugin_api.py:      %s" % ", ".join(only_python))
        print("   GET /plugins serves both. Add the key to the other side.")
        return 1
    print("✓ both languages publish the same %d keys: %s" % (len(rust), ", ".join(rust)))
    print("✓ both publish build.%s" % ", build.".join(BUILD_KEYS))
    return 0


if __name__ == "__main__":
    sys.exit(main())
