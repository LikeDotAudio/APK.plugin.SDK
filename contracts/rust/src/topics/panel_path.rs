// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `topics.rs` — see that file for the whole picture.

use super::*;

// --------------------------------------------- panel path → GUI topic

/// Canonized `topicMaker.jsx` semantics — the TS twin is
/// `contracts/src/topics/gui-path.ts`; the vector suite pins both.
pub fn gui_prefix_from_panel_path(file_path: &str) -> String {
    let segs = gui_segments_from_panel_path(file_path);
    if segs.is_empty() {
        GUI_ROOT.to_string()
    } else {
        format!("{GUI_ROOT}/{}", segs.join("/"))
    }
}

pub fn gui_segments_from_panel_path(file_path: &str) -> Vec<String> {
    let mut parts: Vec<&str> = file_path.split('/').filter(|p| !p.is_empty()).collect();
    if let Some(last) = parts.last() {
        if has_file_extension(last) {
            parts.pop();
        }
    }
    parts.iter().filter_map(|p| {
        let n = normalize_part(p);
        if n.is_empty() { None } else { Some(n) }
    }).collect()
}

const SKIP_TOKENS: [&str; 6] = ["display", "window", "left", "right", "top", "bottom"];

fn has_file_extension(s: &str) -> bool {
    match s.rfind('.') {
        Some(i) => {
            let ext = &s[i + 1..];
            !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric())
        }
        None => false,
    }
}

fn normalize_part(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    if raw.eq_ignore_ascii_case("oagui") {
        return "GUI".to_string();
    }
    if raw.chars().all(|c| c.is_ascii_digit()) {
        return String::new();
    }
    // strip a leading "<n>_" / "<n>-" ordering prefix
    let digits_end = raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len());
    let mut clean = &raw[digits_end..];
    if digits_end > 0 {
        clean = clean.strip_prefix(['_', '-']).unwrap_or(clean);
    }
    if clean.is_empty() {
        return String::new();
    }
    // base = clean with a trailing "[_-]?<digits>" suffix removed, lowercased
    let bytes = clean.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    let mut base_end = bytes.len();
    if i < bytes.len() {
        base_end = i;
        if i > 0 && (bytes[i - 1] == b'_' || bytes[i - 1] == b'-') {
            base_end = i - 1;
        }
    }
    let base = clean[..base_end].to_lowercase();
    if SKIP_TOKENS.contains(&base.as_str()) {
        return String::new();
    }
    // whitespace runs → single '_'
    let mut out = String::with_capacity(clean.len());
    let mut in_ws = false;
    for c in clean.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push('_');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}
