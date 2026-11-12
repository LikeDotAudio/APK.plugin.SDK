# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""APK_Identity — who a placed or discovered device IS, in IFC (network-discovery-6).
Twin of contracts/src/ifc-identity.ts, pinned by the same vectors/ifc_identity.json.

Identity is a serial, a MAC or an NMOS id — never an IP. device_id_for() falls back to
`{protocol}:{address}` and to an FNV-1a hash of make|model|address; those are bus keys, not
identity, and never reach IFC. Pset_ManufacturerOccurrence.SerialNumber carries the serial;
APK_Identity carries {IdentityKey, DeviceId, MacAddress, NmosNodeId, NmosDeviceId, AnnouncedIds,
UsbSerial, Protocol, Firmware}. Standard library only.
"""
from __future__ import annotations

import re

from .identity import canonical_address, is_placeholder_serial

_IPV4 = re.compile(r"(^|[^0-9.])(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}(?![0-9.])")
_IPV6 = re.compile(r"(^|[^0-9a-f:])(([0-9a-f]{1,4}:){7}[0-9a-f]{1,4}|([0-9a-f]{1,4}:){1,7}:|:(:[0-9a-f]{1,4}){1,7}"
                   r"|([0-9a-f]{1,4}:){1,6}(:[0-9a-f]{1,4}){1,6})(?![0-9a-f:])", re.I)
_MAC = re.compile(r"^([0-9a-f]{2}:){5}[0-9a-f]{2}$")
_UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
_HASH_ID = re.compile(r"^[a-z0-9_.-]+:[0-9a-f]{16}$")
_ADDRESS_ID = re.compile(r"::|^[a-z0-9_.-]+:(tcpip|asrl|gpib|usb|vxi|pxi)\d*(::|$)", re.I)

FIELDS = ("IdentityKey", "DeviceId", "SerialNumber", "MacAddress", "NmosNodeId", "NmosDeviceId",
          "AnnouncedIds", "UsbSerial", "Protocol", "Firmware")


def device_id_basis(protocol=None, serial=None, address=None, make=None, model=None) -> str:
    """'serial' | 'address' | 'hash' — the branch of device_id_for() an id comes from."""
    s = (serial or "").strip()
    if s and not is_placeholder_serial(s):
        return "serial"
    if canonical_address((address or "").strip()):
        return "address"
    return "hash"


def contains_ip(text: str) -> bool:
    return bool(_IPV4.search(text) or _IPV6.search(text))


def canonical_mac(mac: str) -> str | None:
    h = re.sub(r"[^0-9a-f]", "", str(mac).lower())
    if len(h) != 12 or h == "0" * 12 or h == "f" * 12:
        return None
    return ":".join(h[i:i + 2] for i in range(0, 12, 2))


def not_identity_reason(text: str) -> str | None:
    if contains_ip(text):
        return "holds an IP address: an address is never identity"
    if _ADDRESS_ID.search(text):
        return "address-derived (a VISA / :: resource): a bus key, never identity"
    if _HASH_ID.search(text):
        return "hash-derived (deviceIdFor fallback): a bus key, never identity"
    return None


def problems(ident: dict) -> list[str]:
    """What is wrong with an APK_Identity dict ([] = valid). Same rules as ApkIdentitySchema."""
    out = []
    if not isinstance(ident, dict):
        return ["APK_Identity is an object"]
    for k in ident:
        if k not in FIELDS:
            out.append(f"unknown field {k}")
    if not ident.get("IdentityKey"):
        out.append("IdentityKey is required")
    for k in ("IdentityKey", "DeviceId", "SerialNumber", "UsbSerial"):
        v = ident.get(k)
        if v is not None:
            why = not_identity_reason(str(v))
            if why:
                out.append(f"{k} {why}")
    if ident.get("SerialNumber") is not None and is_placeholder_serial(str(ident["SerialNumber"])):
        out.append("SerialNumber is a placeholder (0, none, n/a): not an identity")
    if ident.get("MacAddress") is not None and not _MAC.match(str(ident["MacAddress"])):
        out.append("MacAddress is lowercase aa:bb:cc:dd:ee:ff")
    for k in ("NmosNodeId", "NmosDeviceId"):
        if ident.get(k) is not None and not _UUID.match(str(ident[k])):
            out.append(f"{k} is a lowercase UUID")
    for a in ident.get("AnnouncedIds") or []:
        why = not_identity_reason(str(a))
        if why:
            out.append(f"AnnouncedIds {why}")
    return out


def apk_identity_for(protocol=None, serial=None, address=None, make=None, model=None, mac=None,
                     nmos_node_id=None, nmos_device_id=None, usb_serial=None, device_id=None,
                     firmware=None, announced_ids=None) -> dict | None:
    """APK_Identity for a device, or None when nothing it knows is identity.
    Key order: serial, MAC, NMOS device id, NMOS node id, USB serial."""
    s = (serial or "").strip()
    good_serial = s if s and not is_placeholder_serial(s) and not not_identity_reason(s) else None
    m = canonical_mac(mac) if mac else None
    node = nmos_node_id.lower() if nmos_node_id and _UUID.match(nmos_node_id.lower()) else None
    dev = nmos_device_id.lower() if nmos_device_id and _UUID.match(nmos_device_id.lower()) else None
    u = (usb_serial or "").strip()
    usb = u if u and not not_identity_reason(u) else None
    key = (f"serial:{good_serial}" if good_serial else f"mac:{m}" if m else f"nmos:{dev}" if dev
           else f"nmos:{node}" if node else f"usb:{usb}" if usb else None)
    if not key or not_identity_reason(key):
        return None
    out = {"IdentityKey": key}
    if device_id and device_id_basis(protocol, serial, address, make, model) == "serial" and not not_identity_reason(device_id):
        out["DeviceId"] = device_id
    if good_serial:
        out["SerialNumber"] = good_serial
    if m:
        out["MacAddress"] = m
    if node:
        out["NmosNodeId"] = node
    if dev:
        out["NmosDeviceId"] = dev
    announced = [a for a in (announced_ids or []) if a and not not_identity_reason(a)]
    if announced:
        out["AnnouncedIds"] = announced
    if usb:
        out["UsbSerial"] = usb
    if protocol:
        out["Protocol"] = protocol
    if firmware:
        out["Firmware"] = firmware
    return out
