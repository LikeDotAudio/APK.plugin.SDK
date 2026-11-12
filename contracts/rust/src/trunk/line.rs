// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `trunk.rs` — see that file for the whole picture.

use super::*;

// ---------------------------------------------------------------------------
// The Ward-Beck line
// ---------------------------------------------------------------------------

/// One Ward-Beck line. `switch` is `true` for CLOSE, `false` for OPEN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub site: String,
    pub frame: String,
    pub input: String,
    pub input_name: String,
    pub close: bool,
    pub output_name: String,
    pub output: String,
    pub out_frame: String,
    pub out_site: String,
}

impl Line {
    /// A check-in: `SITE,FRAME,CHECKIN,<when>,CLOSE|OPEN,<n> crosspoints,<n>,FRAME,SITE`.
    pub fn checkin(site: &str, frame: &str, when: &str, closed: usize) -> Line {
        Line {
            site: site.into(),
            frame: frame.into(),
            input: CHECKIN.into(),
            input_name: when.into(),
            close: closed > 0,
            output_name: format!("{closed} crosspoints"),
            output: closed.to_string(),
            out_frame: frame.into(),
            out_site: site.into(),
        }
    }
    pub fn is_checkin(&self) -> bool {
        self.input == CHECKIN
    }
    /// Same frame, same site: a crosspoint inside one matrix.
    pub fn is_local(&self) -> bool {
        self.frame == self.out_frame && self.site == self.out_site
    }
    /// The opposite switch, same crosspoint.
    pub fn flipped(&self) -> Line {
        Line { close: !self.close, ..self.clone() }
    }
    /// Parse one line. Fields are trimmed; a field holding a comma or a quote
    /// is double-quoted, a quote inside doubled.
    pub fn parse(text: &str) -> Result<Line, String> {
        let f = split_csv(text.trim());
        if f.len() != 9 {
            return Err(format!("expected 9 fields, got {}", f.len()));
        }
        let close = match f[4].to_ascii_uppercase().as_str() {
            "CLOSE" => true,
            "OPEN" => false,
            other => return Err(format!("field 5 must be CLOSE or OPEN, got {other}")),
        };
        Ok(Line {
            site: f[0].clone(),
            frame: f[1].clone(),
            input: f[2].clone(),
            input_name: f[3].clone(),
            close,
            output_name: f[5].clone(),
            output: f[6].clone(),
            out_frame: f[7].clone(),
            out_site: f[8].clone(),
        })
    }
    /// The canonical spelling.
    pub fn render(&self) -> String {
        [
            q(&self.site),
            q(&self.frame),
            q(&self.input),
            q(&self.input_name),
            (if self.close { "CLOSE" } else { "OPEN" }).to_string(),
            q(&self.output_name),
            q(&self.output),
            q(&self.out_frame),
            q(&self.out_site),
        ]
        .join(",")
    }
    /// As a JSON object (`switch` is `"CLOSE"`/`"OPEN"`) — the shape the vectors carry.
    pub fn to_value(&self) -> Value {
        json!({
            "site": self.site, "frame": self.frame, "input": self.input, "input_name": self.input_name,
            "switch": if self.close { "CLOSE" } else { "OPEN" },
            "output_name": self.output_name, "output": self.output, "out_frame": self.out_frame, "out_site": self.out_site,
        })
    }
}

fn q(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn split_csv(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_q && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => in_q = !in_q,
            ',' if !in_q => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

/// The `…/reject` payload: `<reason>|<line>`.
pub fn render_reject(reason: &str, line: &str) -> String {
    format!("{}|{line}", reason.replace('|', "/"))
}

/// Split a `…/reject` payload into (reason, line). The reason never holds `|`.
pub fn parse_reject(payload: &str) -> Option<(String, String)> {
    let (reason, line) = payload.split_once('|')?;
    Some((reason.to_string(), line.to_string()))
}
