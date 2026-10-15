//! Pins `hg::timestamp::format_timepoint` against the C++ oracle
//! (`tools/oracle/timestamp_probe.cpp`): `put_time(gmtime(tp), fmt)` in
//! UTC with the two format strings the game actually uses.

use oh_math::hg::timestamp::{format_timepoint, to_timepoint};

/// (timestamp, format, expected C++ output) — all 18 probe timestamps ×
/// both game format strings.
const CORPUS: [(u64, &str, &str); 36] = [
    (0, "%Y%m%d_%H%M%S", "19700101_000000"),
    (0, "%Y-%m-%d %H:%M:%S", "1970-01-01 00:00:00"),
    (1, "%Y%m%d_%H%M%S", "19700101_000001"),
    (1, "%Y-%m-%d %H:%M:%S", "1970-01-01 00:00:01"),
    (42, "%Y%m%d_%H%M%S", "19700101_000042"),
    (42, "%Y-%m-%d %H:%M:%S", "1970-01-01 00:00:42"),
    (86400, "%Y%m%d_%H%M%S", "19700102_000000"),
    (86400, "%Y-%m-%d %H:%M:%S", "1970-01-02 00:00:00"),
    (86401, "%Y%m%d_%H%M%S", "19700102_000001"),
    (86401, "%Y-%m-%d %H:%M:%S", "1970-01-02 00:00:01"),
    (1000000, "%Y%m%d_%H%M%S", "19700112_134640"),
    (1000000, "%Y-%m-%d %H:%M:%S", "1970-01-12 13:46:40"),
    (1700000000, "%Y%m%d_%H%M%S", "20231114_221320"),
    (1700000000, "%Y-%m-%d %H:%M:%S", "2023-11-14 22:13:20"),
    (1700000001, "%Y%m%d_%H%M%S", "20231114_221321"),
    (1700000001, "%Y-%m-%d %H:%M:%S", "2023-11-14 22:13:21"),
    (1735689600, "%Y%m%d_%H%M%S", "20250101_000000"),
    (1735689600, "%Y-%m-%d %H:%M:%S", "2025-01-01 00:00:00"),
    (1830000000, "%Y%m%d_%H%M%S", "20271228_132000"),
    (1830000000, "%Y-%m-%d %H:%M:%S", "2027-12-28 13:20:00"),
    (2147483647, "%Y%m%d_%H%M%S", "20380119_031407"),
    (2147483647, "%Y-%m-%d %H:%M:%S", "2038-01-19 03:14:07"),
    (2147483648, "%Y%m%d_%H%M%S", "20380119_031408"),
    (2147483648, "%Y-%m-%d %H:%M:%S", "2038-01-19 03:14:08"),
    (4102444800, "%Y%m%d_%H%M%S", "21000101_000000"),
    (4102444800, "%Y-%m-%d %H:%M:%S", "2100-01-01 00:00:00"),
    (4294967295, "%Y%m%d_%H%M%S", "21060207_062815"),
    (4294967295, "%Y-%m-%d %H:%M:%S", "2106-02-07 06:28:15"),
    (31556735999, "%Y%m%d_%H%M%S", "18001120_005052"),
    (31556735999, "%Y-%m-%d %H:%M:%S", "1800-11-20 00:50:52"),
    (10000000000, "%Y%m%d_%H%M%S", "17020502_181207"),
    (10000000000, "%Y-%m-%d %H:%M:%S", "1702-05-02 18:12:07"),
    (1767225600, "%Y%m%d_%H%M%S", "20260101_000000"),
    (1767225600, "%Y-%m-%d %H:%M:%S", "2026-01-01 00:00:00"),
    (1699999999, "%Y%m%d_%H%M%S", "20231114_221319"),
    (1699999999, "%Y-%m-%d %H:%M:%S", "2023-11-14 22:13:19"),
];

#[test]
fn format_timepoint_matches_cpp_oracle() {
    for (ts, format, expected) in CORPUS {
        let got = format_timepoint(to_timepoint(ts), format);
        assert_eq!(
            got, expected,
            "format_timepoint mismatch for ts={ts} format={format}"
        );
    }
}

#[test]
fn timestamp_roundtrip() {
    // `to_timepoint`/`timestamp` mirror the C++
    // `SCTimePoint{} + seconds(ts)` / `duration_cast<seconds>(...)`.
    // Timestamps >= 9_223_372_037 wrap in the C++ nanosecond conversion
    // (and do not round-trip in C++ either) — excluded here.
    for (ts, _, _) in CORPUS.iter().filter(|(ts, _, _)| *ts <= 9_223_372_036) {
        assert_eq!(oh_math::hg::timestamp::timestamp(to_timepoint(*ts)), *ts);
    }
}
