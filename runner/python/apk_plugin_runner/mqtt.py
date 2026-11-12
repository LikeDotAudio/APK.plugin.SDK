# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""MQTT: the paho client the runner and its agents open.

Part of the `apk_plugin_runner` package (split out of apk_plugin_runner.py, ROAD-3749);
plugins import the package, never this file."""

from __future__ import annotations

import os

from .settings import broker_address, credentials


# ------------------------------------------------------------------- MQTT

def mqtt_client(client_id: str):
    """A paho client on either API generation (Debian ships 1.6, pip 2.x).

    LOOSE-bluetooth-paho-v1: on 2.x the client is asked for callback API
    VERSION2 — VERSION1 is deprecated there, and a release that drops it would
    raise here and leave a plugin silently standalone. 1.x has no
    CallbackAPIVersion and speaks its only API. Every callback in the runner
    and the plugins is written to fit both: `on_connect(client, userdata,
    flags, rc, *rest)` (rc an int on 1.x, a ReasonCode on 2.x — read it with
    `connect_failed`) and `on_message(client, userdata, message)`, the same in
    both.
    """
    import paho.mqtt.client as mqtt
    version = getattr(mqtt, "CallbackAPIVersion", None)
    if version is not None:
        client = mqtt.Client(version.VERSION2, client_id=client_id)
    else:
        client = mqtt.Client(client_id=client_id)
    pair = credentials()
    if pair:
        client.username_pw_set(*pair)
    client.reconnect_delay_set(min_delay=1, max_delay=30)
    return client


def connect_failed(rc) -> bool:
    """True when a CONNACK refused the session: rc is an int on paho 1.x, a
    ReasonCode (with `is_failure`) on the 2.x VERSION2 API."""
    failure = getattr(rc, "is_failure", None)
    return bool(failure) if failure is not None else rc != 0


def agent_client(token: str, name: str):
    """A session of the agent's own, as the Rust template's agent opens one.

    Not the runner's session: that one carries the last will and answers the
    self test, and an agent that publishes through it could wedge the press
    that tells an operator the agent is wedged. Connected asynchronously, so a
    broker that is not up yet is retried, not fatal. The caller may set a will
    and callbacks before calling `start_agent_client`.
    """
    return mqtt_client("apkaudio-%s-%s-%d" % (token, name, os.getpid()))


def start_agent_client(client) -> None:
    host, port = broker_address()
    client.connect_async(host, port, keepalive=30)
    client.loop_start()
