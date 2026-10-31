# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""``APK_SERIAL_SETTINGS`` — the Python twin of contracts/rust/src/serial_settings.rs.

One grammar for the operator's declared serial line settings (read by
PLUGIN.VISA and PLUGIN.SERIAL_COM). The Rust module's docstring holds the
grammar and the reasons; ``vectors/serial_settings.json`` holds the two
languages to one answer (tests/test_serial_settings.py).

    from apkaudio_contracts import serial_settings

    pins = serial_settings.SerialPins.from_env()
    for bad in pins.rejected:
        print(f"APK_SERIAL_SETTINGS: cannot read `{bad}` — ignored")
    line = pins.for_port("ttyUSB0", usb_serial="EGCQb2A8J14")   # LineSettings | None
    line.label()                                                 # '9600 7E2 none'

Stdlib only.
"""
from __future__ import annotations

import os
from dataclasses import dataclass, field
from typing import List, Mapping, Optional

ENV = "APK_SERIAL_SETTINGS"
ANY_PORT = "*"
FLOWS = ("none", "xonxoff", "rtscts")


@dataclass(frozen=True)
class LineSettings:
    baud: int
    bits: int
    parity: str  # 'N' | 'E' | 'O'
    stop_bits: int
    flow: str = "none"  # one of FLOWS

    def label(self) -> str:
        return f"{self.baud} {self.bits}{self.parity}{self.stop_bits} {self.flow}"


def _ascii_digits(word: str) -> bool:
    return word != "" and all("0" <= c <= "9" for c in word)


def parse_settings(text: str) -> Optional[LineSettings]:
    """``9600 7E2`` / ``19200 8N1 rtscts`` -> LineSettings, or None."""
    words = text.split()
    if len(words) < 2 or len(words) > 3:
        return None
    if not _ascii_digits(words[0]):
        return None
    baud = int(words[0])
    if baud == 0 or baud > 0xFFFFFFFF:
        return None
    framing = words[1].upper()
    if len(framing) != 3 or framing[0] not in "78" or framing[1] not in "NEO" or framing[2] not in "12":
        return None
    flow = "none"
    if len(words) == 3:
        flow = words[2].lower()
        if flow not in FLOWS:
            return None
    return LineSettings(baud, int(framing[0]), framing[1], int(framing[2]), flow)


@dataclass(frozen=True)
class SerialPin:
    key: str
    settings: LineSettings


@dataclass
class SerialPins:
    pins: List[SerialPin] = field(default_factory=list)
    rejected: List[str] = field(default_factory=list)

    @classmethod
    def parse(cls, spec: str) -> "SerialPins":
        out = cls()
        for entry in spec.split(","):
            if "=" not in entry:
                continue
            key, value = entry.split("=", 1)
            key = key.strip()
            while key.startswith("/dev/"):
                key = key[len("/dev/"):]
            settings = parse_settings(value)
            if settings is not None and key:
                out.pins.append(SerialPin(key, settings))
            else:
                out.rejected.append(entry.strip())
        return out

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None) -> "SerialPins":
        env = os.environ if env is None else env
        return cls.parse(env.get(ENV, ""))

    def for_port(self, tty_name: str, usb_serial: str = "") -> Optional[LineSettings]:
        """Most specific wins: adapter serial, then tty name, then ``*``; last entry wins within a rank."""
        name = tty_name
        while name.startswith("/dev/"):
            name = name[len("/dev/"):]
        by_serial = by_name = by_default = None
        for pin in self.pins:
            if usb_serial and pin.key == usb_serial:
                by_serial = pin.settings
            elif pin.key == name:
                by_name = pin.settings
            elif pin.key == ANY_PORT:
                by_default = pin.settings
        return by_serial or by_name or by_default


def pinned(spec: str, tty_name: str, usb_serial: str = "") -> Optional[LineSettings]:
    return SerialPins.parse(spec).for_port(tty_name, usb_serial)
