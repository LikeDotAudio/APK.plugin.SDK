# Delivery of APK.plugin.SDK

← [Publishing](INDEX.md) · [Repo README](../../README.md)

| | |
|---|---|
| **Repository** | https://github.com/LikeDotAudio/APK.plugin.SDK — public |
| **Releases** | annotated tags `sdk-vX.Y`, never moved |
| **Published by** | `python3 .apk.scripts/publish_sdk.py --publish X.Y` in the APK.audio monorepo |
| **Consumers** | every `PODS/POD.THICK/PLUGINS/*` crate by git + tag; the CI of every plugin repository via `plugin-ci.yml@main` |

## Publishing a version

1. Commit the change where it lives (`DISCOVERY.Engine/SRC/RUNNER`, `contracts/`, or `DISCOVERY.Engine/SDK/`). The script refuses uncommitted sources: a tag must name committed code.
2. `python3 .apk.scripts/publish_sdk.py --assemble /tmp/sdk --check` — builds the tree and `cargo check`s it alone.
3. `python3 .apk.scripts/publish_sdk.py --publish X.Y` — clones this repository, replaces its contents, commits, tags `sdk-vX.Y`, pushes, then fresh-clones the tag and `cargo check`s it again.
4. Move plugins to the new tag when they need it; the monorepo itself is unaffected (`[patch]`).

`--verify sdk-vX.Y` re-runs the fresh-clone check of any published tag.

## Delivered 2026-10-30 – 2026-11-04

From this repository's own commits (the commit message is the record):

- LOOSE-ci-node20: plugin CI checks out with actions/checkout@v5 (Node 24) (`2d7e828`)
- chore: pin APK.plugin.SDK sdk-v0.3 (`c72b92b`)
- chore: pin APK.plugin.SDK sdk-v0.4 (`8c8eaa2`)
- chore: pin APK.plugin.SDK sdk-v0.5 (`d8bdd0a`)
- fix(audit): runner foundations (session, unkept, jobs, Python panels, shared volumes); engine off the VISA crate, topics from contracts (`d5f3e7f`)
- chore: pin APK.plugin.SDK sdk-v0.6 (SDK docs); the [patch] block keeps DATA.ESP32's ocabin entry (`7a297d3`)
- chore: apkaudio-snmp [patch] -> members/LIB.SNMP, SDK sdk-v0.7, lock refresh (`0181a3f`)
- PLAN-3432.01: SDK README names contracts/python (apkaudio_contracts) (`63b4372`)
- docs(SDK): Trunk-Caster — how it works, its interfaces, principles and Next lists; the plan to bring it onto the SDK (`d2656bf`)
- SDK README: the Use it example pins sdk-v0.9, the current tag (`fa7a3dc`)
- roadmap lane: nothing to do is kept here — every roadmap item lives in the APK.audio roadmap (`9b0df4a`)
- docs(sdk): errors and the way back — contracts/problems, fail_as in self tests (`8e17434`)
- docs(sdk): a plugin sends a code, never a fix — contracts/problems in the SDK tree and principles (`82c5a73`)
