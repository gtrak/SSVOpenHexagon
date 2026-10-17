//! Byte-exact port of SSVOpenHexagon's replay system
//! (`include/SSVOpenHexagon/Core/Replay.hpp` + `src/SSVOpenHexagon/Core/Replay.cpp`).
//!
//! Ports `hg::input_bit`/`input_bitset`, `serialization_result`,
//! `deserialization_result`, `replay_data`, `replay_player`, `replay_file`,
//! `compressed_replay_file` and the file/packet/compression codecs, with the
//! exact field order and encoding pinned by
//! `.agents/plans/001-rust-port/mapping/oracle-vectors.md`:
//!
//! - **File form** (`.ohr`): raw `std::memcpy` per field — host-endian, i.e.
//!   little-endian on the pinned x86_64 targets (explicit `to_le_bytes` here
//!   so the format is deterministic).
//! - **Packet form**: `sf::Packet` wire encoding — integers big-endian,
//!   strings u32-BE length + raw bytes, `append` raw (see [`packet::Packet`]).
//!
//! Two deliberate port deviations (hostile-input failure modes only; the byte
//! format on well-formed input is identical to C++):
//!
//! 1. C++ `std::string` fields hold raw bytes; Rust `String` is UTF-8, so
//!    deserializing a non-UTF-8 `player_name`/`pack_id`/`level_id` (or
//!    [`packet::Packet::read_str`]) marks the result failed instead of
//!    storing invalid bytes.
//! 2. C++ `resize`s to attacker-controlled lengths before reading
//!    (`_inputs.resize(n_inputs)`, `s.resize(s_size)`), which OOMs/terminates
//!    on hostile values; the port bounds-checks against the remaining buffer
//!    up front and fails cleanly (`success = false`).
//!
//! The C++ `SSVOH_TRY` / `printTryFailure` diagnostic (Replay.cpp:25-41) is
//! ported at the same 24 call sites with the same message shape: a failed
//! [de]serialization step prints `Failed [de]serialization operation '<op>'`
//! to stderr (the `op` is the C++'s stringified macro argument, e.g.
//! `write(_version)`, `read_str(_player_name)`) before the result is
//! returned. A nested failure (e.g. inside the `write_str` / `read_str`
//! lambdas) prints at both the inner and the enclosing call site, exactly as
//! the C++ macro nests.

use std::cell::RefCell;
use std::path::Path;

use oh_math::hg::concat::{concat, ConcatPart};
use oh_math::hg::timestamp;

mod compress;
pub mod packet;

pub use compress::{compress_replay_file, decompress_replay_file};
pub use packet::Packet;

/// C++ `replay_file::seed_type` = `random_number_generator_seed_type`
/// (`unsigned long long`) — the `oh-rng` seed alias.
pub type Seed = oh_rng::Seed;

/// C++ `buf_size` — the 2 MiB static (thread-local) buffer cap used by the
/// file/packet/compression paths.
const BUF_SIZE: usize = 2_097_152; // 2097152

thread_local! {
    /// C++ `get_static_buf()` — per-thread 2 MiB scratch buffer (reused by
    /// `serialize_to_file`, `serialize_to_packet`, `deserialize_from_file`,
    /// `compress_replay_file` and `decompress_replay_file`).
    static STATIC_BUF: RefCell<Box<[u8]>> = RefCell::new(vec![0u8; BUF_SIZE].into_boxed_slice());

    /// C++ `get_static_compression_buf()` — per-thread 2 MiB compression
    /// output buffer (used by `compress_replay_file`).
    static STATIC_COMPRESSION_BUF: RefCell<Box<[u8]>> =
        RefCell::new(vec![0u8; BUF_SIZE].into_boxed_slice());
}

// ---------------------------------------------------------------------------
// SSVOH_TRY / printTryFailure — the [de]serialization failure diagnostic
// ---------------------------------------------------------------------------

/// C++ `printTryFailure` (Replay.cpp:25-30) — the line the `SSVOH_TRY` macro
/// prints (to `std::cerr`, here `eprint!`) whenever a [de]serialization step
/// fails. `code` is the C++'s stringified macro argument (`#__VA_ARGS__`) —
/// the failed operation, named exactly as the C++ source spells it.
fn print_try_failure(code: &str) {
    eprint!("{}", try_failure_line(code));
}

/// The `print_try_failure` line (pure, for the shape test) — the C++
/// `::std::cerr << "Failed [de]serialization operation '" << code << "'\n"`.
#[must_use]
fn try_failure_line(code: &str) -> String {
    format!("Failed [de]serialization operation '{code}'\n")
}

// ---------------------------------------------------------------------------
// input_bit / input_bitset
// ---------------------------------------------------------------------------

/// C++ `hg::input_bit` bit positions (`left = 0, right = 1, swap = 2,
/// focus = 3, k_count = 4`).
const INPUT_BITS_MASK: u8 = 0b1111;

/// C++ `hg::input_bitset` (`std::bitset<4>`) — a `u8` holding 4 bits:
/// bit 0 = left, bit 1 = right, bit 2 = swap, bit 3 = focus (the C++
/// `record_input(left, right, swap, focus)` order). Values always use at
/// most the low 4 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputBits(u8);

impl InputBits {
    /// `std::bitset<4>()` — all bits clear.
    pub const fn none() -> Self {
        Self(0)
    }

    /// C++ `record_input`'s bit-set expression
    /// (`ib[left] = left; ib[right] = right; ib[swap] = swap; ib[focus] =
    /// focus;`).
    pub const fn new(left: bool, right: bool, swap: bool, focus: bool) -> Self {
        let mut v = 0u8;
        if left {
            v |= 1 << 0;
        }
        if right {
            v |= 1 << 1;
        }
        if swap {
            v |= 1 << 2;
        }
        if focus {
            v |= 1 << 3;
        }
        Self(v)
    }

    /// C++ `input_bitset{static_cast<unsigned long>(byte)}` — keeps the low
    /// 4 bits, ignores the rest (as `std::bitset<4>` does).
    pub const fn from_byte(byte: u8) -> Self {
        Self(byte & INPUT_BITS_MASK)
    }

    /// C++ `to_ulong()` — the bit pattern as a number (bit 0 = left). The
    /// C++ call site in `replay_data::serialize` casts it to `uint8_t`; the
    /// value only ever uses 4 bits, so nothing is lost.
    #[must_use]
    pub const fn to_u8(self) -> u8 {
        self.0
    }

    /// `bit<0>` — left.
    #[must_use]
    pub const fn left(self) -> bool {
        self.0 & (1 << 0) != 0
    }

    /// `bit<1>` — right.
    #[must_use]
    pub const fn right(self) -> bool {
        self.0 & (1 << 1) != 0
    }

    /// `bit<2>` — swap.
    #[must_use]
    pub const fn swap(self) -> bool {
        self.0 & (1 << 2) != 0
    }

    /// `bit<3>` — focus.
    #[must_use]
    pub const fn focus(self) -> bool {
        self.0 & (1 << 3) != 0
    }
}

// ---------------------------------------------------------------------------
// serialization_result / deserialization_result
// ---------------------------------------------------------------------------

/// C++ `hg::serialization_result` — bytes written + success flag. A failed
/// (partial) write leaves the partially-written bytes in the buffer, exactly
/// like the C++ `make_write` loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerializationResult {
    written_bytes: usize,
    success: bool,
}

impl SerializationResult {
    /// C++ member initializers: `_written_bytes{0}`, `_success{true}`.
    pub const fn new() -> Self {
        Self {
            written_bytes: 0,
            success: true,
        }
    }

    /// C++ `explicit operator bool()`.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// C++ `written_bytes()`.
    #[must_use]
    pub fn written_bytes(&self) -> usize {
        self.written_bytes
    }
}

impl Default for SerializationResult {
    /// C++ default-constructed `serialization_result` (success = true).
    fn default() -> Self {
        Self::new()
    }
}

/// C++ `hg::deserialization_result` — bytes read + success flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeserializationResult {
    read_bytes: usize,
    success: bool,
}

impl DeserializationResult {
    /// C++ member initializers: `_read_bytes{0}`, `_success{true}`.
    pub const fn new() -> Self {
        Self {
            read_bytes: 0,
            success: true,
        }
    }

    /// C++ `explicit operator bool()`.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// C++ `read_bytes()`.
    #[must_use]
    pub fn read_bytes(&self) -> usize {
        self.read_bytes
    }
}

impl Default for DeserializationResult {
    /// C++ default-constructed `deserialization_result` (success = true).
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// make_write / make_read (file-form memcpy codecs, host-endian = LE here)
// ---------------------------------------------------------------------------

/// C++ `make_write` (Replay.cpp) — bounded, host-endian `memcpy` writer over
/// `[pos, buf.len())`. Each `write_*` mirrors one `SSVOH_TRY(write(x))`:
/// on bounds failure the result is marked failed and the byte is *not*
/// written; on success the bytes are copied, `pos` and
/// `result.written_bytes` advance.
struct Writer<'a> {
    result: &'a mut SerializationResult,
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> Writer<'a> {
    fn new(result: &'a mut SerializationResult, buf: &'a mut [u8]) -> Self {
        Self {
            result,
            buf,
            pos: 0,
        }
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> bool {
        if self.pos + bytes.len() > self.buf.len() {
            self.result.success = false;
            return false;
        }
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        self.result.written_bytes += bytes.len();
        true
    }

    fn write_u8(&mut self, v: u8) -> bool {
        self.write_bytes(&[v])
    }

    fn write_u32(&mut self, v: u32) -> bool {
        // `std::memcpy` of the host-endian value (LE on the pinned targets).
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_u64(&mut self, v: u64) -> bool {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_bool(&mut self, v: bool) -> bool {
        self.write_bytes(&[u8::from(v)])
    }

    fn write_f32(&mut self, v: f32) -> bool {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_f64(&mut self, v: f64) -> bool {
        self.write_bytes(&v.to_le_bytes())
    }

    /// C++ `write_str` lambda: `write(uint32(s.size()))` (truncating cast)
    /// then the string one byte at a time. The C++ lambda's two `SSVOH_TRY`
    /// (`write(static_cast<std::uint32_t>(s.size()))`, `write(c)`) print on
    /// failure; mirrored here.
    fn write_str(&mut self, s: &str) -> bool {
        let len = s.len() as u32;
        if !self.write_u32(len) {
            print_try_failure("write(static_cast<std::uint32_t>(s.size()))");
            return false;
        }
        for &c in s.as_bytes() {
            if !self.write_u8(c) {
                print_try_failure("write(c)");
                return false;
            }
        }
        true
    }
}

/// C++ `make_read` (Replay.cpp) — bounded, host-endian `memcpy` reader over
/// `[pos, buf.len())`, mirroring `SSVOH_TRY(read(x))` semantics.
struct Reader<'a> {
    result: &'a mut DeserializationResult,
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(result: &'a mut DeserializationResult, buf: &'a [u8]) -> Self {
        Self {
            result,
            buf,
            pos: 0,
        }
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn read_bytes(&mut self, dst: &mut [u8]) -> bool {
        if self.pos + dst.len() > self.buf.len() {
            self.result.success = false;
            return false;
        }
        dst.copy_from_slice(&self.buf[self.pos..self.pos + dst.len()]);
        self.pos += dst.len();
        self.result.read_bytes += dst.len();
        true
    }

    fn read_u8(&mut self, v: &mut u8) -> bool {
        let mut b = [0u8; 1];
        if self.read_bytes(&mut b) {
            *v = b[0];
            true
        } else {
            false
        }
    }

    fn read_u32(&mut self, v: &mut u32) -> bool {
        let mut b = [0u8; 4];
        if self.read_bytes(&mut b) {
            *v = u32::from_le_bytes(b);
            true
        } else {
            false
        }
    }

    fn read_u64(&mut self, v: &mut u64) -> bool {
        let mut b = [0u8; 8];
        if self.read_bytes(&mut b) {
            *v = u64::from_le_bytes(b);
            true
        } else {
            false
        }
    }

    fn read_bool(&mut self, v: &mut bool) -> bool {
        let mut b: u8 = 0;
        if self.read_u8(&mut b) {
            *v = b != 0;
            true
        } else {
            false
        }
    }

    fn read_f32(&mut self, v: &mut f32) -> bool {
        let mut b = [0u8; 4];
        if self.read_bytes(&mut b) {
            *v = f32::from_le_bytes(b);
            true
        } else {
            false
        }
    }

    fn read_f64(&mut self, v: &mut f64) -> bool {
        let mut b = [0u8; 8];
        if self.read_bytes(&mut b) {
            *v = f64::from_le_bytes(b);
            true
        } else {
            false
        }
    }

    /// C++ `read_str` lambda: `read(uint32 s_size)` then the string one byte
    /// at a time into `s`. The C++ lambda's two `SSVOH_TRY` (`read(s_size)`,
    /// `read(c)`) print on failure; the port-only bounds-check and UTF-8
    /// validation arms (module docs' deviations 1-2) stay silent — they have
    /// no C++ `SSVOH_TRY` site.
    fn read_str(&mut self, s: &mut String) -> bool {
        let mut len: u32 = 0;
        if !self.read_u32(&mut len) {
            print_try_failure("read(s_size)");
            return false;
        }
        // C++: `s.resize(s_size)` before the byte loop. A hostile `len` (up
        // to 2^32-1) makes C++ attempt a ~4 GiB allocation (bad_alloc →
        // terminate); bounds-check the whole string against the remaining
        // buffer instead. Well-formed input reads identically.
        if u128::from(len) > (self.buf.len() - self.pos) as u128 {
            self.result.success = false;
            return false;
        }
        let mut bytes = vec![0u8; len as usize]; // safe: fits in the buffer
        for b in &mut bytes {
            // Mirrors the C++ per-char `SSVOH_TRY(read(c))` loop (cannot
            // fail given the pre-check above).
            if !self.read_u8(b) {
                print_try_failure("read(c)");
                return false;
            }
        }
        match std::str::from_utf8(&bytes) {
            Ok(text) => *s = text.to_string(),
            Err(_) => self.result.success = false,
        }
        true
    }
}

// ---------------------------------------------------------------------------
// replay_data
// ---------------------------------------------------------------------------

/// C++ `hg::replay_data` — the recorded input sequence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayData {
    inputs: Vec<InputBits>,
}

impl ReplayData {
    /// C++ `record_input(left, right, swap, focus)` — appends an input with
    /// those bits set (bit 0 = left … bit 3 = focus).
    pub fn record_input(&mut self, left: bool, right: bool, swap: bool, focus: bool) {
        self.inputs.push(InputBits::new(left, right, swap, focus));
    }

    /// C++ `at(index)` (`SSVOH_ASSERT(index < size())`; indexing past the
    /// end panics, as the C++ assert would abort).
    #[must_use]
    pub fn at(&self, index: usize) -> InputBits {
        self.inputs[index]
    }

    /// C++ `size()`.
    #[must_use]
    pub fn size(&self) -> usize {
        self.inputs.len()
    }

    /// C++ `serialize(buffer, buffer_end)` — `n_inputs` (8 bytes LE,
    /// `std::size_t` on the pinned 64-bit targets; pinned to `u64` for
    /// portability, see 02-replay.md) then one byte per input
    /// (`bitset.to_ulong()`).
    pub fn serialize(&self, buffer: &mut [u8]) -> SerializationResult {
        let mut result = SerializationResult::default();
        let mut w = Writer::new(&mut result, buffer);

        let n_inputs: u64 = self.inputs.len() as u64;
        if !w.write_u64(n_inputs) {
            print_try_failure("write(n_inputs)");
            return result;
        }
        for &ib in &self.inputs {
            if !w.write_u8(ib.to_u8()) {
                print_try_failure("write(ib_byte)");
                return result;
            }
        }
        result
    }

    /// C++ `serialize(buffer, buffer_size)` overload.
    pub fn serialize_with_size(
        &self,
        buffer: &mut [u8],
        buffer_size: usize,
    ) -> SerializationResult {
        debug_assert!(buffer_size <= buffer.len());
        self.serialize(&mut buffer[..buffer_size])
    }

    /// C++ `deserialize(buffer, buffer_end)`.
    pub fn deserialize(&mut self, buffer: &[u8]) -> DeserializationResult {
        let mut result = DeserializationResult::default();
        let mut r = Reader::new(&mut result, buffer);

        let mut n_inputs: u64 = 0;
        if !r.read_u64(&mut n_inputs) {
            print_try_failure("read(n_inputs)");
            return result;
        }
        // C++: `_inputs.resize(n_inputs)` then the per-byte loop. A hostile
        // `n_inputs` (up to 2^64-1) makes C++ attempt a ~16 EiB allocation
        // (bad_alloc → terminate); bounds-check against the remaining buffer
        // instead (each input is exactly 1 byte). Well-formed input reads
        // identically.
        if u128::from(n_inputs) > (buffer.len() - r.pos()) as u128 {
            result.success = false;
            return result;
        }
        self.inputs.resize(n_inputs as usize, InputBits::none()); // safe: <= buffer
        for slot in &mut self.inputs {
            let mut ib_byte: u8 = 0;
            if !r.read_u8(&mut ib_byte) {
                print_try_failure("read(ib_byte)");
                return result;
            }
            *slot = InputBits::from_byte(ib_byte);
        }
        result
    }

    /// C++ `deserialize(buffer, buffer_size)` overload.
    pub fn deserialize_with_size(
        &mut self,
        buffer: &[u8],
        buffer_size: usize,
    ) -> DeserializationResult {
        debug_assert!(buffer_size <= buffer.len());
        self.deserialize(&buffer[..buffer_size])
    }
}

// ---------------------------------------------------------------------------
// replay_player
// ---------------------------------------------------------------------------

/// C++ `hg::replay_player` — a non-owning cursor over a [`ReplayData`].
pub struct ReplayPlayer<'a> {
    replay_data: &'a ReplayData,
    current_index: usize,
}

impl<'a> ReplayPlayer<'a> {
    /// C++ `explicit replay_player(const replay_data& rd)`.
    pub fn new(replay_data: &'a ReplayData) -> Self {
        Self {
            replay_data,
            current_index: 0,
        }
    }

    /// C++ `get_current_and_move_forward()` — the current input, or the
    /// default (all-clear) bitset once the replay is exhausted.
    #[must_use]
    pub fn get_current_and_move_forward(&mut self) -> InputBits {
        if self.replay_data.size() <= self.current_index {
            return InputBits::none();
        }
        let current = self.replay_data.at(self.current_index);
        self.current_index += 1;
        current
    }

    /// C++ `done()`.
    #[must_use]
    pub fn done(&self) -> bool {
        self.current_index == self.replay_data.size()
    }

    /// C++ `reset()`.
    pub fn reset(&mut self) {
        self.current_index = 0;
    }
}

// ---------------------------------------------------------------------------
// replay_file
// ---------------------------------------------------------------------------

/// C++ `hg::replay_file` — the replay record. The field order *is* the wire
/// format (see the layout table in
/// `.agents/plans/001-rust-port/mapping/oracle-vectors.md`):
///
/// ```text
/// version u32 LE, player_name (u32 LE length + raw), seed u64 LE,
/// data (u64 LE input count + one byte per input), pack_id (u32 LE length +
/// raw), level_id (u32 LE length + raw), first_play u8, difficulty_mult
/// f32 LE, played_score f64 LE   — total 84 bytes for the oracle fixture.
/// ```
///
/// String fields are `String` (UTF-8); see the module docs for the UTF-8
/// deviation on deserialization.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplayFile {
    /// C++ `_version` — replay format version.
    pub version: u32,
    /// C++ `_player_name`.
    pub player_name: String,
    /// C++ `_seed` — RNG seed for the session (`Seed` = u64).
    pub seed: Seed,
    /// C++ `_data` — input data.
    pub data: ReplayData,
    /// C++ `_pack_id` — id of the selected pack.
    pub pack_id: String,
    /// C++ `_level_id` — id of the played level.
    pub level_id: String,
    /// C++ `_first_play` — if this was achieved on first level play.
    pub first_play: bool,
    /// C++ `_difficulty_mult` — played difficulty multiplier (f32).
    pub difficulty_mult: f32,
    /// C++ `_played_score` — played score (f64).
    pub played_score: f64,
}

impl ReplayFile {
    /// C++ `serialize(buffer, buffer_end)`.
    pub fn serialize(&self, buffer: &mut [u8]) -> SerializationResult {
        let mut result = SerializationResult::default();
        let mut w = Writer::new(&mut result, buffer);

        if !w.write_u32(self.version) {
            print_try_failure("write(_version)");
            return result;
        }
        if !w.write_str(&self.player_name) {
            print_try_failure("write_str(_player_name)");
            return result;
        }
        if !w.write_u64(self.seed) {
            print_try_failure("write(_seed)");
            return result;
        }

        // C++: `_data.serialize(buffer, buffer_end)` on the *current* (already
        // advanced) position, then continue past its written bytes. (The
        // first writer is dead here; NLL ends its borrows.)
        let pos = w.pos();
        let data_result = self.data.serialize(&mut buffer[pos..]);
        if !data_result.is_success() {
            result.success = false;
            return result;
        }
        result.written_bytes += data_result.written_bytes();

        let mut w = Writer::new(
            &mut result,
            &mut buffer[pos + data_result.written_bytes()..],
        );
        if !w.write_str(&self.pack_id) {
            print_try_failure("write_str(_pack_id)");
            return result;
        }
        if !w.write_str(&self.level_id) {
            print_try_failure("write_str(_level_id)");
            return result;
        }
        if !w.write_bool(self.first_play) {
            print_try_failure("write(_first_play)");
            return result;
        }
        if !w.write_f32(self.difficulty_mult) {
            print_try_failure("write(_difficulty_mult)");
            return result;
        }
        if !w.write_f64(self.played_score) {
            print_try_failure("write(_played_score)");
            return result;
        }
        result
    }

    /// C++ `serialize(buffer, buffer_size)` overload.
    pub fn serialize_with_size(
        &self,
        buffer: &mut [u8],
        buffer_size: usize,
    ) -> SerializationResult {
        debug_assert!(buffer_size <= buffer.len());
        self.serialize(&mut buffer[..buffer_size])
    }

    /// C++ `deserialize(buffer, buffer_end)`.
    pub fn deserialize(&mut self, buffer: &[u8]) -> DeserializationResult {
        let mut result = DeserializationResult::default();
        let mut r = Reader::new(&mut result, buffer);

        if !r.read_u32(&mut self.version) {
            print_try_failure("read(_version)");
            return result;
        }
        if !r.read_str(&mut self.player_name) {
            print_try_failure("read_str(_player_name)");
            return result;
        }
        if !r.read_u64(&mut self.seed) {
            print_try_failure("read(_seed)");
            return result;
        }

        // C++: `_data.deserialize(buffer, buffer_end)` on the current
        // position, then continue past its read bytes.
        let pos = r.pos();
        let data_result = self.data.deserialize(&buffer[pos..]);
        if !data_result.is_success() {
            result.success = false;
            return result;
        }
        result.read_bytes += data_result.read_bytes();

        let mut r = Reader::new(&mut result, &buffer[pos + data_result.read_bytes()..]);
        if !r.read_str(&mut self.pack_id) {
            print_try_failure("read_str(_pack_id)");
            return result;
        }
        if !r.read_str(&mut self.level_id) {
            print_try_failure("read_str(_level_id)");
            return result;
        }
        if !r.read_bool(&mut self.first_play) {
            print_try_failure("read(_first_play)");
            return result;
        }
        if !r.read_f32(&mut self.difficulty_mult) {
            print_try_failure("read(_difficulty_mult)");
            return result;
        }
        if !r.read_f64(&mut self.played_score) {
            print_try_failure("read(_played_score)");
            return result;
        }
        result
    }

    /// C++ `deserialize(buffer, buffer_size)` overload.
    pub fn deserialize_with_size(
        &mut self,
        buffer: &[u8],
        buffer_size: usize,
    ) -> DeserializationResult {
        debug_assert!(buffer_size <= buffer.len());
        self.deserialize(&buffer[..buffer_size])
    }

    /// C++ `serialize_to_file(p)` — serialize into the 2 MiB thread-local
    /// buffer, then write the serialized bytes to the path (binary).
    pub fn serialize_to_file(&self, p: impl AsRef<Path>) -> bool {
        use std::io::Write as _;
        STATIC_BUF.with(|buf_cell| {
            let buf = &mut buf_cell.borrow_mut()[..];
            let sr = self.serialize(buf);
            if !sr.is_success() {
                return false;
            }
            match std::fs::File::create(p.as_ref()) {
                Ok(mut f) => f
                    .write_all(&buf[..sr.written_bytes()])
                    .and_then(|_| f.flush())
                    .is_ok(),
                Err(_) => false,
            }
        })
    }

    /// C++ `deserialize_from_file(p)` — read the whole file (up to the 2 MiB
    /// cap) and deserialize it.
    pub fn deserialize_from_file(&mut self, p: impl AsRef<Path>) -> bool {
        let bytes = match std::fs::read(p.as_ref()) {
            Ok(b) => b,
            Err(_) => {
                // C++: `std::cerr << "Couldn't open replay path '" << p << "'\n"`.
                eprintln!("Couldn't open replay path '{}'", p.as_ref().display());
                return false;
            }
        };
        // C++ reads the file into the 2 MiB static buffer; a larger file is a
        // short read → stream failbit → false.
        if bytes.len() > BUF_SIZE {
            return false;
        }
        self.deserialize(&bytes[..]).is_success()
    }

    /// C++ `serialize_to_packet(p)` — `p << (uint64)written_bytes` (big-endian
    /// u64 length prefix) then `p.append(buf, written_bytes)`.
    pub fn serialize_to_packet(&self, p: &mut Packet) -> bool {
        STATIC_BUF.with(|buf_cell| {
            let buf = &mut buf_cell.borrow_mut()[..];
            let sr = self.serialize(buf);
            if !sr.is_success() {
                return false;
            }
            p.write_u64(sr.written_bytes() as u64);
            p.append(&buf[..sr.written_bytes()]);
            true
        })
    }

    /// C++ `deserialize_from_packet(p)` — `p >> bytes_to_read` (big-endian
    /// u64) then that many raw bytes, deserialized as the file form.
    pub fn deserialize_from_packet(&mut self, p: &mut Packet) -> bool {
        let bytes_to_read = match p.read_u64() {
            Some(n) => n,
            None => return false,
        };
        // C++: read into the static buffer one byte at a time. A hostile
        // length makes C++ attempt a huge allocation (terminate);
        // bounds-check against the packet's remaining bytes instead.
        if u128::from(bytes_to_read) > p.available() as u128 {
            return false;
        }
        let bytes = p
            .take(bytes_to_read as usize) // safe: checked against `available()`
            .expect("checked above");
        self.deserialize(bytes).is_success()
    }

    /// C++ `create_filename()` —
    /// `{version}_{%Y%m%d_%H%M%S UTC}_{player_name}_{level_id}_{difficulty
    /// mult}x_{played seconds}s.ohr`, numbers formatted exactly like C++
    /// `std::ostream <<` (via `oh-math`'s `concat`/`timestamp` ports, pinned
    /// by issue-001 oracle tests).
    #[must_use]
    pub fn create_filename(&self) -> String {
        let tp = timestamp::to_timepoint(timestamp::now_timestamp());
        let tp_str = timestamp::format_timepoint(tp, "%Y%m%d_%H%M%S");
        concat(&[
            ConcatPart::U32(self.version),
            ConcatPart::Str("_"),
            ConcatPart::Str(&tp_str),
            ConcatPart::Str("_"),
            ConcatPart::Str(&self.player_name),
            ConcatPart::Str("_"),
            ConcatPart::Str(&self.level_id),
            ConcatPart::Str("_"),
            ConcatPart::F32(self.difficulty_mult),
            ConcatPart::Str("x_"),
            ConcatPart::F64(self.played_seconds()),
            ConcatPart::Str("s.ohr"),
        ])
    }

    /// C++ `played_seconds()` — `_played_score / 60.0`.
    #[must_use]
    pub fn played_seconds(&self) -> f64 {
        self.played_score / 60.0
    }
}

// ---------------------------------------------------------------------------
// compressed_replay_file
// ---------------------------------------------------------------------------

/// C++ `hg::compressed_replay_file` — raw zlib-compressed replay bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompressedReplayFile {
    /// C++ `_data` — the compressed (zlib) bytes.
    pub data: Vec<u8>,
}

impl CompressedReplayFile {
    /// C++ `serialize_to_file(p)` — write the compressed bytes to the path
    /// (binary).
    pub fn serialize_to_file(&self, p: impl AsRef<Path>) -> bool {
        use std::io::Write as _;
        match std::fs::File::create(p.as_ref()) {
            Ok(mut f) => f.write_all(&self.data).and_then(|_| f.flush()).is_ok(),
            Err(_) => false,
        }
    }

    /// C++ `deserialize_from_file(p)` — read the whole file into `_data`.
    /// (Unlike `replay_file::deserialize_from_file`, there is no size cap
    /// here; the 2 MiB cap applies to the decompression destination.)
    pub fn deserialize_from_file(&mut self, p: impl AsRef<Path>) -> bool {
        match std::fs::read(p.as_ref()) {
            Ok(bytes) => {
                self.data = bytes;
                true
            }
            Err(_) => {
                // C++: `std::cerr << "Couldn't open compressed replay path '"
                //      << p << "'\n"`.
                eprintln!(
                    "Couldn't open compressed replay path '{}'",
                    p.as_ref().display()
                );
                false
            }
        }
    }

    /// C++ `serialize_to_packet(p)` — `p << (uint64)_data.size()` (big-endian)
    /// then `p.append(_data.data(), size)`.
    pub fn serialize_to_packet(&self, p: &mut Packet) -> bool {
        p.write_u64(self.data.len() as u64);
        p.append(&self.data);
        true
    }

    /// C++ `deserialize_from_packet(p)` — `p >> bytes_to_read` (big-endian
    /// u64) then that many raw bytes into `_data`.
    pub fn deserialize_from_packet(&mut self, p: &mut Packet) -> bool {
        let bytes_to_read = match p.read_u64() {
            Some(n) => n,
            None => return false,
        };
        // C++: `_data.resize(bytes_to_read)` then per-byte reads. A hostile
        // length makes C++ attempt a huge allocation (terminate);
        // bounds-check against the packet's remaining bytes instead.
        if u128::from(bytes_to_read) > p.available() as u128 {
            return false;
        }
        match p.take(bytes_to_read as usize) {
            Some(bytes) => {
                self.data = bytes.to_vec();
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `SSVOH_TRY` / `printTryFailure` line is byte-exact with the C++
    /// (`Replay.cpp:25-30`): `Failed [de]serialization operation '<code>'\n`
    /// where `<code>` is the C++'s stringified macro argument. Pinned for the
    /// top-level field ops and the nested `write_str` / `read_str` lambda ops.
    #[test]
    fn try_failure_line_matches_the_cpp_shape() {
        for code in [
            "write(_version)",
            "write_str(_player_name)",
            "read(_seed)",
            "read_str(_level_id)",
            "write(n_inputs)",
            "read(ib_byte)",
            // the nested lambda-internal ops (the C++ stringified forms).
            "write(static_cast<std::uint32_t>(s.size()))",
            "write(c)",
            "read(s_size)",
            "read(c)",
        ] {
            assert_eq!(
                try_failure_line(code),
                format!("Failed [de]serialization operation '{code}'\n")
            );
        }
    }

    /// A too-short buffer fails at the FIRST `SSVOH_TRY` site
    /// (`read(_version)`) — the result is failed and nothing was read (the
    /// diagnostic's path, pinned at the result level; the stderr line itself
    /// is the thin `eprint!` wrapper over [`try_failure_line`]).
    #[test]
    fn too_short_buffer_fails_at_the_first_read_site() {
        let mut rf = ReplayFile::default();
        let dr = rf.deserialize(&[0u8; 3]); // < 4 bytes: `read(_version)` fails
        assert!(!dr.is_success());
        assert_eq!(dr.read_bytes(), 0);
    }
}
