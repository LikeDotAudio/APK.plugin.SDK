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


# contracts/DISCOVERY-LIFECYCLE.md, stage 2: every document says how its plugin
# is raised and which protocols it serves. Required by name in both sources.
LIFECYCLE_KEYS = ("raise", "serves")

# In the monorepo the plugins sit beside this engine; a clone of the SDK alone
# has none, and the config check is skipped there rather than passing vacuously.
PLUGINS = os.path.normpath(os.path.join(HERE, "..", "..", "..", "..", "PLUGINS"))


def plugin_config(src: str):
    """The plugin's config.ini: SRC/config.ini, else the first one in a crate
    folder under SRC (Discovery.AppleTV/, apkaudio-netbox/), never under target/."""
    direct = os.path.join(src, "config.ini")
    if os.path.isfile(direct):
        return direct
    for root, dirs, files in os.walk(src):
        dirs[:] = sorted(d for d in dirs if d not in ("target", "node_modules", "tests", ".git"))
        if "config.ini" in files:
            return os.path.join(root, "config.ini")
    return None


TOKEN_RE = re.compile(r'main!\(\s*token\s*=\s*"([^"]+)"')


def plugin_token(src: str):
    """The token the binary runs under, off its `main!(token = "…")` — not the
    folder name: PLUGIN.MILAN runs as `avb`, PLUGIN.NMOS_CONTROL as `nmos-control`."""
    for root, dirs, files in os.walk(src):
        dirs[:] = sorted(d for d in dirs if d not in ("target", "node_modules", ".git"))
        for name in sorted(files):
            if name.endswith(".rs"):
                with open(os.path.join(root, name), encoding="utf-8", errors="replace") as handle:
                    match = TOKEN_RE.search(handle.read())
                if match:
                    return match.group(1).lower()
    return None


def undeclared_raises() -> list:
    """Every PLUGIN.*/SRC/config.ini whose own [token] section lacks a valid raise."""
    sys.path.insert(0, HERE)
    import configparser
    import apk_plugin_api
    bad = []
    for folder in sorted(os.listdir(PLUGINS)):
        if not folder.startswith("PLUGIN."):
            continue
        ini = plugin_config(os.path.join(PLUGINS, folder, "SRC"))
        if ini is None:
            continue
        token = plugin_token(os.path.join(PLUGINS, folder, "SRC")) or folder[len("PLUGIN."):].lower()
        parser = configparser.ConfigParser(interpolation=None, strict=False)
        try:
            parser.read(ini, encoding="utf-8")
        except configparser.Error as error:
            bad.append("%s (unreadable: %s)" % (ini, error))
            continue
        config = {name: dict(parser[name]) for name in parser.sections()}
        if apk_plugin_api.module_raise(config, token) is None:
            bad.append("%s: [%s] raise = %s" % (os.path.relpath(ini, PLUGINS), token,
                                                 " | ".join(apk_plugin_api.RAISE_MODES)))
    return bad


def main() -> int:
    for side, keys in (("api.rs", rust_build_keys()), ("apk_plugin_api.py", python_build_keys())):
        missing = [k for k in BUILD_KEYS if k not in keys]
        if missing:
            print("🛑 %s publishes no build.%s — a reader cannot tell whether "
                  "this plugin is compatible" % (side, ", build.".join(missing)))
            return 1
    rust, python = rust_keys(), python_keys()
    for side, keys in (("api.rs", rust), ("apk_plugin_api.py", python)):
        missing = [k for k in LIFECYCLE_KEYS if k not in keys]
        if missing:
            print("🛑 %s publishes no %s — the engine cannot tell how this plugin "
                  "is raised (contracts/DISCOVERY-LIFECYCLE.md)" % (side, ", ".join(missing)))
            return 1
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
    print("✓ both publish %s" % ", ".join(LIFECYCLE_KEYS))
    if os.path.isdir(PLUGINS):
        bad = undeclared_raises()
        if bad:
            print("🛑 %d plugin config(s) do not declare how they are raised:" % len(bad))
            for line in bad:
                print("   " + line)
            return 1
        print("✓ every PLUGIN.*/SRC/config.ini declares raise")
    else:
        print("· no PLUGINS/ beside this engine (SDK clone): config declarations not checked")
    return 0


if __name__ == "__main__":
    sys.exit(main())
