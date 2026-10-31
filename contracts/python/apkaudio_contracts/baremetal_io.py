# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""Mailbox topic builders for modules and bare-metal targets."""

from typing import List, Tuple
from .topics import ROOT, DIR_INCOMING, DIR_OUTGOING, retains


def module_incoming(module: str, leaf: str) -> Tuple[str, bool]:
    """Return (topic, retain) for module incoming leaf."""
    retain = retains(DIR_INCOMING, leaf)
    topic = f"{ROOT}/System/Protocols/{module}/{DIR_INCOMING}/{leaf}"
    return topic, retain


def module_outgoing(module: str, leaf: str) -> Tuple[str, bool]:
    """Return (topic, retain) for module outgoing leaf."""
    retain = retains(DIR_OUTGOING, leaf)
    topic = f"{ROOT}/System/Protocols/{module}/{DIR_OUTGOING}/{leaf}"
    return topic, retain


def device_incoming(target: str, path: List[str], leaf: str) -> Tuple[str, bool]:
    """Return (topic, retain) for device incoming leaf."""
    retain = retains(DIR_INCOMING, leaf)
    path_str = "/".join(path)
    if path_str:
        topic = f"{ROOT}/System/BareMetal/{target}/{path_str}/{DIR_INCOMING}/{leaf}"
    else:
        topic = f"{ROOT}/System/BareMetal/{target}/{DIR_INCOMING}/{leaf}"
    return topic, retain


def device_outgoing(target: str, path: List[str], leaf: str) -> Tuple[str, bool]:
    """Return (topic, retain) for device outgoing leaf."""
    retain = retains(DIR_OUTGOING, leaf)
    path_str = "/".join(path)
    if path_str:
        topic = f"{ROOT}/System/BareMetal/{target}/{path_str}/{DIR_OUTGOING}/{leaf}"
    else:
        topic = f"{ROOT}/System/BareMetal/{target}/{DIR_OUTGOING}/{leaf}"
    return topic, retain


def module_inbox(module: str) -> str:
    """Subscription topic for all messages addressed TO this module."""
    return f"{ROOT}/System/Protocols/{module}/{DIR_OUTGOING}/#"


def module_wildcard(module: str) -> str:
    """Wildcard matching all topics under a module."""
    return f"{ROOT}/System/Protocols/{module}/+/#"


def module_port(module: str, identity: str) -> Tuple[str, bool]:
    """PLUGIN.SERIAL_COM's one-port document, ``.../{module}/incoming/port/{identity}`` (retained).

    Twin of ``baremetal_io::module_port``; the identity keeps ``:`` (an address).
    """
    import re
    if not re.fullmatch(r"[A-Za-z0-9._:-]+", identity or ""):
        raise ValueError(f"port identity: {identity!r}")
    topic, retain = module_incoming(module, "port")
    return f"{topic}/{identity}", retain
