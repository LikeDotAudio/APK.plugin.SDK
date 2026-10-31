# `tokens/` — constants more than one directory must agree on, stated once

`brand.json` is the only place `#f4902c` is defined. Before 2026-08-30 it was written
out **1,039 times across 409 files in five top-level directories** and declared nowhere:
the repository had a brand and no name for it, so changing it was 409 files and a `sed`
that nothing could make safe, and a 410th file introducing `#f4902d` would never have
been noticed. `PLAN-162.01` is the measurement.

## Why this lives in `contracts/`

It was an owner's call between three homes, taken 2026-08-30 against the costs written
into that plan. `contracts/` won on reach: it is the one layer this repository already
imports from Rust, TypeScript and Python alike, and it already holds exactly this shape
of thing — one statement of a fact, with a reader per language beside it (`topics`,
`identity`, `heartbeat`).

The cost was named at the time and is not pretended away: **a colour is not a protocol**,
and this folder sits next to `schemas/` and `vectors/`, which are. The line that keeps it
honest is below.

## What may go in here

Values that **more than one top-level directory** must agree on exactly, and that a
human would otherwise retype. Today that is two things: the accent with the three
colours derived from it (`brand.json`), and the three colours a STATE can be with
every word this bench says them with (`state.json`).

What may **not**: anything one app can decide alone. A layout, a spacing scale, a font
stack that only the shell uses — those belong to the app that owns them. This folder
earns its place by being small; the moment it becomes a design system it has become the
category error its own plan warned about, and the fix is to move it out, not to keep
adding.

## Reading it

| From | How |
|---|---|
| TypeScript | `import { ACCENT } from '@apkaudio/contracts'` — or the JSON directly by relative path, which is what a Soft App with no dependency on this package does |
| Rust | `apkaudio_contracts::tokens::ACCENT` — `include_str!`d, so a malformed edit fails the build |
| Python | `json.load(open('APK:BareMetal/contracts/tokens/brand.json'))` |
| A page with no build (plain HTML/JS softapp, OS window) | `<link rel="stylesheet" href="/APK:OS/vendor/apk-contracts/apk-tokens.css">` then `var(--apk-accent)`, `rgba(var(--apk-accent-rgb), .3)`, `var(--apk-state-good)`, or `<span class="apk-state-text" data-apk-state="offline">`; in script, `window.APKContracts.stateTone(word)` / `.ACCENT` from `apk-contracts.global.js` beside it |
| A softapp that deploys its own site | the same files copied into its own `vendor/apk-contracts/` by `python3 .apk.scripts/vendor_contract_web.py <app>/vendor/apk-contracts`, held byte-identical by `check.sh webtokens` |

`state.json` has a fourth reader and it is the awkward one: **three browsers that
cannot import anything.** The widget engine loads plain JSX with no bundler, the OS
shell loads plain JS, and SCAN's Web_Front has no dependency on this package. Each
carries a copy of the sixteen words and the three hexes, and `check.sh statewords`
fails when any of them disagrees with the token — the same arrangement `check.sh
brand` holds the accent under, for the same reason. That check also **censuses** the
tracked JSON for status words the token does not name; those draw amber, which is
the honest default, but they are reported so a sixth vocabulary is a decision rather
than a fall-through.

## The browser copies are generated, not written

`contracts/scripts/gen.ts` writes `generated/web/apk-tokens.css` (both tokens as
CSS variables and `[data-apk-state="<word>"]` rules) and `apk-contracts.{mjs,global.js}`
(an esbuild bundle of `src/web.ts`: the state and brand readers in `src/tokens.ts`
beside the topic builders and the presence and heartbeat rules), and mirrors the
CSS and the global script into the OS shell's `vendor/apk-contracts/`. `npm run
gen:check` fails when any of them is not what the tokens say today; copies
elsewhere are held by `check.sh webtokens`. Edit a token, run `npm run gen`.

## What holds it

`.apk.scripts/check_brand_colour.py`, run by the old `check.sh brand` lane. Both were
deleted with the old check.sh in 2240c51e5, so NOTHING holds this today. It did two
things, and the second is the one that matters:

1. A **ratchet** on how many times the literal is still spelled out, seeded at the count
   on the day it was measured. It may shrink and never grow — the same shape as
   `prologue.baseline.json`, and for the same reason: a corpus measured once and cleaned
   is a corpus that regrows.
2. An **assertion** that every declared consumer still carries the token's value. A file
   that cannot import — `SAMPLE and PLAY`'s PWA `manifest.json` is static JSON served to
   a browser and has no build step — is listed there by path and checked against
   `brand.json`, so drift is caught even where an import is impossible.
