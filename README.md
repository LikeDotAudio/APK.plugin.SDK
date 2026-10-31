# APK.plugin.SDK

The two crates every APK.audio plugin links, so a plugin repository builds on its own:

| Crate | Path | What |
|---|---|---|
| `apk-plugin-runner` | `runner/` | The five things every plugin container does once: connect with a last will, publish its `config.ini` and `incoming/api` retained, answer `outgoing/selftest_run`, keep the health file, stop cleanly on SIGTERM. `apk_plugin_runner::main!` is a plugin's whole `main`. `runner/python/apk_plugin_api.py` is the same `incoming/api` document for a Python probe. |
| `apkaudio-contracts` | `contracts/rust/` | The bus contract: topic grammar and the generated payload types. serde + serde_json + regress and nothing else. |
| `apkaudio_contracts` (Python) | `contracts/python/` | The same contract for a Python plugin or worker: topic families, segments, payload builders, brand tokens. Standard library only. Ships beside `runner/python` so one tag pins both. |

**Wiki:** [Documentation/README.md](Documentation/README.md) — principles, architecture, delivery.

## Use it

```toml
[dependencies]
apk-plugin-runner  = { git = "https://github.com/LikeDotAudio/APK.plugin.SDK", tag = "sdk-v0.7" }
apkaudio-contracts = { git = "https://github.com/LikeDotAudio/APK.plugin.SDK", tag = "sdk-v0.7" }
```

Pin a **tag**. Tags are never moved; a change is a new `sdk-vX.Y`.

A Python plugin takes both halves from the same tag: clone it at `--branch sdk-vX.Y` and put
`runner/python` and `contracts/python` on `PYTHONPATH` (`import apk_plugin_runner`,
`from apkaudio_contracts import families`).

CI for a plugin repository is five lines in `.github/workflows/ci.yml`:

```yaml
on: [push, pull_request]
jobs:
  ci:
    uses: LikeDotAudio/APK.plugin.SDK/.github/workflows/plugin-ci.yml@main
    secrets: inherit
```

## Do not edit this repository

It is **published from the APK.audio monorepo** by `.apk.scripts/publish_sdk.py`; every commit
here is one publish, and `PUBLISHED_FROM` names the monorepo commits it came from. The runner's
source is `PODS/POD.THICK/DISCOVERY.Engine/SRC/RUNNER`, the contracts' is `contracts/`, and this
README, the workflow and the root manifest are `PODS/POD.THICK/DISCOVERY.Engine/SDK/`. An edit
made here is overwritten by the next publish.

Inside the monorepo nothing fetches this repository: the plugin workspace
(`DISCOVERY.Engine/SRC/Cargo.toml`) carries a `[patch."https://github.com/LikeDotAudio/APK.plugin.SDK"]`
that points both names back at those paths.
