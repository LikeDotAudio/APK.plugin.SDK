# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""ONE GlobalId rule, site-wide (IFC-CORE-DELTAS viewers-plumbing-1).

ifc_guid(*roles) = uuid5(NAMESPACE, '/'.join(roles)) as a 128-bit integer,
written as 22 characters of IFC base64 (0-9A-Za-z_$), most significant first.

Twin of contracts/src/ifc/guid.ts and of DATABASE.server.BIM bim/ifc.py guid();
all held to contracts/vectors/ifc-guid.json (generated from ifc.py). Standard
library only. The role grammar (type part/<maker>/<model>, occurrence
inst/mac:<mac> | inst/serial:<maker>/<serial> | inst/nmos:<uuid>, relationship
rel/<kind>/<gidA>/<gidB>, …) is documented in guid.ts; never an IP or a bench ref.
"""
import uuid

NAMESPACE = uuid.UUID("6f1d3a52-7c0b-4b8e-9a51-6b8f0a7d3e11")
ALPHABET = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$"


def compress(u):
    """A UUID (or its 128-bit int) -> 22-char IFC GlobalId."""
    n = u.int if isinstance(u, uuid.UUID) else int(u)
    chars = []
    for _ in range(22):
        chars.append(ALPHABET[n % 64])
        n //= 64
    return "".join(reversed(chars))


def expand(gid):
    """22-char IFC GlobalId -> uuid.UUID."""
    if len(gid) != 22 or any(c not in ALPHABET for c in gid) or ALPHABET.index(gid[0]) > 3:
        raise ValueError("not an IFC GlobalId: %r" % (gid,))
    n = 0
    for c in gid:
        n = n * 64 + ALPHABET.index(c)
    return uuid.UUID(int=n)


def ifc_guid(*roles):
    """The deterministic GlobalId of an object, from its roles."""
    return compress(uuid.uuid5(NAMESPACE, "/".join(roles)))
