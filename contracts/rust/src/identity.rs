// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Device identity derivation — guidelines D2, twin of
//! `contracts/src/identity.ts`, pinned by `vectors/identity.json`.
//! Priority: serial → protocol-native stable address → FNV-1a 64-bit hash of
//! `make|model|address`. Two agents deriving different IDs for one
//! instrument recreates the duplicate-34401A bug — so this is a contract.

/// FNV-1a 64-bit over UTF-8 bytes, lowercase hex (16 chars).
pub fn fnv1a64(input: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in input.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Replace anything outside the deviceId key charset with '-'.
/// NOTE: operates per Unicode scalar, matching the TS regex per-code-unit
/// behavior for all BMP input (vectors avoid astral-plane cases).
fn sanitize_key(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// A serial field that every unit of a model reports alike — `0` (every HP
/// 34401A), `none`, `n/a`, all zeros — is not an identity. Taken as one it
/// filed three 34401As behind one E5810 as the single device `visa:0`.
pub fn is_placeholder_serial(serial: &str) -> bool {
    let t = serial.trim();
    t.chars().all(|c| c == '0') || matches!(t.to_ascii_lowercase().as_str(), "none" | "n/a" | "na" | "unknown")
}

/// One spelling per address. VISA's board number defaults to 0, so
/// `TCPIP0::h::d::INSTR` and `TCPIP::h::d::INSTR` are the same resource — and
/// the engine and the hunter spelling it differently gave each GPIB meter two
/// presence documents. The board-0 prefix is dropped for TCPIP only — the one
/// interface two agents spell differently; USB/GPIB/ASRL ids stay as they were.
pub fn canonical_address(address: &str) -> String {
    match address.strip_prefix("TCPIP0::") {
        Some(rest) => format!("TCPIP::{rest}"),
        None => address.to_string(),
    }
}

/// The D2 rule. Returns `{protocol}:{stableKey}`.
pub fn device_id_for(
    protocol: &str,
    serial: Option<&str>,
    address: Option<&str>,
    make: Option<&str>,
    model: Option<&str>,
) -> String {
    if let Some(s) = serial {
        let t = s.trim();
        if !t.is_empty() && !is_placeholder_serial(t) {
            return format!("{protocol}:{}", sanitize_key(t));
        }
    }
    if let Some(a) = address {
        let t = canonical_address(a.trim());
        if !t.is_empty() {
            return format!("{protocol}:{}", sanitize_key(&t));
        }
    }
    let content = format!(
        "{}|{}|{}",
        make.unwrap_or(""),
        model.unwrap_or(""),
        address.unwrap_or("")
    );
    format!("{protocol}:{}", fnv1a64(&content))
}
