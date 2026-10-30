// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The brand's visual constants — the Rust reader for `contracts/tokens/brand.json`.
//!
//! The accent was written out 914 times across 404 files and declared nowhere;
//! `tokens/brand.json` is now the one place it is stated, and this is one of the
//! three readers. The standing decision that put it in this crate — and the
//! category error it knowingly tolerates, a colour beside `schemas/` — is
//! `002_🏛⚖📜 Philosophy/Styles/where a design token lives.md`.
//!
//! `include_str!` rather than a runtime read, for the reason every other
//! compiled-in default in this tree uses it: a malformed edit fails the BUILD,
//! at the moment somebody made it, rather than at whatever hour the first agent
//! that needed a colour happened to start.
//!
//! Parsed once, lazily, and handed out as `&'static str`. This crate may not
//! depend on a client library and does not want a colour type either: a hex
//! string is what every consumer here actually passes on — to a CSS variable, a
//! manifest field, a JSON payload — and inventing an `Rgb` struct would mean
//! every caller converting straight back.

use std::sync::OnceLock;

use serde::Deserialize;

/// The token file, compiled in. The path is relative to THIS source file.
const BRAND_JSON: &str = include_str!("../../tokens/brand.json");

#[derive(Debug, Deserialize)]
struct Swatch {
    #[serde(default)]
    hex: String,
    #[serde(default)]
    rgba: String,
}

#[derive(Debug, Deserialize)]
struct Brand {
    accent: Swatch,
    #[serde(rename = "accentHover")]
    accent_hover: Swatch,
    #[serde(rename = "accentTint")]
    accent_tint: Swatch,
    #[serde(rename = "onAccent")]
    on_accent: Swatch,
}

fn brand() -> &'static Brand {
    static PARSED: OnceLock<Brand> = OnceLock::new();
    PARSED.get_or_init(|| {
        // A panic is right here and a default is not. The alternative is an agent
        // that starts, paints itself the wrong colour, and tells nobody -- and the
        // file is compiled in, so this can only fire on a build somebody broke.
        serde_json::from_str(BRAND_JSON)
            .expect("contracts/tokens/brand.json is malformed -- see tokens/README.md")
    })
}

/// The accent. Active tool, selected row, focus ring, PWA `theme_color`.
pub fn accent() -> &'static str {
    &brand().accent.hex
}

/// Hover state for solid-accent buttons.
pub fn accent_hover() -> &'static str {
    &brand().accent_hover.hex
}

/// Hover wash for menu rows on dark panels — the accent at 18% over a dark ground.
pub fn accent_tint() -> &'static str {
    &brand().accent_tint.rgba
}

/// Text and icons sitting on an accent fill.
pub fn on_accent() -> &'static str {
    &brand().on_accent.hex
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_file_parses_and_every_swatch_is_present() {
        // The point of this test is the `include_str!`: it asserts the compiled-in
        // file is the shape the readers assume, so a hand edit to brand.json that
        // renames a key fails here rather than at the first caller.
        assert!(super::accent().starts_with('#'));
        assert!(super::accent_hover().starts_with('#'));
        assert!(super::on_accent().starts_with('#'));
        assert!(super::accent_tint().starts_with("rgba("));
    }

    #[test]
    fn the_tint_is_the_accent_at_low_alpha() {
        // Not decoration: the tint is DERIVED from the accent, and the two drifting
        // apart is the exact failure a single spelled-out hex could never catch.
        // The channels are read out of the accent rather than typed here: a
        // literal in this test would be a fifth copy of the value, and this
        // file is the one place that must not hold one.
        let accent = super::accent().trim_start_matches('#');
        let channels: Vec<u8> = (0..3)
            .map(|i| u8::from_str_radix(&accent[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        let expected = format!("rgba({},{},{},", channels[0], channels[1], channels[2]);
        assert!(
            super::accent_tint().starts_with(&expected),
            "accentTint {} is not accent {} at some alpha",
            super::accent_tint(),
            super::accent()
        );
    }
}
