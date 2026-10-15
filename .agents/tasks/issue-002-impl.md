# Task spec — issue 002: `oh-replay` byte-exact replay

You are a worker. Implement exactly this issue. Do not redesign, do not widen scope.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port of the C++ game
(plan `.agents/plans/001-rust-port/PLAN.md`). Issue 01 is landing the workspace,
`oh-math`, and `oh-rng`.

Read first, in order:
1. `.agents/plans/001-rust-port/PLAN.md`, `DECISIONS.md`
2. `.agents/plans/001-rust-port/02-replay.md` (the contract)
3. `.agents/plans/001-rust-port/mapping/oracle-vectors.md` — **the byte-exact pins**
   (replay HEX/CZHEX/PHEX, field layout, SFML Packet encoding table)
4. `include/SSVOpenHexagon/Core/Replay.hpp` + `src/SSVOpenHexagon/Core/Replay.cpp`
5. `.agents/skills/README.md`

## Working agreement

- **Skills are truth.** Write `.agents/skills/lib-zlib.md` and
  `.agents/skills/lib-flate2.md` from real source (flate2 from its checked-out
  crate source under `~/.cargo/registry/src/**`; read it, do not fetch).
- **Write first**: scaffold the crate and module skeletons immediately, compile,
  iterate on named errors.
- **Arithmetic/format exactness**: mirror `Replay.hpp`/`.cpp` 1:1. File format uses
  `to_le_bytes`; packet format uses the SFML rules in the oracle doc (integers
  big-endian, strings u32-BE length + raw bytes; `append` raw).
- **Commit at the first green build**, then keep going.

## What to build

Port, with the exact field order and encoding in the oracle doc:

1. `input_bit`, `input_bitset` → `InputBits` (u8 bitset, bits 0..3 = left,right,swap,focus).
2. `serialization_result` / `deserialization_result` (bytes written/read + success).
3. `replay_data`: `record_input`, `at`, `size`, `PartialEq`, both `serialize`/
   `deserialize` overloads (`(buf, len)` and `(buf, end)` semantics).
4. `replay_player`: `get_current_and_move_forward`, `done`, `reset`.
5. `replay_file` with the exact layout: version u32, player_name (u32-len string),
   seed u64, replay_data, pack_id (u32-len string), level_id (u32-len string),
   first_play u8, difficulty_mult f32, played_score f64.
6. `serialize_to_file`/`deserialize_from_file` (2 MiB buffer cap = 2097152),
   `serialize_to_packet`/`deserialize_from_packet` (u64 BE length prefix + raw),
   `create_filename`, `played_seconds` (`_played_score / 60.0`).
7. `compressed_replay_file` + `compress_replay_file`/`decompress_replay_file` via
   `flate2` with `Compression::best()` (zlib wrapper) and the same buffer cap.

## Verification

Run and paste outputs:
1. `bash tools/gate.sh` — all `GATE_EXIT=` lines.
2. `cargo test -p oh-replay` — the oracle tests:
   - build the fixture replay (version 5, "tester", seed 12345, "ohvrvanilla",
     "cube_1_apeirogon", first_play true, 1.6f, 3600.0, inputs 0000/0100/1010/0000/0100/1001)
     and assert the serialized bytes == `HEX`;
   - deserialize `HEX` and assert the struct equals the fixture;
   - assert `compress_replay_file` output == `CZHEX`;
   - assert `CZHEX` decompresses to the fixture;
   - assert `serialize_to_packet` == `PHEX`.
3. A **discrimination check**: in a SCRATCH copy of the tree (never the reviewed
   tree), flip a byte offset (e.g. write the string length big-endian) and show the
   HEX test fails. Report what you observed.

## Report format

Module map, design summary, gate outputs, skill corrections, deviations, known gaps.
