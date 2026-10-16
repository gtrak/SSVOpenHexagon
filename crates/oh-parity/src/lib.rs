//! Issue 23 — the SSVOpenHexagon Rust-port **parity harness**.
//!
//! This crate is a tests crate: it runs the pinned level/seed/input corpus
//! (`.agents/plans/001-rust-port/parity/`) through the Rust build and
//! compares outcomes against C++-derived expectations. The evidence chain:
//!
//! * `tests/parity_rng.rs` — the RNG wrapper streams (C++-probe-derived: the
//!   REAL `src/SSVOpenHexagon/Core/RandomNumberGenerator.cpp` compiled by
//!   `tools/oracle/parity_probe.cpp`).
//! * `tests/parity_replay.rs` — replay file / compressed / packet bytes and
//!   the `replay_player` cursor walk (C++-probe-derived: the REAL
//!   `src/SSVOpenHexagon/Core/Replay.cpp`).
//! * `tests/parity_pack_ids.rs` — `buildPackId` / `getLevelValidator`
//!   strings for every shipped `_RELEASE` pack (C++-probe-derived: the REAL
//!   `Utils/BuildPackId.cpp` + `Utils/LevelValidator.cpp`).
//! * `tests/parity_level_runs.rs` — the six `_RELEASE` pack/level entries the
//!   determinism harness exercises, run live (`alwaysSpinRight`) AND as
//!   seeded replay executions; the run outcomes (score / frames / input
//!   bytes) are pinned against the corpus (analytically-derived,
//!   Rust-self-consistent — the full C++ game build is not available on
//!   this machine; documented in `PARITY.md`), and the replay envelopes are
//!   byte-exact-checked against the C++-probe-derived corpus bytes.
//!
//! Every test also carries a deliberately-wrong-entry discrimination check
//! (the contract's "the harness itself is tested"): the same comparison
//! logic must FAIL on a corrupted pin.
//!
//! Regenerate the C++-derived corpus: `bash tools/parity.sh`.
//! Results table: `.agents/plans/001-rust-port/PARITY.md`.
