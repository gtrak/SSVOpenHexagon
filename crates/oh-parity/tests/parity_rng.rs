//! Issue 23 — RNG-stream parity. Runs the corpus-pinned
//! `hg::random_number_generator` streams (`.agents/plans/001-rust-port/parity/
//! rng_streams.json`, C++-probe-derived: the REAL
//! `src/SSVOpenHexagon/Core/RandomNumberGenerator.cpp` via
//! `tools/oracle/parity_probe.cpp`) through the Rust `oh-rng` port and
//! compares bit-exactly (the f32/f64 pins are the exact bit patterns —
//! float equality cannot distinguish the last ulp).
//!
//! The harness itself is tested: `deliberately_wrong_pin_discriminates`
//! runs the same comparison on a corrupted pin and requires the mismatch
//! to be reported.

use std::path::Path;

use serde_json::Value;

fn corpus_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.agents/plans/001-rust-port/parity")
        .join(name)
}

fn load(name: &str) -> Value {
    let text = std::fs::read_to_string(corpus_path(name))
        .unwrap_or_else(|e| panic!("read corpus {name}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse corpus {name}: {e}"))
}

/// Parses a corpus `"0x..."` bit pattern.
fn parse_bits(hex: &str) -> u64 {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    u64::from_str_radix(hex, 16).unwrap_or_else(|e| panic!("bad bit pattern {hex:?}: {e}"))
}

/// The corpus-pinned seed list (also asserted against the file itself).
const SEEDS: [u64; 7] = [
    0,
    4,
    12_345,
    987_654_321,
    4_294_967_295,
    4_294_967_296,
    u64::MAX,
];

/// Compares one corpus seed entry against the Rust stream. Returns `Ok(())`
/// or the first mismatch (the deliberately-wrong-entry test relies on the
/// `Err` arm).
fn check_seed(entry: &Value) -> Result<(), String> {
    let seed = entry["seed"].as_u64().unwrap();

    // int stream (C++ `get_int<int>(0, 5)`).
    let expected: Vec<i32> = entry["get_int_int_0_5"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as i32)
        .collect();
    let mut rng = oh_rng::RandomNumberGenerator::new(seed);
    for (i, e) in expected.iter().enumerate() {
        let a = rng.get_int(0_i32, 5_i32);
        if a != *e {
            return Err(format!("seed {seed}: get_int[{i}] = {a}, corpus {e}"));
        }
    }

    // f32 stream — bit-exact.
    let bits: Vec<u64> = entry["get_real_float_0_1_bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| parse_bits(v.as_str().unwrap()))
        .collect();
    let mut rng = oh_rng::RandomNumberGenerator::new(seed);
    for (i, e) in bits.iter().enumerate() {
        let a = rng.get_real(0.0_f32, 1.0_f32);
        if (a.to_bits() as u64) != *e {
            return Err(format!(
                "seed {seed}: get_real_f32[{i}] bits {:#010x}, corpus {e:#010x}",
                a.to_bits()
            ));
        }
    }

    // f64 stream — bit-exact.
    let bits: Vec<u64> = entry["get_real_double_0_1_bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| parse_bits(v.as_str().unwrap()))
        .collect();
    let mut rng = oh_rng::RandomNumberGenerator::new(seed);
    for (i, e) in bits.iter().enumerate() {
        let a = rng.get_real(0.0_f64, 1.0_f64);
        if a.to_bits() != *e {
            return Err(format!(
                "seed {seed}: get_real_f64[{i}] bits {:#018x}, corpus {e:#018x}",
                a.to_bits()
            ));
        }
    }
    Ok(())
}

#[test]
fn rng_streams_match_cpp_probe() {
    let doc = load("rng_streams.json");
    let seeds = doc["seeds"].as_array().expect("corpus `seeds` array");
    assert_eq!(
        seeds.len(),
        SEEDS.len(),
        "the corpus pins one entry per seed"
    );
    for (i, entry) in seeds.iter().enumerate() {
        assert_eq!(
            entry["seed"].as_u64(),
            Some(SEEDS[i]),
            "corpus seed {i} drifted from the harness's seed list"
        );
        check_seed(entry).unwrap_or_else(|e| panic!("RNG stream mismatch: {e}"));
    }
}

#[test]
fn deliberately_wrong_pin_discriminates() {
    // The contract's "the harness itself is tested": the SAME comparison
    // logic must FAIL on a deliberately corrupted corpus entry.
    let doc = load("rng_streams.json");
    let mut entry = doc["seeds"][0].clone();
    // Flip the first f32 pin's low bit.
    let bits = parse_bits(entry["get_real_float_0_1_bits"][0].as_str().unwrap());
    entry["get_real_float_0_1_bits"][0] = Value::String(format!("{:#x}", bits ^ 1));
    let err = check_seed(&entry).expect_err("a corrupted pin must not pass");
    assert!(
        err.contains("get_real_f32[0]"),
        "the mismatch names the field: {err}"
    );

    // And an intact entry passes the same logic (sanity: the corruption,
    // not the logic, is what changed the outcome).
    check_seed(&doc["seeds"][0]).expect("uncorrupted entry passes");
}
