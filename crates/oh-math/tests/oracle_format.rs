//! Pins `hg::concat::format_ostream_f32` / `format_ostream_f64` against the
//! C++ oracle (`tools/oracle/format_probe.cpp`, g++ 15.2 / glibc): the
//! default `std::ostream <<` formatting (glibc `%g`, precision 6).
//!
//! Regenerate the corpus with `bash tools/oracle/run.sh` (section
//! "ostream float formatting").

use oh_math::hg::concat::{format_ostream_f32, format_ostream_f64};

/// Fixed f32 corpus: (exact f32 bit pattern, expected C++ string).
const F32_CORPUS: [(u32, &str); 32] = [
    (0x3FCCCCCD, "1.6"),
    (0x41173333, "9.45"),
    (0x3FACCCCD, "1.35"),
    (0x3DCCCCCC, "0.1"),
    (0x3E4CCCCD, "0.2"),
    (0x3E99999A, "0.3"),
    (0x47F12040, "123456"),
    (0x497423F6, "999999"),
    (0x4996B438, "1.23457e+06"),
    (0x38D1B717, "0.0001"),
    (0x33D6BF95, "1e-07"),
    (0x45610000, "3600"),
    (0x45BB7FFE, "6000"),
    (0x00000000, "0"),
    (0x80000000, "-0"),
    (0xBFCCCCCD, "-1.6"),
    (0x42C80000, "100"),
    (0x47C35000, "100000"),
    (0x3F000000, "0.5"),
    (0x40200000, "2.5"),
    (0x3E000000, "0.125"),
    (0x40490FD0, "3.14159"),
    (0x40C90FD0, "6.28318"),
    (0x501502F9, "1e+10"),
    (0x3727C5AC, "1e-05"),
    (0x437F0000, "255"),
    (0x437F8000, "255.5"),
    (0x3C800000, "0.015625"),
    (0x38972087, "7.2063e-05"),
    (0xC7F1205A, "-123457"),
    (0xB727C5AC, "-1e-05"),
    (0x4B3C614E, "1.23457e+07"),
];

/// Fixed f64 corpus: (exact f64 bit pattern, expected C++ string).
const F64_CORPUS: [(u64, &str); 30] = [
    (0x3FF999999999999A, "1.6"),
    (0x4022E66666666666, "9.45"),
    (0x3FF599999999999A, "1.35"),
    (0x3FB999999999999A, "0.1"),
    (0x40AC200000000000, "3600"),
    (0x40B76FFFBE76C8B4, "6000"),
    (0x40FE240B33333333, "123457"),
    (0x412E847ECCCCCCCD, "999999"),
    (0x4132D68700000000, "1.23457e+06"),
    (0x3F1A36E2EB1C432D, "0.0001"),
    (0x3E7AD7F29ABCAB48, "1e-07"),
    (0x0000000000000000, "0"),
    (0x8000000000000000, "-0"),
    (0xBFF999999999999A, "-1.6"),
    (0x4202A05F20000000, "1e+10"),
    (0x3EE4F8B588E368F1, "1e-05"),
    (0x40F86A0000000000, "100000"),
    (0x400921FB53C8D4F1, "3.14159"),
    (0x3FD5555555555555, "0.333333"),
    (0x4004000000000000, "2.5"),
    (0x3FE0000000000000, "0.5"),
    (0x404F400000000000, "62.5"),
    (0x3F1A36E2EB1C432D, "0.0001"),
    (0xBEE4F8B588E368F1, "-1e-05"),
    (0x41678C29C0000000, "1.23457e+07"),
    (0x412E847F00000000, "1e+06"),
    (0x3F9B645A1CAC0831, "0.02675"),
    (0x3FF000001AD7F29B, "1"),
    (0x3FEFFFFFCA501ACB, "1"),
    (0x3EB0C6F7A0B5ED8D, "1e-06"),
];

/// Sweep samples: f32 magnitudes every 8192 bit patterns starting from bit
/// pattern 1 (probe sweep), positive and negative.
const F32_SWEEP: [(u32, &str); 8] = [
    (0x00000001, "1.4013e-45"),
    (0x80000001, "-1.4013e-45"),
    (0x00002001, "1.14808e-41"),
    (0x80002001, "-1.14808e-41"),
    (0x00004001, "2.29603e-41"),
    (0x80004001, "-2.29603e-41"),
    (0x00006001, "3.44397e-41"),
    (0x80006001, "-3.44397e-41"),
];

#[test]
fn format_ostream_f32_matches_cpp_oracle() {
    for (bits, expected) in F32_CORPUS {
        let v = f32::from_bits(bits);
        assert_eq!(
            format_ostream_f32(v),
            expected,
            "f32 formatting mismatch for bit pattern {bits:#010x}"
        );
    }
}

#[test]
fn format_ostream_f32_sweep_samples_match_cpp_oracle() {
    for (bits, expected) in F32_SWEEP {
        let v = f32::from_bits(bits);
        assert_eq!(
            format_ostream_f32(v),
            expected,
            "f32 sweep formatting mismatch for bit pattern {bits:#010x}"
        );
    }
}

#[test]
fn format_ostream_f64_matches_cpp_oracle() {
    for (bits, expected) in F64_CORPUS {
        let v = f64::from_bits(bits);
        assert_eq!(
            format_ostream_f64(v),
            expected,
            "f64 formatting mismatch for bit pattern {bits:#018x}"
        );
    }
}
