//! Issue 23 — replay-serialization parity. Rebuilds each corpus-pinned
//! replay file (`.agents/plans/001-rust-port/parity/
//! replay_serialization.json`, C++-probe-derived: the REAL
//! `src/SSVOpenHexagon/Core/Replay.cpp` via `tools/oracle/parity_probe.cpp`)
//! with `oh-replay` and compares the byte-exact file / zlib-level-9 /
//! `sf::Packet` forms, plus the `replay_player` cursor walk.
//!
//! Coverage per input class (the C++ replay system's representations):
//! the keyboard stream (`replay_data` input bytes — the mixed fixture
//! exercises all four bits; the level-run fixtures are the
//! `alwaysSpinRight` 0x02 stream), the replay file (the `.ohr` envelope),
//! the compressed replay (`compressed_replay_file`, zlib level 9), and the
//! wire form (`sf::Packet`). The C++ has no "menu action" replay input
//! class — the replay input is exactly the four `input_bit`s (see
//! PARITY.md, contract corrections).
//!
//! The harness itself is tested: `deliberately_wrong_pin_discriminates`
//! runs the same comparison on a corrupted pin and requires the mismatch.

use std::path::Path;

use oh_replay::{compress_replay_file, decompress_replay_file, Packet, ReplayData, ReplayFile};
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

fn hex_of(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(format!("{b:02x}").as_str());
    }
    s
}

/// The corpus input bytes: `input_byte_uniform` (N repetitions) or the
/// explicit `inputs` list.
fn corpus_inputs(f: &Value) -> Vec<u8> {
    let count = f["input_count"].as_u64().unwrap() as usize;
    if let Some(byte) = f.get("input_byte_uniform").and_then(|v| v.as_u64()) {
        vec![byte as u8; count]
    } else {
        let inputs = f["inputs"]
            .as_array()
            .unwrap_or_else(|| panic!("fixture has neither inputs nor input_byte_uniform"));
        assert_eq!(inputs.len(), count, "inputs list length != input_count");
        inputs.iter().map(|v| v.as_u64().unwrap() as u8).collect()
    }
}

/// Records the corpus input bytes into a `ReplayData` (the C++
/// `record_input(left, right, swap, focus)` bit order: 0=left, 1=right,
/// 2=swap, 3=focus — pinned by `tools/oracle/replay_probe.cpp` fixture 1).
fn data_from_bytes(bytes: &[u8]) -> ReplayData {
    let mut data = ReplayData::default();
    for b in bytes {
        data.record_input(
            b & (1 << 0) != 0,
            b & (1 << 1) != 0,
            b & (1 << 2) != 0,
            b & (1 << 3) != 0,
        );
    }
    data
}

/// Rebuilds the corpus fixture's replay file field-for-field.
fn fixture_file(f: &Value) -> ReplayFile {
    ReplayFile {
        version: f["version"].as_u64().unwrap() as u32,
        player_name: f["player_name"].as_str().unwrap().to_string(),
        seed: f["seed"].as_u64().unwrap(),
        data: data_from_bytes(&corpus_inputs(f)),
        pack_id: f["pack_id"].as_str().unwrap().to_string(),
        level_id: f["level_id"].as_str().unwrap().to_string(),
        first_play: f["first_play"].as_bool().unwrap(),
        difficulty_mult: f32::from_bits(
            u32::from_str_radix(
                f["difficulty_mult_bits"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("0x")
                    .unwrap(),
                16,
            )
            .expect("difficulty_mult_bits hex"),
        ),
        played_score: f64::from_bits(
            u64::from_str_radix(
                f["played_score_bits"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("0x")
                    .unwrap(),
                16,
            )
            .expect("played_score_bits hex"),
        ),
    }
}

/// Compares one corpus fixture against the Rust replay codecs. Returns
/// `Ok(())` or the first mismatch (the deliberately-wrong-entry test relies
/// on the `Err` arm).
fn check_fixture(name: &str, f: &Value) -> Result<(), String> {
    let rf = fixture_file(f);

    // -- the `.ohr` file form (host-endian raw fields) --
    let mut buf = [0u8; 1 << 20];
    let sr = rf.serialize(&mut buf);
    if !sr.is_success() {
        return Err(format!("{name}: rust serialize failed"));
    }
    if sr.written_bytes() as u64 != f["serialized_bytes"].as_u64().unwrap() {
        return Err(format!(
            "{name}: file bytes {} != corpus {}",
            sr.written_bytes(),
            f["serialized_bytes"]
        ));
    }
    if !hex_of(&buf[..sr.written_bytes()])
        .eq_ignore_ascii_case(f["serialized_hex"].as_str().unwrap())
    {
        return Err(format!("{name}: file hex mismatch"));
    }

    // -- the file-form deserialization round trip (Rust side) --
    let mut back = ReplayFile::default();
    let dr = back.deserialize(&buf[..sr.written_bytes()]);
    if !dr.is_success() || back != rf {
        return Err(format!("{name}: file-form round trip mismatch"));
    }

    // -- the compressed form (zlib level 9, the C++ `compress2` Z_BEST) --
    let crf =
        compress_replay_file(&rf).ok_or_else(|| format!("{name}: rust compression failed"))?;
    if crf.data.len() as u64 != f["compressed_bytes"].as_u64().unwrap() {
        return Err(format!(
            "{name}: compressed bytes {} != corpus {}",
            crf.data.len(),
            f["compressed_bytes"]
        ));
    }
    if !hex_of(&crf.data).eq_ignore_ascii_case(f["compressed_hex"].as_str().unwrap()) {
        return Err(format!("{name}: compressed hex mismatch"));
    }
    // The C++ probe pinned roundtrip_equal=1; the Rust side must agree.
    if !f["roundtrip_equal"].as_i64().is_some_and(|v| v == 1) {
        return Err(format!("{name}: corpus roundtrip_equal != 1"));
    }
    match decompress_replay_file(&crf) {
        Some(decoded) if decoded == rf => {}
        _ => return Err(format!("{name}: compressed round trip mismatch")),
    }

    // -- the `sf::Packet` wire form --
    let mut p = Packet::new();
    if !rf.serialize_to_packet(&mut p) {
        return Err(format!("{name}: rust packet serialize failed"));
    }
    if p.size() as u64 != f["packet_bytes"].as_u64().unwrap() {
        return Err(format!(
            "{name}: packet bytes {} != corpus {}",
            p.size(),
            f["packet_bytes"]
        ));
    }
    if !hex_of(p.data()).eq_ignore_ascii_case(f["packet_hex"].as_str().unwrap()) {
        return Err(format!("{name}: packet hex mismatch"));
    }
    Ok(())
}

#[test]
fn replay_serialization_matches_cpp_probe() {
    let doc = load("replay_serialization.json");
    let fixtures = doc["fixtures"].as_object().expect("corpus `fixtures` map");
    assert!(
        fixtures.len() == 6,
        "the corpus pins 6 fixtures (4 level pairs + the mixed stream + the \
         replay-player walk), got {}",
        fixtures.len()
    );
    for (name, f) in fixtures {
        if name == "replay_player_walk" {
            continue;
        }
        check_fixture(name, f).unwrap_or_else(|e| panic!("replay mismatch: {e}"));
    }
}

#[test]
fn replay_player_walk_matches_cpp_probe() {
    let doc = load("replay_serialization.json");
    let walk = &doc["fixtures"]["replay_player_walk"];
    let stream: Vec<u8> = walk["stream"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();

    let data = data_from_bytes(&stream);
    let mut rp = oh_replay::ReplayPlayer::new(&data);
    let mut walked: Vec<u8> = Vec::new();
    while !rp.done() {
        walked.push(rp.get_current_and_move_forward().to_u8());
    }
    let expected: Vec<u8> = walk["walk"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();
    assert_eq!(walked, expected, "cursor walk");
    assert_eq!(
        walked.len() as u64,
        walk["done_after"].as_u64().unwrap(),
        "the walk must end after exactly the corpus pop count"
    );
    rp.reset();
    assert_eq!(
        rp.get_current_and_move_forward().to_u8() as u64,
        walk["after_reset_first"].as_u64().unwrap(),
        "after_reset_first"
    );
    assert_eq!(
        data.size() as u64,
        walk["after_reset_size_unchanged"].as_u64().unwrap(),
        "the reset must not touch the data"
    );
}

#[test]
fn deliberately_wrong_pin_discriminates() {
    // The contract's "the harness itself is tested": the SAME comparison
    // logic must FAIL on a deliberately corrupted corpus entry.
    let doc = load("replay_serialization.json");
    let mut f = doc["fixtures"]["mixed_keyboard_stream"].clone();
    // Corrupt the last byte of the pinned file-form hex (the f64
    // played-score tail).
    let hex = f["serialized_hex"].as_str().unwrap();
    let mut chars: Vec<char> = hex.chars().collect();
    let last = chars.pop().expect("non-empty hex");
    chars.push(if last == '0' { '1' } else { '0' });
    f["serialized_hex"] = Value::String(chars.into_iter().collect());
    let err =
        check_fixture("mixed_keyboard_stream", &f).expect_err("a corrupted pin must not pass");
    assert!(
        err.contains("file hex"),
        "the mismatch names the form: {err}"
    );

    // Same for the compressed form.
    let mut f2 = doc["fixtures"]["mixed_keyboard_stream"].clone();
    let hex = f2["compressed_hex"].as_str().unwrap();
    f2["compressed_hex"] = Value::String(format!("00{hex}")); // wrong first byte
    let err2 =
        check_fixture("mixed_keyboard_stream", &f2).expect_err("a corrupted pin must not pass");
    assert!(
        err2.contains("compressed"),
        "the mismatch names the form: {err2}"
    );

    // And an intact fixture passes the same logic (sanity).
    check_fixture(
        "mixed_keyboard_stream",
        &doc["fixtures"]["mixed_keyboard_stream"],
    )
    .expect("uncorrupted fixture passes");
}
