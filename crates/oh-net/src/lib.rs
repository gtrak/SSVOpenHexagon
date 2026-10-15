//! `oh-net` — the SSVOpenHexagon wire protocol (the `Online/Shared.*` +
//! `Online/Sodium.*` port).
//!
//! This crate is the compatibility surface shared by the client (issue 17)
//! and the server (issue 18): the `sf::Packet`-compatible wire codec, every
//! `CTSP*`/`STCP*`/`PInvalid`/`PEncryptedMsg` packet struct, the
//! `make*`/`decode*` functions from `src/SSVOpenHexagon/Online/Shared.cpp`,
//! the libsodium layer from `src/SSVOpenHexagon/Online/Sodium.cpp`, and the
//! `Global/ProtocolVersion.hpp` comparison.
//!
//! | Rust module | C++ |
//! |---|---|
//! | `packet` | `sf::Packet` (SFML fork `src/SFML/Network/Packet.cpp`) |
//! | `protocol` | `Global/ProtocolVersion.hpp`, `Global/Version.hpp` |
//! | `sodium` | `Online/Sodium.hpp` / `Online/Sodium.cpp` |
//! | `shared` | `Online/Shared.hpp` / `Online/Shared.cpp` |
//!
//! The mapping (file/struct/function → Rust, with citations) is
//! `.agents/plans/001-rust-port/mapping/networking.md`; the byte-exact
//! oracle vectors are pinned there and asserted in `tests/wire.rs`.

pub mod packet;
pub mod protocol;
pub mod sodium;
