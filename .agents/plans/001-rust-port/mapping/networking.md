# Mapping 16 — Networking (`16-net-protocol.md`)

Mechanical C++→Rust mapping of the `Online/Shared.*` wire protocol + the
libsodium layer, for the `oh-net` crate. Every line citation below was
verified against the source in this worktree (D-014); the byte-exact ground
truth is `mapping/oracle-vectors.md` ("Net wire vectors", produced by
`tools/oracle/net_probe.cpp` compiling the REAL `Shared.cpp` + `Sodium.cpp`).

Sources:

- C++: `include/SSVOpenHexagon/Online/Shared.hpp` (224 lines),
  `src/SSVOpenHexagon/Online/Shared.cpp` (931 lines),
  `include/SSVOpenHexagon/Online/Sodium.hpp` (76 lines),
  `src/SSVOpenHexagon/Online/Sodium.cpp` (79 lines),
  `include/SSVOpenHexagon/Global/ProtocolVersion.hpp` (15 lines),
  `include/SSVOpenHexagon/Global/Version.hpp:46` (`GAME_VERSION{2,1,7}`),
  `include/SSVOpenHexagon/Online/DatabaseRecords.hpp:39-45`
  (`Database::ProcessedScore`), `src/SSVOpenHexagon/Core/Replay.cpp:380-423`
  (`replay_file::serialize_to_packet` / `deserialize_from_packet`),
  `:467-487` (the compressed form).
- Rust: `crates/oh-net/src/{packet,protocol,sodium,shared}.rs`,
  `crates/oh-net/tests/wire.rs`, `crates/oh-replay/src/packet.rs` (the
  `Packet` codec — see `lib-sfml-packet.md`), `crates/oh-replay/src/{replay,
  compressed_replay}.rs` (the `ReplayFile`/`CompressedReplayFile` types
  reused by the `CTSPReplay`/`CTSPCompressedReplay` variants).

Legend: `Shared.cpp` line numbers are for the file at this commit; the same
numbers are used in `shared.rs`'s per-function doc comments.

---

## §1. Files & modules

| C++ | Rust | notes |
|---|---|---|
| `sf::Packet` (pinned fork, `src/SFML/Network/Packet.cpp`) | `oh_replay::packet::Packet` (`crates/oh-replay/src/packet.rs:34`), re-exported by `oh_net::packet` (`crates/oh-net/src/packet.rs`) | full codec + poison semantics in `lib-sfml-packet.md` |
| `Online/Shared.hpp` (structs) | `crates/oh-net/src/shared.rs` (§3) | one `enum` per direction |
| `Online/Shared.cpp` (encode/decode) | `crates/oh-net/src/shared.rs` (§4, §5) | |
| `Online/Sodium.{hpp,cpp}` | `crates/oh-net/src/sodium.rs` (§6) | sodiumoxide 0.2.7, system libsodium 1.0.18 |
| `Global/ProtocolVersion.hpp` | `crates/oh-net/src/protocol.rs` | `ProtocolVersion` = `u8` (`:11`), `PROTOCOL_VERSION = 0` (`:13`) |
| `Global/Version.hpp:46` | `crates/oh-net/src/protocol.rs` | `GameVersion` struct + `GAME_VERSION` const (2.1.7) |
| `Online/DatabaseRecords.hpp:39-45` | `shared.rs:72` (`ProcessedScore`) | not stored in the DB; wire type only |
| `replay_file` / `compressed_replay_file` | `oh_replay::ReplayFile` / `oh_replay::CompressedReplayFile` | embedded by value in the replay variants |

**Variant layout = wire layout.** C++ `PVClientToServer` / `PVServerToClient`
are `std::variant<PInvalid, PEncryptedMsg, ...concrete...>`
(`Shared.hpp:87-88, 129-130`); `getPacketType<T>` (`Shared.cpp:73-85`)
returns the type's **index in that variant list**, which is the 1-byte
wire type after the header. The Rust enums `ClientToServer`
(`shared.rs:86`, consts at `:205-221`) and `ServerToClient` (`shared.rs:227`,
consts at `:319-334`) keep the same order, so the type-index consts equal
the C++ variant indices (pinned by the probe's `decode_* idx=` lines).

Header for every packet (`clearPacketAndEncodePreambleAndProtocolVersionAndGameVersion`,
`Shared.cpp:110-118` → `encode_header` `shared.rs:344`):

```
'6F' '68'   preamble ("oh" — preamble1stByte :89, preamble2ndByte :90)
00         PROTOCOL_VERSION (encodeProtocolVersion :98-100)
02 01 07   GAME_VERSION major/minor/micro as u8s (encodeVersion :103-108)
<TYPE>     variant index (encodePacketType :120-123)
<fields>   the struct's fields in declaration order
```

---

## §2. Structs (`Shared.hpp` → `shared.rs`)

C++ structs are `clang-format off` one-liners with no constructors; the Rust
variants carry the same fields in the same order (`boost::pfr` iterates
fields in declaration order on both encode and decode — that order IS the
field order on the wire).

| C++ (`Shared.hpp`) | Rust (`shared.rs`) | fields (wire order) |
|---|---|---|
| `PInvalid` `:47` | `ClientToServer::PInvalid` / `ServerToClient::PInvalid` (`:89`, `:229`) | `error: String` (NOT a wire type — decode-only; see §5.3) |
| `PEncryptedMsg` `:52` | `...::PEncryptedMsg` (`:95`, `:234`) | `nonce: [u8;24]`, `messageLength: u64`, `ciphertextLength: u64`, `ciphertext: Vec<u8>` (C++ `Impl::CiphertextVectorPtr` = a pointer to the static ciphertext buffer; Rust owns the `Vec`) |
| `CTSPHeartbeat` `:63` | `...::CTSPHeartbeat` (`:108`) | — |
| `CTSPDisconnect` `:64` | `...::CTSPDisconnect` | — |
| `CTSPPublicKey` `:65` | `...::CTSPPublicKey` | `key: [u8;32]` |
| `CTSPRegister` `:66` | `...::CTSPRegister` | `steamId: u64`, `name: String`, `passwordHash: String` |
| `CTSPLogin` `:67` | `...::CTSPLogin` | same |
| `CTSPLogout` `:68` | `...::CTSPLogout` | `steamId` |
| `CTSPDeleteAccount` `:69` | `...::CTSPDeleteAccount` | `steamId`, `passwordHash` |
| `CTSPRequestTopScores` `:70` | `...::CTSPRequestTopScores` | `loginToken`, `levelValidator` |
| `CTSPReplay` `:71` | `...::CTSPReplay` | `loginToken`, `replayFile: ReplayFile` |
| `CTSPRequestOwnScore` `:72` | `...::CTSPRequestOwnScore` | `loginToken`, `levelValidator` |
| `CTSPRequestTopScoresAndOwnScore` `:73` | `...::CTSPRequestTopScoresAndOwnScore` | same |
| `CTSPStartedGame` `:74` | `...::CTSPStartedGame` | same |
| `CTSPCompressedReplay` `:75` | `...::CTSPCompressedReplay` | `loginToken`, `compressedReplayFile: CompressedReplayFile` |
| `CTSPRequestServerStatus` `:76` | `...::CTSPRequestServerStatus` | `loginToken` |
| `CTSPReady` `:77` | `...::CTSPReady` | `loginToken` |
| `STCPKick` `:106` | `ServerToClient::STCPKick` (`:245`) | — |
| `STCPPublicKey` `:107` | `...::STCPPublicKey` | `key: [u8;32]` |
| `STCPRegistrationSuccess` `:108` | `...::STCPRegistrationSuccess` | — |
| `STCPRegistrationFailure` `:109` | `...::STCPRegistrationFailure` | `error: String` |
| `STCPLoginSuccess` `:110` | `...::STCPLoginSuccess` | `loginToken: u64`, `loginName: String` |
| `STCPLoginFailure` `:111` | `...::STCPLoginFailure` | `error` |
| `STCPLogoutSuccess` `:112` | `...::STCPLogoutSuccess` | — |
| `STCPLogoutFailure` `:113` | `...::STCPLogoutFailure` | — |
| `STCPDeleteAccountSuccess` `:114` | `...::STCPDeleteAccountSuccess` | — |
| `STCPDeleteAccountFailure` `:115` | `...::STCPDeleteAccountFailure` | `error` |
| `STCPTopScores` `:116` | `...::STCPTopScores` | `levelValidator`, `scores: Vec<ProcessedScore>` |
| `STCPOwnScore` `:117` | `...::STCPOwnScore` | `levelValidator`, `score: ProcessedScore` |
| `STCPTopScoresAndOwnScore` `:118` | `...::STCPTopScoresAndOwnScore` | `levelValidator`, `scores`, `ownScore: Option<ProcessedScore>` |
| `STCPServerStatus` `:119` | `...::STCPServerStatus` | `protocolVersion: u8` (C++ `ProtocolVersion` = `uint8_t`), `gameVersion: GameVersion`, `supportedLevelValidators: Vec<String>` |

`Database::ProcessedScore` (`DatabaseRecords.hpp:39-45` → `shared.rs:72`):
`position: u32`, `userName: String`, `scoreTimestamp: u64`,
`scoreValue: f64` — encoded by `encode_score` (`shared.rs:602`): u32-BE,
string (u32-BE len + raw), u64-BE, raw f64 (host-endian LE).

Encoding of the special field types (C++ `encodeField` overloads → the
match arms in `encode_cts_oh_packet` (`:360`)/`encode_stc_oh_packet`
(`:488`), `shared.rs:356-600`):

| C++ type | C++ (`Shared.cpp`) | wire | Rust |
|---|---|---|---|
| `std::array<unsigned char, 24>` (nonce) | array overload `:560-568` | 24 raw bytes | `append(nonce)` (CTS `:375`, STC `:524`) |
| `std::uint64_t` | `:547-552` | 8 **BE** | `write_u64` |
| `std::string` | `:547-552` | u32-BE len + raw | `write_str` |
| `SodiumPublicKeyArray` (32×u8) | array overload `:560-568` | 32 raw bytes | `append(key)` (CTS `:392`, STC `:513`) |
| `CiphertextVectorPtr` | `:615-620` + `encodeFirstNVectorElements :503-513` | exactly `ciphertextLength` raw bytes | `append(&ciphertext[..min(len)])` (CTS `:386`, STC `:531`) |
| `std::vector<ProcessedScore>` | vector overload `:569-578` (u64-BE count + elements) | | `encode_scores` (`shared.rs:610`) |
| `std::vector<std::string>` | `:569-578` | u64-BE count + strings | `:590` |
| `std::optional<ProcessedScore>` | `:580-589` | 1-byte set flag (0/1) + value when set | `:571` |
| `hg::replay_file` | `:591-596` (`(void)rf.serialize_to_packet(p)`) | u64-BE byte count + raw file bytes (`Replay.cpp:380-395`) | `replay_file.serialize_to_packet(r.p)` (`:440`) |
| `hg::compressed_replay_file` | `:598-605` | u64-BE size + raw zlib bytes (`Replay.cpp:467-487`) | `:473` |
| `hg::GameVersion` | `:606-612` | three i32-BE | `:585` |

---

## §3. Encode functions

| C++ (`Shared.cpp`) | Rust (`shared.rs`) |
|---|---|
| `encodePreamble` `:92-96` + `encodeProtocolVersion` `:98-100` + `encodeVersion` `:103-108` (+ the `clear` of `clearPacketAndEncode...` `:110-118`) | `encode_header` `:342` (resets the packet first — C++ `p.clear()`) |
| `encodePacketType` `:120-123` | the `write_u8(TYPE)` first line of each encoder arm |
| `encodeOHPacket` `:623-631` (pfr: type byte + every field) | `encode_cts_oh_packet` `:356`, `encode_stc_oh_packet` `:490` (one match arm per type = the pfr field loop) |
| `makeClientToServerPacket` `:725-729` (+ explicit instantiations `:731, 734-737`) | `make_client_to_server_packet` `:618` |
| `makeClientToServerEncryptedPacket` `:741-748` over `makeEncryptedPacketImpl` `:690-719` | `make_client_to_server_encrypted_packet` `:633` — build inner packet (type byte + fields, NO header), draw a random nonce, `messageLength` = inner size, `ciphertextLength = 16 + messageLength`, `secretbox_seal`, then `make_client_to_server_packet(PEncryptedMsg{..})` |
| `makeServerToClientPacket` `:873-877` (+ `:879, 881-885`) | `make_server_to_client_packet` `:657` |
| `makeServerToClientEncryptedPacket` `:889-896` | `make_server_to_client_encrypted_packet` `:664` |

C++ `make*Packet` is `void` and CLEARS the target packet; the Rust `make_*`
also reset the packet (via `encode_header`) and take `&mut Packet` — the
callers pass a fresh `Packet` either way.

The C++ `PEncryptedMsg.ciphertext` field is an `Impl::CiphertextVectorPtr`
(struct `Shared.hpp:40-44`, `namespace Impl` `:38-45`: a wrapper that defers allocation to the static
`getStaticCiphertextBuffer` `:521-525`); the encoder writes only
`ciphertextLength` bytes of it (`:615-620`). The Rust variant stores a
`Vec<u8>` directly (no static buffer needed — the value is owned by the
packet variant, mirroring the wire content exactly).

---

## §4. Decode functions

| C++ (`Shared.cpp`) | Rust (`shared.rs`) |
|---|---|
| `verifyReceivedPacketPreambleAndProtocolVersionAndGameVersion` `:469-486` | `verify_preamble` `:960` — matches the 1st/2nd preamble byte and the protocol version (the `matchOrPrintError` mismatch line, verbatim incl. the unclosed value-quote and the NUL byte in the expected value, via `mismatch_line` `:1019`, C++ statement `:414-416`); the three game-version bytes are **skipped** (`skipOrPrintError` — read, never compared; the C++ comment at `:483` says "Game version is currently ignored") |
| `extractPacketType` `:489-500` | `extract_packet_type` `:1031` |
| `decodeClientToServerPacket` `:857-868` (preamble check, then inner) | `decode_client_to_server_packet` `:1647` |
| `decodeClientToServerPacketInner` `:844-855` (`INJECT_COMMON_PACKET_HANDLING_CODE` `:773-792` + `HANDLE_PACKET` `:758-771` expanded over `SSVOH_CTS_PACKETS` at `:850-851`) | `decode_cts_inner` `:1123` |
| `decodeServerToClientPacket` `:920-931` | `decode_server_to_client_packet` `:1662` |
| `decodeServerToClientPacketInner` `:907-918` | `decode_stc_inner` `:1392` |
| `makeExtractAllMembers` `:801-821` (reads EVERY field; on a failed field appends `Error decoding field #i \n` — note the trailing space, `:814` — and keeps going) | the field reads inside each `decode_*_inner` match arm (each failed read appends the element error + `Error decoding field #i \n` and the remaining fields are still read — the packet is poisoned, so they fail too; identical error-string accumulation, pinned by `tests/wire.rs::negative_cts_truncated_register`) |
| `decodeEncryptedPacket` `:824-838` (no-key error at `:830`) | the `P_ENCRYPTED_MSG` branch of each inner decoder (`:1137`, `:1406`) |
| `decryptPacket` `:634-686` | `decrypt_packet` `:1046` (nonce `:1051-1060` = the C++ array extractor `:175-199`; `messageLength` `:1062`; `ciphertextLength` `:1070`; the per-byte ciphertext read loop with the `Error decoding client ciphertext at index 'i'\n` line, C++ `:668`; `secretbox_open` → the `Failure decrypting encrypted client message\n` line, C++ `:679`; the resize/append of exactly `messageLength` bytes, C++ `:674, 683-684` → `:1102`) |
| unknown-type line (`static_cast<int>(*pt)`, `:853` / `:916`) | `Unknown packet type '{pt}'\n` (decimal, matching the `int` cast) `:1323`, `:1586` |

### 4.1 Field extractors (the `Extractor<T>` specializations)

| C++ (`Shared.cpp`) | Rust (`shared.rs`) | failure line (verbatim) |
|---|---|---|
| generic/POD `:130-172` | `Reader::u8/u32/u64/i32/f64/bool_/string` (`:695+`) | `Error extracting single object\n` (`shared.rs:798`; C++ `:139`) |
| struct (pfr recursion) `:146-163` | `extract_score` `:853` (each field's own failure line; no struct-level wrapper) | the fields' lines |
| `std::array<T,N>` `:175-199` | the nonce read loop in `decrypt_packet` `:1051` | `Error extracting array element at index 'i'\n` |
| `std::vector<T>` `:201-232` | `extract_scores` `:820` (and the inline validator-vector read in `decode_stc_inner` `:1556`) | `Error extracting vector size\n` / `Error extracting vector element at index 'i'\n` |
| `std::optional<T>` `:235-265` | `extract_optional_score` `:900` | `Error extracting optional set flag\n` / `Error extracting optional element\n` |
| `replay_file` `:268-283` | `replay_file.deserialize_from_packet(r.p)` in `decode_cts_inner` `:1281` | `Error deserializing replay\n` |
| `compressed_replay_file` `:286-301` | `:1302` | `Error deserializing compressed replay\n` |
| `GameVersion` `:304-334` | `extract_game_version` `:924` | `Error deserializing {major,minor,micro} version\n` |

`Reader` (`shared.rs:695`) wraps `&mut Packet` with a `valid` poison flag
that reproduces the fork's `checkSize`/`m_isValid` semantics
(`lib-sfml-packet.md`): once a read fails, every later read fails without
consuming bytes — which is what makes the C++ "keep extracting after a
failure" produce one error line per remaining field, and what the Rust port
reproduces by poisoning `Reader` (and, for the replay extracts, the
underlying `Packet` too).

### 4.2 The `error_oss` accumulation

C++ passes a `std::ostringstream&` that the caller may pre-populate; on
failure the whole content becomes `PInvalid.error`
(`errorOss.str()`, e.g. `Shared.cpp:854`). Rust takes `error_oss: &mut String`
and does the same: pre-existing content is preserved and the returned
`PInvalid.error` is the full content. Pinned byte-for-byte (including the
NUL byte and trailing spaces) by `tests/wire.rs` negative tests against the
probe's output.

### 4.3 `PInvalid` is not a decodable type

The C++ decode dispatch (`HANDLE_PACKET` expansion) covers only the concrete
types — there is **no `PInvalid` arm** (`Shared.cpp:758-771, 844-855,
907-918`), and the encode side never instantiates `make*Packet<PInvalid>`
(only `PEncryptedMsg` + the concrete types get explicit instantiations,
`Shared.cpp:731, 879`). So a type-0 byte on the wire falls through to
`Unknown packet type '0'\n` — and the Rust `PInvalid` encoder arm exists
only as a documented superset so the enum is total (the C++ API simply has
no way to produce it). Pinned: `neg_cts_type0_not_decodable`.

### 4.4 Documented deviations (hostile-input territory only)

Well-formed traffic is byte-identical C++↔Rust; the two differences show up
only for attacker-controlled bytes:

1. **Vector size** — C++ `Extractor<std::vector>::doExtractInto` does
   `result.resize(count)` *before* reading the elements (`Shared.cpp:201-232`):
   a count larger than the remaining bytes `terminate`s (or, with a small
   count + huge declared element sizes, reads OOB). The Rust port bounds
   checks `count` against the remaining bytes and fails at the size check
   instead (`shared.rs:820-833`). Same precedent as `oh-replay`.
2. **Decrypted message length** — C++ resizes the scratch buffer to the
   *declared* `messageLength` and appends exactly that many bytes
   (`Shared.cpp:674, 683-684`): a `messageLength` larger than the true
   plaintext zero-pads (harmless), smaller is an out-of-bounds read. The
   Rust port zero-pads up to `messageLength` but truncates to it
   (`shared.rs:1102`); the larger case produces identical bytes.

---

## §5. Sodium (`Online/Sodium.*` → `sodium.rs`)

Full treatment (constants, argument-order trap, nonce findings, KAT
strategy, the contract-primitive correction) is in `lib-libsodium.md`;
the map:

| C++ | Rust (`sodium.rs`) |
|---|---|
| `SodiumPublicKeyArray`/`SodiumSecretKeyArray`/`SodiumReceiveKeyArray`/`SodiumTransmitKeyArray`/`SodiumNonceArray` (all `std::array<unsigned char, 32/32/32/32/24>`; aliases `Sodium.hpp:22-27`, size consts `:16-20`) | the `Sodium*Array` type aliases (`sodium.rs:63-71`), the `SODIUM_*_BYTES` consts (`:47-60`) |
| `getCiphertextLength(mlen)` (`Sodium.hpp:29-33`) | `get_ciphertext_length` `:88` |
| `generateNonce()` (`Sodium.cpp:14-19`) | `generate_nonce` `:95` (24 fresh random bytes; **random per message, carried in `PEncryptedMsg.nonce`** — not a counter) |
| `SodiumPSKeys` (`Sodium.hpp:37-41`) / `SodiumRTKeys` (`:43-47`) | `SodiumPsKeys` `:104` / `SodiumRtKeys` `:113` |
| `generateSodiumPSKeys()` (`Sodium.cpp:21-26`) | `generate_sodium_ps_keys` `:122` |
| `calculateServerSessionSodiumRTKeys` (`Sodium.cpp:28-42`) | `calculate_server_session_sodium_rt_keys` `:145` |
| `calculateClientSessionSodiumRTKeys` (`Sodium.cpp:44-58`) | `calculate_client_session_sodium_rt_keys` `:164` |
| `sodiumKeyToString` (`Sodium.hpp:60-71`) | `sodium_key_to_string` `:185` (decimal concatenation, no separators) |
| `sodiumHash(in)` (`Sodium.cpp:62-74`) | `sodium_hash` `:196` (keyed BLAKE2b, 32-byte **zero** key, 32-byte output) |
| `randomUInt64()` (`Sodium.cpp:76-79`) | `random_uint64` `:209` (host-endian reinterpretation) |
| `crypto_secretbox_easy` (`Shared.cpp:704-706`) | `secretbox_seal` `:224` |
| `crypto_secretbox_open_easy` (`Shared.cpp:676-677`) | `secretbox_open` `:240` |
| `libsodium_init()` | `ensure_initialized` (idempotent `Once`) |

---

## §6. Contract corrections (D-014)

Found while implementing against the real source (the plan file
`16-net-protocol.md` is corrected in `lib-libsodium.md` §"The exact C++
surface"):

1. **Primitive list** — the contract named `crypto_box_curve25519xsalsa20poly1305`
   and `crypto_sign_ed25519`; the source uses neither. The real surface is
   `crypto_kx` (key exchange) + `crypto_secretbox` (message encryption) +
   `crypto_generichash` (`sodiumHash`) + `randombytes`. `sodiumoxide` 0.2.7
   (the D-004 choice) covers all of it.
2. **Packet encoding pins** — the pre-existing `oracle-vectors.md` pin
   ("BE ints + u32-BE string lengths") was re-verified against the fork's
   `Packet.cpp` and is **correct** (no correction); the float/double
   host-endian trap and the bool=1-byte rule are now documented per-type in
   `lib-sfml-packet.md`.
3. **Line citations** — the landed `sodium.rs`/`protocol.rs` doc comments
   carried stale line numbers (e.g. `generateNonce` is `Sodium.cpp:14-19`,
   not 13-17; `PROTOCOL_VERSION` is `ProtocolVersion.hpp:13`, not 15);
   corrected in the deliverable-1 commit. All new citations in this doc and
   in `shared.rs` were re-verified against the sources.

---

## §7. Verification

- `tools/oracle/net_probe.cpp` (run via `tools/oracle/run.sh`, the
  `=== net ===` section): compiles the real `Shared.cpp` + `Sodium.cpp`
  against the `tools/oracle/shim/` `Packet` shim + system libsodium 1.0.18;
  46 checks, all `ok=1`, deterministic (two-run diff clean). Vectors:
  `mapping/oracle-vectors.md` ("Net wire vectors").
- `crates/oh-net/tests/wire.rs` — 20 tests: byte-exact encodes for all 30
  packet shapes (15 CTS + 15 STC incl. the own-score Some/None pair), full
  decode round-trips, the kx/secretbox/generichash cross-implementation KATs
  (Rust output == the C++ probe's pinned bytes), and the exact `PInvalid`
  error strings (11 negative cases).
- `crates/oh-net/src/sodium.rs` unit tests — 9 tests (key sizes,
  exchange symmetry incl. random keypairs, fixed key/nonce seal+open,
  tamper rejection, `sodiumKeyToString`, `sodiumHash` determinism, nonce
  freshness); with `packet.rs` (8) and `protocol.rs` (4) that is the
  21-test lib suite.
- `cargo test -p oh-net`: 41 tests green (21 lib + 20 wire); clippy
  `-D warnings` clean.

---

# Issue 17 — client, LeaderboardCache, SQLite database (`17-net-client-db.md`)

Continues the `oh-net` lane: the online **client** (`Core/HexagonClient`),
the **leaderboard cache** (`Core/LeaderboardCache`), and the server-side
**SQLite database** (`Online/Database`). All citations verified against
the source at this commit (D-014); the database's ground truth is the
probe output `tools/oracle/db_probe_schema.txt` (the skill
`lib-sqlite.md` carries the full query map).

## §8. Files & modules (issue 17)

| C++ | Rust | notes |
|---|---|---|
| `Core/LeaderboardCache.{hpp,cpp}` (76 + 52 lines) | `crates/oh-net/src/leaderboard_cache.rs` | 6 methods + the 6-s staleness window |
| `Core/HexagonClient.{hpp,cpp}` (992 + 207 lines) | `crates/oh-net/src/client.rs` | the pull-model client (§10) |
| `Online/Database.{hpp,cpp}` (391 lines) | `crates/oh-net/src/database.rs` | rusqlite 0.31 `bundled` (D-004); `lib-sqlite.md` |
| `Online/DatabaseRecords.hpp` (`User`, `LoginToken`, `Score`) | `crates/oh-net/src/db_records.rs` | `ProcessedScore` MOVED here from `shared.rs` (its C++ namespace) and re-exported from `shared.rs` — the wire path is unchanged (extend-only) |
| — (SFML `sf::TcpSocket`, pinned fork `src/SFML/Network/TcpSocket.cpp`) | `crates/oh-net/src/client.rs`: `NetSocket` trait + `StdTcpSocket` + `FramedSocket` | the transport split, §10.1 |

## §9. LeaderboardCache (`leaderboard_cache.rs`)

| C++ (`Core/LeaderboardCache.cpp`) | Rust | notes |
|---|---|---|
| `receivedScores` `:18-24` | `received_scores` | replace the whole vector + refresh the cache time (the C++ `operator[]` creates the entry if absent) |
| `receivedOwnScore` `:26-32` | `received_own_score` | store + refresh |
| `requestedScores` `:34-38` | `requested_scores` | refresh only (creates the entry if absent) — the UI just (re)sent a request |
| `shouldRequestScores` `:39-51` | `should_request_scores` | no entry → `true`; age `> 6 s` (STRICT `>`, `:48`) → `true` |
| `getScores` `:53-57` | `get_scores` → `Option<&Vec<ProcessedScore>>` | C++ asserts `hasInformation` + `unordered_map::at` (faults when absent); the port returns `None` — the callers (`MenuGame.cpp:5608`) always check `has_information` first |
| `getOwnScore` `:59-67` | `get_own_score` → `Option<Option<&ProcessedScore>>` | the double `Option` preserves BOTH C++ states: no entry (assert/`at` fault) vs entry-without-own-score (`nullptr`) |
| `hasInformation` `:69-73` | `has_information` | |

Documented equivalents: (1) the `HRClock::now()` calls (`Utils/Clock.hpp:14`,
monotonic; `:20,29,36,48`) are funnelled through an injectable provider
(default `Instant::now`) so the 6-s window is testable without sleeping;
(2) the header's `getSupported` (`LeaderboardCache.hpp:50`) is omitted —
it is **defined nowhere** in the C++ tree (grep-verified dead
declaration, never called). Six in-module tests pin the window
(incl. the exactly-6-s boundary under the strict `>`), the replace/
refresh semantics, the optional-own-score states, and per-level
independence.

## §10. HexagonClient (`client.rs`)

### §10.0 Contract correction (D-014)

The issue-017 contract said the C++ client is "a background thread +
queues" and to reproduce that model. **It is not.** `HexagonClient` owns
a non-blocking `sf::TcpSocket` and does ALL I/O from the game loop:
`update()` (per frame, `HexagonClient.cpp:777-794`) drives
`sendHeartbeatIfNecessary()` (the 45-s window, `:508-530`) + one
`receiveDataFromServer()` (`:532-775`); the only "queue" is the
`std::deque<Event>` of APPLICATION events the caller drains with
`pollEvent()` (`:975-984`). There is no thread in the class (the
`<thread>` include at `:19` serves only the 50 ms ticket-poll sleep,
`:64`). The port reproduces the pull model exactly: `update()` +
`poll_event()`.

### §10.1 The socket layer (the SFML `TcpSocket` split)

The pinned fork (`src/SFML/Network/TcpSocket.cpp`, read directly) splits
into the two layers the client uses:

| C++ (fork `TcpSocket.cpp`) | Rust | notes |
|---|---|---|
| `send(const void*, size, sent&)` `:234-268` | `NetSocket::send` → `SendResult` | loops until all bytes are out or would-block; `Partial(n)` ONLY when some bytes went out (`:259-263`); `size == 0` → `Error` (`:239-243`) |
| `receive(void*, size, received&)` `:272-305` | `NetSocket::recv` → `RecvResult` | a single non-blocking `recv`; `recv()==0` → `Disconnected` (`:297-300`) |
| `connect(ip, port, timeout)` (called in blocking mode) + the client's `setBlocking` flips (`HexagonClient.cpp:104-114`) | `NetSocket::connect(addr, timeout)` | the C++ 0.5 s timeout; non-blocking after success. `StdTcpSocket`: `TcpStream::connect_timeout` + `set_nonblocking(true)` |
| `disconnect()` `:213-219` | `FramedSocket::disconnect` | closes AND resets the pending-RECEIVE state (`m_pendingPacket`) — NOT the send side |
| `send(Packet&)` `:309-365` | `FramedSocket::send_packet` | u32-`htonl` size prefix + payload; `m_blockToSendBuffer` is REBUILT from the current packet on every call (`:320-336`) and the send resumes from `Packet::m_sendPos` (`:356-363`) — the port mirrors the rebuild + offset-preservation (incl. the C++'s unsigned-underflow corner on a stale offset) |
| `receive(Packet&)` `:368-424` | `FramedSocket::receive_packet` | `packet.clear()` FIRST (`:371` — the user packet never holds partial data); the 4 size bytes then the payload accumulate in `m_pendingPacket` across calls (`:376-410`, 1024-byte payload chunks); a 0-length payload still returns `Done` with an empty packet (`:415-417`) |

**Rust-1.97 note:** `TcpStream::send`/`recv` no longer exist as inherent
methods in this toolchain — `StdTcpSocket` uses `std::io::Write::write` /
`Read::read` (same non-blocking contract).

### §10.2 State machine + events

`State` (the C++ numeric values, `HexagonClient.hpp:41-49`):
`Disconnected=0, InitError=1, Connecting=2, ConnectionError=3,
Connected=4, LoggedIn=5, LoggedInReady(=LoggedIn_Ready)=6`.

`Event` — the 15-variant C++ `std::variant` (`HexagonClient.hpp:52-77`),
`E`-prefix dropped (variant order preserved).

| C++ transition | Rust |
|---|---|
| ctor `:437-470`: PS keys → ticket poll (`:55-89`: `run_callbacks` + 50 ms sleep, `tries > 15` → fail) → `InitError` on failure, else `connect()` | `with_socket` (same body; `new` = default socket + clock) |
| `connect` `:395-434`: `Connecting` → socket init (fail → `EConnectionFailure` + `ConnectionError`) → heartbeat → public key (same failure path) → `EConnectionSuccess` + `Connected` | `connect` (the exact `"Failure connecting, error <reason>"` strings) |
| `disconnect` `:482-505`: (LoggedIn\|Ready) → `sendLogout`; (Connected\|LoggedIn\|Ready) → `sendDisconnect`; close; `Disconnected` | `disconnect` (also from `Drop` — the C++ dtor `:475-479`) |
| `STCPKick` `:558-565` | `EKicked` + `disconnect()`, returns `true` |
| `STCPPublicKey` `:567-604` | store key → `calculate_client_session_sodium_rt_keys` (fail → `disconnect` + `false`) |
| `STCPLoginSuccess` `:621-639` | token + name stored, `LoggedIn`, then **immediately** `sendRequestServerStatus` (the arm's return value) |
| `STCPServerStatus` `:717-772` | game-version mismatch → `EGameVersionMismatch` ONLY (session proceeds); protocol mismatch → `EProtocolVersionMismatch` + `disconnect()` + `true`; else validators INSERTed (set union, `:740-743`), `LoggedInReady`, `ELoginSuccess`, `sendReady` |
| `STCPTopScoresAndOwnScore` `:700-715` | the top-scores event ALWAYS; the own-score event only when `ownScore` is present |
| every other STCP | its event, `true` |
| `PInvalid`/`PEncryptedMsg` arms `:547-556` | `false` (no event) — an undecryptable/still-encrypted message is a receive failure |
| `tryLogoutFromServer` `:850-863` | state → `Connected`, token + name CLEARED before the logout packet goes out |
| `trySendCompressedReplay` `:887-906` | unsupported level → **`true` no-op** (not a failure) |
| `tryRegister`/`tryLogin` `:813-848` | state `Connected` gate + `name.empty() \|\| name.size() > 32 \|\| password.empty()` → the `"...too long or empty"` failure event (byte count, as `str::len()`) |

### §10.3 Documented deviations (client)

- **Logging**: the `clog`/`SSVOH_CLOG*` calls (the `ssvu::lo` app logger)
  and `_verbose` are dropped — diagnostic only, no protocol behaviour
  depends on them, and `oh-net` has no log dependency. (`sendCompressedReplay`
  keeps its `_level_validator` parameter unused for that reason — the C++
  uses it only for the verbose log.)
- **Time**: `HRClock::now()` + `std::this_thread::sleep_for` go through
  the injectable `ClientClock` (default `Instant::now` + `thread::sleep`)
  so the 45-s heartbeat window and the 16-try ticket poll are testable.
  `_lastHeartbeatTime`'s C++ epoch-0 default → an `Instant` at
  construction (unobservable: the first heartbeat is sent in the
  ctor's `connect()`, and `update()` early-returns without a socket).
- **Steam**: `Steam::steam_manager&` → the `SteamManager` trait (the four
  polled methods, `Steam.hpp:41-66`); the client owns a `Box<dyn>`
  (the C++ holds a reference for the object's lifetime).
- **`sodiumHash` digest → `passwordHash` `String`**: the C++ stores the
  32 RAW digest bytes in a `std::string` (not UTF-8 in general); the
  issue-16 wire codec models the field as `String` and encodes
  `len()` + `as_bytes()` — the port wraps the digest with
  `from_utf8_unchecked` (byte-exact; the string is never re-interpreted
  as text).
- **Password salt**: `SecretPasswordSalt.hpp` is absent from the tree
  (grep-verified), so the C++ build hashes `salt + password` — the port
  pins that fallback as `PASSWORD_SALT` (`salt_and_hash_pwd`,
  `HexagonClient.cpp:799-811`).
- **`update()`**'s C++ `try/catch` has no analogue (the ported methods
  return `bool`); the C++'s `receiveDataFromServer(_packetBuffer)` member
  borrow is `mem::replace`'d in the port (same content semantics — the
  buffer is `reset()` before every send and `clear()`-ed at the start of
  every C++ `receive(Packet&)`).

## §11. Database (`database.rs`, `db_records.rs`)

Authoritative map (schema + all 19 query methods + traps + citations):
`.agents/skills/lib-sqlite.md`. Summary of the non-obvious bits:

- The DDL is the sqlite_orm-generated CREATE statements, byte-pinned by
  `tools/oracle/db_probe.cpp` (compiles the REAL `Database.cpp`) —
  double-quoted identifiers, trailing-space column definitions, reverse
  `make_table` creation order (`scores`, `sqlite_sequence`,
  `loginTokens`, `users`; `sqlite_orm.h:18005-18013`), committed as
  `tools/oracle/db_probe_schema.txt`.
- INSERTs EXCLUDE the autoincrement PK (the sqlite_orm plain-`insert`
  serializer, `sqlite_orm.h:16137-16152` — the first `id=0`-insert test
  exposed this); `loginTokens.userId` is UNIQUE; staleness is a SIGNED
  `< 3600 s` in-memory filter (not a SQL `WHERE timestamp < ?`); the
  top-scores/own-score queries are INNER-JOINed (deleted users are
  silently excluded) and `getScore` scans the ordered rows in memory for
  the position; `addScore` is an in-place update (lower-or-equal →
  no-op); `execute` is the `sqlite3_exec` multi-statement walk with
  tab-separated row printing.
- `Database::open(path)` takes the path from the composition (the C++
  hardcodes `"ohdb.sqlite"` at `Database.cpp:44` + a process singleton).

## §12. Verification (issue 17)

- `crates/oh-net/tests/database.rs` — 20 tests: the `.schema` diff
  against the committed C++ probe dump, the no-op-on-reopen schema, and
  the per-function round-trips (the UNIQUE(userId) behaviour, the signed
  token validity with a pinned clock, the stale-token select/remove,
  the score insert/update/no-op, top-scores order/limit/JOIN exclusion,
  `execute` multi-statement + error reporting).
- `leaderboard_cache.rs` — 6 in-module tests (the 6-s boundary under the
  strict `>`, replace/refresh, the optional own-score states,
  independence).
- `client.rs` — 26 in-module tests: a scripted raw-socket fake drives
  the connect flow (heartbeat + public-key frames byte-pinned), the
  16-try ticket-poll failure, the connect-failure events, the
  partial-send 5-retry limit, the full ENCRYPTED RT-key round trip
  (server RT keys computed with the issue-16 server function; the
  encrypted `STCPLoginSuccess` decrypts only if the client's
  `key_receive` is right), every state transition (ready gates,
  game-mismatch-event-only vs protocol-mismatch-disconnect, kick, the
  login→status→ready chain, the validator union), the `try_*`
  validation events, the compressed-replay unsupported-level TRUE no-op,
  the 45-s heartbeat window (controlled clock), framing chunking across
  `update()` calls (3-byte raw reads) + partial-send resume arithmetic,
  the salted password hash, and `Drop` → `disconnect`.
- `cargo test -p oh-net`: 93 tests green (53 lib — 26 client + 6
  leaderboard + 21 pre-existing (5 packet + 4 protocol + 3 sodium + 9
  shared) — + 20 database + 20 wire); clippy `-D warnings` clean.

**Not yet covered** (issue 18's lane, listed so the contract is clear):
the Rust↔C++ live loopback (the plan's "Rust client ↔ Rust server"
full-flow test needs issue 18's `HexagonServer` port; the client's
decrypt path is pinned by the in-crate encrypted round trip instead),
and the `oh-app` wiring of `HexagonClient` into `mainClient` (the
`crates/oh-app/**` path is out of this issue's scope; the `client.rs`
comments in `oh-app` already mark the seam).
