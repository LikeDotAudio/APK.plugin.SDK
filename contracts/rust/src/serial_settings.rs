// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! `APK_SERIAL_SETTINGS` — the operator's declared line settings, one grammar.
//!
//! WHY IT IS HERE. Two plugins read the same environment variable:
//! `PLUGIN.VISA` (`serial_hunter/pinned.rs`, where it switches the probing
//! ladder off for a declared port) and `PLUGIN.SERIAL_COM` (`exchange.rs`, where
//! it is the settings an exchange uses when the request names none). Each held
//! a byte-for-byte copy of the parser, and SERIAL_COM's docstring said "read the
//! way PLUGIN.VISA reads it" — a rule held by a comment. The 2026-10-30 audit
//! asked for one parser; this is it, and `vectors/serial_settings.json` holds
//! this file and `python/apkaudio_contracts/serial_settings.py` to one answer.
//!
//! THE GRAMMAR (unchanged from what both plugins already accepted):
//!
//! ```text
//! spec     := entry ("," entry)*
//! entry    := key "=" settings          (an entry with no "=" is skipped)
//! key      := "*" | tty name | USB adapter serial   ("/dev/" prefix ignored)
//! settings := baud " " framing [" " flow]
//! baud     := decimal u32, > 0
//! framing  := bits parity stop          e.g. 8N1, 7E2 (case-insensitive)
//!             bits 7|8, parity N|E|O, stop 1|2
//! flow     := none | xonxoff | rtscts   (case-insensitive; default none)
//! ```
//!
//! The most specific key wins: adapter serial, then tty name, then `*`. A
//! serial follows the meter to whatever port it is replugged into. An entry
//! whose settings do not parse is REJECTED, not guessed, and reported back in
//! [`SerialPins::rejected`] so the caller can say so in its own voice (both
//! plugins print a `🛑 … cannot read … — ignored` line).
//!
//! ONE DELIBERATE TIGHTENING: a baud of `0` is refused. Both copies accepted
//! it; no line runs at zero baud, so a `0` is a typo, and a typo here is the
//! thing that makes four bench meters beep (`serial-ladder-beeps-meters`).
//!
//! No I/O beyond [`SerialPins::from_env`]; no serial library. Converting to a
//! `serialport`/termios setting is the plugin's adapter's job.

use serde::{Deserialize, Serialize};

/// The environment variable both plugins read.
pub const ENV: &str = "APK_SERIAL_SETTINGS";

/// The key that matches every port.
pub const ANY_PORT: &str = "*";

/// Flow control. Serialized lowercase (`none`, `xonxoff`, `rtscts`) — the same
/// spelling `PLUGIN.VISA`'s on-disk cache already uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flow {
    None,
    XonXoff,
    RtsCts,
}

impl Flow {
    pub fn label(&self) -> &'static str {
        match self {
            Flow::None => "none",
            Flow::XonXoff => "xonxoff",
            Flow::RtsCts => "rtscts",
        }
    }

    fn parse(word: &str) -> Option<Flow> {
        match word.to_ascii_lowercase().as_str() {
            "none" => Some(Flow::None),
            "xonxoff" => Some(Flow::XonXoff),
            "rtscts" => Some(Flow::RtsCts),
            _ => None,
        }
    }
}

/// One complete line configuration: `9600 7E2`, `19200 8N1 rtscts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LineSettings {
    pub baud: u32,
    /// 7 or 8.
    pub bits: u8,
    /// `N`, `E` or `O` (always upper case).
    pub parity: char,
    /// 1 or 2.
    pub stop_bits: u8,
    pub flow: Flow,
}

impl LineSettings {
    /// `9600 7E2 none` — always with the flow word, and read back by [`parse_settings`].
    pub fn label(&self) -> String {
        format!("{} {}{}{} {}", self.baud, self.bits, self.parity, self.stop_bits, self.flow.label())
    }
}

/// Parse `9600 7E2` or `9600 7E2 rtscts`. `None` for anything else.
pub fn parse_settings(text: &str) -> Option<LineSettings> {
    let mut words = text.split_whitespace();
    let baud: u32 = words.next()?.parse().ok()?;
    if baud == 0 {
        return None;
    }
    let framing: Vec<char> = words.next()?.to_ascii_uppercase().chars().collect();
    let (bits, parity, stop_bits) = match framing.as_slice() {
        [b @ ('7' | '8'), p @ ('N' | 'E' | 'O'), s @ ('1' | '2')] => {
            (*b as u8 - b'0', *p, *s as u8 - b'0')
        }
        _ => return None,
    };
    let flow = match words.next() {
        None => Flow::None,
        Some(w) => Flow::parse(w)?,
    };
    if words.next().is_some() {
        return None;
    }
    Some(LineSettings { baud, bits, parity, stop_bits, flow })
}

/// One accepted `key=settings` entry, key already stripped of `/dev/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialPin {
    pub key: String,
    pub settings: LineSettings,
}

/// A whole `APK_SERIAL_SETTINGS` value, parsed once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SerialPins {
    /// Accepted entries, in the order written.
    pub pins: Vec<SerialPin>,
    /// Entries that had a `=` but settings that did not parse, trimmed, as written.
    pub rejected: Vec<String>,
}

impl SerialPins {
    /// Parse a spec. Never fails: unreadable entries land in `rejected`.
    pub fn parse(spec: &str) -> SerialPins {
        let mut out = SerialPins::default();
        for entry in spec.split(',') {
            let Some((key, value)) = entry.split_once('=') else { continue };
            let key = key.trim().trim_start_matches("/dev/");
            match parse_settings(value) {
                Some(settings) if !key.is_empty() => {
                    out.pins.push(SerialPin { key: key.to_string(), settings })
                }
                _ => out.rejected.push(entry.trim().to_string()),
            }
        }
        out
    }

    /// Read [`ENV`]. An unset variable is an empty set of pins.
    pub fn from_env() -> SerialPins {
        std::env::var(ENV).map(|s| SerialPins::parse(&s)).unwrap_or_default()
    }

    /// The settings declared for this port, if any. `tty_name` is the bare
    /// node name (`ttyUSB0`; a `/dev/` prefix is ignored); `usb_serial` is the
    /// adapter's serial number, `""` when it has none. Most specific wins:
    /// serial, then name, then `*`; within one rank the LAST entry wins.
    pub fn for_port(&self, tty_name: &str, usb_serial: &str) -> Option<LineSettings> {
        let name = tty_name.trim_start_matches("/dev/");
        let (mut by_serial, mut by_name, mut by_default) = (None, None, None);
        for pin in &self.pins {
            if !usb_serial.is_empty() && pin.key == usb_serial {
                by_serial = Some(pin.settings);
            } else if pin.key == name {
                by_name = Some(pin.settings);
            } else if pin.key == ANY_PORT {
                by_default = Some(pin.settings);
            }
        }
        by_serial.or(by_name).or(by_default)
    }

    pub fn is_empty(&self) -> bool {
        self.pins.is_empty()
    }
}

/// One-call form: the pin for this port in `spec`, rejected entries dropped.
pub fn pinned(spec: &str, tty_name: &str, usb_serial: &str) -> Option<LineSettings> {
    SerialPins::parse(spec).for_port(tty_name, usb_serial)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_reads_back() {
        let s = parse_settings("9600 7e2").unwrap();
        assert_eq!(s.label(), "9600 7E2 none");
        assert_eq!(parse_settings(&s.label()), Some(s));
    }

    #[test]
    fn flow_serializes_the_way_visas_cache_spells_it() {
        let s = parse_settings("19200 8N1 rtscts").unwrap();
        let json = serde_json::to_value(s).unwrap();
        assert_eq!(json["flow"], "rtscts");
        assert_eq!(json["parity"], "N");
    }

    #[test]
    fn rejected_entries_are_reported_not_guessed() {
        let p = SerialPins::parse("*=9600 7E2, ttyUSB1=fast, nonsense, =9600 8N1");
        assert_eq!(p.pins.len(), 1);
        assert_eq!(p.rejected, vec!["ttyUSB1=fast".to_string(), "=9600 8N1".to_string()]);
    }
}
