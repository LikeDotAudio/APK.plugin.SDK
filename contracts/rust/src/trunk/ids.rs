// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `trunk.rs` — see that file for the whole picture.

// ---------------------------------------------------------------------------
// Identities
// ---------------------------------------------------------------------------

/// A user port: `U<nn>` (two digits at least).
pub fn user_port(n: usize) -> String {
    format!("U{n:02}")
}

/// A caster (trunk port): `C<n>`.
pub fn caster_port(n: usize) -> String {
    format!("C{n}")
}

/// `U05` → ("user", 5); `C3` → ("caster", 3). Only the canonical spelling.
pub fn parse_port(s: &str) -> Option<(&'static str, usize)> {
    let (kind, digits) = match s.as_bytes().first()? {
        b'U' => ("user", &s[1..]),
        b'C' => ("caster", &s[1..]),
        _ => return None,
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: usize = digits.parse().ok()?;
    let canonical = if kind == "user" { user_port(n) } else { caster_port(n) };
    (canonical == s).then_some((kind, n))
}

/// A frame: `<protocol or site slug>-<nn>`.
pub fn frame_id(prefix: &str, n: usize) -> String {
    format!("{prefix}-{n:02}")
}

/// A trunk: `trunk-<nnnnn>`.
pub fn trunk_id(n: usize) -> String {
    format!("trunk-{n:05}")
}

/// A site's slug: lower-case ASCII alphanumerics joined by single `-`.
pub fn slugify(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// A channel of a trunk is its index, `0..capacity`.
pub fn is_channel(index: usize, capacity: usize) -> bool {
    index < capacity
}
