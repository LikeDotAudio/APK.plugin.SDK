# Trunk-Caster interfaces: folders, HTTP, Ward-Beck TCP, MQTT, environment

← [Manual and Artifacts](INDEX.md) · [How it works](TRUNK-CASTER.md) · [Repo README](../../README.md)

These are the interfaces Trunk-Caster has **today**, in its own tree. Each was taken from
`TRUNK-CASTER-SDK/README.md`, `plans/`, `src/` and the runtime and simulator sources. None of them
is an APK.audio contract yet. [How it works, Part B](TRUNK-CASTER.md#part-b--bringing-it-onto-the-sdk-the-plan-not-built)
is the plan for replacing them with this SDK's runner, topics and panels.

## Folder contract (`src/folders.rs`)

| Folder | What is in it |
|---|---|
| `KNOWN PROTOCOLS/<Vendor>/<Protocol>/` (and `<Vendor>/<Protocol>/` at the root) | Manuals (Markdown). |
| `TRUNK-ADVERTISEMENT/` | One JSON per frame, posted by the runtime about itself. |
| `TRUNK-CASTER-INTERFACE/` | Seekers drop `<id>.json`; the runtime answers `<id>.reply.json`; deleting the request releases it. |
| `TRUNK-USAGE/` | `state.json` (frames, trunks, connections, port names, last activity, crosspoint gains), `gains.jsonl`, `voices/`, `index.html`, `space.html`. |
| `TRUNK-USERS/` | One JSON per request (who, whom, when, why, where, how, how long); `labels.jsonl`; `index.html`. |
| `TRUNK-CROSS POINTS/` | `crosspoints.json`, `deltas.jsonl`, `wardbeck.log`, `checkin.wb`, `gpio.jsonl`, `renames.jsonl`, `seeking.json`, `seeks/`, `settings.json`, `index.html`. |
| `TRUNK-SIMULATOR/` | `fader.json`, `timeline.json`, `runs/`, `runs.json`, `captures/`, `index.html`, `racks.html`. |
| `TRUNK-RELAY/` | `relay.json`, `state.json`, `events.jsonl`, `index.html`. |
| `TRUNK-GLOBAL/` | `sites.json`, `globe.json`, `regions.json`, `preferences.jsonl`, `index.html`. |

## Ports

| Port | What |
|---|---|
| `TRUNK_PORT` | Pages, the JSON API and the `/ws` stream. The default is 7878 when run by hand (`cargo run -- serve`) and 8080 in its container. |
| `TRUNK_WB_PORT` (7879) | Ward-Beck lines over TCP. Simulators and real frames announce, receive lines, ack and check in here. |
| MQTT (1883 in its own compose) | The parallel transport for lines and voices. |

## HTTP API (all JSON)

| Method | Path | Body → Result |
|---|---|---|
| GET | `/api/status` | `Status` |
| GET | `/api/advertisements` | `[Frame]` |
| GET | `/api/protocols` | `[Protocol]` |
| GET | `/api/state` | routing state |
| GET | `/api/users` | recent `[UserRecord]` |
| GET | `/api/crosspoints` | settings, counters, crosspoints, recent events |
| GET | `/api/wardbeck` | recent Ward-Beck lines (text) |
| GET | `/api/checkin` | latest check-in (text) |
| GET | `/api/gpio`, `/api/renames`, `/api/seeks`, `/api/relay`, `/api/policy`, `/api/fader`, `/api/timeline`, `/api/crosscheck`, `/api/log` | as named |
| GET | `/api/tree`, `/api/device/<frame>` | the discovery tree; one device's lines, trunks, labels, timeline |
| GET | `/api/phonebook`, `/api/gpio/functions` | the phone book; the GPIO key functions |
| POST | `/api/request` | `Request` → `Reply` |
| POST | `/api/release` | `{id}` → `Reply` |
| POST | `/api/gpio` | `GpioCommand` (`pin` or `function`) → `GpioEvent` |
| POST | `/api/rename` | `Rename` (explicit `l6`, `l8`, `l16` accepted) → `[RenameDispatch]` |
| POST | `/api/panel-seek` | `PanelSeek` → `PanelOffer` |
| POST | `/api/fader` | `Fader` → `Fader` |
| POST | `/api/crosspoints/settings` | `CrosspointSettings` |
| POST | `/api/crosspoints/reset` | `{reset:true}` |
| GET | `/api/taps`, `/api/taps/<frame>` | `Taps`, `TapStrip` |
| POST | `/api/gain` | `GainSet` (trunk+end, connection+hop, or frame+input+output) → `GainEvent` |
| POST | `/api/taps/unity` | `TapsUnity {frame?}` → `TapsUnityResult` |
| GET | `/api/gains` | recent `[GainEvent]` |
| GET | `/api/voices`, `/api/voices/<frame>` | `VoiceSummary`, `FrameVoice` |
| GET | `/voices/<frame>-<ident\|call>.wav` | the re-assembled recording (`audio/wav`) |
| GET | `/api/bandwidth`, `/api/airports` | `Bandwidth`, `[Airport]` |
| POST | `/api/capture/start {note}`, `/api/capture/stop` | start / close a capture |
| GET | `/api/captures`, `/api/analyze/<id>`, `/api/capture/<id>` | captures; summary and check-ins; everything |

The full machine-readable catalogue is `TYPESCRIPT/api/openapi.json`, generated from
`TYPESCRIPT/api/routes.ts`. `/ws` streams a status frame twice a second and a globe delta every two
seconds.

## MQTT topics (`src/mqtt.rs`, plan 11)

| Topic | Direction | Payload |
|---|---|---|
| `trunk-caster/<protocol>/<frame>/line` | caster → frame | one Ward-Beck line |
| `trunk-caster/<protocol>/<frame>/ack` | frame → caster | the line, applied (double validated) |
| `trunk-caster/<protocol>/<frame>/reject` | frame → caster | `<reason>\|<line>` |
| `trunk-caster/<protocol>/<frame>/voice1` | frame → caster | 1-second voice chunks |
| `trunk-caster/<protocol>/<frame>/voice5` | frame → caster | 5-second voice chunks |

The voice payload is
`V1|<id>|<cut>|<index>|<total samples>|<rate>|<crc32>|<kind>|<text b64>|<pcm b64>|<sent ms>`.
Voices use their own broker connection at both ends (`voice-<host>`, `caster-voice-<site>`).

## Environment

The caster reads:

| Variable | Default | What it sets |
|---|---|---|
| `TRUNK_ROOT` | `/data` in the container | the project root |
| `TRUNK_SITE` | — | the SITE name |
| `TRUNK_BIND` | `0.0.0.0` in the container | the bind address |
| `TRUNK_PORT` | — | the pages and API port |
| `TRUNK_WB_PORT` | — | the Ward-Beck TCP port |
| `TRUNK_MQTT` | — | the broker; unset means Ward-Beck only |
| `TRUNK_PUBLIC_URL` | — | what the pages print |
| `TRUNK_VOICES_DIR` | — | where the WAVs go |

Flags still work: `--root`, `--site`, `--port`, `--no-http`, `--frames`, `--user-ports`, `--casters`, `--channels`, `--tick-ms`,
`--ticks` and `--once`.

The simulator (`frame-sim`) reads:

| Variable | Default | What it sets |
|---|---|---|
| `SIM_PROTOCOL` | — | the protocol it hosts |
| `SIM_NAME` | — | its name |
| `SIM_SITE` | — | its SITE |
| `SIM_FRAMES` | — | how many frames it hosts |
| `SIM_USER_PORTS` | — | user ports per frame |
| `SIM_CASTERS` | — | casters per frame |
| `SIM_CASTER` | — | the caster's Ward-Beck host:port |
| `SIM_CHECKIN_SECONDS` | 20 | the check-in interval |
| `SIM_MQTT` | — | the broker |
| `SIM_VALIDATE_MS` | 3000 | how long a line waits for its twin |
| `SIM_VOICE` | `on` | `off` silences it |
| `SIM_VOICE_FAULTS` | 0.05 | the share of voice chunks damaged |
| `SIM_VOICE_PACE` | 1.0 | 1 is real time |
| `SIM_VOICE_GAP_MS` | 600 | the gap between sayings |
| `SIM_VOICE_DIR` | `/home/sim/voices` | where it keeps its WAVs |
| `SIM_HTTP` | caster host:8080 | where `/api/sites` is read |

## Button-panel API (matrix simulators, `PANELS/README.md`)

The vendor matrix simulators (Python, under `KNOWN PROTOCOLS/VENDOR/<Vendor>/SIMULATOR/`, started
together by `TRUNK-SIMULATOR/launch_all.py`) serve a plain HTTP API for Stream Deck-style panels.
It has no auth and is meant for a lab network.

```
GET /api/help
GET /api/matrices
GET /api/ports?matrix=LOND
GET /api/claim?matrix=LOND&port=0&name=StreamDeck%20XL
GET /api/key?matrix=LOND&port=7&kind=talk&target=25&state=1     press
GET /api/key?matrix=LOND&port=7&kind=talk&target=25&state=0     release
GET /api/call?matrix=LOND&port=7&icom=PARI&remote=5
GET /api/hangup?matrix=LOND&port=7
GET /api/port?matrix=LOND&port=7            who is calling me, what I am talking to
```

Feedback (lighting a key when called) comes from polling `/api/port`.
