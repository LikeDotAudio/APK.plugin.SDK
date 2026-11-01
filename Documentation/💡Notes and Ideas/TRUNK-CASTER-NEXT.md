# Trunk-Caster's own "Next" lists

← [Notes and Ideas](INDEX.md) · [How it works](TRUNK-CASTER.md) · [Repo README](../../README.md)

Each Trunk-Caster plan (`TRUNK-CASTER-SDK/plans/01..11`) ends with a **Next** list. This page
gathers those lists verbatim in substance, so the rewrite onto the SDK can see them. They are not
APK.audio plans. APK.audio plans are cards in the monorepo's `Documentation/🛣Roadmap/roadmap.htm`.

| Plan | Next |
|---|---|
| 01 Known protocols | Richer digests (ports, OIDs, API endpoints) pulled out of the manuals; a per-vendor `device.json` describing a real frame's address so the runtime can bind to it. |
| 02 Advertisement | Other devices (real frames, other runtimes) post their own files; the runtime treats them as remote frames it can trunk to (multi-site). |
| 03 Caster interface | Priorities and pre-emption; reservations (hold a trunk for a show); requests that name a protocol rather than a frame. |
| 04 Usage | Drag-to-connect on the space view; a per-protocol capacity forecast from the timeline. |
| 05 Users | Export to CSV; per-show grouping; authentication of `who`. |
| 06 Cross points | Push Ward-Beck lines to a TCP or serial listener; accept Ward-Beck lines as commands. |
| 07 Simulator | Scenario files (a show's call sheet replayed on the simulated clock); failure injection (stop a simulator container and watch via routing take over); a simulator that answers SNMP so the RTS rename path can be exercised; per-device samples. |
| 08 Relay | Bind local ports to real audio devices (WASAPI or ASIO) so a closed crosspoint moves audio; expose the relay's own Ward-Beck lines on a local TCP port. |
| 09 SDK | Publish as a workspace member; a Python mirror of the types; JSON Schema export from the Rust types. |
| 10 Runtime | Bind frames to real devices over the Ward-Beck port; accept Ward-Beck lines as commands; a Windows service install; move the relay's audio into the runtime. |
| 11 TAPS, voices, bandwidth | Let a TAPS fader scale the PCM crossing it; G.711 or Opus instead of 16-bit PCM; a capacity model with red trunks when voice exceeds a channel's budget. |

The README's own headline list is: bind a frame to a real device (vendor + address) so renames,
GPIO and crosspoints go out on a real transport; agree the RTS OID; let the relay drive actual audio
devices; multi-site (two runtimes exchanging Ward-Beck lines).

Several of these line up with what the SDK already gives a plugin: plan 09's "JSON Schema export"
and "Python mirror" match what `apkaudio-contracts` does with its schemas and its Python twin. That
pairing is an observation for the rewrite, not something built.
