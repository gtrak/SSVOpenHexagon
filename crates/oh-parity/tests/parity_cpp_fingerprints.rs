//! Parity test — the C++ column (task 030b): the per-frame fingerprint
//! vectors of `parity/cpp_fingerprints.json` (the instrumented C++
//! reference binary, `tools/cpp-oracle/fingerprint.patch`, executed in the
//! `tools/cpp-oracle` container on the byte-exact `.ohr.z` replays).
//!
//! For every corpus case:
//! 1. rebuild the `ReplayFile` from the committed fields (pack, level,
//!    seed, difficulty_mult, first_play, player_name, input bytes) and
//!    byte-exactly compare `oh_replay::compress_replay_file`'s output
//!    against the committed `.ohr.z` hex — the interchange-format pin: the
//!    port produces EXACTLY the bytes the C++ executed;
//! 2. run the same replay through the port's
//!    `run_replay_until_death_and_get_score` (the C++
//!    `runReplayUntilDeathAndGetScore` twin) with the 030b fingerprint
//!    enabled, and compare the full per-frame `<frame> <hash> <draws>`
//!    sequence against the committed C++ column — frame by frame, hash by
//!    hash (raw IEEE-bit hashes: a 1-ULP divergence in ANY field changes
//!    the hash), with the first divergent frame reported;
//! 3. compare the final result: the frame count (exact) and the printed
//!    `Final time` (the C++ `std::ostream` 6-significant-digit format,
//!    reproduced by `oh_math::hg::concat::format_ostream_f64` — the same
//!    string the C++ printed) + the `custom_score` (no shipped level
//!    script calls `l_overrideScore`).
//!
//! The check survives without the C++ present: the C++ column is frozen in
//! the JSON. Definition (the durable interface):
//! `.agents/plans/001-rust-port/parity/cpp-fingerprint-definition.md` (v1).
//!
//! # Lane 030c — the three-way proof (C++ ↔ LuaJIT ↔ Luau), and D-037 —
//! # the shipping column
//!
//! This file replays the SAME corpus through the port's ONE VM (Luau —
//! D-037 deleted the backend split). It was originally written so that the
//! same file could produce the LuaJIT column (default features, gate stage
//! 4) and the Luau column (`--no-default-features --features backend-luau`,
//! gate stage 5) of the 030c three-way proof; after D-037 there is a single
//! build, and it is the shipping one: the C++ column (the committed
//! `cpp_fingerprints.json`, 3627 frames) is compared against the port's
//! shipping configuration on every gate — stronger than shipping the other
//! VM and inferring. `cpp_fingerprint_per_case_table` prints the per-case
//! `case | port vs C++` rows the report's three-way table was built from,
//! and — unlike the all-or-nothing corpus test — reports EVERY case's
//! outcome even when an earlier one diverged (with the first divergent
//! frame + field, exactly as 030b's comparator reports it).

mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use common::{compose, corpus_path, read_corpus, release_dir};

/// One corpus case of `parity/cpp_fingerprints.json`.
#[derive(Debug, Clone)]
struct Case {
    name: String,
    pack: String,
    level: String,
    seed: u64,
    difficulty_mult: f32,
    first_play: bool,
    player_name: String,
    input_bytes_hex: String,
    replay_z_hex: String,
    final_time_print: String,
    custom_score: f32,
    frames: u64,
    fingerprints: Vec<String>,
}

impl Case {
    fn from(v: &serde_json::Value) -> Self {
        let get = |k: &str| v.get(k).unwrap_or_else(|| panic!("case: missing `{k}`"));
        Self {
            name: get("name").as_str().unwrap().into(),
            pack: get("pack").as_str().unwrap().into(),
            level: get("level").as_str().unwrap().into(),
            seed: get("seed").as_u64().unwrap(),
            difficulty_mult: get("difficulty_mult").as_f64().unwrap() as f32,
            first_play: get("first_play").as_bool().unwrap(),
            player_name: get("player_name").as_str().unwrap().into(),
            input_bytes_hex: get("input_bytes_hex").as_str().unwrap().into(),
            replay_z_hex: get("replay_z_hex").as_str().unwrap().into(),
            final_time_print: get("final_time_print").as_str().unwrap().into(),
            custom_score: get("custom_score").as_f64().unwrap() as f32,
            frames: get("frames").as_u64().unwrap(),
            fingerprints: get("fingerprints")
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap().to_string())
                .collect(),
        }
    }
}

fn cases() -> Vec<Case> {
    let doc = read_corpus(corpus_path("cpp_fingerprints.json"));
    doc["cases"]
        .as_array()
        .expect("cpp_fingerprints.json: `cases` array")
        .iter()
        .map(Case::from)
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(format!("{b:02x}").as_str());
    }
    s
}

/// The fingerprint temp-file path, unique per `compare_case` CALL: the
/// tests run as parallel threads in one process, and two of them (the
/// 030b corpus test and the 030c per-case table test) drive the SAME case
/// names — a shared `oh030b-parity-<case>.fp.txt` would be truncated by
/// `set_fingerprint_path`'s `File::create` under the other test's open
/// handle (a flaky frame-count/hash mismatch). The file name is irrelevant
/// to the comparison (the C++ writes its own file in the container; only
/// the line CONTENT is compared), so one sequence number per call
/// removes the race.
static FP_SEQ: AtomicU64 = AtomicU64::new(0);

fn fingerprint_temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "oh030b-parity-{}-{}-{}.fp.txt",
        name,
        std::process::id(),
        FP_SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Rebuild the case's `ReplayFile` from the committed fields.
fn build_replay(c: &Case) -> oh_replay::ReplayFile {
    let mut data = oh_replay::ReplayData::default();
    let bytes: Vec<u8> = (0..c.input_bytes_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&c.input_bytes_hex[i..i + 2], 16).unwrap())
        .collect();
    for b in bytes {
        data.record_input(b & 0x01 != 0, b & 0x02 != 0, b & 0x04 != 0, b & 0x08 != 0);
    }
    oh_replay::ReplayFile {
        version: 0,
        player_name: c.player_name.clone().into(),
        seed: c.seed,
        data,
        pack_id: c.pack.clone().into(),
        level_id: c.level.clone().into(),
        first_play: c.first_play,
        difficulty_mult: c.difficulty_mult,
        played_score: 0.0,
    }
}

/// One corpus case through the port: the byte-exact compression pin + the
/// full per-frame fingerprint comparison + the final result. Returns the
/// simulated frame count on success; on any divergence the Err carries the
/// 030b-attribution diagnostic (first divergent frame + which column
/// differs, or the final-result mismatch) with the case name prefixed.
///
/// The interchange-format pin (1) panics rather than returning: it is pure
/// Rust (`oh_replay`'s zlib) and cannot depend on the VM backend, so a
/// failure there is a corpus bug, not a three-way finding.
fn compare_case(assets: &Arc<oh_assets::Assets>, c: &Case) -> Result<u64, String> {
    let rf = build_replay(c);

    // (1) The interchange-format pin: the port's compression of this
    // ReplayFile must be byte-exactly the hex the C++ executed.
    let crf = oh_replay::compress_replay_file(&rf)
        .unwrap_or_else(|| panic!("{}: compression failed", c.name));
    if hex(&crf.data) != c.replay_z_hex {
        return Err(format!(
            "{}: the port's .ohr.z bytes are not byte-exactly the bytes the C++ executed (interchange-format pin, VM-independent)",
            c.name
        ));
    }

    // (2) The replay run with the 030b fingerprint enabled.
    let root = release_dir();
    let comp = compose(assets.clone(), root, false);
    let fp_path = fingerprint_temp_path(&c.name);
    comp.game.set_fingerprint_path(Some(&fp_path));

    let result = comp
        .game
        .run_replay_until_death_and_get_score(&rf, 1, 1.0)
        .ok_or_else(|| {
            format!(
                "{}: replay playback exceeded the 1-second processing budget",
                c.name
            )
        })?;

    // Back off the fingerprint's file writes (the C++ `fputs` is unbuffered
    // per call here too; the drop closes + flushes either way).
    comp.game.set_fingerprint_path(None);
    let text = std::fs::read_to_string(&fp_path)
        .unwrap_or_else(|e| panic!("{}: read the fingerprint file: {e}", c.name));
    let lines: Vec<&str> = text.lines().collect();

    // Frame count: one line per `update` call through the death frame.
    if lines.len() as u64 != c.frames {
        return Err(format!(
            "{}: the port simulated {} frames, the C++ {} (first port line: {:?})",
            c.name,
            lines.len(),
            c.frames,
            lines.first()
        ));
    }

    // The per-frame (frame, hash, draws) triple — the whole-engine oracle.
    for (i, line) in lines.iter().enumerate() {
        let expected = &c.fingerprints[i];
        if line != expected {
            let exp_parts: Vec<&str> = expected.split(' ').collect();
            let got_parts: Vec<&str> = line.split(' ').collect();
            let field = match (exp_parts.len(), got_parts.len()) {
                (3, 3) if exp_parts[0] == got_parts[0] => {
                    if exp_parts[2] == got_parts[2] {
                        "hash"
                    } else {
                        "draws"
                    }
                }
                _ => "frame",
            };
            return Err(format!(
                "{}: FIRST DIVERGENT FRAME {i} (field: {field})\n  C++  : {expected}\n  port : {line}\n  (attribution: same hash + different draws = the cosmetic-RNG split class; different hash = a real gameplay/state divergence — see the definition note's 'The RNG trap')",
                c.name
            ));
        }
    }

    // (3) The final result.
    if result.custom_score != c.custom_score {
        return Err(format!(
            "{}: customScore mismatch (no shipped level script calls l_overrideScore): port {} vs C++ {}",
            c.name, result.custom_score, c.custom_score
        ));
    }
    let printed = oh_math::hg::concat::format_ostream_f64(result.played_time_seconds);
    if printed != c.final_time_print {
        return Err(format!(
            "{}: the `Final time` the C++ printed (std::ostream 6-significant-digit format; the exact f64 bits are already pinned by the final frame's hash field 9): port {:?} vs C++ {:?}",
            c.name, printed, c.final_time_print
        ));
    }
    Ok(lines.len() as u64)
}

/// One corpus case through the port, all-or-nothing: the 030b gate
/// semantics (the first divergence panics, with the 030b attribution
/// message unchanged).
fn check_case(assets: &Arc<oh_assets::Assets>, c: &Case) {
    compare_case(assets, c).unwrap_or_else(|msg| panic!("{msg}"));
}

/// The whole corpus: every C++ column case replays through the port to the
/// identical per-frame fingerprint sequence + final result.
#[test]
fn cpp_fingerprint_vectors_match_the_port() {
    let doc = read_corpus(corpus_path("cpp_fingerprints.json"));
    let evidence = doc.get("evidence").and_then(|v| v.as_str());
    assert!(
        evidence.is_some(),
        "cpp_fingerprints.json must carry its `evidence` provenance"
    );

    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    for c in cases() {
        check_case(&assets, &c);
    }
}

/// The 030c per-case table — EVERY corpus case through the shipping VM,
/// one result line per case — `OK (N frames)` or the first divergent frame
/// and field (030b's attribution). Unlike the all-or-nothing corpus test
/// above, an early case's divergence does not hide the later cases'
/// results; the whole table is printed, and the test panics with it if ANY
/// case diverged. The C++ column is the committed `cpp_fingerprints.json`
/// (regenerated by neither side); this is the shipping configuration's
/// proof of the C++ parity guarantee (D-037: 030c's 3627/3627 frames were
/// produced by this exact configuration).
#[test]
fn cpp_fingerprint_per_case_table() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let backend = "Luau";
    eprintln!("030c per-case table [{backend} vs C++]:");
    let mut table: Vec<String> = Vec::new();
    let mut failures = 0usize;
    for c in cases() {
        let outcome = compare_case(&assets, &c);
        let row = match &outcome {
            Ok(frames) => format!("  {:24} frames={:>5}  OK", c.name, frames),
            Err(msg) => {
                failures += 1;
                format!("  {:24} MISMATCH: {msg}", c.name)
            }
        };
        eprintln!("{row}");
        table.push(row);
    }
    assert_eq!(
        failures,
        0,
        "030c: {failures} case(s) diverge from the C++ column under {backend}:\n{}",
        table.join("\n")
    );
    eprintln!(
        "030c: all {} cases match the C++ column under {backend}",
        cases().len()
    );
}

/// The harness itself is tested: a 1-ULP drift in a hashed field must be
/// discriminated by the comparison (the contract's deliberately-wrong-entry
/// check). Flip the last frame's hash of one case and expect the panic.
#[test]
fn deliberately_wrong_fingerprint_discriminates() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let c = cases().into_iter().next().expect("at least one case");

    // The real run must pass the real vectors (already proven by the
    // corpus test above; re-run the prefix up to the frame before the last
    // to keep this test fast, then corrupt the LAST line).
    let rf = build_replay(&c);
    let root = release_dir();
    let comp = compose(assets.clone(), root, false);
    let fp_path = fingerprint_temp_path("discriminating");
    comp.game.set_fingerprint_path(Some(&fp_path));
    let _ = comp
        .game
        .run_replay_until_death_and_get_score(&rf, 1, 1.0)
        .expect("replay terminated");
    comp.game.set_fingerprint_path(None);
    let text = std::fs::read_to_string(&fp_path).expect("read the fingerprint file");
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();

    // The uncorrupted comparison passes:
    assert_eq!(lines.len(), c.fingerprints.len(), "frame counts agree");
    for (a, b) in lines.iter().zip(&c.fingerprints) {
        assert_eq!(a, b, "uncorrupted vectors must match");
    }

    // Corrupt the last line's hash (a 1-ULP-class change in ANY field):
    // the comparison must flag the LAST frame as the first divergence.
    let last = lines.len() - 1;
    let h = u64::from_str_radix(lines[last].split(' ').nth(1).unwrap(), 16).expect("hash hex");
    let corrupted = format!(
        "{} {:016x} {}",
        lines[last].split(' ').next().unwrap(),
        h.wrapping_add(1),
        lines[last].split(' ').nth(2).unwrap()
    );
    lines[last] = corrupted;

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for (i, line) in lines.iter().enumerate() {
            if line != &c.fingerprints[i] {
                panic!("first divergent frame {i}");
            }
        }
    }));
    assert!(
        panicked.is_err(),
        "a corrupted final hash must be discriminated"
    );
}
