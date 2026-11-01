# Principles Trunk-Caster keeps

← [Philosophy](INDEX.md) · [How it works](TRUNK-CASTER.md) · [Repo README](../../README.md)

These principles come from the Trunk-Caster tree (`TRUNK-CASTER-SDK/README.md`, `plans/`, `src/`).
The "Kept by" column names that tree's code, not this repository's. [How it works, Part B](../%F0%9F%A7%A9Manual%20and%20Artifacts/TRUNK-CASTER.md)
is the plan for carrying them onto this SDK.

| Principle | Kept by |
|---|---|
| **Speak by protocol, not by brand.** A Dante trunk is a Dante trunk whatever vendor is behind it. Vendors are merged away into one entry per protocol. | `RUST-RUNTIME/src/protocol.rs` (`canonicalize`). `Frame.implemented_by` is informational and never used to address a trunk. |
| **Nothing in the SDK knows a brand as an address.** | `TRUNK-CASTER-SDK/src/lib.rs`: "a trunk is identified by protocol and frame, never by vendor" (plan 09). |
| **One sentence for every switch.** Crosspoints, end-to-end connections and check-ins are all a Ward-Beck line, readable by a person and a machine. | `TRUNK-CASTER-SDK/src/wardbeck.rs`; `TRUNK-CROSS POINTS/wardbeck.log`, `checkin.wb`. |
| **State can be rebuilt from the last check-in.** A listener that missed deltas needs only the latest CHECKIN and its CLOSE lines. | `Line::checkin`, `checkin.wb`; frame-sim checks in every `SIM_CHECKIN_SECONDS`. |
| **Anything to anything.** Direct when a trunk has room, else via one or two frames, never over-committing a frame. | `RUST-RUNTIME/src/routing.rs` (plan 03). |
| **Trust but verify.** Every live connection is cross-checked; a simulator's reported state is compared with the caster's belief and resent on persistent mismatch. | The runtime tick's cross-check and Ward-Beck inbox (plan 07, plan 10). |
| **Two transports must agree.** A simulator applies a line only once it has arrived over both TCP and MQTT; a lone copy is rejected and reported. | `TRUNK-CASTER-SDK/src/mqtt.rs`, `SIM_VALIDATE_MS`. |
| **One shape, one place.** New components depend on the crate rather than re-declaring shapes; `wire_version` is bumped on incompatible changes. | `TRUNK-CASTER-SDK/src/types.rs`, `WIRE_VERSION` (plan 09). |
| **Say what is real and what is simulated.** Routing, lines, contracts, the SNMP PDU and the voice bytes are real; the frames are simulated. | `TRUNK-CASTER-SDK/README.md`, "What is real and what is simulated". |
| **Do not write to equipment on a guess.** The RTS rename OID is a placeholder, flagged in every dispatch, and nothing is sent until a target is configured. | `vendor-policy.json`, `RUST-RUNTIME/src/snmp.rs`. |
