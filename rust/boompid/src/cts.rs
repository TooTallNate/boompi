//! Bluetooth Current Time Service (CTS) value parsing.
//!
//! Phones expose CTS (service `0x1805`) as a GATT server - iOS
//! famously so - which makes any connected phone a time source for the
//! RTC-less boxes. This module is the pure, unit-testable half: byte
//! parsing and calendar math. The BlueZ plumbing (discovering and
//! reading the characteristics) lives in `bluetooth.rs`.
//!
//! Characteristics:
//! - `0x2A2B` Current Time: Exact Time 256 - the phone's *local*
//!   date/time (year u16le, month, day, hours, minutes, seconds,
//!   day-of-week, 1/256 fractions, adjust reason).
//! - `0x2A0F` Local Time Information: UTC offset (15-minute increments)
//!   plus DST offset - needed to turn the local reading into UTC.

/// A local calendar date/time parsed from a `0x2A2B` read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// Parse a Current Time (`0x2A2B`) value. Returns `None` for truncated
/// or out-of-range payloads (a zeroed year means "unknown" per spec).
pub fn parse_current_time(bytes: &[u8]) -> Option<CivilTime> {
    if bytes.len() < 7 {
        return None;
    }
    let t = CivilTime {
        year: u16::from_le_bytes([bytes[0], bytes[1]]),
        month: bytes[2],
        day: bytes[3],
        hour: bytes[4],
        minute: bytes[5],
        second: bytes[6],
    };
    let plausible = (1970..2100).contains(&t.year)
        && (1..=12).contains(&t.month)
        && (1..=31).contains(&t.day)
        && t.hour <= 23
        && t.minute <= 59
        && t.second <= 59;
    plausible.then_some(t)
}

/// Parse a Local Time Information (`0x2A0F`) value into a UTC offset in
/// seconds (timezone + DST), or `None` when the phone reports unknown.
pub fn parse_utc_offset(bytes: &[u8]) -> Option<i32> {
    if bytes.len() < 2 {
        return None;
    }
    // Timezone: i8 count of 15-minute increments from UTC; -128 unknown.
    let tz = bytes[0] as i8;
    if tz == -128 || !(-48..=56).contains(&tz) {
        return None;
    }
    // DST offset: u8 count of 15-minute increments; 255 unknown.
    let dst = match bytes[1] {
        0 | 2 | 4 | 8 => bytes[1] as i32,
        _ => return None,
    };
    Some((tz as i32 + dst) * 15 * 60)
}

/// Days from 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix time (ms) for a civil date/time at the given UTC offset.
pub fn epoch_ms(t: CivilTime, utc_offset_secs: i32) -> u64 {
    let days = days_from_civil(t.year as i64, t.month as i64, t.day as i64);
    let secs = days * 86_400
        + t.hour as i64 * 3_600
        + t.minute as i64 * 60
        + t.second as i64
        - utc_offset_secs as i64;
    secs.max(0) as u64 * 1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ct(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> CivilTime {
        CivilTime { year, month, day, hour, minute, second }
    }

    #[test]
    fn parses_ios_style_current_time() {
        // 2026-08-17 14:30:05, Monday, 0 fractions, manual update.
        let bytes = [0xEA, 0x07, 8, 17, 14, 30, 5, 1, 0, 1];
        assert_eq!(parse_current_time(&bytes), Some(ct(2026, 8, 17, 14, 30, 5)));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_current_time(&[]), None);
        assert_eq!(parse_current_time(&[0; 10]), None); // year 0 = unknown
        let bad_month = [0xEA, 0x07, 13, 17, 14, 30, 5];
        assert_eq!(parse_current_time(&bad_month), None);
    }

    #[test]
    fn utc_offsets() {
        assert_eq!(parse_utc_offset(&[0, 0]), Some(0));
        // UTC-8 (Pacific standard): -32 quarter-hours.
        assert_eq!(parse_utc_offset(&[(-32i8) as u8, 0]), Some(-8 * 3600));
        // UTC-8 + 1h DST = Pacific daylight.
        assert_eq!(parse_utc_offset(&[(-32i8) as u8, 4]), Some(-7 * 3600));
        // Unknowns.
        assert_eq!(parse_utc_offset(&[0x80, 0]), None);
        assert_eq!(parse_utc_offset(&[0, 255]), None);
    }

    #[test]
    fn epoch_round_trips() {
        // 2026-08-17 00:00:00 UTC = 1786924800.
        assert_eq!(epoch_ms(ct(2026, 8, 17, 0, 0, 0), 0), 1_786_924_800_000);
        // Same wall clock in PDT (UTC-7) is 7h later in UTC.
        assert_eq!(
            epoch_ms(ct(2026, 8, 17, 0, 0, 0), -7 * 3600),
            1_786_924_800_000 + 7 * 3_600_000
        );
        // Epoch itself.
        assert_eq!(epoch_ms(ct(1970, 1, 1, 0, 0, 0), 0), 0);
    }
}
