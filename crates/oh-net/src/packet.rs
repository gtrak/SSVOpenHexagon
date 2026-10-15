//! The `sf::Packet`-compatible wire codec.
//!
//! **Contract note (D-014):** the issue-016 contract listed
//! `crates/oh-net/src/packet.rs` as a new type. The `sf::Packet` analogue was
//! already ported — byte-exact against the same pinned fork source and the
//! same oracle vectors — as [`oh_replay::Packet`] in issue 02, and issue 016
//! depends on `oh-replay` regardless (the `CTSPReplay` /
//! `CTSPCompressedReplay` fields are the C++ `replay_file` /
//! `compressed_replay_file` whose `serialize_to_packet` /
//! `deserialize_from_packet` go through this codec). Re-exporting the one
//! verified type instead of duplicating the codec keeps the encoding rule
//! pinned in a single place.
//!
//! ## Encoding (re-derived from the fork source)
//!
//! Pinned fork `vittorioromeo/SFML@dacceddbd1adee49c7bfd120266e200800550394`,
//! `src/SFML/Network/Packet.cpp` (the CMakeLists.txt CPM fetch at
//! `CMakeLists.txt:105-114`):
//!
//! | C++ type | bytes |
//! |---|---|
//! | `bool` | 1 (0/1; any nonzero reads as `true`) |
//! | `int8`/`uint8` | 1 raw |
//! | `int16`/`uint16` | 2 **big-endian** (`htons`) |
//! | `int32`/`uint32` | 4 **big-endian** (`htonl`) |
//! | `int64`/`uint64` | 8 **big-endian** (manual byte shifts) |
//! | `float`/`double` | raw `memcpy` — **host-endian** (LE on the pinned x86 targets); NOT byte-swapped |
//! | `std::string`/`const char*` | u32 **big-endian** length, then raw bytes |
//! | `std::wstring`/`sf::String` | u32 BE length, then 4-byte BE per `char32_t` (not used by SSVOpenHexagon's protocol) |
//! | `append(data, n)` | raw bytes |
//!
//! Read failure semantics (fork `Packet::checkSize`): a read past the end
//! marks the packet invalid (`operator bool()` → false) and leaves the
//! value unchanged; every subsequent read also fails. [`oh_replay::Packet`]
//! models that as `Option` returns — `None` on out-of-range data, with the
//! read position left at the point of failure. In this protocol's decode
//! path the two models are equivalent: every decode returns an error
//! immediately on the first failed read, so no decode ever reads after a
//! failed read.
//!
//! The rules above are pinned by the oracle-vectors corpus
//! (`.agents/plans/001-rust-port/mapping/oracle-vectors.md`,
//! "SFML `Packet` encoding rules") and by the byte-exact unit tests below;
//! the mapping doc for this lane is
//! `.agents/plans/001-rust-port/mapping/networking.md` and the skill doc is
//! `.agents/skills/lib-sfml-packet.md`.

/// `sf::Packet` analogue — the SFML wire codec (byte-exact port, issue 02).
pub use oh_replay::Packet;

#[cfg(test)]
mod tests {
    //! Byte-exact pins for the encoding rules in the module docs, exercised
    //! through `oh_net::packet` so this crate pins the surface it ships.

    use super::Packet;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Every integer width, the bool, and the string prefix — one write of
    /// each, in order, must produce exactly this stream.
    #[test]
    fn integer_and_string_encodings_are_big_endian_except_bool() {
        let mut p = Packet::new();
        p.write_bool(true);
        p.write_u8(0xAB);
        p.write_i8(-1);
        p.write_u16(0x1234);
        p.write_i16(-2); // 0xFFFE
        p.write_u32(0xDEAD_BEEF);
        p.write_i32(-1); // 0xFFFFFFFF
        p.write_u64(0x0102_0304_0506_0708);
        p.write_str("ab");

        // Concatenated from the labeled parts (no hand-rolled full hex):
        let expected = [
            "01",               // bool true
            "ab",               // u8 0xAB
            "ff",               // i8 -1
            "1234",             // u16 0x1234
            "fffe",             // i16 -2 (two's complement 0xFFFE)
            "deadbeef",         // u32 0xDEADBEEF
            "ffffffff",         // i32 -1
            "0102030405060708", // u64
            "00000002",         // string length (u32 BE)
            "6162",             // "ab"
        ]
        .concat();
        assert_eq!(hex(p.data()), expected);
    }

    #[test]
    fn float_and_double_are_raw_host_bytes() {
        // The fork writes raw `memcpy` (host-endian): on the pinned x86
        // targets the wire bytes are the LITTLE-endian bit pattern.
        let mut p = Packet::new();
        p.write_f32(1.5f32); // bits 0x3FC00000
        p.write_f64(-2.5f64); // bits 0xC004000000000000
        assert_eq!(hex(p.data()), "0000c03f00000000000004c0");
    }

    #[test]
    fn empty_string_is_length_only() {
        let mut p = Packet::new();
        p.write_str("");
        assert_eq!(hex(p.data()), "00000000");
    }

    #[test]
    fn append_is_raw() {
        let mut p = Packet::new();
        p.append(&[0x00, 0xFF, 0x7F]);
        assert_eq!(hex(p.data()), "00ff7f");
    }

    #[test]
    fn reads_round_trip_and_match_write_widths() {
        let mut p = Packet::new();
        p.write_bool(false);
        p.write_u8(7);
        p.write_u16(0xBEEF);
        p.write_i32(-65_537); // 0xFFFF0001
        p.write_u64(1 << 40);
        p.write_f64(0.5);
        p.write_str("x");

        let mut q = Packet::new();
        q.append(p.data());
        assert_eq!(q.read_bool(), Some(false));
        assert_eq!(q.read_u8(), Some(7));
        assert_eq!(q.read_u16(), Some(0xBEEF));
        assert_eq!(q.read_i32(), Some(-65_537));
        assert_eq!(q.read_u64(), Some(1 << 40));
        assert_eq!(q.read_f64(), Some(0.5));
        assert_eq!(q.read_str(), Some("x".to_string()));
        assert_eq!(q.available(), 0);
    }

    /// A failed read leaves the read position unchanged (fork semantics);
    /// a read that still fits then succeeds. The C++ packet additionally
    /// poisons every later read (`m_isValid`); the decode path never relies
    /// on that (it returns on the first failed read — see module docs).
    #[test]
    fn failed_read_leaves_position_unchanged() {
        let mut p = Packet::new();
        p.write_u16(1); // 2 bytes only
        assert_eq!(p.read_u32(), None); // needs 4, has 2
        assert_eq!(p.available(), 2); // position unchanged
        assert_eq!(p.read_u16(), Some(1)); // a fitting read still works
        assert_eq!(p.read_u8(), None); // now empty
        assert_eq!(p.read_u8(), None);
    }

    /// Any nonzero byte reads as `true` (fork: `value != 0`).
    #[test]
    fn bool_reads_accept_any_nonzero_byte() {
        let mut p = Packet::new();
        p.append(&[0x2A]);
        assert_eq!(p.read_bool(), Some(true));
        let mut q = Packet::new();
        q.append(&[0]);
        assert_eq!(q.read_bool(), Some(false));
    }

    /// `String` is UTF-8 in Rust while `std::string` holds raw bytes: a
    /// non-UTF-8 payload is `None` (documented deviation, same as
    /// oh-replay's `read_str`).
    #[test]
    fn string_read_rejects_non_utf8() {
        let mut p = Packet::new();
        p.write_u32(2);
        p.append(&[0xFF, 0xFE]);
        assert_eq!(p.read_str(), None);
    }
}
