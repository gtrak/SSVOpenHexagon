# Issue 02 — `oh-replay`: byte-exact replay format, zlib compression, packet codec

## Objective

Port `hg::replay_data`, `hg::replay_player`, `hg::replay_file`,
`hg::compressed_replay_file` and the compression/decompression functions with
**byte-exact** compatibility: a Rust-written `.ohr` file must load in the C++
build and vice versa, and the compressed form must decompress in both directions.

## Key decisions

- Mirror `include/SSVOpenHexagon/Core/Replay.hpp` + `src/.../Replay.cpp` 1:1.
- `std::byte` buffers → `&[u8]` / `&mut [u8]`; the C++ `serialization_result` /
  `deserialization_result` (bytes-written/read + success flag) map to a Rust
  `Result`-like struct with the same saturated semantics (partial write ⇒ failure).
- `input_bit` bitset → a small `InputBits` newtype over `u8`, bits 0..3 =
  left,right,swap,focus. Keep `to_ulong()` equivalence (bit order).
- zlib → `flate2` with `Compression::best()` and the zlib (not raw-deflate) wrapper
  (**D-004 zlib entry**).
- Size limits: C++ uses a 2 MiB static buffer (`buf_size = 2097152`). Keep the
  same cap and same failure behavior.

## Files

| File | Change |
|---|---|
| `crates/oh-replay/Cargo.toml` | new — deps `flate2`, `oh-rng` |
| `crates/oh-replay/src/lib.rs` | new — structs + serialize/deserialize |
| `crates/oh-replay/src/compress.rs` | new — zlib compress/decompress |
| `crates/oh-replay/tests/oracle.rs` | new — vectors from `mapping/oracle-vectors.md` |
| `.agents/skills/lib-zlib.md` | new — from real `Replay.cpp` usage + zlib docs source |
| `.agents/skills/lib-flate2.md` | new — from the crate's real source |

## Steps

1. Port `input_bit`, `input_bitset` (as `InputBits`), `serialization_result`,
   `deserialization_result`.
2. Port `replay_data` (`record_input`, `at`, `size`, `==`, `serialize`,
   `deserialize`, both the `(buf, size)` and `(buf, end)` overloads).
3. Port `replay_player` (`get_current_and_move_forward`, `done`, `reset`).
4. Port `replay_file` with the **exact field order** in
   `mapping/oracle-vectors.md`: version u32, player_name u32-len string, seed u64,
   replay_data, pack_id u32-len string, level_id u32-len string, first_play u8,
   difficulty_mult f32, played_score f64.
5. Port `serialize_to_file` / `deserialize_from_file`, `serialize_to_packet` /
   `deserialize_from_packet` (u64 length prefix + raw bytes), `create_filename`
   (format from source), `played_seconds` (`_played_score / 60.0`).
6. Port `compressed_replay_file` + `compress_replay_file` / `decompress_replay_file`
   using `flate2` with `Compression::best()` and the 2 MiB buffer cap.
7. Write `lib-zlib.md` and `lib-flate2.md` from real source (read the flate2 crate
   source in `~/.cargo/registry/src/**` after first build; record the version).

## Verification

- Round-trip tests in Rust (self-consistency) **and** cross-compatibility tests:
  - `replay_file` → bytes must equal `HEX` in `oracle-vectors.md`.
  - The hex in `oracle-vectors.md` must deserialize back to the same struct.
  - `compress_replay_file` output must equal `CZHEX`.
  - The `CZHEX` blob must decompress to the original replay.
  - `serialize_to_packet` must equal `PHEX`.
- Additionally, write a small Rust test binary or `#[test]` that **emits** the
  hex for the fixture, so the orchestrator can diff it against a fresh C++ run.
- `tools/gate.sh` green.

## Notes / risks

- The `usize` field for `n_inputs` is 8 bytes on x86_64/Windows x64 — pin to `u64`
  explicitly to stay portable (and note the deviation for 32-bit builds; the game
  targets 64-bit).
- `ssvu::concat` float formatting for filenames must match C++ default ostream
  (`std::to_string`-ish); pin with a test once issue-01's `Concat` exists. If exact
  match is impossible, record the deviation and the reason in the report.
- Endianness: C++ writes host-endian (LE on target platforms). Rust must use
  `to_le_bytes` explicitly so the format is deterministic.
