// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Boundary time conversions — twin of `contracts/src/time.ts`, pinned by
//! `vectors/identity.json`. ISO-8601 UTC with milliseconds, byte-identical
//! to JavaScript's `Date.toISOString()`. No chrono — the crate rule is
//! serde + serde_json only, and this is 30 lines of civil-calendar math.

/// Unix seconds (int or float, as v40 emits) → `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn from_unix_seconds(seconds: f64) -> String {
    iso_from_unix_ms((seconds * 1000.0).round() as i64)
}

pub fn iso_from_unix_ms(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    let msec = rem % 1000;
    let sec = (rem / 1000) % 60;
    let min = (rem / 60_000) % 60;
    let hour = rem / 3_600_000;
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}.{msec:03}Z")
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYY-MM-DDTHH:MM:SS[.mmm]Z` → Unix milliseconds. The inverse of
/// `iso_from_unix_ms`, and the twin of `Date.parse()` at the one place the
/// TypeScript side uses it: differencing two `AgentCounters` samples.
///
/// Returns `None` on anything it cannot read rather than guessing a moment.
/// A rate is a division by an elapsed time; inventing that time from a
/// malformed stamp produces a plausible number from no measurement, which is
/// the failure `rates_between` refuses in every other direction too.
///
/// Deliberately strict about the shape: this parses the format this crate
/// emits and the format the schema's own pattern admits, not ISO-8601 at
/// large. Offsets other than `Z` are rejected, not silently read as UTC.
pub fn unix_ms_from_iso(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || !s.ends_with('Z') {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> { s.get(from..to)?.parse().ok() };
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let msec = match b.len() {
        20 => 0,
        24 if b[19] == b'.' => num(20, 23)?,
        _ => return None,
    };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    if h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    Some(days_from_civil(y, mo as u32, d as u32) * 86_400_000
        + h * 3_600_000
        + mi * 60_000
        + sec * 1000
        + msec)
}

/// (year, month, day) → days since 1970-01-01. Inverse of `civil_from_days`;
/// Howard Hinnant's algorithm, same paper.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Time of day AS A FREQUENCY — twin of `timeOfDayHz` in
/// `contracts/src/time.ts`, pinned by `vectors/identity.json`.
///
/// Hours are thousands, minutes are units, seconds are thousandths,
/// milliseconds are millionths:
///
/// ```text
/// f = HH × 1000 + MM + SS / 1000 + ms / 1_000_000     Hz
///
/// 12:49:00.000  →  12 049.000000 Hz   (12.049 kHz)
/// 13:49:33.500  →  13 049.033500 Hz
/// 23:59:59.999  →  23 059.059999 Hz
/// 00:00:00.000  →           0      Hz
/// ```
///
/// The point of this shape and not a cleverer one: `12049` on a frequency
/// counter IS twelve forty-nine, read by eyes, with no decoder in between.
/// PLAN-458.01 carries the argument, the twenty-four-hour rule, and the two
/// consequences that are not defects — midnight is DC, and the first twenty
/// minutes of the day fall below hearing.
///
/// An OFFSET, never a named zone: the bus is ISO-8601 UTC by contract and this
/// is a wall-clock reading, and a zone name would need a tz database in a crate
/// whose rule is serde and serde_json only.
///
/// Whole MICROHERTZ, divided once. Four additions of binary fractions do not
/// land on the same double in two languages; one correctly-rounded division
/// does, which is what lets the vectors pin an exact number for both twins.
pub fn time_of_day_hz(unix_ms: i64, utc_offset_minutes: i64) -> f64 {
    time_of_day_micro_hz(unix_ms, utc_offset_minutes) as f64 / 1_000_000.0
}

/// The same instant in whole MICROHERTZ, which is the exact quantity —
/// `time_of_day_hz` is this divided once, and a caller that must not round
/// should ask for this and place its own decimal point.
///
/// A generator's frequency goes out as a DECIMAL STRING and 13.0490335 has no
/// exact binary form: KOMAND builds `13.0490335` by shifting the point nine
/// places left in this integer, never by dividing and formatting a double.
/// That is the same lesson `converters::apply_converter` learned the hard way
/// when 2057.515 MHz left the bench as `2057514999.9999998`.
pub fn time_of_day_micro_hz(unix_ms: i64, utc_offset_minutes: i64) -> i64 {
    let local = unix_ms + utc_offset_minutes * 60_000;
    // Euclidean, so a pre-epoch instant is a time of day rather than a
    // negative frequency — `%` keeps the sign of the dividend in both twins.
    let day_ms = local.rem_euclid(86_400_000);
    let hour = day_ms / 3_600_000;
    let minute = (day_ms / 60_000) % 60;
    let second = (day_ms / 1000) % 60;
    let milli = day_ms % 1000;
    hour * 1_000_000_000 + minute * 1_000_000 + second * 1000 + milli
}
