//! Minimal `sf::Packet` wire codec (the SFML `Packet` analogue used by
//! `replay_file::serialize_to_packet` / `deserialize_from_packet`).
//!
//! Encoding is the pinned fork
//! `vittorioromeo/SFML@dacceddbd1adee49c7bfd120266e200800550394`,
//! `src/SFML/Network/Packet.cpp` — the table in
//! `.agents/plans/001-rust-port/mapping/oracle-vectors.md`
//! ("SFML `Packet` encoding rules"):
//!
//! | type              | bytes                                             |
//! |---                |---                                                |
//! | `bool`            | 1 (0/1)                                           |
//! | `int8`/`uint8`    | 1 raw                                             |
//! | `int16`/`uint16`  | 2 **big-endian** (`htons`)                        |
//! | `int32`/`uint32`  | 4 **big-endian** (`htonl`)                        |
//! | `int64`/`uint64`  | 8 **big-endian** (manual byte shifts)             |
//! | `float`/`double`  | raw `memcpy` — host-endian (LE on the pinned x86  |
//! |                   | targets); explicit `to_le_bytes` for determinism  |
//! | `String`          | u32 **big-endian** length, then raw bytes         |
//! | `append(data, n)` | raw bytes                                         |
//!
//! C++ `sf::Packet&` maps to `&mut Packet`; `p << v` maps to `write_*`,
//! `p >> v` to `read_*` (`Option`, `None` on out-of-range data — SFML sets a
//! read-failure flag and leaves the value unchanged; reads never advance past
//! the failed point).
//!
//! Note the trap this codec exists to encode: the replay *file* form is
//! raw little-endian `memcpy`, while the *packet* form goes through this
//! big-endian codec (`HEX` starts `05000000`, `PHEX` starts
//! `0000000000000054`).

/// `sf::Packet` analogue: a byte buffer with the SFML wire encodings above.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Packet {
    data: Vec<u8>,
    read_pos: usize,
    write_pos: usize,
}

impl Packet {
    /// C++ `sf::Packet()` default ctor.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// C++ `size()` — total bytes in the packet.
    #[must_use]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// C++ `data()` — the raw byte buffer.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Bytes left to read (C++ `inRange` arithmetic).
    #[must_use]
    pub fn available(&self) -> usize {
        self.data.len() - self.read_pos
    }

    /// C++ `reset()` — clear the contents and both positions.
    pub fn reset(&mut self) {
        self.data.clear();
        self.read_pos = 0;
        self.write_pos = 0;
    }

    /// C++ `append(data, n)` — raw bytes copied at the write position
    /// (resizes the buffer if the write position is past the end).
    pub fn append(&mut self, bytes: &[u8]) {
        let start = self.write_pos;
        if start + bytes.len() > self.data.len() {
            self.data.resize(start + bytes.len(), 0);
        }
        self.data[start..start + bytes.len()].copy_from_slice(bytes);
        self.write_pos += bytes.len();
    }

    /// C++ internal `readData` — read `n` raw bytes, advancing the read
    /// position; `None` (position unchanged) if fewer than `n` remain.
    #[must_use]
    pub fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.read_pos + n > self.data.len() {
            return None;
        }
        let out = &self.data[self.read_pos..self.read_pos + n];
        self.read_pos += n;
        Some(out)
    }

    // --- writes (`p << v`) ---

    /// C++ `p << bool` — 1 byte, 0/1.
    pub fn write_bool(&mut self, v: bool) {
        self.append(&[u8::from(v)]);
    }

    /// C++ `p << int8` — 1 raw byte (two's-complement bits).
    pub fn write_i8(&mut self, v: i8) {
        self.append(&[v as u8]);
    }

    /// C++ `p << uint8` — 1 raw byte.
    pub fn write_u8(&mut self, v: u8) {
        self.append(&[v]);
    }

    /// C++ `p << int16` — 2 bytes big-endian (the value's bit pattern).
    pub fn write_i16(&mut self, v: i16) {
        self.write_u16(v as u16);
    }

    /// C++ `p << uint16` — 2 bytes big-endian (`htons`).
    pub fn write_u16(&mut self, v: u16) {
        self.append(&v.to_be_bytes());
    }

    /// C++ `p << int32` — 4 bytes big-endian (the value's bit pattern).
    pub fn write_i32(&mut self, v: i32) {
        self.write_u32(v as u32);
    }

    /// C++ `p << uint32` — 4 bytes big-endian (`htonl`).
    pub fn write_u32(&mut self, v: u32) {
        self.append(&v.to_be_bytes());
    }

    /// C++ `p << int64` — 8 bytes big-endian (the value's bit pattern).
    pub fn write_i64(&mut self, v: i64) {
        self.write_u64(v as u64);
    }

    /// C++ `p << uint64` — 8 bytes big-endian (manual byte shifts).
    pub fn write_u64(&mut self, v: u64) {
        self.append(&v.to_be_bytes());
    }

    /// C++ `p << float` — raw `memcpy` (host-endian; the pinned x86 targets
    /// are little-endian, so `to_le_bytes` keeps the format deterministic).
    pub fn write_f32(&mut self, v: f32) {
        self.append(&v.to_le_bytes());
    }

    /// C++ `p << double` — raw `memcpy` (host-endian; `to_le_bytes` on the
    /// pinned x86 targets).
    pub fn write_f64(&mut self, v: f64) {
        self.append(&v.to_le_bytes());
    }

    /// C++ `p << std::string` — u32 big-endian length, then raw bytes. The
    /// C++ `static_cast<uint32_t>(size)` truncates; mirror that.
    pub fn write_str(&mut self, s: &str) {
        self.write_u32(s.len() as u32);
        if !s.is_empty() {
            self.append(s.as_bytes());
        }
    }

    // --- reads (`p >> v`) ---

    /// C++ `p >> bool` — 1 byte; any nonzero value is `true`.
    #[must_use]
    pub fn read_bool(&mut self) -> Option<bool> {
        self.take(1).map(|b| b[0] != 0)
    }

    /// C++ `p >> int8` — 1 raw byte (two's-complement bits).
    #[must_use]
    pub fn read_i8(&mut self) -> Option<i8> {
        self.take(1).map(|b| b[0] as i8)
    }

    /// C++ `p >> uint8` — 1 raw byte.
    #[must_use]
    pub fn read_u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    /// C++ `p >> int16` — 2 bytes big-endian.
    #[must_use]
    pub fn read_i16(&mut self) -> Option<i16> {
        self.read_u16().map(|v| v as i16)
    }

    /// C++ `p >> uint16` — 2 bytes big-endian.
    #[must_use]
    pub fn read_u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    /// C++ `p >> int32` — 4 bytes big-endian.
    #[must_use]
    pub fn read_i32(&mut self) -> Option<i32> {
        self.read_u32().map(|v| v as i32)
    }

    /// C++ `p >> uint32` — 4 bytes big-endian.
    #[must_use]
    pub fn read_u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// C++ `p >> int64` — 8 bytes big-endian.
    #[must_use]
    pub fn read_i64(&mut self) -> Option<i64> {
        self.read_u64().map(|v| v as i64)
    }

    /// C++ `p >> uint64` — 8 bytes big-endian.
    #[must_use]
    pub fn read_u64(&mut self) -> Option<u64> {
        self.take(8)
            .map(|b| u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    /// C++ `p >> float` — raw `memcpy` (little-endian on the pinned x86
    /// targets).
    #[must_use]
    pub fn read_f32(&mut self) -> Option<f32> {
        self.take(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// C++ `p >> double` — raw `memcpy` (little-endian on the pinned x86
    /// targets).
    #[must_use]
    pub fn read_f64(&mut self) -> Option<f64> {
        self.take(8)
            .map(|b| f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    /// C++ `p >> std::string` — u32 big-endian length, then raw bytes.
    /// `None` if the length or the bytes are out of range, or if the bytes
    /// are not valid UTF-8 (C++ `std::string` holds raw bytes; Rust `String`
    /// is UTF-8 — see the module docs of [`crate::ReplayFile`]).
    #[must_use]
    pub fn read_str(&mut self) -> Option<String> {
        let len = self.read_u32()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).ok()
    }
}
