// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Behaviour lock for `topics::sanitize_segment` — the single sanitiser that
//! replaced ten divergent private `seg()` copies in the protocol crates.
//!
//! The contract is simple and total: whatever goes in, what comes out is a
//! legal T4 topic level, i.e. `is_segment` returns true for every output.

use apkaudio_contracts::topics::{is_segment, sanitize_segment};

/// The property that matters: output is ALWAYS a valid segment.
#[test]
fn output_is_always_a_valid_segment() {
    let hostile = [
        "Garage speaker",
        "a/b+c#d",
        "Living Room TV.",
        "Studio 2.4",
        "printer._ipp._tcp.local.",
        "  padded  ",
        "",
        "///",
        "###",
        "+",
        "..:..",
        "____",
        "Ünïcödé nàme",
        "\t\n mixed \r whitespace \t",
        "device:0a1b2c",
    ];
    for raw in hostile {
        let cleaned = sanitize_segment(raw);
        assert!(
            is_segment(&cleaned),
            "sanitize_segment({raw:?}) produced {cleaned:?}, which is_segment rejects"
        );
    }
}

#[test]
fn mqtt_metacharacters_cannot_survive() {
    // These three are the ones that would fork or widen the topic tree.
    for raw in ["a/b", "a+b", "a#b"] {
        let cleaned = sanitize_segment(raw);
        assert!(!cleaned.contains('/'), "{raw:?} leaked a separator");
        assert!(!cleaned.contains('+'), "{raw:?} leaked a single-level wildcard");
        assert!(!cleaned.contains('#'), "{raw:?} leaked a multi-level wildcard");
    }
}

#[test]
fn spaces_become_underscores() {
    assert_eq!(sanitize_segment("Garage speaker"), "Garage_speaker");
}

#[test]
fn dots_become_underscores_so_t4_holds() {
    // The old per-crate copies let '.' through; is_segment rejects it.
    assert_eq!(sanitize_segment("Studio 2.4"), "Studio_2_4");
}

#[test]
fn surrounding_whitespace_and_trailing_separators_are_trimmed() {
    assert_eq!(sanitize_segment("  Living Room TV.  "), "Living_Room_TV");
    assert_eq!(sanitize_segment("printer._ipp._tcp.local."), "printer__ipp__tcp_local");
}

/// DNS-SD service types lead with an underscore; trimming it would corrupt
/// the identifier, so only trailing underscores are dropped.
#[test]
fn leading_underscores_are_significant_and_preserved() {
    assert_eq!(sanitize_segment("_ipp._tcp"), "_ipp__tcp");
    assert_eq!(sanitize_segment("_raop._udp."), "_raop__udp");
}

#[test]
fn empty_and_wholly_illegal_input_never_yields_an_empty_level() {
    for raw in ["", "   ", "///", "###", "...", "___"] {
        assert_eq!(sanitize_segment(raw), "_", "input {raw:?} collapsed the topic path");
    }
}

#[test]
fn hyphens_and_alphanumerics_pass_through_untouched() {
    assert_eq!(sanitize_segment("apk-audio-01"), "apk-audio-01");
}

#[test]
fn sanitizing_twice_changes_nothing_further() {
    for raw in ["Garage speaker", "a/b+c#d", "Studio 2.4", "", "printer._tcp.local."] {
        let once = sanitize_segment(raw);
        assert_eq!(sanitize_segment(&once), once, "not idempotent for {raw:?}");
    }
}
