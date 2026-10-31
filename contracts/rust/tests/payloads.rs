// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Payload-vector suite — consumes the SAME vectors/payloads and
//! vectors/identity.json as the TypeScript tests, but through the
//! typify-GENERATED types: this is the codegen-fidelity proof (step 3b).

use apkaudio_contracts::agent_counters::{rates_between, AgentCounters, Sampler};
use apkaudio_contracts::control_value::ControlValue;
use apkaudio_contracts::gen::bare_metal_telemetry::BareMetalTelemetry;
use apkaudio_contracts::device_record::{map_v40_visa_record, DeviceRecord, LegacyVisaRecordV0};
use apkaudio_contracts::discovery_presence::DiscoveryPresence;
use apkaudio_contracts::heartbeat::{AgentHeartbeat, LegacyFailoverHeartbeatV0};
use apkaudio_contracts::scan_log_line::ScanLogLine;
use apkaudio_contracts::{identity, time};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vectors")
}

fn docs_in(rel: &str) -> Vec<(String, String)> {
    let dir = vectors_dir().join("payloads").join(rel);
    let mut out = vec![];
    for entry in fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}")) {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            out.push((
                format!("{rel}/{}", path.file_name().unwrap().to_string_lossy()),
                fs::read_to_string(&path).unwrap(),
            ));
        }
    }
    assert!(!out.is_empty(), "no vectors under {rel}");
    out
}

#[test]
fn agent_heartbeat_valid() {
    for (name, json) in docs_in("AgentHeartbeat/valid") {
        serde_json::from_str::<AgentHeartbeat>(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn agent_heartbeat_invalid() {
    for (name, json) in docs_in("AgentHeartbeat/invalid") {
        assert!(serde_json::from_str::<AgentHeartbeat>(&json).is_err(), "{name} should fail");
    }
}

#[test]
fn agent_heartbeat_legacy_named_not_v1() {
    for (name, json) in docs_in("AgentHeartbeat/legacy-v0") {
        serde_json::from_str::<LegacyFailoverHeartbeatV0>(&json)
            .unwrap_or_else(|e| panic!("{name} as v0: {e}"));
        assert!(serde_json::from_str::<AgentHeartbeat>(&json).is_err(), "{name} must not pass v1");
    }
}

#[test]
fn device_record_valid() {
    for (name, json) in docs_in("DeviceRecord/valid") {
        serde_json::from_str::<DeviceRecord>(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn device_record_invalid() {
    for (name, json) in docs_in("DeviceRecord/invalid") {
        assert!(serde_json::from_str::<DeviceRecord>(&json).is_err(), "{name} should fail");
    }
}

#[test]
fn discovery_presence_valid() {
    for (name, json) in docs_in("DiscoveryPresence/valid") {
        serde_json::from_str::<DiscoveryPresence>(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

/// Refused by [`discovery_presence::parse`], the reader every consumer goes
/// through, rather than by serde alone: the generated fields are `Option`, so
/// serde would take a body missing a key, and tolerant reading would take one
/// carrying identity the owner's rule keeps out of this document.
#[test]
fn discovery_presence_invalid() {
    for (name, json) in docs_in("DiscoveryPresence/invalid") {
        assert!(
            apkaudio_contracts::discovery_presence::parse(json.as_bytes()).is_none(),
            "{name} should fail"
        );
    }
}

#[test]
fn device_record_legacy_named_not_v1() {
    for (name, json) in docs_in("DeviceRecord/legacy-v0") {
        serde_json::from_str::<LegacyVisaRecordV0>(&json)
            .unwrap_or_else(|e| panic!("{name} as v0: {e}"));
        assert!(serde_json::from_str::<DeviceRecord>(&json).is_err(), "{name} must not pass v1");
    }
}

#[test]
fn map_v40_visa_record_vectors() {
    for (name, json) in docs_in("DeviceRecord/map") {
        let doc: Value = serde_json::from_str(&json).unwrap();
        let v0: LegacyVisaRecordV0 = serde_json::from_value(doc["input"].clone())
            .unwrap_or_else(|e| panic!("{name} input: {e}"));
        let expected: DeviceRecord = serde_json::from_value(doc["expected"].clone())
            .unwrap_or_else(|e| panic!("{name} expected: {e}"));
        let got = map_v40_visa_record(&v0).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(got, expected, "{name}");
    }
}

#[test]
fn identity_and_time_vectors() {
    let v: Value =
        serde_json::from_str(&fs::read_to_string(vectors_dir().join("identity.json")).unwrap())
            .unwrap();
    for case in v["deviceId"].as_array().unwrap() {
        let input = &case["input"];
        let got = identity::device_id_for(
            input["protocol"].as_str().unwrap(),
            input["serial"].as_str(),
            input["address"].as_str(),
            input["make"].as_str(),
            input["model"].as_str(),
        );
        assert_eq!(got, case["deviceId"].as_str().unwrap(), "case {case}");
    }
    for case in v["fnv1a64"].as_array().unwrap() {
        assert_eq!(
            identity::fnv1a64(case["input"].as_str().unwrap()),
            case["hex"].as_str().unwrap()
        );
    }
    for case in v["fromUnixSeconds"].as_array().unwrap() {
        assert_eq!(
            time::from_unix_seconds(case["seconds"].as_f64().unwrap()),
            case["iso"].as_str().unwrap()
        );
    }
    // Exact equality, not an epsilon. The conversion sums whole microhertz and
    // divides once precisely so that one double is reachable from both twins;
    // a tolerance would forgive the drift this file exists to catch.
    for case in v["timeOfDayHz"].as_array().unwrap() {
        let (ms, off) = (
            case["unixMs"].as_i64().unwrap(),
            case["utcOffsetMinutes"].as_i64().unwrap(),
        );
        assert_eq!(
            time::time_of_day_micro_hz(ms, off),
            case["microHz"].as_i64().unwrap(),
            "case {case}"
        );
        assert_eq!(
            time::time_of_day_hz(ms, off),
            case["hz"].as_f64().unwrap(),
            "case {case}"
        );
    }
}

#[test]
fn agent_counters_valid() {
    for (name, json) in docs_in("AgentCounters/valid") {
        serde_json::from_str::<AgentCounters>(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn agent_counters_invalid() {
    for (name, json) in docs_in("AgentCounters/invalid") {
        assert!(serde_json::from_str::<AgentCounters>(&json).is_err(), "{name} should fail");
    }
}

/// The differencing rule, against the same two samples the TypeScript suite
/// uses. `ratesBetween` and `rates_between` must agree on WHEN THEY REFUSE,
/// which is the half of this contract that carries the honesty.
#[test]
fn agent_counters_rates_between_agrees_with_ts() {
    let a: AgentCounters = serde_json::from_str(
        r#"{"schemaVersion":1,"agent":"apkaudio-ptp","at":"2026-08-29T14:00:00.000Z",
            "startedAt":"2026-08-29T14:00:00.000Z","intervalMs":10000,
            "counters":{"udpSeen":100,"l2Seen":200}}"#,
    )
    .unwrap();
    let b: AgentCounters = serde_json::from_str(
        r#"{"schemaVersion":1,"agent":"apkaudio-ptp","at":"2026-08-29T14:00:10.000Z",
            "startedAt":"2026-08-29T14:00:00.000Z","intervalMs":10000,
            "counters":{"udpSeen":150,"l2Seen":200}}"#,
    )
    .unwrap();

    let rates = rates_between(&a, &b).expect("two honest samples must difference");
    assert_eq!(rates["udpSeen"], 5.0, "50 over 10s");
    assert_eq!(rates["l2Seen"], 0.0, "flat is a rate, not an absence");

    // Refusals — each one a case where a number could be drawn and must not be.
    assert!(rates_between(&b, &a).is_none(), "out of order");
    assert!(rates_between(&a, &a).is_none(), "same instant");

    let restarted: AgentCounters = serde_json::from_str(
        r#"{"schemaVersion":1,"agent":"apkaudio-ptp","at":"2026-08-29T14:00:10.000Z",
            "startedAt":"2026-08-29T14:00:05.000Z","intervalMs":10000,
            "counters":{"udpSeen":3}}"#,
    )
    .unwrap();
    assert!(rates_between(&a, &restarted).is_none(), "across a restart");

    let other_agent: AgentCounters = serde_json::from_str(
        r#"{"schemaVersion":1,"agent":"apkaudio-AVB-Milan","at":"2026-08-29T14:00:10.000Z",
            "startedAt":"2026-08-29T14:00:00.000Z","intervalMs":10000,
            "counters":{"udpSeen":150}}"#,
    )
    .unwrap();
    assert!(rates_between(&a, &other_agent).is_none(), "two different agents");

    // A counter that went backwards without startedAt moving is a publisher
    // bug. It is DROPPED, not drawn — and the sample's other counters survive.
    let went_backwards: AgentCounters = serde_json::from_str(
        r#"{"schemaVersion":1,"agent":"apkaudio-ptp","at":"2026-08-29T14:00:10.000Z",
            "startedAt":"2026-08-29T14:00:00.000Z","intervalMs":10000,
            "counters":{"udpSeen":50,"l2Seen":300}}"#,
    )
    .unwrap();
    let rates = rates_between(&a, &went_backwards).unwrap();
    assert!(!rates.contains_key("udpSeen"), "backwards counter must be dropped");
    assert_eq!(rates["l2Seen"], 10.0, "the rest of the sample still counts");
}

/// The publisher cannot construct a dishonest sample: counters are `u64` and
/// `add`-only, and `publication()` hands back `retain = false` rather than
/// leaving that bool to the call site.
#[test]
fn sampler_publishes_non_retained_and_monotonic() {
    let mut s = Sampler::new("apkaudio-ptp", 1_788_012_000_000, 10_000).unwrap();
    s.declare("udpSeen");
    s.declare("l2Seen");
    let (topic, bytes, retain) = s.publication(1_788_012_000_000).unwrap();
    assert_eq!(topic, "APK.audio/System/Counters/apkaudio-ptp");
    assert!(!retain, "a retained counter outlives its process and lies");

    // A declared-but-unmoved counter is present at zero, so "nothing yet" is
    // distinguishable from "this agent does not count that".
    let first: AgentCounters = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(first.counters.len(), 2);

    s.add("udpSeen", 50);
    s.inc("l2Seen");
    let second = s.sample(1_788_012_010_000).unwrap();
    assert_eq!(first.started_at, second.started_at, "startedAt never moves in one process");
    let rates = rates_between(&first, &second).unwrap();
    assert_eq!(rates["udpSeen"], 5.0);
}

/// A counter key the schema forbids must be refused at sample time rather
/// than published and rejected by the consumer.
#[test]
fn sampler_refuses_a_counter_name_the_schema_forbids() {
    let mut s = Sampler::new("apkaudio-ptp", 1_788_012_000_000, 10_000).unwrap();
    s.add("udp_seen", 1);
    assert!(s.sample(1_788_012_010_000).is_err(), "snake_case key must not reach the bus");
}

/// typify does not carry numeric bounds into Rust, so `minimum: 0` on a
/// counter is enforced by zod on the CONSUMER side and by `u64` on the
/// PUBLISHER side, but not by `serde` in between. `rates_between` therefore
/// refuses a sample carrying a negative rather than dividing by it — see the
/// module docs on `agent_counters`.
#[test]
fn rates_between_refuses_a_negative_counter_serde_let_through() {
    let a: AgentCounters = serde_json::from_str(
        r#"{"schemaVersion":1,"agent":"apkaudio-ptp","at":"2026-08-29T14:00:00.000Z",
            "startedAt":"2026-08-29T14:00:00.000Z","intervalMs":10000,"counters":{"udpSeen":10}}"#,
    )
    .unwrap();
    let hostile: AgentCounters = serde_json::from_str(
        r#"{"schemaVersion":1,"agent":"apkaudio-ptp","at":"2026-08-29T14:00:10.000Z",
            "startedAt":"2026-08-29T14:00:00.000Z","intervalMs":10000,"counters":{"udpSeen":-5}}"#,
    )
    .unwrap();
    assert!(rates_between(&a, &hostile).is_none(), "a negative count is not a measurement");
}

/// `unix_ms_from_iso` is the inverse this contract's differencing rests on.
#[test]
fn iso_round_trips_through_unix_ms() {
    for ms in [0_i64, 1_788_012_000_000, 1_788_012_010_123, -86_400_000] {
        assert_eq!(time::unix_ms_from_iso(&time::iso_from_unix_ms(ms)), Some(ms), "{ms}");
    }
    assert_eq!(time::unix_ms_from_iso("2026-08-29T14:00:00Z"), Some(1_788_012_000_000));
    // Not ISO-8601 at large: an offset other than Z is refused, not read as UTC.
    assert_eq!(time::unix_ms_from_iso("2026-08-29T14:00:00+02:00"), None);
    assert_eq!(time::unix_ms_from_iso("yesterday"), None);
}

/// The scan narration line. Its `level` is the field PLAN-1040.01 found riding
/// in a payload with no vocabulary anywhere — the topic grammar's `LOG_LEVELS`
/// judges a topic SEGMENT of a different family and never looked at it. These
/// two tests are the generated Rust reader held to the same corpus the zod
/// reader is, so a level added on one side fails the other side's CI.
#[test]
fn scan_log_line_valid() {
    for (name, json) in docs_in("ScanLogLine/valid") {
        serde_json::from_str::<ScanLogLine>(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn scan_log_line_invalid() {
    for (name, json) in docs_in("ScanLogLine/invalid") {
        assert!(serde_json::from_str::<ScanLogLine>(&json).is_err(), "{name} should fail");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The families this file was NOT reading
// ─────────────────────────────────────────────────────────────────────────────
// `BareMetalTelemetry` and `ControlValue` have been in `gen.ts`'s REGISTRY, and
// so have had generated Rust types, for as long as the other four — and the
// arms below did not exist, so their vectors were read by the TypeScript suite
// alone. That is the same gap PLAN-873.01 was carved for `TimeMark`, found
// twice more while looking for it, and it happened for the same reason: the
// tests in this file name their directories one at a time, so a family that is
// never named is never missed. `payload_vector_families_all_have_a_reader` at
// the bottom is what stops that recurring.

#[test]
fn bare_metal_telemetry_valid() {
    for (name, json) in docs_in("BareMetalTelemetry/valid") {
        serde_json::from_str::<BareMetalTelemetry>(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn bare_metal_telemetry_invalid() {
    for (name, json) in docs_in("BareMetalTelemetry/invalid") {
        assert!(serde_json::from_str::<BareMetalTelemetry>(&json).is_err(), "{name} should fail");
    }
}

#[test]
fn control_value_valid() {
    for (name, json) in docs_in("ControlValue/valid") {
        serde_json::from_str::<ControlValue>(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn control_value_invalid() {
    for (name, json) in docs_in("ControlValue/invalid") {
        assert!(serde_json::from_str::<ControlValue>(&json).is_err(), "{name} should fail");
    }
}

/// Only the half of the legacy rule this side can state. The TypeScript suite
/// asserts both — that a v0 document parses as `LegacyControlValueV0Schema`
/// AND fails as v1 — because it has both schemas. Rust has no v0 twin for
/// `ControlValue` (unlike `AgentHeartbeat` and `DeviceRecord`, which do), so
/// minting one here to satisfy symmetry would be inventing a contract rather
/// than mirroring one. What is asserted is the half that carries the hazard: a
/// pre-schema document must never be read as a current one.
#[test]
fn control_value_legacy_is_not_v1() {
    for (name, json) in docs_in("ControlValue/legacy-v0") {
        assert!(serde_json::from_str::<ControlValue>(&json).is_err(), "{name} must not pass v1");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The census — PLAN-873.01
// ─────────────────────────────────────────────────────────────────────────────
// EVERY DIRECTORY UNDER `vectors/payloads/` IS EITHER READ HERE OR NAMED HERE
// WITH A REASON. The tests above name their families one at a time, which is
// why a sixth family (`TimeMark`) could appear with ten documents that only the
// TypeScript side ever read, and nothing failed: this file did not know the
// directory existed. A test that enumerates the directory instead cannot have
// that blind spot.
//
// An abstention is a decision and belongs beside the thing abstained from, not
// in a docstring in another crate — that is the shape of defect PLAN-656.01
// closed and PLAN-873.01 found reopened one layer up. So it is written here,
// and it is checked in both directions: a family in neither list fails, and a
// list entry with no directory fails too, so a rename cannot leave a stale
// abstention standing in for a family nobody reads.

/// Families whose vectors are deserialised above through the typify-generated
/// type. Adding an arm without adding the name here is caught by the second
/// half of the census, not by this one — this list is the claim, the arms are
/// the proof.
const RUST_READS: &[&str] = &[
    "AgentCounters",
    "AgentHeartbeat",
    "BareMetalTelemetry",
    "ControlValue",
    "DeviceRecord",
    "DiscoveryPresence",
    "ScanLogLine",
];

/// Families deliberately NOT given a Rust reader, each with the reason.
///
/// `TimeMark` is out by the decision taken in PLAN-656.01: its only Rust
/// consumer, `APK:audio:komand`'s `time_of_day.rs::ingest_mark`, reads exactly
/// two of the seven fields through `serde_json::Value::get` and must stay a
/// tolerant reader, because the payload's whole rule is that the other five
/// instants must not be read off a RETAINED message. A generated struct with
/// seven required fields is an invitation to the deserialisation that rule
/// exists to punish, so the abstention is the contract — not an omission, and
/// not a gap waiting to be filled.
const RUST_ABSTAINS: &[(&str, &str)] = &[
    (
        "TimeMark",
        "PLAN-656.01: the only Rust consumer must stay a tolerant two-field reader \
         of a retained message; a generated seven-field struct invites the \
         deserialisation the retained-instant rule forbids",
    ),
    (
        "WebRtcSignal",
        "2026-10-30 audit: the Rust producer owns its serde enum (PLUGIN.SOUNDCARD_IN \
         src/signal.rs `Signal`, linked by SOUNDCARD_OUT); contracts publishes the \
         JSON Schema and these cases for it to test against",
    ),
    (
        "MonitorProbeTelemetry",
        "2026-10-30 audit: PLUGIN.DESKTOP_MONITOR builds the document as serde_json \
         and holds its key sets in SRC/vectors/probe_documents.json; contracts \
         publishes the JSON Schema for the readers (multiviewer, NMOS bridge)",
    ),
    (
        "MonitorProbeStatus",
        "2026-10-30 audit: as MonitorProbeTelemetry — the producer's own test holds it",
    ),
    (
        "BingoDrawn",
        "2026-10-30 audit: both ends are plain-HTML pages (GAME.bingo publishes, \
         GAME.BingoCard reads) and no Rust process touches Games/Bingo/Drawn; \
         the zod schema and the zod-free rule in the browser bundle are its readers",
    ),
];

#[test]
fn payload_vector_families_all_have_a_reader() {
    let payloads = vectors_dir().join("payloads");
    let mut on_disk: Vec<String> = fs::read_dir(&payloads)
        .unwrap_or_else(|e| panic!("read {payloads:?}: {e}"))
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    assert!(!on_disk.is_empty(), "no payload vector families at {payloads:?}");

    let mut unaccounted = vec![];
    for family in &on_disk {
        let read = RUST_READS.contains(&family.as_str());
        let abstained = RUST_ABSTAINS.iter().any(|(n, _)| n == family);
        assert!(
            !(read && abstained),
            "{family} is claimed as both read and abstained — pick one"
        );
        if !read && !abstained {
            unaccounted.push(family.clone());
        }
    }
    assert!(
        unaccounted.is_empty(),
        "payload vector families with no Rust reader and no stated abstention: {unaccounted:?}. \
         Add an arm to this file and list it in RUST_READS, or state why not in RUST_ABSTAINS."
    );

    // The other direction: a name in either list that is no longer on disk is a
    // decision about something that does not exist, and a renamed family would
    // otherwise slip through the check above wearing its old name.
    for family in RUST_READS.iter().chain(RUST_ABSTAINS.iter().map(|(n, _)| n)) {
        assert!(
            on_disk.iter().any(|d| d == family),
            "{family} is listed here but has no directory under {payloads:?}"
        );
    }
}
