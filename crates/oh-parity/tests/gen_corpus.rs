//! Task 030b — the synthetic replay corpus GENERATOR (the C++ column).
//!
//! Builds the v1 fingerprint-corpus replays (definition:
//! `.agents/plans/001-rust-port/parity/cpp-fingerprint-definition.md`),
//! compresses each with `oh_replay::compress_replay_file` (the byte-exact
//! oracle-pinned `.ohr.z` interchange format, issue 02), and writes
//! `<name>.ohr.z` into the directory named by `OH030B_CORPUS_OUT`
//! (default: `$CARGO_TARGET_DIR/oh030b-corpus`) so the C++ headless run can
//! consume the same bytes (the container mounts `$BUILD` at `/build`):
//!
//! ```text
//! OH030B_CORPUS_OUT=/home/gary/dev/oh-cpp-build/corpus \
//!   cargo test -p oh-parity --test gen_corpus
//! tools/cpp-oracle/run.sh run -- -headless /build/corpus/<name>.ohr.z \
//!   -fingerprint /build/corpus/<name>.fp.txt
//! ```
//!
//! The committed vectors (`parity/cpp_fingerprints.json`) carry each replay's
//! exact `.ohr.z` bytes (hex) + input bytes (hex) + the C++ fingerprint
//! output; `parity_cpp_fingerprints.rs` regenerates every replay
//! in-process and byte-compares the compression against the committed hex
//! (the interchange-format pin) before replaying it through the port.

/// One corpus case: the replay file fields + the input stream.
struct Case {
    name: &'static str,
    pack: &'static str,
    level: &'static str,
    seed: u64,
    difficulty_mult: f32,
    /// (input byte value, run length) segments.
    inputs: &'static [(u8, usize)],
}

/// The v1 corpus (the four seeded-replay runs pinned by
/// `parity/level_runs.json` — same levels, seed, difficulty, streams —
/// plus the difficulty-branch and the mixed-input cases).
const CASES: [Case; 6] = [
    Case {
        name: "apeirogon_spin439",
        pack: "ohvrvanilla_vittorio_romeo_cube_1",
        level: "ohvrvanilla_vittorio_romeo_cube_1_apeirogon",
        seed: 987654321,
        difficulty_mult: 1.0,
        inputs: &[(0x02, 439)],
    },
    Case {
        name: "autotest0_spin1180",
        pack: "ohvrvanilla_vittorio_romeo_experimental_1",
        level: "ohvrvanilla_vittorio_romeo_experimental_1_autotest0",
        seed: 987654321,
        difficulty_mult: 1.0,
        inputs: &[(0x02, 1180)],
    },
    Case {
        name: "arcadia_spin456",
        pack: "ohvrvanilla_vittorio_romeo_orthoplex_1",
        level: "ohvrvanilla_vittorio_romeo_orthoplex_1_arcadia",
        seed: 987654321,
        difficulty_mult: 1.0,
        inputs: &[(0x02, 456)],
    },
    Case {
        name: "bipolarity_spin358",
        pack: "ohvrvanilla_vittorio_romeo_orthoplex_1",
        level: "ohvrvanilla_vittorio_romeo_orthoplex_1_bipolarity",
        seed: 987654321,
        difficulty_mult: 1.0,
        inputs: &[(0x02, 358)],
    },
    Case {
        name: "apeirogon_spin439_dm25",
        // The level script's `u_getDifficultyMult() > 1.5` onInit branch +
        // the `getSpeedMultDM()` / `getDelayMultDM()` difficulty scaling.
        pack: "ohvrvanilla_vittorio_romeo_cube_1",
        level: "ohvrvanilla_vittorio_romeo_cube_1_apeirogon",
        seed: 987654321,
        difficulty_mult: 2.5,
        inputs: &[(0x02, 439)],
    },
    Case {
        name: "autotest0_mixed",
        // Mixed stream: right, right+swap, right, focus, no-input — the
        // replay executor feeds exactly these bits (bit 0 = left, bit 1 =
        // right, bit 2 = swap, bit 3 = focus; tools/oracle/replay_probe.cpp
        // fixture 2 bit order).
        pack: "ohvrvanilla_vittorio_romeo_experimental_1",
        level: "ohvrvanilla_vittorio_romeo_experimental_1_autotest0",
        seed: 314159,
        difficulty_mult: 1.5,
        inputs: &[
            (0x02, 200),
            (0x0A, 100),
            (0x02, 100),
            (0x08, 100),
            (0x00, 500),
        ],
    },
];

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(format!("{b:02x}").as_str());
    }
    s
}

fn build_replay(case: &Case) -> oh_replay::ReplayFile {
    let mut data = oh_replay::ReplayData::default();
    for &(byte, count) in case.inputs {
        for _ in 0..count {
            data.record_input(
                byte & 0x01 != 0,
                byte & 0x02 != 0,
                byte & 0x04 != 0,
                byte & 0x08 != 0,
            );
        }
    }
    oh_replay::ReplayFile {
        version: 0,
        player_name: "testProfile".into(),
        seed: case.seed,
        data,
        pack_id: case.pack.into(),
        level_id: case.level.into(),
        first_play: true,
        difficulty_mult: case.difficulty_mult,
        // The C++ headless run is `first_play`-driven from the file; the
        // value only enters the envelope (and the music-segment gate).
        played_score: 0.0,
    }
}

#[test]
fn write_corpus_replays() {
    let out = std::env::var("OH030B_CORPUS_OUT").unwrap_or_else(|_| {
        std::env::var("CARGO_TARGET_DIR")
            .map(|t| format!("{t}/oh030b-corpus"))
            .unwrap_or_else(|_| "oh030b-corpus".into())
    });
    std::fs::create_dir_all(&out).expect("corpus dir");

    for case in &CASES {
        let rf = build_replay(case);
        let crf =
            oh_replay::compress_replay_file(&rf).expect("compression (byte-exact zlib level 9)");
        let path = format!("{out}/{}.ohr.z", case.name);
        std::fs::write(&path, &crf.data).expect("write .ohr.z");
        let input_hex: String = (0..rf.data.size())
            .map(|i| format!("{:02x}", rf.data.at(i).to_u8()))
            .collect();
        println!(
            "corpus {} {} = {} ({} input bytes)\n  z={}",
            case.name,
            hex(&crf.data),
            path,
            input_hex.len(),
            input_hex
        );
    }
}
