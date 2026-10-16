# lib-flate2 — flate2 (Rust), the replay compression backend

Pinned version: **flate2 1.1.10** (workspace dep, `Cargo.lock:702-712`), used on
its **`zlib` feature** — `flate2 = { version = "1", default-features = false,
features = ["zlib"] }` (root `Cargo.toml:40`, `crates/oh-replay/Cargo.toml:11`
`flate2 = { workspace = true }`). The `zlib` feature is
`["any_c_zlib", "libz-sys"]` (`flate2-1.1.10/Cargo.toml`), i.e. the **C
backend**, not the default `rust_backend` (miniz_oxide). Read from checked-out
source: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/flate2-1.1.10/`
and the consumer `crates/oh-replay/src/compress.rs`.

Companion skills: `.agents/skills/lib-mlua.md` (D-028a, the web Lua gate) and
`.agents/skills/lib-web-stack.md` (the wasm C-toolchain matrix this skill's
web-substitution point feeds).

## Why the `zlib` feature (D-006), not the default rust backend

The C++ build compresses replays with system zlib `compress2(..., Z_BEST_
COMPRESSION)` (level 9) / `uncompress` (`src/SSVOpenHexagon/Core/Replay.cpp`).
The replay **CZHEX oracle pin** in `mapping/oracle-vectors.md` requires the
Rust side to emit **byte-identical** compressed bytes, which is only true when
the same `deflate` implementation runs — i.e. flate2's C backend driving the
system `deflate`/`inflate` (`deflateInit2(level, Z_DEFLATED, 15, 8,
Z_DEFAULT_STRATEGY)` + `deflate(Z_FINISH)` = the `compress2` setup;
`inflateInit2(15)` + `inflate(FINISH)` = `uncompress`). The default
miniz_oxide rust backend produces *compatible but NOT byte-identical*
deflate (different level-9 Huffman/honesty choices), so it is **off by
default** here. This is the desktop-oracle property: byte-exactness is pinned
against system zlib only.

## `oh-replay`'s use (`crates/oh-replay/src/compress.rs`)

Imports (`compress.rs:14`): `flate2::{Compress, Compression, Decompress,
FlushCompress, FlushDecompress, Status}`.

- `compress_replay_file(&ReplayFile) -> Option<CompressedReplayFile>`
  (`compress.rs:23`): serialize into the 2 MiB `STATIC_BUF`, then
  `Compress::new(Compression::best(), true /* zlib header */)`,
  `c.compress(&src[..written], dst, FlushCompress::Finish)` →
  `Ok(Status::StreamEnd)` → take `c.total_out()` bytes (`compress.rs:38-45`).
  `Compression::best()` = level 9 = C++ `Z_BEST_COMPRESSION`. Any other
  `Status` → `eprintln!` the C++'s `"Failed compression of replay file, error
  code: '{:?}'"` + `None` (`compress.rs:47-52`).
- `decompress_replay_file(&CompressedReplayFile) -> Option<ReplayFile>`
  (`compress.rs:63`): `Decompress::new(true /* zlib header */)`,
  `d.decompress(&crf.data, buf, FlushDecompress::Finish)` →
  `Ok(Status::StreamEnd)` → `ReplayFile::default().deserialize(&buf[..n])`
  (`compress.rs:70-83`); failure → the C++'s message + `None`.

The 2 MiB `STATIC_BUF` / `STATIC_COMPRESSION_BUF` mirror the C++ static
buffers; the destination size is the output cap (the C++ `in_out_dest_len`).

## The replay interchange contract (D-028b) — what actually travels

Replays are exchanged as an **encrypted compressed blob** —
`ClientToServer::CTSPCompressedReplay` → `send_encrypted`
(`crates/oh-net/src/client.rs:892`); the server calls
`oh_replay::decompress_replay_file` (`crates/oh-net/src/server.rs:1989`). There
is **no hash/signature/checksum over the compressed payload** in `oh-replay` /
`oh-net` (the `levelValidator` is a separate field — the supported-level gate +
verbose log only). So the promise across a client/server boundary is
**"same replay semantics — decompression recovers the same logical replay"**,
NOT byte-identity. Byte-identity is asserted only by the desktop `oh-parity`
oracle pins.

## P0.1 — miniz_oxide interchange: PASS (Phase 0, 2026-10-16)

Throwaway cross-check (fixture `test/fixtures/cube_1_apeirogon.ohr.z`, a
569-byte serialized replay) using **miniz_oxide 0.9.1**
(`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/miniz_oxide-0.9.1/`,
the version already in `Cargo.lock:1409-1417` as flate2's optional rust
backend — read, not fetched):

- repo zlib stream (101 B) → `miniz_oxide::inflate::decompress_to_vec_zlib`
  (`miniz_oxide-0.9.1/src/inflate/mod.rs:159`) → exact 569 bytes ✔
- `miniz_oxide::deflate::compress_to_vec_zlib(.., 9)`
  (`miniz_oxide-0.9.1/src/deflate/mod.rs:143`) (112 B) → repo
  `decompress_replay_file` → `ReplayFile ==` original (field-for-field) ✔
- compressed bytes differ (101 vs 112) — expected; the contract is **lossless
  logical recovery, not byte-identity** ✔
- committed fixture bytes == local libz `compress2` output (the zlib side is the
  genuine repo/oracle behavior) ✔

**Verdict:** miniz_oxide is a safe web substitution for **interchange**
(D-028b). Byte-identity remains a desktop-oracle property only.

## The wasm constraint (why the web build substitutes miniz_oxide)

`libz-sys 1.1.30` (`Cargo.lock:1309-1313`) builds the zlib C source via
`cc::Build` (`libz-sys-1.1.30/build.rs` `build_zlib` → `cfg.compile("z")`),
which needs a **C compiler for the target**. On `wasm32-unknown-unknown` there
is no system libz and no wasm C compiler in the default toolchain, so
`libz-sys` (hence flate2's `zlib` backend) does **not** cross-compile — this is
the D-028b substitution point. The web build therefore uses **miniz_oxide**
(pure Rust, no C) for the interchange, accepting the semantic (not
byte-identical) contract above. See `.agents/skills/lib-web-stack.md` for the
full C-toolchain matrix.

## What NOT to do

- Do **not** switch `oh-replay` to flate2's default `rust_backend` on desktop —
  it breaks the CZHEX byte-exact oracle pin (D-006).
- Do **not** route desktop replay compression through miniz_oxide expecting
  byte-identity; it is the web-interchange substitute only (P0.1: lossless,
  not byte-identical).
