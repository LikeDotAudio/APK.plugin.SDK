# Architecture of APK.plugin.SDK

← [Manual and Artifacts](INDEX.md) · [Repo README](../../README.md)

## What is in the tree, and where it really lives

| SDK path | Monorepo source | Why it is here |
|---|---|---|
| `Cargo.toml`, `README.md`, `.gitignore`, `Documentation/`, `.github/workflows/plugin-ci.yml` | `PODS/POD.THICK/DISCOVERY.Engine/SDK/` | The SDK's own files. |
| `runner/` (`apk-plugin-runner`) | `PODS/POD.THICK/DISCOVERY.Engine/SRC/RUNNER` | What every plugin `main` is. Its `Cargo.toml` is rewritten at publish: no `workspace =`, the edition written out from the engine's `[workspace.package]`, contracts at `../contracts/rust`. |
| `contracts/rust/` (`apkaudio-contracts`) | `contracts/rust` (its own `Cargo.lock` dropped) | The bus contract. |
| `contracts/tokens/` | `contracts/tokens` | `src/tokens.rs` compiles `../../tokens/brand.json` in. |
| `contracts/vectors/` | `contracts/vectors` | `tests/vectors.rs` and `tests/payloads.rs` read them. |
| `contracts/problems/` | `contracts/problems` | `src/problems.rs` compiles `../../problems/problems.json` in: every error code with its title, cause and fix steps (`problems::problem(code)`, `fix_for`), and `self_test::Check::fail_as` puts `problem` + `fix` in a self-test report. |
| `docker/plugin-base.yml` | `PODS/POD.THICK/DISCOVERY.Engine/Docker/plugin-base.yml` | What every plugin compose `extends:`; the CI places it at `DISCOVERY.Engine/Docker/` so `docker compose config` resolves. |
| `PUBLISHED_FROM` | written by `publish_sdk.py` | The monorepo and engine commits of this publish. |

The relative layout `contracts/rust` beside `contracts/tokens` is the monorepo's own, so the
`include_str!` paths in the contracts crate are unchanged.

## How a plugin resolves it

Standalone (a plugin repository cloned alone, its CI): cargo fetches this repository at the
pinned tag and finds `apk-plugin-runner` and `apkaudio-contracts` by name.

In the monorepo: the plugin is a member of the `DISCOVERY.Engine/SRC` workspace, whose
`[patch."https://github.com/LikeDotAudio/APK.plugin.SDK"]` replaces both with the local paths.
The lockfile records them as path packages, exactly as before, so `cargo build --locked`
(the plugin image) never needs the network for them.

## The CI workflow

`.github/workflows/plugin-ci.yml` is a `workflow_call` workflow: `cargo test` and `cargo clippy`
on every crate under the plugin's `SRC/`, the api-shape check of the SDK at the plugin's tag
(`runner/python/check-api-shape.py`), and `docker compose config` on the plugin's compose file.
A plugin that links a private sibling needs the `APK_DEPS_TOKEN` secret.
