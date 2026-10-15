//! Port of `include/SSVOpenHexagon/Utils/Timestamp.hpp` +
//! `src/SSVOpenHexagon/Utils/Timestamp.cpp`.
//!
//! `format_timepoint` reproduces `std::put_time` (via `gmtime`, i.e. UTC)
//! for the specifier subset the game uses — `%Y %m %d %H %M %S` (the two
//! format strings in the game are `"%Y%m%d_%H%M%S"` in `Replay.cpp` and
//! `"%Y-%m-%d %H:%M:%S"` in `MenuGame.cpp`). Other `%X` specifiers are
//! copied through verbatim, matching glibc's behavior for specifiers it
//! does not interpret in this context.
//!
//! `%Y` is printed with at least four digits (glibc zero-pads to 4 for
//! years in `[1000, 9999]`, which covers every time point this module can
//! produce — see the wrap note on [`to_timepoint`]).
//!
//! **1:1 overflow note.** The C++ `toTimepoint` is
//! `SCTimePoint{} + std::chrono::seconds(timestamp)` on a nanosecond-based
//! `system_clock`, i.e. a seconds→nanoseconds conversion in `i64`. For
//! `timestamp >= 9_223_372_037` seconds (year 2959) that conversion
//! overflows and the C++ build exhibits the wrapped (negative) time point
//! (`tools/oracle/timestamp_probe.cpp`: ts=1e10 → `1702-05-02 18:12:07`,
//! ts=31556735999 → `1800-11-20 00:50:52`). This port mirrors that wrap
//! bit-exactly.

use std::time::{Duration, SystemTime};

/// `hg::Utils::timestamp` — seconds since the epoch, as the C++
/// `duration_cast<seconds>(tp.time_since_epoch()).count()` cast to
/// `uint64_t`: post-epoch time points give the second count, pre-epoch
/// time points give the two's-complement wrap of the negative count.
#[must_use]
pub fn timestamp(tp: SystemTime) -> u64 {
    match tp.duration_since(SystemTime::UNIX_EPOCH) {
        // C++ `duration_cast` truncates the sub-second part.
        Ok(d) => d.as_secs(),
        Err(e) => {
            // C++: negative `i64` count → `uint64_t` (two's complement).
            0u64.wrapping_sub(e.duration().as_secs())
        }
    }
}

/// `hg::Utils::nowTimestamp` — `timestamp(SCClock::now())`.
#[must_use]
pub fn now_timestamp() -> u64 {
    timestamp(SystemTime::now())
}

/// `hg::Utils::toTimepoint` — epoch + `timestamp` seconds, mirroring the
/// C++ `SCTimePoint{} + std::chrono::seconds(timestamp)` *including its*
/// `i64` nanosecond-conversion wrap (see module docs).
#[must_use]
pub fn to_timepoint(timestamp: u64) -> SystemTime {
    // `std::chrono::seconds(ts)` → `time_point + duration`: on a
    // nanosecond-based clock the common-type conversion multiplies by
    // 1e9 in `i64` (wrapping for ts >= 9_223_372_037).
    let ns = (timestamp as i64).wrapping_mul(1_000_000_000);
    // `system_clock::to_time_t` (duration_cast<seconds>): truncating `i64`
    // division by 1e9.
    let secs = ns / 1_000_000_000;
    if secs >= 0 {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
    } else {
        // secs is in [-9_223_372_036, -1], so -secs cannot overflow.
        SystemTime::UNIX_EPOCH - Duration::from_secs((-secs) as u64)
    }
}

/// `hg::Utils::formatTimepoint(time, format)` — `put_time(gmtime(time),
/// format)` in UTC, for the `%Y %m %d %H %M %S` subset (see module docs).
#[must_use]
pub fn format_timepoint(time: SystemTime, format: &str) -> String {
    // C++ `to_time_t(time)` is an `i64` (negative for pre-epoch / wrapped
    // time points); `gmtime` is applied to that value.
    let secs = timestamp(time) as i64;
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);

    let mut out = String::with_capacity(format.len());
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.next() {
                Some('%') => out.push_str("%%"),
                Some(spec) => match spec {
                    'Y' => push_int_padded(&mut out, year, 4),
                    'm' => push_int_padded(&mut out, month, 2),
                    'd' => push_int_padded(&mut out, day, 2),
                    'H' => push_int_padded(&mut out, hour, 2),
                    'M' => push_int_padded(&mut out, minute, 2),
                    'S' => push_int_padded(&mut out, second, 2),
                    // glibc: unhandled specifiers are copied through.
                    other => {
                        out.push('%');
                        out.push(other);
                    }
                },
                None => out.push('%'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn push_int_padded(out: &mut String, value: i64, min_digits: usize) {
    // Every value here is in `[0, 9999]` (see the module docs `%Y` note),
    // so the `u64` conversion cannot overflow.
    out.push_str(&format!("{:0width$}", value as u64, width = min_digits));
}

/// Civil calendar date (year, month, day) from days since the Unix epoch.
///
/// The standard "days → y/m/d" algorithm (Hinnant, `civil_from_days`),
/// copied verbatim (including C++'s truncating-division `era` trick);
/// pinned against `tools/oracle/timestamp_probe.cpp` output.
#[must_use]
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}
