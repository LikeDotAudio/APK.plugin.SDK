# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The one broker candidate list — moved here from NODE.BareMetal's
``Baremetal.Manager/mqtt_broker_discovery.py`` (2026-10-30 audit, DockTor
IMPROVEMENTS #1) so DockTor and the databus agent ``pip install`` it instead of
COPYing / sys.path-importing another repository's file. API unchanged:

    from apkaudio_contracts import broker_candidates as B
    for host, port, label in B.candidates(configured=("broker", 1883), include_loopback=False):
        ...
    print(B.describe_posture(allow_public=True, public_first=True))

Owns: the candidate order, and the two opt-ins that guard it. ``candidates()``
puts the configured broker first and the loopback next;
``public_brokers_allowed()`` reads the operator's ``APK_ALLOW_PUBLIC_BROKERS``;
``describe_posture()`` prints in one line what a given run may reach.

Must not: fall back to a public broker on its own. A public entry requires the
CALLER to pass ``allow_public`` AND the operator to have set that variable —
both, never either (PLAN-06.01, PLAN-101.01). ``public_first=True`` reorders a
list it cannot add to; ``include_loopback=False`` is for a caller inside a
container and is honoured only when ``configured`` names somewhere else.

The public broker names are WIRE CONSTANTS of an opt-in path, not a discovery
list: nothing reaches them unless both gates are open. Stdlib only.
"""

import os

# The opt-in. One name, so `grep -rn APK_ALLOW_PUBLIC_BROKERS` finds every
# chain that can reach a broker this shop does not own.
ALLOW_PUBLIC_ENVIRONMENT = "APK_ALLOW_PUBLIC_BROKERS"

LOOPBACK_BROKER = ("127.0.0.1", 1883, "Local Loopback Broker")

# Kept rather than deleted: a bench that WANTS a remote broker is a real case.
# It just has to be typed, once, on purpose.
PUBLIC_MQTT_BROKERS = [
    ("test.mosquitto.org", 1883, "Eclipse Mosquitto Public Broker"),
    ("broker.hivemq.com", 1883, "HiveMQ Public Broker"),
    ("broker.emqx.io", 1883, "EMQX Public Broker"),
]


def public_brokers_allowed():
    """True only where the operator opted in with APK_ALLOW_PUBLIC_BROKERS."""
    return os.environ.get(ALLOW_PUBLIC_ENVIRONMENT, "").strip().lower() in ("1", "true", "yes", "on")


def candidates(configured=None, allow_public=False, include_loopback=True,
               public_first=False):
    """The broker candidate list, local first.

    `configured` is an optional (host, port) from a config.ini — it goes ahead of
    the loopback, because a shop that named its own broker meant it. It is
    dropped when it IS the loopback, so the list never tests the same socket
    twice under two labels.

    `include_loopback=False` is the one exception to "local first, always", and
    only a caller INSIDE A CONTAINER may ask for it: 127.0.0.1 there is the
    container, not the bench, so the loopback is a socket that cannot ever
    answer and every publish spends a connection refusal on it. It is ignored
    unless `configured` names somewhere else to go — a container nobody told
    where the broker is keeps the loopback, because then the refusal is the
    true report of that, and the list is never empty.

    Public brokers are appended only when the caller allows them and the
    operator opted in. A caller that never passes `allow_public` can never
    reach a public broker, whatever the environment says.

    `public_first=True` PREPENDS them instead — the ordering this file was
    written to prevent, offered back to the one caller that wants it: the
    DockTor, whose report is meant to reach a page hosted off this
    bench whether or not a broker is up on it. `report.py` states that case at
    its call site, and `grep -rn public_first` is the whole list of chains that
    publish to a stranger's broker before their own.

    It changes nothing unless both opt-ins have already put public brokers in
    the list, so it can reorder and cannot introduce one.
    """
    found = [LOOPBACK_BROKER]
    if configured:
        host, port = configured[0], int(configured[1])
        if (host, port) != (LOOPBACK_BROKER[0], LOOPBACK_BROKER[1]):
            found.insert(0, (host, port, "Configured Broker"))
            if not include_loopback:
                found.remove(LOOPBACK_BROKER)

    if allow_public and public_brokers_allowed():
        # PREPEND, not insert-at-0-in-a-loop: the public entries keep their own
        # order relative to each other either way round, so a reader comparing
        # the two orders is comparing one thing and not two.
        found = (PUBLIC_MQTT_BROKERS + found) if public_first else (found + PUBLIC_MQTT_BROKERS)
    return found


def describe_posture(allow_public=False, public_first=False):
    """One line for a probe's banner: what this run may and may not reach."""
    if allow_public and public_brokers_allowed():
        if public_first:
            return (f"PUBLIC FIRST, local second; public brokers ENABLED by "
                    f"{ALLOW_PUBLIC_ENVIRONMENT}")
        return f"local first; public brokers ENABLED by {ALLOW_PUBLIC_ENVIRONMENT}"
    if allow_public:
        return f"local only (set {ALLOW_PUBLIC_ENVIRONMENT}=1 to allow public brokers)"
    return "local only; this chain has no public fallback by design"
