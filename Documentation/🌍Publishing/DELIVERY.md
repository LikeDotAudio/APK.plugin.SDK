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
