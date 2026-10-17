# Audit: `Core/Replay` (C++) ↔ `crates/oh-replay` (Rust)

**Scope.** C++ module pair read completely: `src/SSVOpenHexagon/Core/Replay.cpp` (568 lines) +
`include/SSVOpenHexagon/Core/Replay.hpp` (169 lines). Rust counterpart read completely:
`crates/oh-replay/src/lib.rs` (899 lines), `crates/oh-replay/src/packet.rs` (330 lines),
`crates/oh-replay/src/compress.rs` (96 lines), plus `crates/oh-replay/tests/oracle.rs` and
`crates/oh-replay/Cargo.toml`. Support ports verified: `crates/oh-math/src/hg/timestamp.rs`,
`crates/oh-math/src/hg/concat.rs`, `crates/oh-rng/src/lib.rs` (`Seed = u64`),
`include/SSVOpenHexagon/Utils/{Concat.hpp,Timestamp.hpp}`, `src/SSVOpenHexagon/Utils/Timestamp.cpp`,
`include/SSVOpenHexagon/Core/RandomNumberGeneratorTypes.hpp:9`,
`include/SSVOpenHexagon/Global/Assert.hpp:18-33`.
Pinned oracles: `.agents/plans/001-rust-port/mapping/oracle-vectors.md`
(`HEX`/`CZHEX`/`PHEX` fixtures, SFML `Packet` encoding table).

**Audit focus: byte-level format, compression, packet codec.**

No event handlers, subscriptions, or risingEdge polls exist in either module.
No network/sound/screenshot/window ops. File I/O side effects are covered below.

---

## 1. C++ BEHAVIOR INVENTORY

All lines are `src/SSVOpenHexagon/Core/Replay.cpp` unless noted (hpp =
`include/SSVOpenHexagon/Core/Replay.cpp`... i.e. `include/SSVOpenHexagon/Core/Replay.hpp`).

### File-statics / globals

| item | what it does | loc |
|---|---|---|
| `printTryFailure(const char*)` | `[[gnu::cold]] static`; prints `Failed [de]serialization operation '<code>'` to `std::cerr` | cpp:25-28 |
| `SSVOH_TRY(...)` macro | runs op; if `!result._success`: `printTryFailure(#op)` then `return result` | cpp:30-41 |
| `make_write(result, buffer, buffer_end)` | lambda: bounds-checked `memcpy` of a datum; on overflow `_success=false`, no write/advance; else advance + `_written_bytes += sizeof` | cpp:43-57 |
| `make_read(result, buffer, buffer_end)` | lambda: bounds-checked `memcpy` into target; on overflow `_success=false`, no read/advance | cpp:60-73 |
| `buf_size = 2097152` | 2 MiB scratch cap | cpp:327 |
| `get_static_buf()` | `thread_local std::byte buf[buf_size]` — scratch for file/packet/decompress paths | cpp:329-332 |
| `get_static_compression_buf()` | separate `thread_local` 2 MiB buffer for `compress2` output | cpp:500-503 |

### `replay_data` (hpp:70-96)

| fn | behavior | loc |
|---|---|---|
| `record_input(l,r,s,f)` | `emplace_back()` a zero `input_bitset`, sets bit 0=left, 1=right, 2=swap, 3=focus | cpp:77-83 |
| `at(i)` | `SSVOH_ASSERT(i < size())`, returns `_inputs[i]` (release: assert empty → UB read) | cpp:87-92 |
| `size()` | `_inputs.size()` | cpp:94-97 |
| `operator==` / `!=` | vector compare / negation | cpp:99-108 |
| `serialize(buf, size)` / `deserialize(buf, size)` | overloads delegating to pointer/end form | cpp:111-120 |
| `serialize(buf, end)` | `SSVOH_TRY(write(n_inputs))` — `n_inputs` is `std::size_t` (8 bytes LE on pinned x86_64, host-endian `memcpy`); then per input: `SSVOH_TRY(write((uint8)ib.to_ulong()))` | cpp:123-138 (130, 135) |
| `deserialize(buf, end)` | `SSVOH_TRY(read(n_inputs))`; **`_inputs.resize(n_inputs)`** (attacker-sized allocation → bad_alloc/terminate on hostile value); per input `SSVOH_TRY(read(ib_byte)); _inputs[i] = input_bitset{(unsigned long)ib_byte}` (low 4 bits kept) | cpp:141-163 (148, 150, 159) |

### `replay_player` (hpp:98-110)

| fn | behavior | loc |
|---|---|---|
| ctor | stores `const replay_data&`, `_current_index = 0` | cpp:170-172 |
| `get_current_and_move_forward()` | if `size() <= _current_index` return default (all-clear) bitset; else return `at(_current_index++)` | cpp:175-181 |
| `done()` | `_current_index == _replay_data.size()` | cpp:185-188 |
| `reset()` | `_current_index = 0` | cpp:190-193 |

### `replay_file` (hpp:112-151)

Fields (hpp:116-124): `_version u32`, `_player_name string`, `_seed seed_type (= u64, hpp:114)`,
`_data replay_data`, `_pack_id string`, `_level_id string`, `_first_play bool`,
`_difficulty_mult float`, `_played_score double`.

| fn | behavior | loc |
|---|---|---|
| `operator==` / `!=` | all 9 fields incl. floats | cpp:196-213 |
| `serialize(buf, size)` / `deserialize(buf, size)` | overloads | cpp:216-225 |
| `serialize(buf, end)` | `write(_version)` (u32 LE); `write_str(_player_name)` = `write(u32 size)` (truncating cast) + byte loop; `write(_seed)` (u64 LE); **`_data.serialize(buffer, buffer_end)`** on the advanced pointer, fail → `result._success=false; return` (no `_written_bytes` add); else `buffer += / result._written_bytes += data_result._written_bytes`; then `write_str(_pack_id)`, `write_str(_level_id)`, `write(_first_play)` (1 byte 0/1), `write(_difficulty_mult)` (f32 LE), `write(_played_score)` (f64 LE) | cpp:228-268 (236-240, 246-248, 262-266) |
| `deserialize(buf, end)` | mirror; `read_str` = `read(u32 s_size)`; **`s.resize(s_size)`** (attacker-sized alloc); byte loop; `_data.deserialize(buffer, buffer_end)` on advanced pointer with same fail/no-add semantics | cpp:271-324 (280, 282, 291, 302-304, 318-322) |
| `serialize_to_file(p)` | serialize into `get_static_buf()` (2 MiB cap); fail → `false` (after `SSVOH_TRY` stderr prints inside `serialize`); else `ofstream(p, binary\|out)` (truncating), `write(buf, sr.written_bytes())`, `flush`, `return bool(os)` | cpp:335-349 |
| `deserialize_from_file(p)` | `ifstream(p, binary\|in)`; open fail → **`std::cerr << "Couldn't open replay path '<p>'\n"`**, `false`; `tellg()` size; `seekg(0)`; read whole file **into the 2 MiB static buffer** (file > 2 MiB → short read → failbit → silent `false`, no allocation beyond 2 MiB); `deserialize(buf, bytes_to_read)` | cpp:353-378 (359, 363-370) |
| `serialize_to_packet(p)` | serialize into static buffer (fail → `false` + stderr inside); `p << (u64)written_bytes` (SFML BE); `p.append(buf, written_bytes)`; `true` | cpp:380-394 |
| `deserialize_from_packet(p)` | `p >> bytes_to_read` (u64 BE), fail → `false`; **byte-by-byte loop writing into the 2 MiB static buffer with NO bounds check against `bytes_to_read`** (payload > 2 MiB with enough data → heap buffer overflow / UB; short packet → silent `false`); `deserialize(buf, bytes_to_read)` | cpp:397-420 (408, 410-416) |
| `create_filename()` | `nowTimestamp()` → `toTimepoint` → `formatTimepoint("%Y%m%d_%H%M%S")` (UTC) → `Utils::concat(_version,'_',tp_str,'_',_player_name,'_',_level_id,'_',_difficulty_mult,"x_",played_seconds(),"s.ohr")` — mixed-type concat → `ostringstream` path (floats = default 6-sig-digit `%g`) | cpp:423-430 |
| `played_seconds()` | `_played_score / 60.0` | cpp:432-435 |

### `compressed_replay_file` (hpp:153-162)

| fn | behavior | loc |
|---|---|---|
| `serialize_to_file(p)` | `ofstream(p, binary\|out)`, write raw `_data`, flush, `bool(os)` | cpp:437-445 |
| `deserialize_from_file(p)` | open fail → **`std::cerr << "Couldn't open compressed replay path '<p>'\n"`**, `false`; `tellg` size; **`_data.resize(bytes_to_read)` (unbounded)**; read; `bool(is)` | cpp:447-465 (453, 461) |
| `serialize_to_packet(p)` | `p << (u64)_data.size()` (BE); `p.append(_data.data(), size)`; `true` | cpp:467-473 |
| `deserialize_from_packet(p)` | `p >> bytes_to_read` (u64 BE); **`_data.resize(bytes_to_read)` (attacker-sized alloc)**; byte-by-byte read, fail → `false` | cpp:475-497 (487, 489-495) |

### Free functions

| fn | behavior | loc |
|---|---|---|
| `compress_replay_file(rf)` | serialize into `get_static_buf()` (fail → `nullopt`, after `SSVOH_TRY` stderr inside); `compress2(compression_buf, in_out_dest_len=buf_size /*2 MiB out cap*/, buf, written, Z_BEST_COMPRESSION)`; `rc != Z_OK` → **`std::cerr << "Failed compression of replay file, error code: '<rc>'\n"`** + `nullopt`; else `_data` = first `in_out_dest_len` bytes | cpp:506-539 (526) |
| `decompress_replay_file(crf)` | `uncompress(buf /*2 MiB out cap*/, in_out_dest_len, crf._data.data(), crf._data.size())`; `rc != Z_OK` → same stderr message (C++ copy-paste: says "compression") + `nullopt`; else default `replay_file result;`, `result.deserialize(buf, in_out_dest_len)` → fail (with `SSVOH_TRY` stderr inside) → `nullopt` | cpp:541-568 (552) |

### State transitions

- `replay_player._current_index`: 0 → … → `size` via `get_current_and_move_forward` (never exceeds `size`; exhausted calls return the all-clear bitset without advancing); `reset` → 0; `done()` true iff at `size`.
- `Writer`/`Reader` (via `make_write`/`make_read`): on overflow the result is marked failed, position frozen; partial bytes remain in the buffer (C++ leaves the partially-written prefix intact).
- `serialization_result._success` true→false at first failed op; `written_bytes`/`read_bytes` only count successful ops.

---

## 2. RUST IMPLEMENTED LIST

`lib.rs` unless noted.

| item | what it does | loc |
|---|---|---|
| `Seed` alias | `oh_rng::Seed = u64` (C++ `seed_type` = `unsigned long long`) | lib.rs:40-42 |
| `BUF_SIZE` | `2_097_152` (C++ `buf_size`) | lib.rs:46 |
| `STATIC_BUF` / `STATIC_COMPRESSION_BUF` | `thread_local! RefCell<Box<[u8]>>` 2 MiB each (C++ `get_static_buf` / `get_static_compression_buf`) | lib.rs:48-58 |
| `INPUT_BITS_MASK` | `0b1111` (C++ 4-bit `input_bitset`) | lib.rs:66 |
| `InputBits(u8)` + `none/new/from_byte/to_u8/left/right/swap/focus` | C++ `input_bitset` semantics: bit 0=left … 3=focus; `from_byte` masks to low 4 bits (`std::bitset<4>` ctor semantics); `to_u8` = `to_ulong()` | lib.rs:68-138 |
| `SerializationResult{written_bytes, success}` + `new/is_success/written_bytes/Default` | C++ `serialization_result` (`operator bool` → `is_success`) | lib.rs:144-180 |
| `DeserializationResult{read_bytes, success}` + `new/is_success/read_bytes/Default` | C++ `deserialization_result` | lib.rs:182-214 |
| `Writer` (`write_bytes/u8/u32/u64/bool/f32/f64/str`) | C++ `make_write`: bounded, host-endian LE `to_le_bytes` (pinned x86_64), overflow → `success=false`, no advance; `write_str` = u32-LE length (truncating `as u32`) + byte loop | lib.rs:222-296 |
| `Reader` (`read_bytes/u8/u32/u64/bool/f32/f64/str`) | C++ `make_read`: bounded, LE; `read_str`: u32-LE length, **up-front bounds check vs remaining buffer** (deviation #2), byte loop into `Vec`, **UTF-8 validation** (deviation #1) | lib.rs:298-422 |
| `ReplayData{inputs}` + `record_input/at/size/serialize/serialize_with_size/deserialize/deserialize_with_size`, `PartialEq/Eq` | C++ `replay_data`; `at` panics (C++ `SSVOH_ASSERT`); `deserialize` up-front-bounds-checks `n_inputs` vs remaining buffer (each input 1 byte) then `resize` (deviation #2); `serialize` writes `u64` count + 1 byte/input | lib.rs:424-517 |
| `ReplayPlayer<'a>` + `new/get_current_and_move_forward/done/reset` | C++ `replay_player`; exhausted → `InputBits::none()` (C++ `return {};`) | lib.rs:524-562 |
| `ReplayFile` (pub fields, C++ order) + `serialize/serialize_with_size/deserialize/deserialize_with_size` | C++ `replay_file` field order = wire order; nested `data.serialize/deserialize` on the advanced slice with the same fail-without-counting semantics | lib.rs:564-715 |
| `ReplayFile::serialize_to_file` | serialize into `STATIC_BUF`; `File::create` (truncating) + `write_all(&buf[..written])` + `flush`; any failure → `false` | lib.rs:719-736 |
| `ReplayFile::deserialize_from_file` | `std::fs::read` (whole file, **unbounded allocation**); Err → `eprintln!("Couldn't open replay path '{}'")` + `false`; `len > BUF_SIZE` → silent `false`; else `deserialize` | lib.rs:739-756 |
| `ReplayFile::serialize_to_packet` | serialize into `STATIC_BUF`; `p.write_u64(written as u64)` (BE) + `p.append(&buf[..written])`; `true` | lib.rs:758-771 |
| `ReplayFile::deserialize_from_packet` | `p.read_u64()` (BE); bounds-check `bytes_to_read` vs `p.available()` (deviation #2 — but note the **inaccurate comment**, see DIFFERENT #2); `p.take(n)` into heap `Vec` (no 2 MiB cap); `deserialize` | lib.rs:773-790 |
| `ReplayFile::create_filename` | `timestamp::to_timepoint(now_timestamp())` + `format_timepoint("%Y%m%d_%H%M%S")` + `concat([U32, Str×, F32, Str, F64, Str])` — mirrors the C++ `concat` ostringstream call 1:1 | lib.rs:796-815 |
| `ReplayFile::played_seconds` | `played_score / 60.0` | lib.rs:817-819 |
| `CompressedReplayFile{data: Vec<u8>}` + `serialize_to_file/deserialize_from_file/serialize_to_packet/deserialize_from_packet`, `PartialEq/Eq` | C++ `compressed_replay_file`; `deserialize_from_file` = `fs::read` (unbounded, matches C++ uncapped resize); open/read Err → `eprintln!("Couldn't open compressed replay path '{}'")`; packet form u64-BE length + raw, with up-front `available()` check (deviation #2) | lib.rs:824-899 |
| `compress_replay_file` (compress.rs) | serialize into `STATIC_BUF`; flate2 `Compress::new(Compression::best(), zlib=true)` + `FlushCompress::Finish` into `STATIC_COMPRESSION_BUF` (2 MiB out cap = C++ `in_out_dest_len`); `Status::StreamEnd` → `Some`; else `eprintln!("Failed compression of replay file, error code: '{Status:?}'")` + `None` | compress.rs:21-49 |
| `decompress_replay_file` (compress.rs) | flate2 `Decompress::new(zlib=true)` + `FlushDecompress::Finish` into `STATIC_BUF` (2 MiB out cap); `StreamEnd` → `ReplayFile::default()` + `deserialize`; fail → same (C++-identical, copy-paste) message + `None` | compress.rs:54-95 |
| `Packet` (packet.rs) | SFML `Packet` wire codec: u64/u32/u16 BE, u8/bool 1 byte, f32/f64 raw LE, string u32-BE length + raw, `append` raw; `read_*` → `Option`, failed reads leave position unchanged (SFML inRange semantics); plus `reset/take/size/data/available` helpers | packet.rs:36-330 |
| Oracle tests | `crates/oh-replay/tests/oracle.rs` pins `HEX` (84 B), `CZHEX` (76 B), `PHEX` (92 B), `PLAYED_SECONDS=60`, the SFML packet table (BE ints, LE floats, u32-BE strings), `create_filename` shape, player walk/exhaust, hostile-input clean-fail | oracle.rs (whole file) |

**Rust extras not in C++** (all benign, no behavior change): `SerializationResult::new`/`Default`,
`InputBits::none`/bit accessors, `ReplayData`/`ReplayFile` `Default`, `CompressedReplayFile`
`PartialEq` (C++ has no `operator==` on it), `Packet::reset/take/size/data/available`,
`i8/i16/i32/i64` Packet variants (SFML surface, not used by the replay paths).

**Explicit documented deviations (markers quoted):** lib.rs:16-27 module doc — *"Two deliberate
port deviations (hostile-input failure mode only; the byte format on well-formed input is
identical to C++): 1. … Rust `String` is UTF-8, so deserializing a non-UTF-8 `player_name`/
`pack_id`/`level_id` (or `Packet::read_str`) marks the result failed… 2. C++ `resize`s to
attacker-controlled lengths before reading … the port bounds-checks against the remaining
buffer up front and fails cleanly."* Re-quoted at lib.rs:391-392 (`read_str`), lib.rs:398-401,
lib.rs:491-494 (`ReplayData::deserialize`), lib.rs:778-779, lib.rs:880-882.

---

## 3. DIFF (adversarial)

Every C++ behavior was checked against the Rust side; byte-level format, compression and
packet codec are **present and oracle-pinned** (HEX/CZHEX/PHEX + packet table in
`crates/oh-replay/tests/oracle.rs`). Findings:

## MISSING

| item | C++ evidence (file:line) | absence proof (command + result) | risk |
|---|---|---|---|
| **Stderr diagnostic on every failed [de]serialization op**: `SSVOH_TRY` calls `printTryFailure`, which prints `Failed [de]serialization operation '<expr>'\n` to `std::cerr` before returning the failed result. Fires in `replay_data::serialize/deserialize` (ops at cpp:130, 135, 148, 159) and `replay_file::serialize/deserialize` (ops at cpp:246-248, 262-266, 302-304, 318-322, plus the `write_str`/`read_str` inner ops cpp:236/240/280/291), i.e. also inside `serialize_to_file`, `deserialize_from_packet`, `compress_replay_file` (serialize leg) and `decompress_replay_file` (deserialize leg). Rust's `Writer`/`Reader`/`serialize`/`deserialize` mark `success=false` and return **silently** — the failure is observable only via the return value. | `src/SSVOpenHexagon/Core/Replay.cpp:25-28` (`printTryFailure`), `:30-41` (`SSVOH_TRY` printing at `:37`), call sites `:130,135,148,159,236,240,246-248,262-266,280,291,302-304,318-322`; consumed by `:340` (`serialize_to_file`), `:511` (`compress_replay_file`), `:555` (`decompress_replay_file`) | `grep -rn "Failed \[de\]serialization\|printTryFailure" crates/oh-replay/` → only 3 **doc-comment** mentions of `SSVOH_TRY` (lib.rs:223, 299, 408); no printing code. `grep -rn "eprintln\|print!" crates/oh-replay/src/` → exactly 4 prints: compress.rs:50, compress.rs:87 (compression failure, C++-matched), lib.rs:744 + lib.rs:856 (open-failure messages, C++-matched). Nothing prints the per-op failure message. | internal |

No other C++ behavior lacks a Rust counterpart: every function, field, overload, buffer
constant, thread-local, file open/write/read, packet op, compression op, error message
listed in §1 has a behaviorally equivalent Rust implementation (§2).

## SEAMS

Accepted deviations with their declaring markers (hostile-input failure modes only;
well-formed byte format identical):

1. **UTF-8 validation of `player_name`/`pack_id`/`level_id` (and `Packet::read_str`).**
   C++ `std::string` holds raw bytes (hpp:117,120,121; read at cpp:280-291); Rust `String`
   is UTF-8, so a non-UTF-8 string makes `read_str` fail the whole deserialization
   (`Reader::read_str` `std::str::from_utf8` → `success=false`, lib.rs:411-414;
   `Packet::read_str` `String::from_utf8(...).ok()`, packet.rs:324-329).
   *Marker:* lib.rs:19-23 (module doc deviation #1), re-quoted lib.rs:391-392 and
   packet.rs doc. *Residual note:* a C++-produced `.ohr` with a non-UTF-8
   `player_name`/`pack_id`/`level_id` is readable by C++ but rejected by Rust — a
   data-format edge for legacy files, accepted by design.
2. **Up-front bounds checks instead of attacker-sized `resize`.** C++
   `_inputs.resize(n_inputs)` (cpp:150), `s.resize(s_size)` (cpp:282),
   `_data.resize(bytes_to_read)` (cpp:461, 487) allocate up to hostile values
   (bad_alloc → terminate); Rust pre-checks against the remaining buffer/packet and
   fails cleanly (`ReplayData::deserialize` lib.rs:495-499; `Reader::read_str`
   lib.rs:398-407; `ReplayFile::deserialize_from_packet` lib.rs:779-783;
   `CompressedReplayFile::deserialize_from_packet` lib.rs:880-885).
   *Marker:* lib.rs:24-27 (module doc deviation #2) + the four site comments.

## DIFFERENT

Present, but renamed or behaviorally different (none affect well-formed input):

1. **`replay_file::seed_type` alias** (hpp:114) has no Rust counterpart on `ReplayFile`;
   the type survives as the crate-level `pub type Seed = oh_rng::Seed` (= `u64`,
   lib.rs:40-42; `oh-rng/src/lib.rs:18-20`). Renamed/relocated. *Risk: none.*
2. **`ReplayFile::deserialize_from_packet` buffer handling + an inaccurate comment.**
   C++ (cpp:408-416) copies the payload **byte-by-byte into the fixed 2 MiB
   `get_static_buf()` with no bounds check against `bytes_to_read`** → a >2 MiB payload
   with enough data heap-overflows the buffer (UB); a short packet → silent `false`.
   Rust (lib.rs:779-785) reads the payload into an unbounded heap `Vec` and then
   deserializes → a >2 MiB payload *succeeds* in Rust. Both differ only for
   hostile/non-self-produced packets (every C++-produced replay is <2 MiB because the
   producer paths use the same 2 MiB buffer). **Additionally, the Rust comment
   lib.rs:778-779 — "A hostile length makes C++ attempt a huge allocation (terminate)" —
   is wrong for this function**: `replay_file::deserialize_from_packet` allocates nothing;
   the C++ failure mode is a fixed-buffer overflow, not an allocation. (The comment is
   correct for `compressed_replay_file::deserialize_from_packet`, which does
   `_data.resize`, cpp:487.) *Risk: internal.*
3. **`ReplayFile::deserialize_from_file` allocation profile.** C++ (cpp:353-378) reads
   into the 2 MiB static buffer; a >2 MiB file is a short read → failbit → silent
   `false`, with memory bounded at 2 MiB. Rust (lib.rs:739-756) does `std::fs::read`
   first — allocating the **entire** file — and only then returns `false` for
   `len > BUF_SIZE` (lib.rs:749-751). Observable result identical; a hostile multi-GB
   file OOMs Rust where C++ just fails. This is the same deviation-#2 category but is
   not listed in the module-doc's two deviations (only the `resize` cases are); the
   site comment lib.rs:748-749 acknowledges the C++ cap without flagging the
   Rust allocation as a deviation. *Risk: internal.*
4. **Open-failure stderr scope.** C++ prints `Couldn't open replay path` /
   `Couldn't open compressed replay path` **only when the stream fails to open**
   (cpp:359, 453); a mid-read I/O error returns `false` silently. Rust prints the same
   message on **any** `std::fs::read` error, including mid-read (lib.rs:744, 856-861).
   *Risk: none.*
5. **Compression error message payload.** C++ prints the numeric zlib `rc`
   (cpp:526, 552: `error code: '-5'` for `Z_BUF_ERROR`); Rust prints flate2's `Status`
   `Debug` name (compress.rs:50, 87: `error code: 'Status::BufError'`). Both print the
   same sentence skeleton (including the C++ copy-paste quirk of calling
   decompress failure "compression" — deliberately preserved, compress.rs:84-86).
   *Risk: none.*
6. **Endianness pinning.** C++ file form is implicit *host-endian* `memcpy`
   (cpp:43-73); Rust is explicit `to_le_bytes`/`from_le_bytes` (lib.rs:261-279,
   340-388). Identical on the pinned x86_64 targets (oracle HEX pins it); on a
   big-endian host the C++ build would follow the host while Rust stays LE.
   *Risk: none on pinned targets.*
7. **`input_bit` enum type** (hpp:26-34) is not ported as a type; its bit positions
   (0..3) are encoded in `InputBits::new`'s parameter order + `INPUT_BITS_MASK`
   (lib.rs:64-82). Semantics fully present; only the named type is absent
   (callers in `oh-game` use `InputBits::left()/right()/swap()/focus()`).
   *Risk: none.*
8. **`at()` failure mode.** C++ `SSVOH_ASSERT` aborts in debug, is compiled out in
   release (`Assert.hpp:33` → OOB `operator[]` = UB); Rust `self.inputs[index]`
   panics in all builds (lib.rs:442-444). The only caller (`ReplayPlayer`) pre-checks
   the index on both sides, so no observable difference. *Risk: none.*
9. **Default construction of a deserialized `replay_file`.** C++ `replay_file result;`
   (cpp:553) leaves scalars uninitialized (written only if `deserialize` succeeds);
   Rust `ReplayFile::default()` zero-fills (compress.rs:77-79). Unobservable on every
   success path; the C++ form is UB if fields were read after a failure. *Risk: none.*
10. **Bool byte read.** C++ `read(_first_play)` memcpy's an arbitrary byte into a `bool`
    (strictly UB; in practice nonzero → `true`, cpp:320); Rust `read_bool` is `b != 0`
    (lib.rs:360-368). Observably identical. *Risk: none.*

## VERIFICATION ASSETS

- `crates/oh-replay/tests/oracle.rs` pins the C++ oracle bytes exactly: `HEX`
  (84 B file form), `CZHEX` (76 B zlib level-9), `PHEX` (92 B packet form: u64-BE
  `0000000000000054` prefix + LE body), `PLAYED_SECONDS=60`, the SFML packet encoding
  table (BE integers, raw-LE floats, u32-BE strings), `create_filename` shape
  (`5_YYYYMMDD_HHMMSS_tester_cube_1_apeirogon_1.6x_60s.ohr`), and clean-fail on
  hostile `n_inputs`/string-length/packet-length.
- flate2 is pinned to the system-zlib backend
  (`Cargo.toml:41`: `default-features = false, features = ["zlib"]`, DECISIONS D-006),
  which is what makes `compress2`-equivalent output byte-identical (CZHEX pin).
- `oh-math` `timestamp` (put_time/gmtime UTC subset, incl. the C++ `i64`
  seconds→ns wrap) and `concat` (glibc `%g` 6-sig-digit float formatting,
  `concat_mixed=v_n_1.35x60` pin) back `create_filename`.

---

## VERDICT

**PASS for the audit focus (byte-level format, compression, packet codec) — and the
pair passes 1:1 overall except for one internal-risk gap:** the C++
`SSVOH_TRY`/`printTryFailure` stderr diagnostic (`Failed [de]serialization
operation '…'`, Replay.cpp:25-41, fired from 15 call sites) has no Rust
counterpart — Rust failures are silent. All well-formed behavior, including every
byte of the `.ohr` file form, the zlib stream, and the SFML packet form, is present
and oracle-pinned.
