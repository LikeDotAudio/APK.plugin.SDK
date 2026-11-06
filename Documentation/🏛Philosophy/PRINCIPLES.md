# Principles APK.plugin.SDK keeps

← [Philosophy](INDEX.md) · [Repo README](../../README.md)

| Principle | Kept by |
|---|---|
| **One source, no second copy.** The SDK is an output of the monorepo, like an image: the runner and the contracts are edited in one place and published, never copied by hand and never checked out a second time inside the monorepo. | `.apk.scripts/publish_sdk.py` (monorepo) assembles every file; `PUBLISHED_FROM` names the commits; this repository is deliberately not a submodule. |
| **A plugin repository builds on its own.** Nothing a plugin needs from the house lives behind a `../../..` path. | The runner's manifest is rewritten at publish to a plain member (`publish_sdk.py`, `rewrite_runner_manifest`); `escapes()` refuses a tree with any path leaving it or any inherited field. |
| **Tags are never moved.** A plugin pins `sdk-vX.Y`; moving a tag would change what an already-green plugin builds against. | `publish()` refuses a version whose tag exists. |
| **The monorepo does not fetch itself.** Day-to-day work builds the local runner and contracts, exactly as before the SDK existed. | `[patch."https://github.com/LikeDotAudio/APK.plugin.SDK"]` in `DISCOVERY.Engine/SRC/Cargo.toml`; with the lockfile in place cargo never contacts GitHub (the image builds `--locked`). |
| **Contracts carry types, not clients.** `apkaudio-contracts` is serde + serde_json + regress only. | `contracts/rust/Cargo.toml` header. |
| **A plugin sends a code, never a fix.** An error a plugin reports is a code from the problems catalogue; every reader looks the same title, cause and steps up, so a better fix reaches every plugin and app at once. | `contracts/problems/problems.json` (shipped as `contracts/problems/`), `apkaudio_contracts::problems`, `Check::fail_as` in `self_test.rs`; root doctrine *Every error carries its way back*. |
| **Public is the runner and the contracts, nothing else.** Plugin code and house libraries stay in their own private repositories. | `publish_sdk.py` copies exactly the paths in its docstring. |
