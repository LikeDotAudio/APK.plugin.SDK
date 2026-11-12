# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
import json
from pathlib import Path

from apkaudio_contracts import ifc_identity as I
from apkaudio_contracts.identity import device_id_for

ROOT = Path(__file__).resolve().parents[2]
VEC = json.loads((ROOT / "vectors" / "ifc_identity.json").read_text(encoding="utf-8"))
SNAKE = {"nmosNodeId": "nmos_node_id", "nmosDeviceId": "nmos_device_id", "usbSerial": "usb_serial",
         "deviceId": "device_id", "announcedIds": "announced_ids"}


def test_basis_vectors_mirror_device_id_for():
    for c in VEC["basis"]:
        i = c["input"]
        assert I.device_id_basis(**i) == c["basis"], i
        did = device_id_for(**i)
        if c["basis"] == "serial":
            assert I.not_identity_reason(did) is None, did
        else:
            assert I.not_identity_reason(did) is not None, did


def test_not_identity_vectors():
    for c in VEC["notIdentity"]:
        assert (I.not_identity_reason(c["input"]) is not None) == c["rejected"], c["input"]


def test_identity_vectors():
    for c in VEC["identity"]:
        kw = {SNAKE.get(k, k): val for k, val in c["facts"].items()}
        got = I.apk_identity_for(**kw)
        assert got == c["expect"], c["facts"]
        if got:
            assert I.problems(got) == [], got


def test_problems_refuse_addresses():
    assert I.problems({"IdentityKey": "dante:192.168.1.20"})
    assert I.problems({"IdentityKey": "serial:X", "DeviceId": "visa:TCPIP::10.0.0.5::INSTR"})
    assert I.problems({"IdentityKey": "serial:X", "SerialNumber": "0"})
    assert I.problems({"IdentityKey": "serial:X", "MacAddress": "00-1D-C1-12-34-56"})
    assert I.problems({"IdentityKey": "serial:X", "Address": "x"})
    assert I.problems({"IdentityKey": "serial:X", "MacAddress": "00:1d:c1:12:34:56"}) == []
