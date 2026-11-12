// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `trunk.rs` — see that file for the whole picture.

use super::*;

// ---------------------------------------------------------------------------
// Panel labels: 6, 8 and 16 characters, an emoji counted once
// ---------------------------------------------------------------------------

/// A name cut for every panel width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub full: String,
    pub l6: String,
    pub l8: String,
    pub l16: String,
    pub chars: usize,
    pub over6: bool,
    pub over8: bool,
    pub over16: bool,
}

impl Label {
    pub fn new(full: &str) -> Label {
        let full = full.trim();
        let g = graphemes(full);
        let cut = |n: usize| g.iter().take(n).cloned().collect::<String>();
        Label {
            full: full.to_string(),
            l6: cut(6),
            l8: cut(8),
            l16: cut(16),
            chars: g.len(),
            over6: g.len() > 6,
            over8: g.len() > 8,
            over16: g.len() > 16,
        }
    }
    /// Derive, then apply explicit cuts (each fitted to its width).
    pub fn with_overrides(full: &str, l6: Option<&str>, l8: Option<&str>, l16: Option<&str>) -> Label {
        let mut l = Label::new(full);
        if let Some(s) = l6 {
            l.l6 = fit(s, 6);
        }
        if let Some(s) = l8 {
            l.l8 = fit(s, 8);
        }
        if let Some(s) = l16 {
            l.l16 = fit(s, 16);
        }
        l
    }
    pub fn to_value(&self) -> Value {
        json!({
            "full": self.full, "l6": self.l6, "l8": self.l8, "l16": self.l16, "chars": self.chars,
            "over6": self.over6, "over8": self.over8, "over16": self.over16,
        })
    }
}

/// The first `n` panel characters of `s`.
pub fn fit(s: &str, n: usize) -> String {
    graphemes(s.trim()).into_iter().take(n).collect()
}

/// Panel characters in `s`.
pub fn width(s: &str) -> usize {
    graphemes(s).len()
}

fn is_combining(c: char) -> bool {
    let u = c as u32;
    (0x0300..=0x036F).contains(&u)
        || (0x1AB0..=0x1AFF).contains(&u)
        || (0x1DC0..=0x1DFF).contains(&u)
        || (0x20D0..=0x20FF).contains(&u)
        || (0xFE00..=0xFE0F).contains(&u)
        || (0xFE20..=0xFE2F).contains(&u)
        || (0x1F3FB..=0x1F3FF).contains(&u)
        || (0xE0020..=0xE007F).contains(&u)
        || u == 0x200C
        || u == 0x200D
}

fn is_regional_indicator(c: char) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&(c as u32))
}

/// Grapheme clusters, the Trunk-Caster rule: combining marks, variation
/// selectors, skin tones, ZWJ sequences, flag pairs and CRLF are one each.
pub fn graphemes(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut join_next = false;
    let mut pending_flag = false;
    for c in s.chars() {
        let glue = join_next
            || is_combining(c)
            || (pending_flag && is_regional_indicator(c))
            || (c == '\n' && out.last().is_some_and(|g| g == "\r"));
        if glue && !out.is_empty() {
            out.last_mut().unwrap().push(c);
        } else {
            out.push(c.to_string());
        }
        join_next = c == '\u{200D}';
        pending_flag = is_regional_indicator(c) && !glue;
    }
    out
}
