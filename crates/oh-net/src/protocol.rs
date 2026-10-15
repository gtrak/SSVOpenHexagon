//! The protocol/game version surface: `Global/ProtocolVersion.hpp` and the
//! `GameVersion` half of `Global/Version.hpp` (the half the wire protocol
//! uses — the header's game-version bytes and the `STCPServerStatus` field).

/// C++ `hg::ProtocolVersion` (`Global/ProtocolVersion.hpp:13`):
/// `using ProtocolVersion = std::uint8_t`.
pub type ProtocolVersion = u8;

/// C++ `hg::PROTOCOL_VERSION` (`Global/ProtocolVersion.hpp:15`) — fixed at 0.
/// A received header with any other value is rejected by the decode path
/// (`shared::decode_*_packet`), mirroring
/// `verifyReceivedPacketPreambleAndProtocolVersionAndGameVersion`
/// (`Online/Shared.cpp`).
pub const PROTOCOL_VERSION: ProtocolVersion = 0;

/// C++ `hg::GameVersion` (`Global/Version.hpp`).
///
/// C++ defines `operator==`, `operator!=` and `operator<`; the net surface
/// uses only the equality pair (the client's `serverGameVersion !=
/// GAME_VERSION` check, `HexagonClient.cpp:750`). All three are ported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameVersion {
    /// C++ `major`.
    pub major: i32,
    /// C++ `minor`.
    pub minor: i32,
    /// C++ `micro`.
    pub micro: i32,
}

impl GameVersion {
    /// Designated constructor (C++ aggregate-init).
    #[must_use]
    pub const fn new(major: i32, minor: i32, micro: i32) -> Self {
        Self {
            major,
            minor,
            micro,
        }
    }

    /// C++ `operator<` (`Global/Version.hpp`): lexicographic
    /// `(major, minor, micro)`.
    #[must_use]
    pub const fn is_less_than(self, other: Self) -> bool {
        if self.major != other.major {
            return self.major < other.major;
        }
        if self.minor != other.minor {
            return self.minor < other.minor;
        }
        self.micro < other.micro
    }
}

/// C++ `hg::GAME_VERSION{2, 1, 7}` (`Global/Version.hpp`,
/// `GAME_VERSION_STR = "2.1.7"`).
///
/// The packet header encodes these three values as single bytes
/// (`static_cast<uint8_t>` in `encodeVersion`, `Online/Shared.cpp`), so the
/// game version must stay within `0..=255` per component.
pub const GAME_VERSION: GameVersion = GameVersion::new(2, 1, 7);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_fixed_at_zero() {
        assert_eq!(PROTOCOL_VERSION, 0);
    }

    #[test]
    fn game_version_constants_match_the_cpp_header() {
        assert_eq!(GAME_VERSION, GameVersion::new(2, 1, 7));
        assert!(GAME_VERSION != GameVersion::new(2, 1, 8));
        assert!(GAME_VERSION != GameVersion::new(3, 1, 7));
        assert!(GAME_VERSION != GameVersion::new(2, 2, 7));
    }

    #[test]
    fn game_version_less_than_is_lexicographic() {
        // C++ `operator<`: major, then minor, then micro.
        assert!(GameVersion::new(1, 9, 9).is_less_than(GameVersion::new(2, 0, 0)));
        assert!(GameVersion::new(2, 1, 7).is_less_than(GameVersion::new(2, 1, 8)));
        assert!(GameVersion::new(2, 1, 7).is_less_than(GameVersion::new(2, 2, 0)));
        assert!(!GameVersion::new(2, 1, 7).is_less_than(GameVersion::new(2, 1, 7)));
        assert!(!GameVersion::new(3, 0, 0).is_less_than(GameVersion::new(2, 9, 9)));
    }

    #[test]
    fn header_bytes_are_the_u8_truncation_of_the_version() {
        // C++ `encodeVersion`: `static_cast<uint8_t>` of each component.
        let v = GAME_VERSION;
        let header = [v.major as u8, v.minor as u8, v.micro as u8];
        assert_eq!(header, [2, 1, 7]);
    }
}
