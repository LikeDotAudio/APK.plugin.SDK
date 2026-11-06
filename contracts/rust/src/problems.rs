// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Every error this bench can name, and the way back from it — the Rust reader
//! for `contracts/problems/problems.json`, `include_str!`d so a malformed edit
//! fails the build.
//!
//! ```
//! use apkaudio_contracts::problems;
//! let p = problems::problem("device_unreachable")
//!     .detail("no reply from 10.0.0.7:5025")
//!     .subject("HP 34401A");
//! eprintln!("{}", p.text());
//! assert!(!p.fix.is_empty());
//! ```
//!
//! A plugin puts the CODE in `ControlError.code`; it does not send the fix over
//! the wire — readers look it up here. A code the catalogue does not know is
//! kept, with the generic way forward (`script_error`'s steps) behind it.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const PROBLEMS_JSON: &str = include_str!("../../problems/problems.json");

/// One catalogue entry.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Entry {
    /// input, output, control, bus, device, config, browser.
    pub area: String,
    /// What happened, one line.
    pub title: String,
    /// Why.
    pub cause: String,
    /// What to do now, most likely step first.
    pub fix: Vec<String>,
    /// Whether trying again, unchanged, can work.
    pub retry: bool,
}

#[derive(Deserialize)]
struct Catalogue {
    problems: BTreeMap<String, Entry>,
}

/// The catalogue, by code.
pub fn catalogue() -> &'static BTreeMap<String, Entry> {
    static CAT: OnceLock<BTreeMap<String, Entry>> = OnceLock::new();
    CAT.get_or_init(|| {
        serde_json::from_str::<Catalogue>(PROBLEMS_JSON)
            .expect("contracts/problems/problems.json is malformed -- see problems/README.md")
            .problems
    })
}

/// True when the catalogue knows `code`.
pub fn is_known(code: &str) -> bool {
    catalogue().contains_key(code)
}

/// The fix steps for a code, or empty when the catalogue does not know it.
pub fn fix_for(code: &str) -> Vec<String> {
    catalogue().get(code).map(|e| e.fix.clone()).unwrap_or_default()
}

/// One occurrence: a code, its entry, and what the caller knows about it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Problem {
    pub code: String,
    pub area: String,
    pub title: String,
    pub cause: String,
    pub fix: Vec<String>,
    pub retry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

/// The Problem for a code.
pub fn problem(code: &str) -> Problem {
    let entry = catalogue()
        .get(code)
        .or_else(|| catalogue().get("script_error"))
        .expect("problems.json must keep script_error");
    Problem {
        code: code.to_string(),
        area: entry.area.clone(),
        title: entry.title.clone(),
        cause: entry.cause.clone(),
        fix: entry.fix.clone(),
        retry: entry.retry,
        detail: None,
        subject: None,
    }
}

impl Problem {
    /// The specifics of this occurrence. For an unknown code it becomes the title.
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        let d = d.into();
        if is_known(&self.code) {
            self.detail = Some(d);
        } else {
            self.title = d;
        }
        self
    }
    /// What it happened to.
    pub fn subject(mut self, s: impl Into<String>) -> Self {
        self.subject = Some(s.into());
        self
    }
    /// A step only this caller knows, put before the catalogue's.
    pub fn first(mut self, step: impl Into<String>) -> Self {
        self.fix.insert(0, step.into());
        self
    }
    /// Numbered plain text, for a log line or a terminal.
    pub fn text(&self) -> String {
        let mut out = self.title.clone();
        if let Some(s) = &self.subject {
            out.push_str(" — ");
            out.push_str(s);
        }
        out.push_str(&format!(" [{}]", self.code));
        if let Some(d) = &self.detail {
            out.push_str(&format!("\nDetails: {d}"));
        }
        out.push_str(&format!("\nWhy: {}", self.cause));
        for (i, step) in self.fix.iter().enumerate() {
            out.push_str(&format!("\n{}. {step}", i + 1));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_has_a_way_back() {
        assert!(catalogue().len() >= 20);
        for (code, e) in catalogue() {
            assert!(!e.fix.is_empty(), "{code}");
            assert!(
                ["input", "output", "control", "bus", "device", "config", "browser"].contains(&e.area.as_str()),
                "{code}"
            );
        }
    }

    #[test]
    fn unknown_code_keeps_code() {
        let p = problem("wobble").detail("The wobble fired");
        assert_eq!(p.code, "wobble");
        assert_eq!(p.title, "The wobble fired");
        assert!(!p.fix.is_empty());
    }

    #[test]
    fn text_is_numbered() {
        let t = problem("no_lock").subject("Clock A").first("Check BNC 3").text();
        assert!(t.starts_with("No lock — Clock A [no_lock]"));
        assert!(t.contains("\n1. Check BNC 3"));
    }
}
