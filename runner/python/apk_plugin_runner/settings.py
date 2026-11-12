# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Config: the INI subset, the module topic, the broker address, credentials, health file.

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import ipaddress
import os
from typing import Dict, Optional, Tuple

from .contract import DEFAULT_HEALTH_FILE, DEFAULT_STALL_SECS, MODULE_ROOT


# ---------------------------------------------------------------- config

def ini_to_json(text: str) -> Dict[str, Dict[str, str]]:
    """`ini_to_json` — `{section: {key: value}}`, every value a string."""
    root: Dict[str, Dict[str, str]] = {}
    section = ""
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or line.startswith(";"):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1].strip()
            root.setdefault(section, {})
            continue
        if "=" in line:
            key, value = line.split("=", 1)
        elif ":" in line:
            key, value = line.split(":", 1)
        else:
            continue
        root.setdefault(section, {})[key.strip()] = value.strip()
    return root


def module_topic(config: Dict[str, Dict[str, str]], token: str) -> str:
    """The section's `topic` key (section matched without case), else the default."""
    for name, section in config.items():
        if name.lower() == token.lower() and isinstance(section.get("topic"), str):
            return section["topic"]
    return "%s/%s" % (MODULE_ROOT, token)


def broker_is_local(host: str) -> bool:
    """`broker_is_local` — syntactic, no DNS; anything not obviously ours is public."""
    host = host.strip().lstrip("[").rstrip("]")
    lower = host.lower()
    if lower in ("localhost", "broker") or lower.endswith(".local") or lower.endswith(".localhost"):
        return True
    try:
        ip = ipaddress.ip_address(host)
    except ValueError:
        return False
    if ip.version == 4:
        return ip.is_loopback or ip.is_private or ip.is_link_local
    return ip.is_loopback or ip.is_link_local or ((int(ip) >> 112) & 0xFE00) == 0xFC00


def broker_address() -> Tuple[str, int]:
    host = (os.environ.get("MQTT_HOST") or "").strip() or "127.0.0.1"
    try:
        port = int((os.environ.get("MQTT_PORT") or "").strip())
    except ValueError:
        port = 1883
    return host, port


def credentials() -> Optional[Tuple[str, str]]:
    """`broker_auth::credentials` — both set, the username not blank, or None."""
    user = os.environ.get("APK_MQTT_USERNAME")
    password = os.environ.get("APK_MQTT_PASSWORD")
    if user is None or password is None or not user.strip() or not password:
        return None
    return user, password


def health_file() -> str:
    return os.environ.get("APK_PLUGIN_HEALTH_FILE") or DEFAULT_HEALTH_FILE


def stall_after_s() -> int:
    try:
        return int(os.environ.get("APK_PLUGIN_STALL_SECS", ""))
    except ValueError:
        return DEFAULT_STALL_SECS


def machine_name() -> Optional[str]:
    try:
        with open("/proc/sys/kernel/hostname", encoding="utf-8") as handle:
            return handle.read().strip() or None
    except OSError:
        return None
