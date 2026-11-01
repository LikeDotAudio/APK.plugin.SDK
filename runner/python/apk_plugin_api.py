# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""`incoming/api` for the plugins that are Python scripts, not runner binaries.

THE SAME DOCUMENT `apk-plugin-runner`'s `api.rs` PUBLISHES, and deliberately so:
one HTTP gateway (`GET /plugins` on the BareMetal node) reads both, and a reader
that has to ask which language a plugin was written in before it can parse the
answer is a reader with two parsers. Every key here is a key there.

WHY THIS IS NOT IN EACH PROBE. A Python plugin (BLUETOOTH today) that wrote
its own copy of the document shape would be one more chance to drift out of
the gateway's reach — the same argument the runner crate makes for the Rust
ones. Restored by PLAN-3402.01 after a50b469 removed it as unused: BLUETOOTH
published nothing at `incoming/api`, so to the gateway it did not exist.

HOW A PROBE FINDS IT. The probe puts this directory on `sys.path` itself:
`APK_PLUGIN_API_PATH` if set, else `DISCOVERY.Engine/SRC/RUNNER/python` reached
from the probe's own path (see PLUGIN.BLUETOOTH's `_load_plugin_api`). A probe
that finds neither keeps running and publishes no document — the API is
commentary, not the plugin's job.

    import apk_plugin_api
    api = apk_plugin_api.PluginApi("bluetooth", config=my_config_dict,
                                   plugin_version="0.1.0")
    api.attach(client)            # in on_connect, after the other subscribes
    ...
    api.publish(client, agents=["monitor"], presence_online=0)

⚠️ NO CREDENTIAL EVER GOES IN THE DOCUMENT. It is published RETAINED, so it
   outlives the process and `topics.sh` prints it. `broker.authenticated` is a
   BOOLEAN and there is no field for a username. The Rust side carries the same
   warning for the same reason.
"""

from __future__ import annotations

import json
import os
import sys
import time
from typing import List, Any, Dict, Iterable, Optional

# ⚠️ THE CONTRACT IS contracts/{rust,python}, AND IT IS SPELLED HERE RATHER THAN
#    IMPORTED, for the reason agent_supervision.py gives at the same three
#    constants: `apkaudio_contracts` is not on the probes' sys.path in this
#    image, and an import that resolves on the bench and not in the container
#    turns a missing API into a plugin that will not start at all.
#    `api` retains (it is a conclusion); `api_get` does not (it is a gesture).
ROOT = "APK.audio"
MODULE_ROOT = ROOT + "/System/Protocols"
LEAF_API = "api"
LEAF_API_GET = "api_get"


def contract_version() -> Optional[str]:
    """`apkaudio_contracts.CONTRACT_VERSION`, or None where it is not importable.

    Tried, never required, for the reason the constants above are spelled
    rather than imported: a probe must start whether or not the contracts
    package is on its path. The repository's own copy is tried second, so a
    probe run from the source tree reports the real number.
    """
    try:
        from apkaudio_contracts import CONTRACT_VERSION
        return CONTRACT_VERSION
    except Exception:                                   # noqa: BLE001
        pass
    here = os.path.dirname(os.path.abspath(__file__))
    candidate = os.path.normpath(os.path.join(here, *[".."] * 6, "contracts", "python"))
    if os.path.isdir(candidate) and candidate not in sys.path:
        sys.path.append(candidate)
        try:
            from apkaudio_contracts import CONTRACT_VERSION
            return CONTRACT_VERSION
        except Exception:                               # noqa: BLE001
            return None
    return None


def api_topic(token: str) -> str:
    """`APK.audio/System/Protocols/<token>/incoming/api` — retained."""
    return "%s/%s/incoming/%s" % (MODULE_ROOT, token, LEAF_API)


def api_get_topic(token: str) -> str:
    """`APK.audio/System/Protocols/<token>/outgoing/api_get` — NOT retained."""
    return "%s/%s/outgoing/%s" % (MODULE_ROOT, token, LEAF_API_GET)


RAISE_MODES = ("on-sighting", "always-on", "compose")


def _module_key(config: Dict[str, Any], token: str, key: str) -> Optional[str]:
    """The token's own section, matched without case (api.rs's module_key)."""
    for name, section in (config or {}).items():
        if name.lower() == token.lower() and isinstance(section, dict):
            value = section.get(key)
            return value if isinstance(value, str) else None
    return None


def module_raise(config: Dict[str, Any], token: str) -> Optional[str]:
    """`raise` from the token's section, or None when missing or not a mode."""
    declared = (_module_key(config, token, "raise") or "").strip().lower()
    return declared if declared in RAISE_MODES else None


def module_serves(config: Dict[str, Any], token: str) -> List[str]:
    """`serves = a, b` from the token's section, else the token itself."""
    listed = [p.strip().lower() for p in (_module_key(config, token, "serves") or "").split(",") if p.strip()]
    return listed or [token.lower()]


class PluginApi:
    """One plugin's self-description, and the press that asks for it again.

    `config` is the plugin's configuration AS LOADED — the dict it is actually
    running on, not the file on disk. The two can differ (an overlay, a command
    line flag), and the whole value of this field is that it shows which one
    won.
    """

    def __init__(self, token: str, config: Optional[Dict[str, Any]] = None,
                 config_source: str = "config.ini",
                 broker_host: str = "", broker_port: int = 0,
                 authenticated: bool = False,
                 plugin_crate: Optional[str] = None,
                 plugin_version: Optional[str] = None,
                 broker_is_ours: Optional[bool] = None,
                 stall_after_s: Optional[int] = None,
                 tick_s: Optional[int] = None,
                 health_file: Optional[str] = None,
                 kind: str = "plugin",
                 pod: Optional[str] = "POD.THICK",
                 siblings: Iterable[str] = ()):
        self.token = token
        # LOOSE-template-python-twin: what `apk_plugin_runner.py` knows and a
        # bare probe does not — the watchdog's threshold and tick, the health
        # file, whether the broker is the bench's. Left None by a probe that
        # runs no watchdog, so the document never claims a threshold that
        # governs nothing.
        self.broker_is_ours = broker_is_ours
        self.stall_after_s = stall_after_s
        self.tick_s = tick_s
        self.health_file = health_file
        self.kind = kind
        self.pod = pod
        self.siblings = list(siblings)
        # PLAN-3407.01: the PLUGIN's own name and version, as the Rust side's
        # `stamp!` records them. None is published as null, never guessed.
        self.plugin_crate = plugin_crate
        self.plugin_version = plugin_version
        self.contract_version = contract_version()
        self.config = config or {}
        self.config_source = config_source
        self.broker_host = broker_host
        self.broker_port = broker_port
        self.authenticated = authenticated
        self.started = time.monotonic()
        self._client = None
        # The runner's `loopback_api_section()` once the server is bound.
        self.loopback: Optional[Dict[str, Any]] = None

    # ---------------------------------------------------------------- the doc

    def document(self, agents: Iterable[str] = (), presence_online: int = 0,
                 engaged: bool = False, last_tick_age_s: Optional[int] = None,
                 extra: Optional[Dict[str, Any]] = None) -> dict:
        """The same shape `api.rs::document` builds. Keys in the same order.

        `extra` is for what only this plugin knows — the devices it holds open,
        the port it opened. It is merged at the TOP level and cannot overwrite a
        standard key, because the gateway's sections are named and a plugin that
        redefined `config` would break the one thing every reader depends on.
        """
        document = {
            "token": self.token,
            "kind": self.kind,
            # How it is raised and which protocols it serves (contracts/
            # DISCOVERY-LIFECYCLE.md, stage 2) -- api.rs's module_raise/serves.
            "raise": module_raise(self.config, self.token),
            "serves": module_serves(self.config, self.token),
            "pod": self.pod,
            # Python plugins are one module per process; the grouped case is the
            # Rust runner's and this stays empty rather than absent, so the
            # gateway's readers find the same keys either way.
            "siblings": list(self.siblings),
            "build": {
                # There is no crate version here to report; the language is the
                # honest answer to "what built this", and it is what tells a
                # reader why `runner_version` is absent.
                "language": "python",
                # PLAN-3407.01 — the same two keys api.rs publishes.
                "plugin_crate": self.plugin_crate,
                "plugin_version": self.plugin_version,
                "contract_version": self.contract_version,
                "image_rev": os.environ.get("APK_IMAGE_REV"),
                "node": os.environ.get("APK_NODE") or None,
            },
            "process": {
                "pid": os.getpid(),
                "uptime_s": int(time.monotonic() - self.started),
                "agents": list(agents),
            },
            "liveness": {
                "last_tick_age_s": last_tick_age_s,
                # A bare probe runs no watchdog and is restarted by Docker on
                # exit only: None, rather than a threshold that governs
                # nothing. `apk_plugin_runner.py` fills all three.
                "stall_after_s": self.stall_after_s,
                "tick_s": self.tick_s,
                "stall_detection": last_tick_age_s is not None,
                "health_file": self.health_file,
            },
            # Where the plugin's own loopback telemetry answers (ROAD-3472):
            # `{bind, port, routes}` when apk_plugin_runner served one (it sets
            # `self.loopback`, ROAD-3478), else null — a bare probe or a plugin
            # with no agent.toml, which the supervisor reads as `unknown`.
            "loopback": self.loopback,
            "broker": {
                "host": self.broker_host,
                "port": self.broker_port,
                "authenticated": bool(self.authenticated),
                # `ours` is broker_is_local; the Python runner makes the same
                # test, a bare probe does not (None, not a guess).
                "ours": self.broker_is_ours,
            },
            "config": {"source": self.config_source, "value": self.config},
            "presence": {"online_devices": presence_online},
            "selftest": {"engaged": bool(engaged)},
            # Only the two this helper owns. A Python probe does not carry the
            # contract's full leaf table, and inventing one here would be a
            # second copy of it — the Rust side reads the real one.
            "topics": {
                api_topic(self.token): {
                    "leaf": LEAF_API, "direction": "incoming", "retained": True},
                api_get_topic(self.token): {
                    "leaf": LEAF_API_GET, "direction": "outgoing", "retained": False},
            },
        }
        for key, value in (extra or {}).items():
            document.setdefault(key, value)
        return document

    # --------------------------------------------------------------- the bus

    def attach(self, client) -> None:
        """Subscribe to the refresh press. Call from `on_connect`, every time.

        EVERY TIME, not once: a subscription does not survive a reconnect with a
        clean session, and a plugin that answered `api_get` until its first
        broker blip and not after is worse than one that never did.
        """
        self._client = client
        client.subscribe(api_get_topic(self.token), qos=1)

    def handle(self, message, **fields) -> bool:
        """If `message` is the refresh press, publish and return True.

        A RETAINED PRESS IS IGNORED — the rule `selftest_run` already follows.
        Retained, it would re-fire on every reconnect, and a bench of plugins
        all re-describing themselves the moment the broker comes back is a
        publish storm for a document that is retained anyway.
        """
        if message.topic != api_get_topic(self.token):
            return False
        if getattr(message, "retain", False):
            return True
        self.publish(self._client, **fields)
        return True

    def publish(self, client, **fields) -> None:
        """Put the description on the bus, retained. Never raises.

        A description that fails to publish must not take the plugin down with
        it: the plugin's job is the thing it hunts for, and this is commentary.
        """
        if client is None:
            return
        try:
            client.publish(api_topic(self.token),
                           json.dumps(self.document(**fields)),
                           qos=1, retain=True)
        except Exception as error:                      # noqa: BLE001
            print("⚠️  [%s] could not publish its API document: %s" % (self.token, error))
