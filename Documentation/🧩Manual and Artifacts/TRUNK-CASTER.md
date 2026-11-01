# How Trunk-Caster works, and how it comes onto the SDK

← [Manual and Artifacts](INDEX.md) · [Interfaces](TRUNK-CASTER-INTERFACES.md) · [Principles](../%F0%9F%8F%9BPhilosophy/TRUNK-CASTER-PRINCIPLES.md) · [Repo README](../../README.md)

Trunk-Caster is an intercom trunk broker. This page has two parts. **Part A** describes the
program as it exists today in its own tree (`TRUNK-CASTER-SDK/`, `RUST-RUNTIME/`,
`TRUNK-SIMULATOR/`, `PANELS/`, `KNOWN PROTOCOLS/`), and every statement in it comes from that
tree's READMEs, plans and source. **Part B** is the plan for rewriting it onto this SDK. Nothing
in Part B is built yet.

Paths in Part A are relative to the Trunk-Caster tree, not to this repository.

---

## Part A — How it works today

### The idea

Trunk-Caster is protocol-first. It reads the manuals in `KNOWN PROTOCOLS` (and any vendor folders
beside it) and finds every protocol they describe. It then **advertises each protocol as a set of
identical virtual intercom frames**. It speaks by protocol, not by brand: a Dante trunk is a Dante
trunk whether Riedel, Clear-Com, RTS, Telos or Intracom is behind it. Anything can connect to
anything. A connection goes direct when a trunk has room, or *via* another frame when direct
capacity is low. Every switch Trunk-Caster makes is written as one sentence, the Ward-Beck line.

### Vocabulary

| Word | Meaning |
|---|---|
| protocol | What is advertised and trunked (Dante, SIP, 4-Wire, …). Never a brand. |
| frame | A virtual intercom matrix advertised for one protocol: 56 user ports + 8 casters (configurable). Ids look like `dante-03`. |
| caster | A trunk port on a frame. There are 8 per frame. |
| trunk | A link between one caster on frame A and one caster on frame B. Carries up to 8 channels. |
| connection | One conversation, user port to user port, over one or more trunks (hops). |
| direct / via | One hop, or routed through one or two intermediate frames when direct capacity is low. |
| panel | A user port. It takes the caller's name. |
| crosspoint | A closed input→output switch inside one frame (`U03→C1`, `C1→U03`, `C0→C5` on a via frame). |
| tap / fader | The gain of one crosspoint: the signal coming in on its input, crossed to its output. |
| voice | What a simulated frame says, in the accent of its country. |
| wave sender / cut | Each saying is cut into 1-second and 5-second chunks, and both cuts are sent. |
| check-in | A frame restating its state in Ward-Beck lines, periodically. |
| relay | The matrices hosted on this computer (two 8×8, one 2×2). They are real frames of protocol `relay`. |
| site | The name a runtime puts in the SITE fields (`--site`, default `trunk-caster`). |

### The Ward-Beck line

```
SITE,FRAME,INPUT,INPUT_NAME,CLOSE/OPEN,OUTPUT_NAME,OUTPUT,FRAME,SITE
```

The left side is where the signal enters and the right side is where it leaves. Each side names its own frame
and site.

* **Same frame on both sides:** a crosspoint inside that frame.
* **Different frames or sites:** an end-to-end connection, anything to anything.
* **`CLOSE` and `OPEN`:** `CLOSE` makes the connection, and `OPEN` says it is no longer there.
* **`INPUT_NAME` and `OUTPUT_NAME`:** they carry the people on the ports. A panel label is the name of whoever called through it.
* **Check-in:** `SITE,FRAME,CHECKIN,<time>,CLOSE|OPEN,<n> crosspoints,<n>,FRAME,SITE`.
  * It says `CLOSE` when the frame has crosspoints closed and `OPEN` when it is idle.
  * It is followed by one `CLOSE` line per closed crosspoint, so a listener that missed deltas can rebuild the frame from the latest check-in alone.
* **Commas inside a field:** the field is quoted, CSV-style, and a `"` doubles. See `TRUNK-CASTER-SDK/src/wardbeck.rs`, `q` and `split_csv`.

The line comes from Ward-Beck Systems' plain-text crosspoint control
(`KNOWN PROTOCOLS/Ward-Beck Systems/Crosspoint-Line/`). It is also a protocol in its own right
(`wardbeck-line`) and gets frames like any other. Every delta, end-to-end connection and check-in
is written as one of these lines, to `TRUNK-CROSS POINTS/wardbeck.log` and `checkin.wb`.

### The repertoire (`KNOWN PROTOCOLS`)

* **Layout:** one folder per vendor and one sub-folder per protocol, with Markdown manuals inside.
* **What is read:** the runtime reads `**Type:**`, `## Talking to the panel` and `## Trunking notes`. Any other file counts as a document.
* **Merging:** vendors are merged away, giving one entry per protocol that lists every vendor implementing it.
* **Canonical names:** `RUST-RUNTIME/src/protocol.rs` (`canonicalize`) maps folder names to ids. For example `2-Wire-Encore`, `2-Wire-TW` and `2-Wire-Performer` all become `2-wire`, each with a flavour.
* **Unknown folders:** kept under a slug, so nothing is dropped.

The families documented are:

| Family | Carries |
|---|---|
| AES67 | audio |
| Dante | audio + discovery |
| SMPTE ST 2110 | audio essence in a media fabric |
| SIP | call control + audio |
| 4-Wire | analog audio |
| 2-Wire Party-Line | analog audio + power + call |
| GPIO | on/off state |
| SNMP | status + alarms |
| Vendor IP Transport | audio + full key control, same brand |
| Digital Networked Party-Line | multi-channel party-line + control |
| Wireless Beltpack | audio + control over radio |
| Vendor AoIP | audio + logic |

The generated simulator compose file has one service per discovered protocol. There are 28 today.

### Frames and advertisements

* **Frames per protocol:** ten (`--frames`), each 56 user ports + 8 casters (`--user-ports`, `--casters`), with 8 channels per trunk (`--channels`).
* **Advertisement files:** the runtime posts one JSON per frame in `TRUNK-ADVERTISEMENT/`, the `Frame` type in `src/types.rs`. It holds:
  * id, protocol, family and `implemented_by`;
  * user ports, casters and channels;
  * `caster_status`: free or trunked, which trunk, which peer;
  * `port_names` (renamed after callers) and `live_ports`;
  * `last_connection` and `endpoint`.
* **Rewrites:** a file is rewritten only when its frame changes, and stale files are removed.
* **`implemented_by`:** informational only. It is never used to address a trunk.

### Asking for a trunk, and routing

* **Asking:** a seeker drops `<id>.json` (a `Request`: who, from, to, optional ports, why, where, `hold_seconds`, `gpio`) into `TRUNK-CASTER-INTERFACE/`.
  * The runtime answers with `<id>.reply.json` (a `Reply`: granted or denied, the `Connection`, the reason).
  * Deleting the request releases the connection.
  * Denied requests are retried every 10 s while the file is present.
  * The same request is accepted at `POST /api/request`.
* **Routing order:**
  * **Direct**, when an existing trunk between the two frames has a free channel, or when both frames have a free caster (a new trunk is created).
  * **Via one** intermediate frame, otherwise.
  * **Via two** intermediate frames, otherwise.
  * Among candidates, paths that need the fewest new trunks are preferred.
* **Limits:** a frame is never over-committed within one plan, and a trunk with no channels in use is torn down so its casters free up.

A connection from `A:Up` to `B:Uq` closes these crosspoints, in both directions:

* `Up-Cx` on frame A;
* `Cin-Cout` on every via frame;
* `Cy-Uq` on frame B.

### Crosspoints (`TRUNK-CROSS POINTS`)

| File | What it holds |
|---|---|
| `crosspoints.json` | Every crosspoint closed right now. |
| `deltas.jsonl` | One event per set or clear. It ends after `max_events` (default 1000). |
| `wardbeck.log` | The same deltas as lines, plus every end-to-end connection and check-in header. |
| `checkin.wb` | The latest check-in: per frame, a CHECKIN line and then its CLOSE lines, followed by a site section (`FRAME = *`) with every end-to-end line. |
| `gpio.jsonl`, `renames.jsonl` | Every GPIO command and every port rename. |
| `seeking.json`, `seeks/` | Panel seeks and the trunks allowed for each. |

### Renames and the vendor policy

A port takes its caller's name. Every caller gets 6-, 8- and 16-character cuts of the name, with full emoji
support: an emoji, flag or ZWJ sequence counts as one character. `vendor-policy.json` says how a
rename reaches each vendor's equipment:

* **`default`:** the name is kept inside Trunk-Caster.
* **RTS:** set to `snmp`. The runtime builds a real SNMPv2c SET PDU (`RUST-RUNTIME/src/snmp.rs`) and sends it only once a `target` is set.
  * The OID shipped is a placeholder. The RTS SNMP manual describes monitoring only, so the writable port-label object must be confirmed against the RTS MIB first.

### The runtime (`RUST-RUNTIME`, binary `trunk-caster`)

**Subcommands:**

| Subcommand | What it does |
|---|---|
| `discover` | Reads the manuals and prints the repertoire. |
| `advertise` | Writes one advertisement per frame, renders the pages, and exits. |
| `serve` | Runs: the folders plus the HTTP interface. |
| `status` | Reports status. |
| `compose` | Regenerates the simulator compose file. |

**Modules:** `discover`, `protocol`, `frame`, `routing`, `crosspoints`, `wb` (the Ward-Beck TCP
port), `interface`, `users`, `relay`, `snmp`, `runtime` (the tick), `http`, `html`, `ws`, `util`.

**The tick:** every 250 ms (500 ms in Docker), the runtime:

1. re-reads the fader and the settings;
2. advances the simulated clock;
3. drains the Ward-Beck inbox (an announce gets the frame's state sent back; a check-in is compared, with a resend on a persistent mismatch);
4. serves interface files and answers seeks;
5. expires connections;
6. generates load;
7. cross-checks, samples and checks in, each when it is due;
8. persists, at most once a second.

Writes are queued under the lock and flushed after it, and deletes go to a janitor thread.

### Simulator containers and the double-validated line

Each intercom simulator (`TRUNK-SIMULATOR/frame-sim`) is a small binary in its own container.
`SIM_PROTOCOL=dante`, for example, hosts `dante-01..10`. It:

* connects to the caster's Ward-Beck TCP port and announces itself with CHECKIN lines;
* applies CLOSE/OPEN lines to its 64×64 matrices, echoing each applied line as its acknowledgement;
* rejects lines that name ports it does not have;
* checks in with its full state every `SIM_CHECKIN_SECONDS` (20).

The caster compares what each simulator reports with what it believes, and resends state on a
persistent mismatch. Real frames can use the same port and the same lines.

**MQTT runs in parallel with the Ward-Beck port:**

* Every line the caster sends over TCP is also published to `trunk-caster/<protocol>/<frame>/line`.
* A simulator applies a CLOSE/OPEN line only once it has arrived over **both** transports. It then echoes the line on TCP and acks it on `…/ack`.
* A copy whose twin does not arrive within `SIM_VALIDATE_MS` (3000) is dropped and reported on `…/reject` as `<reason>|<line>`.
* The caster counts acks and rejects per frame, and its persistent-mismatch resend recovers anything rejected.
* The MQTT client is std-only (`trunk_caster_sdk::mqtt`, MQTT 3.1.1, QoS 0).

### Voices, TAPS and bandwidth

Every simulated frame talks, all at once, in the accent of its country (espeak-ng at 8 kHz, 16-bit
mono, in the frame-sim image).

* **What it says:** first its ident (frame, site, country, continent, airport code, protocol, simulator), then each of its connections. It learns a connection from the end-to-end `CLOSE` and stops at its `OPEN`.
* **How it is sent:** each saying is cut into 1-second and 5-second chunks with CRC-32. Both cuts go out at the same time, on `…/voice1` and `…/voice5`, over a separate broker connection.
* **Re-assembly:** the caster rebuilds each saying second by second. It takes the 1-second chunk when that is intact, otherwise that second's slice of the 5-second chunk (*corrected*), otherwise silence (*lost*).
* **Fault injection:** `SIM_VOICE_FAULTS` (default 5%) damages chunks so the correction has work to do.
* **Measured:** with 707 frames talking, about 1 second in 30 is corrected from the 5-second cut, and about 1 in 1000 is lost on both.

**TAPS and Mixes** is a mixer on the Overlord page with one fader per closed crosspoint, from -60 to +20 dB. Its
values are stored as `xp_gains` in `TRUNK-USAGE/state.json`. Gains also sit at each trunk end
and on each hop of a call. Every change is a `GainEvent` in `TRUNK-USAGE/gains.jsonl`.

**Bandwidth:** every voice chunk heard is metered per frame and per protocol, and spread over every trunk its
calls cross. With every frame connected that is about 100–150 Mbit/s.

### The simulator: fader, broadcast day, recorder

* **The fader** is `TRUNK-SIMULATOR/fader.json`, re-read every tick. It sets:
  * speed (simulated seconds per real second; 0 freezes);
  * baseline requests per simulated minute and mean hold (600 s);
  * cross-check and sample intervals, running, and seed;
  * `day`, the news cycle.
* **The broadcast day** follows the news cycle:
  * hourly rushes (×3, :55 to :10);
  * radio prep on the hours and halves (×2);
  * the six o'clock news (×6, building from an hour before);
  * quiet overnight (×0.3 before 05:00).
* **Local time:** every site follows its own solar time from its longitude, so the busy part of the day sweeps around the globe.
* **Park and sweep:**
  * A finished call stays connected but muted until the next sweep (`round_robin_seconds`, 60), and calling the same place again before then picks it up.
  * Every line has a 15-minute lease (`lease_seconds`), reset on use. At zero the line is red ("may be dead"), parked, and reclaimed first.
* **Daily reset:** at 11:00 UTC on the simulated clock everything is released and every sender reset. Traffic reconnects at 12:00.
* **Cross-check:** every live connection is verified for chained hops, existing trunks, capacity, no shared user port and no double-bound caster.
* **Recorder:**
  * A run is the stretch between parameter changes. Each closed run is summarised in `TRUNK-SIMULATOR/runs/`.
  * `/analyzer` captures START/STOP into `TRUNK-SIMULATOR/captures/`, with check-ins at one hour, half an hour and a quarter hour of the simulated day.
  * Every append-only log is capped at 100 MB and rotated once.

### The globe (`TRUNK-GLOBAL`)

* **Data files:**
  * `sites.json` lists where frames live: a country capital hosts 10 frames, a state or province 1, and a smaller city an 8×8 plus a 2×2.
  * `globe.json` says how the globe is drawn.
  * `regions.json` holds usage per region.
  * `preferences.jsonl` logs every user preference ever set.
* **What frames carry:** each frame carries its site, country and coordinates, and the SITE field of every line is the frame's site.
* **Vendor badges on flows:** 🔵 Clear-Com, 🔴 Riedel, 🟣 RTS, 🟡 Telos, 🟢 Intracom, 🟠 Ward-Beck, ⚪ relay.
* **Discovery tree:** `/api/tree` runs Global → continent → country → site → device, and `/api/device/<frame>` gives one device's own lines, trunks, labels and timeline.

### The relay (`TRUNK-RELAY`)

`relay.json` lists the matrices this computer hosts: two 8×8 and one 2×2, each a frame of protocol
`relay`, so any frame can be routed to a relay port. `state.json` is the live patch.

The Relay page enumerates the computer's audio devices in the browser and meters every input
(peak, RMS, peak hold in dBFS). It binds devices to relay ports and logs peaks over
`over_threshold_dbfs` (−6) to `events.jsonl`.

### Panels, keys, lights and the phone book

* **`/panels`:** lists every panel with its trunk-caster keys. The casters its call rides are lit, and its names are shown at 6, 8 and 16 characters.
* **The light board:** one light per frame. It is:
  * lit when the frame carries a call;
  * blinking on a change or a GPIO blink;
  * red when a line may be dead;
  * grey when every line is parked;
  * hollow when the frame's simulator is away.
* **GPIO keys:** every pin is a key function that rides the call's trunks.

| Pin | Function |
|---|---|
| 1 | blink panel |
| 2 | check lights |
| 3 | set latch (never parked, swept or reclaimed) |
| 4 | set listen |
| 5 | unset latch |
| 6 | clear |
| 7 | last caller |
| 8 | ping |

* **Phone book:** everyone who has called, where their names are, how often they call and whom they last called.
* **Button panels (`PANELS/`):** Stream Deck-style definitions validated by `panel.schema.json`.
  * Each definition is `{ device, matrix, port, buttons[] }`, and each button has an action: `talk`, `listen`, `call`, `hangup`, `reply`, `page` or `none`, in `momentary` or `latch` mode, with `idle` / `active` / `incoming` colours.
  * They reach a matrix simulator's HTTP button API through Bitfocus Companion, the Stream Deck app, or `streamdeck_emulator.py`.
* **The soft panel (`TRUNK-SIMULATOR/trunk_panel.py`):** makes a computer an intercom port. It has 8 TX and 8 RX keys and runs over the Trunk-Caster audio and control protocol (`tc_protocol.py`):
  * UDP, a series of 256 sockets per intercom;
  * 20 ms frames of 8 kHz 16-bit PCM;
  * HELLO / KEY / STATE messages.

### Pages and streaming

* **Pages:**
  * `/` is the globe.
  * `/continents`, `/overlord` (the control room), `/usage`, `/space`, `/racks`, `/relay`, `/users`, `/crosspoints`, `/simulator`, `/panels` and `/analyzer` are the other views.
  * `/home` is the about page.
  * The pages are also written as static HTML into their folders.
* **Streaming:** telemetry streams over a WebSocket at `/ws`. It sends a status frame twice a second and a globe delta every two seconds, and pages update in place.

### The crate (`TRUNK-CASTER-SDK`)

* **What it is:** the one crate every Trunk-Caster component speaks through.
* **Modules:**

| Module | Holds |
|---|---|
| `types.rs` | Every JSON shape. `WIRE_VERSION` is 1, bumped on incompatible changes. |
| `folders.rs` | The folder contract. |
| `wardbeck.rs` | The line. |
| `client.rs` | `FileClient` over the folders, and `HttpClient` over the runtime's port. |
| `mqtt.rs` | The second transport. |
| `voice.rs` | WAV, CRC-32, the cuts, the re-assembler, the accents. |
| `sites.rs` | Sites, continents, oceans, airports. |
| `labels.rs` | The 6/8/16 cuts. |

* **TypeScript mirror:** `TYPESCRIPT/sdk` and `TYPESCRIPT/api`. The full route catalogue is `TYPESCRIPT/api/openapi.json`.

### What is real and what is simulated

The source draws this line itself:

* **Real:**
  * discovery, routing, capacity and crosspoints;
  * the Ward-Beck lines and the folder and HTTP contracts;
  * the SNMP PDU and the relay's frames;
  * the voice traffic, which is real 8 kHz audio over a real broker, so the bandwidth is measured.
* **Simulated:** the frames themselves. Nothing yet speaks Dante, SIP or 4-Wire on a wire.

---

## Part B — Bringing it onto the SDK (the plan, not built)

Today Trunk-Caster does not use this SDK.

* **Bus:** it has its own MQTT client and its own topic root (`trunk-caster/…`).
* **Interface:** its own HTTP server and pages.
* **Configuration:** environment variables and JSON files.
* **Self-test:** none on the bus.

The owner has asked for a rewrite so it fits the APK.audio ecosystem. The table below is how each
concept would map. It is a plan: no row is implemented.

| Trunk-Caster today | On the SDK (planned) |
|---|---|
| `RUST-RUNTIME` (`trunk-caster serve`) | A plugin crate on `apk-plugin-runner` (`apk_plugin_runner::main!`). It gets the last will, `config.ini` and `incoming/api` published retained, the self test on `outgoing/selftest_run`, the health file and SIGTERM handling from the runner, like every other plugin. |
| `frame-sim` (one container per protocol) | A plugin binary on the same runner, one container per protocol under a compose profile. |
| Environment variables, `fader.json`, `settings.json`, `relay.json` | `config.ini` keys, each read by a named function and published retained to `<topic>/config`. |
| `trunk-caster/<protocol>/<frame>/{line,ack,reject,voice1,voice5}` | Topics under the contract's grammar, `APK.audio/System/Protocols/<token>/…`, built with `apkaudio-contracts` rather than spelled by hand. The Ward-Beck line stays the payload. |
| Advertisements (`TRUNK-ADVERTISEMENT/*.json`) | One device per frame under the plugin's `Device/<key>/…` tree, plus a presence document at `APK.audio/Discovery/<token>/<id>`, as the other plugins do. |
| JSON shapes in `src/types.rs` (`WIRE_VERSION`) | Payload types and schemas in `apkaudio-contracts`, with vectors in `contracts/vectors`. |
| Requests, releases, GPIO, renames (`POST /api/…`, interface files) | Commands on the plugin's `outgoing/…` topics, answered on `incoming/…`. |
| The light board, frame cards, panel keys | Device panels: templates in the plugin's `SRC/gui/`, published retained at `APK.audio/System/Gui/Panels/<device>` by the runner's panel book. |
| Pages (`/`, `/overlord`, `/racks`, `/relay`, `/simulator`, …) | APK.audio soft apps, one per view, opened from the OS menus and reading the bus instead of the caster's own HTTP port. |
| Its own Mosquitto | The APK.audio broker. |
| The double-validated line (TCP + MQTT) | Kept as the frame protocol. The MQTT half moves to the contract topics; the Ward-Beck TCP port stays for real frames. |

Open questions the plan still has to settle:

* whether the static-file folder contract survives, or the bus replaces it;
* how the voice traffic (about 100–150 Mbit/s with every frame connected) sits beside the house bus;
* which pieces are one plugin and which are several.
