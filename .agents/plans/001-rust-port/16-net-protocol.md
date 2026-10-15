# Issue 16 — `oh-net` protocol: packets, sodium, shared codecs

## Objective

Port the wire protocol shared by client and server: the packet structs (`CTSP*`,
`STCP*`, `PInvalid`, `PEncryptedMsg`), the `sf::Packet`-based encoding, the
libsodium encryption/authentication, and the `encode/decode` functions from
`Online/Shared.*`.

## Key decisions

- **`sf::Packet` semantics are a compatibility surface.** In SFML, `Packet::operator<<`
  writes each type with a 4-byte length prefix for strings/vectors and host-endian
  fixed-width for integrals, and `append` writes raw. **Read the fork's actual
  `Packet.cpp`** (or the upstream SFML equivalent the fork matches) and reproduce
  the exact byte layout in a Rust `Packet` type with the same `<<`/`>>`/`append`
  semantics. Document the encoding rules in `lib-sfml-packet.md`.
- Sodium (D-004 sodium entry): `sodiumoxide` — it exposes the same primitives
  (`crypto_box_curve25519xsalsa20poly1305`, `crypto_sign_ed25519`,
  `randombytes`) with matching key/nonce sizes. Verify the exact functions used in
  `Online/Sodium.*`/`Shared.cpp` and match.
- The protocol version (`Global/ProtocolVersion.hpp`) is fixed; a mismatch must
  produce the same rejection.

## Files

| File | Change |
|---|---|
| `crates/oh-net/Cargo.toml` | new — deps `sodiumoxide`, `oh-replay` |
| `crates/oh-net/src/{packet,protocol,sodium,shared}.rs` | new |
| `crates/oh-net/tests/wire.rs` | new — byte-exact packet vectors |
| `.agents/skills/lib-sfml-packet.md` | new — from real SFML fork source |
| `.agents/skills/lib-libsodium.md` | new — from real `Online/Sodium.*` + sodium docs source |

## Steps

1. Port the `Packet` type with exact `<<`/`>>` encoding for every used type.
2. Port every packet struct with exact field order/types (transcribed from
   `Shared.hpp`).
3. Port `makeClientToServerPacket`, `decodeClientToServerPacket`,
   the server-side counterparts, `makeClientToServerEncryptedPacket`.
4. Port the sodium layer: key exchange, nonce handling, encrypt/decrypt.
5. Port `ProtocolVersion` comparison.

## Verification

- Generate wire vectors by compiling the real `Shared.cpp` against an SFML
  `Packet` shim (as done for replays) and assert the Rust output is byte-identical
  for a representative packet of each type.
- Encryption round-trip with fixed keys/nonces (deterministic) equals the C++
  ciphertext.
- `tools/gate.sh` green.

## Notes / risks

- Encryption nonces must be handled identically (random vs counter) or cross-
  implementation tests must use fixed nonces; check the source.
- `sf::Packet` also has a validation/endianness note: confirm whether the fork
  writes a length header per string (it does in upstream SFML 2.x: u32 length +
  bytes) and whether bool is 1 byte.
