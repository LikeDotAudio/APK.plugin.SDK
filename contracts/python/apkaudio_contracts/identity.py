# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""
Device identity derivation — guidelines D2, twin of contracts/src/identity.ts
and contracts/rust/src/identity.rs, pinned by the same vectors/identity.json.

Priority: serial -> protocol-native stable address -> FNV-1a 64-bit hash of
`make|model|address`. Two agents deriving different IDs for one instrument
recreates the duplicate-34401A bug — so this is a contract.

WHY THERE IS NOW A PYTHON TWIN. There was not one, so every Python publisher
on this bus spelled identity for itself: `instrument_worker.py` filed its
presence under the launcher's container id (`{model}-{serial}`, lowercased)
while the VISA hunter published the same instrument at `visa:{serial}`. One
33220A, two rows, and no rule anywhere that said which was right. The rule was
always right here — it just could not be reached from Python.
"""

FNV_OFFSET_BASIS = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
_MASK64 = 0xFFFFFFFFFFFFFFFF


def fnv1a64(value: str) -> str:
    """FNV-1a 64-bit over UTF-8 bytes, lowercase hex (16 chars)."""
    digest = FNV_OFFSET_BASIS
    for byte in value.encode("utf-8"):
        digest ^= byte
        digest = (digest * FNV_PRIME) & _MASK64
    return f"{digest:016x}"


def sanitize_key(value: str) -> str:
    """Replace anything outside the deviceId key charset with '-'."""
    return "".join(
        c if (c.isascii() and c.isalnum()) or c in "._:-" else "-" for c in value
    )


def is_placeholder_serial(serial: str) -> bool:
    """A serial every unit of a model reports alike (`0`, `none`, `n/a`, all
    zeros) is not an identity — see the Rust/TS twins."""
    t = serial.strip()
    return all(c == "0" for c in t) or t.lower() in ("none", "n/a", "na", "unknown")


def canonical_address(address: str) -> str:
    """One spelling per address: `TCPIP0::…` is `TCPIP::…` (board 0 is VISA's
    default). See the Rust/TS twins."""
    if address.startswith("TCPIP0::"):
        return "TCPIP::" + address[len("TCPIP0::"):]
    return address


def device_id_for(protocol, serial=None, address=None, make=None, model=None) -> str:
    """The D2 rule. Returns `{protocol}:{stableKey}`."""
    if serial is not None:
        trimmed = serial.strip()
        if trimmed and not is_placeholder_serial(trimmed):
            return f"{protocol}:{sanitize_key(trimmed)}"
    if address is not None:
        trimmed = canonical_address(address.strip())
        if trimmed:
            return f"{protocol}:{sanitize_key(trimmed)}"
    content = f"{make or ''}|{model or ''}|{address or ''}"
    return f"{protocol}:{fnv1a64(content)}"
