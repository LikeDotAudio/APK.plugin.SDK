# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Broker credentials, the Python twin of contracts/rust/src/broker_auth.rs.

Every MQTT session on this bench authenticates from the same two environment
variables. The Rust side has read them through ``broker_auth::credentials()``
since the broker operator overlay; the Python services (OsApi, DockTor, the
BareMetal manager, the databus agent, the FrontEnd launcher) each either read
them by hand or not at all, so on a broker that refuses anonymous sessions they
connected anonymously and were refused (SDK/contracts audit BM-02, WB-04).

    from apkaudio_contracts import broker_auth

    client = mqtt.Client(...)
    print('connecting', broker_auth.apply(client))      # username_pw_set when set
    publish.single(topic, payload, hostname=h, port=p, auth=broker_auth.paho_auth())

Same rule as the Rust: both set (username not blank, password not empty) or
it is anonymous; a half-set pair is a misconfiguration, said out loud by
``describe()``, never a credential. Stdlib only; the names are held to the
Rust constants by tests/test_broker_auth.py.
"""
from __future__ import annotations

import os
from typing import Mapping, Optional, Tuple

USERNAME_VARIABLE = "APK_MQTT_USERNAME"
PASSWORD_VARIABLE = "APK_MQTT_PASSWORD"


def credentials(env: Optional[Mapping[str, str]] = None, prefix: str = "APK_MQTT") -> Optional[Tuple[str, str]]:
    """(username, password) when both are set, else None (anonymous).

    ``prefix`` names the pair ``{prefix}_USERNAME`` / ``{prefix}_PASSWORD``; the
    default is the bench's (``APK_MQTT``, the Rust constants). Another prefix is
    for a process that also speaks to a second broker under its own names.
    """
    env = os.environ if env is None else env
    username = env.get(f"{prefix}_USERNAME")
    password = env.get(f"{prefix}_PASSWORD")
    if username is None or password is None or not username.strip() or password == "":
        return None
    return username, password


def describe(env: Optional[Mapping[str, str]] = None) -> str:
    """How a session will connect, for a log line. Never prints the password."""
    env = os.environ if env is None else env
    user = bool((env.get(USERNAME_VARIABLE) or "").strip())
    pw = bool(env.get(PASSWORD_VARIABLE) or "")
    if user and pw:
        return f"as {env.get(USERNAME_VARIABLE)}"
    if not user and not pw:
        return "anonymously"
    return (f"ANONYMOUSLY — {USERNAME_VARIABLE}/{PASSWORD_VARIABLE} are half set, "
            "which is a misconfiguration, not a credential")


def apply(client, env: Optional[Mapping[str, str]] = None) -> str:
    """Call ``client.username_pw_set`` when credentials are set; return describe()."""
    creds = credentials(env)
    if creds is not None:
        client.username_pw_set(creds[0], creds[1])
    return describe(env)


def paho_auth(env: Optional[Mapping[str, str]] = None) -> Optional[dict]:
    """The ``auth=`` argument for paho ``publish.single`` / ``publish.multiple``."""
    creds = credentials(env)
    return None if creds is None else {"username": creds[0], "password": creds[1]}
