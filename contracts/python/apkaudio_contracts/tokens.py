# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""The design tokens as Python values — contracts/tokens/{brand,state}.json,
shipped INSIDE this package (``brand.json`` / ``state.json`` beside this file,
written by contracts/scripts/gen.ts and held byte-identical by ``gen:check``),
so ``pip install contracts/python`` alone carries them and a service stops
COPYing ``contracts/tokens/brand.json`` into its image (DockTor).

    from apkaudio_contracts import tokens
    tokens.BRAND_ACCENT_HEX          # '#f4902c'
    tokens.brand()["accentHover"]["hex"]
    tokens.state_tone("online")      # 'good'
"""
from __future__ import annotations

import json
from functools import lru_cache
from pathlib import Path

_HERE = Path(__file__).resolve().parent


@lru_cache(maxsize=None)
def brand() -> dict:
    """tokens/brand.json, whole."""
    return json.loads((_HERE / "brand.json").read_text(encoding="utf-8"))


@lru_cache(maxsize=None)
def state() -> dict:
    """tokens/state.json, whole."""
    return json.loads((_HERE / "state.json").read_text(encoding="utf-8"))


BRAND_ACCENT_HEX: str = brand()["accent"]["hex"]
BRAND_ACCENT_HOVER_HEX: str = brand()["accentHover"]["hex"]
BRAND_ACCENT_TINT: str = brand()["accentTint"]["rgba"]
BRAND_ON_ACCENT_HEX: str = brand()["onAccent"]["hex"]


def state_tone(word: object) -> str:
    """The tone ('good' | 'dead' | 'kinda') a liveness word is drawn in; unknown words are the default."""
    s = state()
    return s["words"].get(str(word if word is not None else "").strip().lower(), s["default"])


def state_hex(word: object) -> str:
    return state()["tones"][state_tone(word)]["hex"]
