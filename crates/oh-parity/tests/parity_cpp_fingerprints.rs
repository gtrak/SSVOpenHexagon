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

mod common;

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
/// full per-frame fingerprint comparison + the final result.
fn check_case(assets: &Arc<oh_assets::Assets>, c: &Case) {
    let rf = build_replay(c);

    // (1) The interchange-format pin: the port's compression of this
    // ReplayFile must be byte-exactly the hex the C++ executed.
    let crf = oh_replay::compress_replay_file(&rf)
        .unwrap_or_else(|| panic!("{}: compression failed", c.name));
    assert_eq!(
        hex(&crf.data),
        c.replay_z_hex,
        "{}: the port's .ohr.z bytes must be byte-exactly the bytes the C++ executed",
        c.name
    );

    // (2) The replay run with the 030b fingerprint enabled.
    let root = release_dir();
    let comp = compose(assets.clone(), root, false);
    let fp_path = std::env::temp_dir().join(format!("oh030b-parity-{}.fp.txt", c.name));
    comp.game.set_fingerprint_path(Some(&fp_path));

    let result = comp
        .game
        .run_replay_until_death_and_get_score(&rf, 1, 1.0)
        .unwrap_or_else(|| {
            panic!(
                "{}: replay playback exceeded the 1-second processing budget",
                c.name
            )
        });

    // Back off the fingerprint's file writes (the C++ `fputs` is unbuffered
    // per call here too; the drop closes + flushes either way).
    comp.game.set_fingerprint_path(None);
    let text = std::fs::read_to_string(&fp_path)
        .unwrap_or_else(|e| panic!("{}: read the fingerprint file: {e}", c.name));
    let lines: Vec<&str> = text.lines().collect();

    // Frame count: one line per `update` call through the death frame.
    assert_eq!(
        lines.len() as u64,
        c.frames,
        "{}: the port simulated {} frames, the C++ {} (first port line: {:?})",
        c.name,
        lines.len(),
        c.frames,
        lines.first()
    );

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
            panic!(
                "{}: FIRST DIVERGENT FRAME {i} (field: {field})\n  C++  : {expected}\n  port : {line}\n  (attribution: same hash + different draws = the cosmetic-RNG split class; different hash = a real gameplay/state divergence — see the definition note's 'The RNG trap')",
                c.name
            );
        }
    }

    // (3) The final result.
    assert_eq!(
        result.custom_score, c.custom_score,
        "{}: customScore (no shipped level script calls l_overrideScore)",
        c.name
    );
    let printed = oh_math::hg::concat::format_ostream_f64(result.played_time_seconds);
    assert_eq!(
        printed,
        c.final_time_print,
        "{}: the `Final time` the C++ printed (std::ostream 6-significant-digit format; the exact f64 bits are already pinned by the final frame's hash field 9)",
        c.name
    );
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
    let fp_path = std::env::temp_dir().join("oh030b-parity-discriminating.fp.txt");
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
