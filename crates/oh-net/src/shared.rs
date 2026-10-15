//! The `Online/Shared.*` port: every `CTSP*`/`STCP*`/`PInvalid`/`PEncryptedMsg`
//! packet struct plus the `make*`/`decode*` functions from
//! `src/SSVOpenHexagon/Online/Shared.cpp`.
//!
//! ## Wire layout
//!
//! Every packet (either direction) is:
//!
//! ```text
//! preamble       'o' 'h'                    (Shared.cpp:89-90)
//! protocol ver   PROTOCOL_VERSION = 0       (Shared.cpp:98-100)
//! game version   major minor micro (each u8; GAME_VERSION = 2.1.7)
//!                                  (Shared.cpp:103-108, Global/Version.hpp:46)
//! packet type    1 byte — the variant index (Shared.cpp:120-123)
//! fields         the struct's fields in declaration order
//! ```
//!
//! `PInvalid` (type 0) and `PEncryptedMsg` (type 1) come before the concrete
//! packet types in both variants (`Shared.hpp:87-88`, `129-130`). Note the
//! C++ decode dispatch has **no `PInvalid` arm** (the `HANDLE_PACKET`
//! expansion covers only the concrete types, `Shared.cpp:758-771, 844-855`),
//! so a type-0 byte decodes as `PInvalid` with
//! `error = "Unknown packet type '0'\n"`; the C++ encode side also never
//! instantiates `make*Packet<PInvalid>` (only `PEncryptedMsg` + the concrete
//! types are explicitly instantiated, `Shared.cpp:731, 879`), so the Rust
//! `PInvalid` arm in the encoders here is a documented superset.
//!
//! ## Decoding
//!
//! `decode_*_packet` mirrors the C++ failure model: any failure yields
//! `PInvalid { error }` where `error` is the accumulated `error_oss` content
//! (the C++ `std::ostringstream&` parameter — it may arrive pre-populated
//! and its full content becomes the error string). Field extraction continues
//! after a failure (C++ `makeExtractAllMembers` sets `success = false` and
//! keeps reading, `Shared.cpp:801-821`); because the SFML packet is poisoned
//! after the first failed read (fork `checkSize`, `Packet.cpp:589-594`), every
//! later field fails too and appends its own error lines. The exact error
//! strings are pinned by `tools/oracle/net_probe.cpp`.
//!
//! Two documented deviations, both hostile-input territory only (well-formed
//! traffic is byte-identical):
//!
//! - a `std::vector` whose declared element count exceeds the remaining
//!   bytes fails cleanly at the size check instead of `resize`-ing to an
//!   attacker-controlled length (the C++ would `terminate` or emit one
//!   error line per element); same precedent as `oh-replay`;
//! - after decrypting a `PEncryptedMsg`, C++ appends exactly
//!   `messageLength` bytes of a buffer that `crypto_secretbox_open_easy`
//!   filled with `ciphertextLength - 16` bytes (zero padding when
//!   `messageLength` is larger; out-of-bounds write when smaller). The port
//!   zero-pads up to `messageLength` and truncates to it.
//!
//! See `.agents/plans/001-rust-port/mapping/networking.md` for the full
//! C++→Rust mapping.

use crate::packet::Packet;
use crate::protocol::{GameVersion, GAME_VERSION, PROTOCOL_VERSION};
use crate::sodium::{
    get_ciphertext_length, secretbox_open, secretbox_seal, SodiumNonceArray, SodiumReceiveKeyArray,
    SodiumTransmitKeyArray, SODIUM_NONCE_BYTES,
};

/// C++ `preamble1stByte` (`Shared.cpp:89`).
pub const PREAMBLE_1ST_BYTE: u8 = b'o';
/// C++ `preamble2ndByte` (`Shared.cpp:90`).
pub const PREAMBLE_2ND_BYTE: u8 = b'h';

/// C++ `hg::Database::ProcessedScore` (`DatabaseRecords.hpp:39-45`) — the
/// score row the `STCPTopScores`/`STCPOwnScore`/`STCPTopScoresAndOwnScore`
/// packets carry.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedScore {
    /// C++ `position`.
    pub position: u32,
    /// C++ `userName`.
    pub user_name: String,
    /// C++ `scoreTimestamp`.
    pub score_timestamp: u64,
    /// C++ `scoreValue`.
    pub score_value: f64,
}

/// C++ `hg::PVClientToServer` (`Shared.hpp:87-89`) — the client→server
/// packet variant. Variant order = wire type byte.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientToServer {
    /// C++ `PInvalid` (`Shared.hpp:47-49`) — type 0. See the module docs
    /// for the encode/decode asymmetry.
    PInvalid {
        /// C++ `error`.
        error: String,
    },
    /// C++ `PEncryptedMsg` (`Shared.hpp:52-58`) — type 1. The envelope of
    /// `make*EncryptedPacket`.
    PEncryptedMsg {
        /// C++ `nonce`.
        nonce: SodiumNonceArray,
        /// C++ `messageLength`.
        message_length: u64,
        /// C++ `ciphertextLength` (= `messageLength + 16`).
        ciphertext_length: u64,
        /// C++ `ciphertext` (the `Impl::CiphertextVectorPtr` payload — the
        /// first `ciphertext_length` bytes of the pointed-to vector,
        /// `Shared.cpp:615-620`).
        ciphertext: Vec<u8>,
    },
    /// C++ `CTSPHeartbeat` (`Shared.hpp:63`).
    CTSPHeartbeat,
    /// C++ `CTSPDisconnect` (`Shared.hpp:64`).
    CTSPDisconnect,
    /// C++ `CTSPPublicKey` (`Shared.hpp:65`).
    CTSPPublicKey {
        /// C++ `key`.
        key: crate::sodium::SodiumPublicKeyArray,
    },
    /// C++ `CTSPRegister` (`Shared.hpp:66`).
    CTSPRegister {
        /// C++ `steamId`.
        steam_id: u64,
        /// C++ `name`.
        name: String,
        /// C++ `passwordHash`.
        password_hash: String,
    },
    /// C++ `CTSPLogin` (`Shared.hpp:67`).
    CTSPLogin {
        /// C++ `steamId`.
        steam_id: u64,
        /// C++ `name`.
        name: String,
        /// C++ `passwordHash`.
        password_hash: String,
    },
    /// C++ `CTSPLogout` (`Shared.hpp:68`).
    CTSPLogout {
        /// C++ `steamId`.
        steam_id: u64,
    },
    /// C++ `CTSPDeleteAccount` (`Shared.hpp:69`).
    CTSPDeleteAccount {
        /// C++ `steamId`.
        steam_id: u64,
        /// C++ `passwordHash`.
        password_hash: String,
    },
    /// C++ `CTSPRequestTopScores` (`Shared.hpp:70`).
    CTSPRequestTopScores {
        /// C++ `loginToken`.
        login_token: u64,
        /// C++ `levelValidator`.
        level_validator: String,
    },
    /// C++ `CTSPReplay` (`Shared.hpp:71`).
    CTSPReplay {
        /// C++ `loginToken`.
        login_token: u64,
        /// C++ `replayFile` — the `replay_file` packet form (u64-BE byte
        /// count + raw file bytes; `Replay.cpp:380-395`).
        replay_file: oh_replay::ReplayFile,
    },
    /// C++ `CTSPRequestOwnScore` (`Shared.hpp:72`).
    CTSPRequestOwnScore {
        /// C++ `loginToken`.
        login_token: u64,
        /// C++ `levelValidator`.
        level_validator: String,
    },
    /// C++ `CTSPRequestTopScoresAndOwnScore` (`Shared.hpp:73`).
    CTSPRequestTopScoresAndOwnScore {
        /// C++ `loginToken`.
        login_token: u64,
        /// C++ `levelValidator`.
        level_validator: String,
    },
    /// C++ `CTSPStartedGame` (`Shared.hpp:74`).
    CTSPStartedGame {
        /// C++ `loginToken`.
        login_token: u64,
        /// C++ `levelValidator`.
        level_validator: String,
    },
    /// C++ `CTSPCompressedReplay` (`Shared.hpp:75`).
    CTSPCompressedReplay {
        /// C++ `loginToken`.
        login_token: u64,
        /// C++ `compressedReplayFile` — u64-BE byte count + raw zlib bytes
        /// (`Replay.cpp:467-487`).
        compressed_replay_file: oh_replay::CompressedReplayFile,
    },
    /// C++ `CTSPRequestServerStatus` (`Shared.hpp:76`).
    CTSPRequestServerStatus {
        /// C++ `loginToken`.
        login_token: u64,
    },
    /// C++ `CTSPReady` (`Shared.hpp:77`).
    CTSPReady {
        /// C++ `loginToken`.
        login_token: u64,
    },
}

impl ClientToServer {
    /// The wire type byte of each variant (the `PVClientToServer` variant
    /// index; `getPacketType`, `Shared.cpp:72-85`).
    pub const P_INVALID: u8 = 0;
    pub const P_ENCRYPTED_MSG: u8 = 1;
    pub const CTSP_HEARTBEAT: u8 = 2;
    pub const CTSP_DISCONNECT: u8 = 3;
    pub const CTS_PUBLIC_KEY: u8 = 4;
    pub const CTSP_REGISTER: u8 = 5;
    pub const CTSP_LOGIN: u8 = 6;
    pub const CTSP_LOGOUT: u8 = 7;
    pub const CTSP_DELETE_ACCOUNT: u8 = 8;
    pub const CTSP_REQUEST_TOP_SCORES: u8 = 9;
    pub const CTSP_REPLAY: u8 = 10;
    pub const CTSP_REQUEST_OWN_SCORE: u8 = 11;
    pub const CTSP_REQUEST_TOP_SCORES_AND_OWN_SCORE: u8 = 12;
    pub const CTSP_STARTED_GAME: u8 = 13;
    pub const CTSP_COMPRESSED_REPLAY: u8 = 14;
    pub const CTSP_REQUEST_SERVER_STATUS: u8 = 15;
    pub const CTSP_READY: u8 = 16;
}

/// C++ `hg::PVServerToClient` (`Shared.hpp:129-131`) — the server→client
/// packet variant. Variant order = wire type byte.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerToClient {
    /// C++ `PInvalid` (`Shared.hpp:47-49`) — type 0.
    PInvalid {
        /// C++ `error`.
        error: String,
    },
    /// C++ `PEncryptedMsg` (`Shared.hpp:52-58`) — type 1.
    PEncryptedMsg {
        /// C++ `nonce`.
        nonce: SodiumNonceArray,
        /// C++ `messageLength`.
        message_length: u64,
        /// C++ `ciphertextLength`.
        ciphertext_length: u64,
        /// C++ `ciphertext`.
        ciphertext: Vec<u8>,
    },
    /// C++ `STCPKick` (`Shared.hpp:106`).
    STCPKick,
    /// C++ `STCPPublicKey` (`Shared.hpp:107`).
    STCPPublicKey {
        /// C++ `key`.
        key: crate::sodium::SodiumPublicKeyArray,
    },
    /// C++ `STCPRegistrationSuccess` (`Shared.hpp:108`).
    STCPRegistrationSuccess,
    /// C++ `STCPRegistrationFailure` (`Shared.hpp:109`).
    STCPRegistrationFailure {
        /// C++ `error`.
        error: String,
    },
    /// C++ `STCPLoginSuccess` (`Shared.hpp:110`).
    STCPLoginSuccess {
        /// C++ `loginToken`.
        login_token: u64,
        /// C++ `loginName`.
        login_name: String,
    },
    /// C++ `STCPLoginFailure` (`Shared.hpp:111`).
    STCPLoginFailure {
        /// C++ `error`.
        error: String,
    },
    /// C++ `STCPLogoutSuccess` (`Shared.hpp:112`).
    STCPLogoutSuccess,
    /// C++ `STCPLogoutFailure` (`Shared.hpp:113`).
    STCPLogoutFailure,
    /// C++ `STCPDeleteAccountSuccess` (`Shared.hpp:114`).
    STCPDeleteAccountSuccess,
    /// C++ `STCPDeleteAccountFailure` (`Shared.hpp:115`).
    STCPDeleteAccountFailure {
        /// C++ `error`.
        error: String,
    },
    /// C++ `STCPTopScores` (`Shared.hpp:116`).
    STCPTopScores {
        /// C++ `levelValidator`.
        level_validator: String,
        /// C++ `scores`.
        scores: Vec<ProcessedScore>,
    },
    /// C++ `STCPOwnScore` (`Shared.hpp:117`).
    STCPOwnScore {
        /// C++ `levelValidator`.
        level_validator: String,
        /// C++ `score`.
        score: ProcessedScore,
    },
    /// C++ `STCPTopScoresAndOwnScore` (`Shared.hpp:118`).
    STCPTopScoresAndOwnScore {
        /// C++ `levelValidator`.
        level_validator: String,
        /// C++ `scores`.
        scores: Vec<ProcessedScore>,
        /// C++ `ownScore` (`std::optional`).
        own_score: Option<ProcessedScore>,
    },
    /// C++ `STCPServerStatus` (`Shared.hpp:119`).
    STCPServerStatus {
        /// C++ `protocolVersion` (a single byte on the wire; the C++ field
        /// is the `ProtocolVersion` = `uint8_t` alias).
        protocol_version: u8,
        /// C++ `gameVersion` (encoded as three `int32`s, `Shared.cpp:607-611`).
        game_version: GameVersion,
        /// C++ `supportedLevelValidators`.
        supported_level_validators: Vec<String>,
    },
}

impl ServerToClient {
    /// The wire type byte of each variant (the `PVServerToClient` variant
    /// index).
    pub const P_INVALID: u8 = 0;
    pub const P_ENCRYPTED_MSG: u8 = 1;
    pub const STC_KICK: u8 = 2;
    pub const STC_PUBLIC_KEY: u8 = 3;
    pub const STC_REGISTRATION_SUCCESS: u8 = 4;
    pub const STC_REGISTRATION_FAILURE: u8 = 5;
    pub const STC_LOGIN_SUCCESS: u8 = 6;
    pub const STC_LOGIN_FAILURE: u8 = 7;
    pub const STC_LOGOUT_SUCCESS: u8 = 8;
    pub const STC_LOGOUT_FAILURE: u8 = 9;
    pub const STC_DELETE_ACCOUNT_SUCCESS: u8 = 10;
    pub const STC_DELETE_ACCOUNT_FAILURE: u8 = 11;
    pub const STC_TOP_SCORES: u8 = 12;
    pub const STC_OWN_SCORE: u8 = 13;
    pub const STC_TOP_SCORES_AND_OWN_SCORE: u8 = 14;
    pub const STC_SERVER_STATUS: u8 = 15;
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// C++ `clearPacketAndEncodePreambleAndProtocolVersionAndGameVersion`
/// (`Shared.cpp:110-118`): clear the packet, then write the 7-byte header
/// (`'o' 'h'`, protocol version, major/minor/micro as `uint8_t`).
fn encode_header(p: &mut Packet) {
    p.reset();
    p.write_u8(PREAMBLE_1ST_BYTE);
    p.write_u8(PREAMBLE_2ND_BYTE);
    p.write_u8(PROTOCOL_VERSION);
    // C++ `encodeVersion` (`Shared.cpp:103-108`): `static_cast<uint8_t>` of
    // each component.
    p.write_u8(GAME_VERSION.major as u8);
    p.write_u8(GAME_VERSION.minor as u8);
    p.write_u8(GAME_VERSION.micro as u8);
}

/// C++ `encodeOHPacket` (`Shared.cpp:623-631`): the packet-type byte, then
/// every field in declaration order (`boost::pfr::for_each_field` over the
/// struct — declaration order; the `encodeField` overloads at
/// `Shared.cpp:537-620` give the per-type encodings).
fn encode_cts_oh_packet(p: &mut Packet, v: &ClientToServer) {
    match v {
        ClientToServer::PInvalid { error } => {
            p.write_u8(ClientToServer::P_INVALID);
            p.write_str(error);
        }
        ClientToServer::PEncryptedMsg {
            nonce,
            message_length,
            ciphertext_length,
            ciphertext,
        } => {
            p.write_u8(ClientToServer::P_ENCRYPTED_MSG);
            // C++ `std::array<T,N>` overload: element-wise `p << arr[i]`
            // (each 1 raw byte; `Shared.cpp:560-568`).
            p.append(nonce);
            p.write_u64(*message_length);
            p.write_u64(*ciphertext_length);
            // C++ `encodeFirstNVectorElements` (`Shared.cpp:503-513, 615-620`):
            // the first `ciphertextLength` bytes of the vector. Well-formed
            // envelopes always have exactly `ciphertextLength` bytes
            // (`makeEncryptedPacketImpl` resizes to it, `Shared.cpp:705-706`).
            debug_assert!(
                ciphertext.len() >= *ciphertext_length as usize,
                "ciphertext longer than ciphertext_length"
            );
            p.append(&ciphertext[..(*ciphertext_length as usize).min(ciphertext.len())]);
        }
        ClientToServer::CTSPHeartbeat => p.write_u8(ClientToServer::CTSP_HEARTBEAT),
        ClientToServer::CTSPDisconnect => p.write_u8(ClientToServer::CTSP_DISCONNECT),
        ClientToServer::CTSPPublicKey { key } => {
            p.write_u8(ClientToServer::CTS_PUBLIC_KEY);
            p.append(key);
        }
        ClientToServer::CTSPRegister {
            steam_id,
            name,
            password_hash,
        } => {
            p.write_u8(ClientToServer::CTSP_REGISTER);
            p.write_u64(*steam_id);
            p.write_str(name);
            p.write_str(password_hash);
        }
        ClientToServer::CTSPLogin {
            steam_id,
            name,
            password_hash,
        } => {
            p.write_u8(ClientToServer::CTSP_LOGIN);
            p.write_u64(*steam_id);
            p.write_str(name);
            p.write_str(password_hash);
        }
        ClientToServer::CTSPLogout { steam_id } => {
            p.write_u8(ClientToServer::CTSP_LOGOUT);
            p.write_u64(*steam_id);
        }
        ClientToServer::CTSPDeleteAccount {
            steam_id,
            password_hash,
        } => {
            p.write_u8(ClientToServer::CTSP_DELETE_ACCOUNT);
            p.write_u64(*steam_id);
            p.write_str(password_hash);
        }
        ClientToServer::CTSPRequestTopScores {
            login_token,
            level_validator,
        } => {
            p.write_u8(ClientToServer::CTSP_REQUEST_TOP_SCORES);
            p.write_u64(*login_token);
            p.write_str(level_validator);
        }
        ClientToServer::CTSPReplay {
            login_token,
            replay_file,
        } => {
            p.write_u8(ClientToServer::CTSP_REPLAY);
            p.write_u64(*login_token);
            // C++ `(void)rf.serialize_to_packet(p)` (`Shared.cpp:591-596`).
            replay_file.serialize_to_packet(p);
        }
        ClientToServer::CTSPRequestOwnScore {
            login_token,
            level_validator,
        } => {
            p.write_u8(ClientToServer::CTSP_REQUEST_OWN_SCORE);
            p.write_u64(*login_token);
            p.write_str(level_validator);
        }
        ClientToServer::CTSPRequestTopScoresAndOwnScore {
            login_token,
            level_validator,
        } => {
            p.write_u8(ClientToServer::CTSP_REQUEST_TOP_SCORES_AND_OWN_SCORE);
            p.write_u64(*login_token);
            p.write_str(level_validator);
        }
        ClientToServer::CTSPStartedGame {
            login_token,
            level_validator,
        } => {
            p.write_u8(ClientToServer::CTSP_STARTED_GAME);
            p.write_u64(*login_token);
            p.write_str(level_validator);
        }
        ClientToServer::CTSPCompressedReplay {
            login_token,
            compressed_replay_file,
        } => {
            p.write_u8(ClientToServer::CTSP_COMPRESSED_REPLAY);
            p.write_u64(*login_token);
            // C++ `(void)crf.serialize_to_packet(p)` (`Shared.cpp:598-605`).
            compressed_replay_file.serialize_to_packet(p);
        }
        ClientToServer::CTSPRequestServerStatus { login_token } => {
            p.write_u8(ClientToServer::CTSP_REQUEST_SERVER_STATUS);
            p.write_u64(*login_token);
        }
        ClientToServer::CTSPReady { login_token } => {
            p.write_u8(ClientToServer::CTSP_READY);
            p.write_u64(*login_token);
        }
    }
}

/// C++ `encodeOHPacket` for the server→client variant.
fn encode_stc_oh_packet(p: &mut Packet, v: &ServerToClient) {
    match v {
        ServerToClient::PInvalid { error } => {
            p.write_u8(ServerToClient::P_INVALID);
            p.write_str(error);
        }
        ServerToClient::PEncryptedMsg {
            nonce,
            message_length,
            ciphertext_length,
            ciphertext,
        } => {
            p.write_u8(ServerToClient::P_ENCRYPTED_MSG);
            p.append(nonce);
            p.write_u64(*message_length);
            p.write_u64(*ciphertext_length);
            debug_assert!(
                ciphertext.len() >= *ciphertext_length as usize,
                "ciphertext longer than ciphertext_length"
            );
            p.append(&ciphertext[..(*ciphertext_length as usize).min(ciphertext.len())]);
        }
        ServerToClient::STCPKick => p.write_u8(ServerToClient::STC_KICK),
        ServerToClient::STCPPublicKey { key } => {
            p.write_u8(ServerToClient::STC_PUBLIC_KEY);
            p.append(key);
        }
        ServerToClient::STCPRegistrationSuccess => {
            p.write_u8(ServerToClient::STC_REGISTRATION_SUCCESS);
        }
        ServerToClient::STCPRegistrationFailure { error } => {
            p.write_u8(ServerToClient::STC_REGISTRATION_FAILURE);
            p.write_str(error);
        }
        ServerToClient::STCPLoginSuccess {
            login_token,
            login_name,
        } => {
            p.write_u8(ServerToClient::STC_LOGIN_SUCCESS);
            p.write_u64(*login_token);
            p.write_str(login_name);
        }
        ServerToClient::STCPLoginFailure { error } => {
            p.write_u8(ServerToClient::STC_LOGIN_FAILURE);
            p.write_str(error);
        }
        ServerToClient::STCPLogoutSuccess => {
            p.write_u8(ServerToClient::STC_LOGOUT_SUCCESS);
        }
        ServerToClient::STCPLogoutFailure => {
            p.write_u8(ServerToClient::STC_LOGOUT_FAILURE);
        }
        ServerToClient::STCPDeleteAccountSuccess => {
            p.write_u8(ServerToClient::STC_DELETE_ACCOUNT_SUCCESS);
        }
        ServerToClient::STCPDeleteAccountFailure { error } => {
            p.write_u8(ServerToClient::STC_DELETE_ACCOUNT_FAILURE);
            p.write_str(error);
        }
        ServerToClient::STCPTopScores {
            level_validator,
            scores,
        } => {
            p.write_u8(ServerToClient::STC_TOP_SCORES);
            p.write_str(level_validator);
            encode_scores(p, scores);
        }
        ServerToClient::STCPOwnScore {
            level_validator,
            score,
        } => {
            p.write_u8(ServerToClient::STC_OWN_SCORE);
            p.write_str(level_validator);
            encode_score(p, score);
        }
        ServerToClient::STCPTopScoresAndOwnScore {
            level_validator,
            scores,
            own_score,
        } => {
            p.write_u8(ServerToClient::STC_TOP_SCORES_AND_OWN_SCORE);
            p.write_str(level_validator);
            encode_scores(p, scores);
            // C++ `encodeField` for `std::optional` (`Shared.cpp:580-589`):
            // the `has_value` flag, then the value when set.
            p.write_bool(own_score.is_some());
            if let Some(score) = own_score {
                encode_score(p, score);
            }
        }
        ServerToClient::STCPServerStatus {
            protocol_version,
            game_version,
            supported_level_validators,
        } => {
            p.write_u8(ServerToClient::STC_SERVER_STATUS);
            p.write_u8(*protocol_version);
            // C++ `encodeField` for `GameVersion` (`Shared.cpp:606-612`):
            // three `int32`s.
            p.write_i32(game_version.major);
            p.write_i32(game_version.minor);
            p.write_i32(game_version.micro);
            // C++ `encodeField` for `std::vector` (`Shared.cpp:569-578`):
            // u64 count + elements.
            p.write_u64(supported_level_validators.len() as u64);
            for validator in supported_level_validators {
                p.write_str(validator);
            }
        }
    }
}

/// C++ `ProcessedScore` field encoding (declaration order: `position`
/// u32, `userName` string, `scoreTimestamp` u64, `scoreValue` double).
fn encode_score(p: &mut Packet, score: &ProcessedScore) {
    p.write_u32(score.position);
    p.write_str(&score.user_name);
    p.write_u64(score.score_timestamp);
    p.write_f64(score.score_value);
}

/// C++ `std::vector<ProcessedScore>` encoding (u64-BE count + elements).
fn encode_scores(p: &mut Packet, scores: &[ProcessedScore]) {
    p.write_u64(scores.len() as u64);
    for score in scores {
        encode_score(p, score);
    }
}

/// C++ `makeClientToServerPacket` (`Shared.cpp:725-729`).
pub fn make_client_to_server_packet(p: &mut Packet, packet: &ClientToServer) {
    encode_header(p);
    encode_cts_oh_packet(p, packet);
}

/// C++ `makeClientToServerEncryptedPacket`
/// (`Shared.cpp:741-748`) over `makeEncryptedPacketImpl`
/// (`Shared.cpp:690-719`): encode the inner packet (type byte + fields,
/// **no header**), generate the nonce, seal it with `key_transmit`, and
/// write the `PEncryptedMsg` envelope (`makeClientToServerPacket` of the
/// envelope — header + type 1 + nonce + lengths + ciphertext).
///
/// The C++ returns `bool` (false only if `crypto_secretbox_easy` fails,
/// which happens only for `mlen > SIZETYPE_MAX`, 4 GiB); the port returns
/// `()` — packet-sized inputs cannot fail.
pub fn make_client_to_server_encrypted_packet(
    key_transmit: &SodiumTransmitKeyArray,
    p: &mut Packet,
    packet: &ClientToServer,
) {
    // C++ `getStaticPacketBuffer()` (thread-local scratch, `Shared.cpp:527-531`).
    let mut inner = Packet::new();
    encode_cts_oh_packet(&mut inner, packet);

    let nonce = crate::sodium::generate_nonce();
    let message_length = inner.size() as u64;
    let ciphertext = secretbox_seal(inner.data(), &nonce, key_transmit);
    make_client_to_server_packet(
        p,
        &ClientToServer::PEncryptedMsg {
            nonce,
            message_length,
            ciphertext_length: get_ciphertext_length(message_length),
            ciphertext,
        },
    );
}

/// C++ `makeServerToClientPacket` (`Shared.cpp:873-877`).
pub fn make_server_to_client_packet(p: &mut Packet, packet: &ServerToClient) {
    encode_header(p);
    encode_stc_oh_packet(p, packet);
}

/// C++ `makeServerToClientEncryptedPacket` (`Shared.cpp:889-896`). See
/// [`make_client_to_server_encrypted_packet`] for the `bool` → `()` note.
pub fn make_server_to_client_encrypted_packet(
    key_transmit: &SodiumTransmitKeyArray,
    p: &mut Packet,
    packet: &ServerToClient,
) {
    let mut inner = Packet::new();
    encode_stc_oh_packet(&mut inner, packet);

    let nonce = crate::sodium::generate_nonce();
    let message_length = inner.size() as u64;
    let ciphertext = secretbox_seal(inner.data(), &nonce, key_transmit);
    make_server_to_client_packet(
        p,
        &ServerToClient::PEncryptedMsg {
            nonce,
            message_length,
            ciphertext_length: get_ciphertext_length(message_length),
            ciphertext,
        },
    );
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// The SFML-packet read state with the fork's poison semantics
/// (`Packet.cpp:589-594`): once a read fails, every later read fails too.
/// [`Packet`] itself only models the "read past the end" failure
/// (position unchanged, retry possible), so the poison flag lives here —
/// the decode path is exactly the C++ code that relies on it.
struct Reader<'a> {
    p: &'a mut Packet,
    valid: bool,
}

impl<'a> Reader<'a> {
    fn new(p: &'a mut Packet) -> Self {
        Self { p, valid: true }
    }

    fn u8(&mut self) -> Option<u8> {
        if !self.valid {
            return None;
        }
        match self.p.read_u8() {
            Some(v) => Some(v),
            None => {
                self.valid = false;
                None
            }
        }
    }

    fn u32(&mut self) -> Option<u32> {
        if !self.valid {
            return None;
        }
        match self.p.read_u32() {
            Some(v) => Some(v),
            None => {
                self.valid = false;
                None
            }
        }
    }

    fn u64(&mut self) -> Option<u64> {
        if !self.valid {
            return None;
        }
        match self.p.read_u64() {
            Some(v) => Some(v),
            None => {
                self.valid = false;
                None
            }
        }
    }

    fn i32(&mut self) -> Option<i32> {
        if !self.valid {
            return None;
        }
        match self.p.read_i32() {
            Some(v) => Some(v),
            None => {
                self.valid = false;
                None
            }
        }
    }

    fn f64(&mut self) -> Option<f64> {
        if !self.valid {
            return None;
        }
        match self.p.read_f64() {
            Some(v) => Some(v),
            None => {
                self.valid = false;
                None
            }
        }
    }

    fn bool_(&mut self) -> Option<bool> {
        if !self.valid {
            return None;
        }
        match self.p.read_bool() {
            Some(v) => Some(v),
            None => {
                self.valid = false;
                None
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        if !self.valid {
            return None;
        }
        match self.p.read_str() {
            Some(v) => Some(v),
            None => {
                self.valid = false;
                None
            }
        }
    }
}

/// C++ `extractInto` single-object failure line (`Shared.cpp:130-172`, the failure line at 139).
const ERR_SINGLE_OBJECT: &str = "Error extracting single object\n";

/// C++ `makeExtractAllMembers` per-field failure line — note the trailing
/// space before the newline (`Shared.cpp:813-815`).
fn err_field(error_oss: &mut String, index: usize) {
    error_oss.push_str(&format!("Error decoding field #{index} \n"));
}

/// C++ `extractInto` for `std::array<T,N>` failure line
/// (`Shared.cpp:175-199`).
fn err_array_element(error_oss: &mut String, index: usize) {
    error_oss.push_str(&format!(
        "Error extracting array element at index '{index}'\n"
    ));
}

/// C++ `extractInto` for `std::vector<ProcessedScore>`
/// (`Shared.cpp:201-232`): u64-BE count, then the elements in order.
/// On the first failed element the C++ returns `false` (no per-element
/// continuation inside the vector). The element count is bounds-checked
/// against the remaining bytes — a documented deviation from the C++
/// `resize(count)` (hostile-input territory; see the module docs).
fn extract_scores(r: &mut Reader, error_oss: &mut String) -> Option<Vec<ProcessedScore>> {
    let count = match r.u64() {
        Some(n) => n,
        None => {
            error_oss.push_str("Error extracting vector size\n");
            return None;
        }
    };
    if count > r.p.available() as u64 {
        // Every element read would fail; the C++ would `resize` to the
        // attacker-controlled count first (terminate) — fail cleanly here.
        error_oss.push_str("Error extracting vector size\n");
        return None;
    }
    let mut out = Vec::new();
    for i in 0..count {
        match extract_score(r, error_oss) {
            Some(score) => out.push(score),
            None => {
                error_oss.push_str(&format!("Error extracting vector element at index '{i}'\n"));
                return None;
            }
        }
    }
    Some(out)
}

/// C++ `ProcessedScore` extraction (declaration order: `position` u32,
/// `userName` string, `scoreTimestamp` u64, `scoreValue` double) — each
/// failed read appends the single-object line (the C++ `extractInto` for
/// the struct recurses into its fields via `boost::pfr::for_each_field`,
/// `Shared.cpp:146-163`; no extra per-field wrapper is printed at this
/// nesting level).
fn extract_score(r: &mut Reader, error_oss: &mut String) -> Option<ProcessedScore> {
    let mut ok = true;
    let position = match r.u32() {
        Some(v) => v,
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            ok = false;
            0
        }
    };
    let user_name = match r.string() {
        Some(v) => v,
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            ok = false;
            String::new()
        }
    };
    let score_timestamp = match r.u64() {
        Some(v) => v,
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            ok = false;
            0
        }
    };
    let score_value = match r.f64() {
        Some(v) => v,
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            ok = false;
            0.0
        }
    };
    if !ok {
        return None;
    }
    Some(ProcessedScore {
        position,
        user_name,
        score_timestamp,
        score_value,
    })
}

/// C++ `extractInto` for `std::optional<T>` (`Shared.cpp:235-265`):
/// 1-byte set flag, then the value when set.
fn extract_optional_score(
    r: &mut Reader,
    error_oss: &mut String,
) -> Option<Option<ProcessedScore>> {
    let set = match r.bool_() {
        Some(v) => v,
        None => {
            error_oss.push_str("Error extracting optional set flag\n");
            return None;
        }
    };
    if !set {
        return Some(None);
    }
    match extract_score(r, error_oss) {
        Some(score) => Some(Some(score)),
        None => {
            error_oss.push_str("Error extracting optional element\n");
            None
        }
    }
}

/// C++ `Extractor<hg::GameVersion>` (`Shared.cpp:304-334`): three `int32`s.
fn extract_game_version(r: &mut Reader, error_oss: &mut String) -> Option<GameVersion> {
    let major = match r.i32() {
        Some(v) => v,
        None => {
            error_oss.push_str("Error deserializing major version\n");
            return None;
        }
    };
    let minor = match r.i32() {
        Some(v) => v,
        None => {
            error_oss.push_str("Error deserializing minor version\n");
            return None;
        }
    };
    let micro = match r.i32() {
        Some(v) => v,
        None => {
            error_oss.push_str("Error deserializing micro version\n");
            return None;
        }
    };
    Some(GameVersion::new(major, minor, micro))
}

/// C++ `verifyReceivedPacketPreambleAndProtocolVersionAndGameVersion`
/// (`Shared.cpp:469-486`): the 1st/2nd preamble byte and the protocol
/// version must MATCH; the game-version bytes are skipped (read, not
/// checked — "Game version is currently ignored").
///
/// The mismatch line mirrors the C++ `ostream` formatting exactly —
/// including that the `uint8_t` values stream as their *characters* and
/// that the value's opening quote is never closed
/// (`Shared.cpp:404-419`):
/// `"Error, <name> has value '<v>, which doesn't match expected value '<e>'\n"`
/// (the protocol-version mismatch embeds a NUL byte, the expected value 0).
fn verify_preamble(r: &mut Reader, error_oss: &mut String) -> bool {
    // Preamble 1st byte.
    match r.u8() {
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            error_oss.push_str("Error extracting preamble 1st byte\n");
            return false;
        }
        Some(b) => {
            if b != PREAMBLE_1ST_BYTE {
                mismatch_line(error_oss, "preamble 1st byte", b, PREAMBLE_1ST_BYTE);
                return false;
            }
        }
    }
    // Preamble 2nd byte (the C++ name is the typo'd "2st",
    // `Shared.cpp:479-481` — pinned by the oracle).
    match r.u8() {
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            error_oss.push_str("Error extracting preamble 2st byte\n");
            return false;
        }
        Some(b) => {
            if b != PREAMBLE_2ND_BYTE {
                mismatch_line(error_oss, "preamble 2st byte", b, PREAMBLE_2ND_BYTE);
                return false;
            }
        }
    }
    // Protocol version.
    match r.u8() {
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            error_oss.push_str("Error extracting protocol version\n");
            return false;
        }
        Some(b) => {
            if b != PROTOCOL_VERSION {
                mismatch_line(error_oss, "protocol version", b, PROTOCOL_VERSION);
                return false;
            }
        }
    }
    // Game version: skipped (read failure only).
    for name in ["major version", "minor version", "micro version"] {
        if r.u8().is_none() {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            error_oss.push_str(&format!("Error extracting {name}\n"));
            return false;
        }
    }
    true
}

/// The `AdvancedMatcher::extractAndMatchOrPrintError` mismatch line
/// (`Shared.cpp:404-419`), byte-exact: the C++ streams `unsigned char`
/// values as single characters (a raw byte is appended —
/// `from_utf8_unchecked` mirrors that for bytes outside ASCII).
fn mismatch_line(error_oss: &mut String, name: &str, value: u8, expected: u8) {
    error_oss.push_str("Error, ");
    error_oss.push_str(name);
    error_oss.push_str(" has value '");
    error_oss.push_str(unsafe { std::str::from_utf8_unchecked(&[value]) });
    error_oss.push_str(", which doesn't match expected value '");
    error_oss.push_str(unsafe { std::str::from_utf8_unchecked(&[expected]) });
    error_oss.push_str("'\n");
}

/// C++ `extractPacketType` (`Shared.cpp:489-500`): the 1-byte type after
/// the header.
fn extract_packet_type(r: &mut Reader, error_oss: &mut String) -> Option<u8> {
    match r.u8() {
        Some(t) => Some(t),
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            error_oss.push_str("Error extracting packet type\n");
            None
        }
    }
}

/// C++ `decryptPacket` (`Shared.cpp:634-686`) + `decodeEncryptedPacket`
/// (`Shared.cpp:824-837`): nonce (24 bytes) + `messageLength` +
/// `ciphertextLength` + the ciphertext bytes, then
/// `crypto_secretbox_open_easy`. Returns the decrypted inner packet.
fn decrypt_packet(
    r: &mut Reader,
    error_oss: &mut String,
    key_receive: &SodiumReceiveKeyArray,
) -> Option<Packet> {
    // Nonce — `Extractor<std::array<T, N>>` (`Shared.cpp:175-199`).
    let mut nonce = [0u8; SODIUM_NONCE_BYTES];
    for (i, slot) in nonce.iter_mut().enumerate() {
        match r.u8() {
            Some(b) => *slot = b,
            None => {
                err_array_element(error_oss, i);
                error_oss.push_str("Error decoding client nonce\n");
                return None;
            }
        }
    }
    // Message length.
    let message_length = match r.u64() {
        Some(v) => v,
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            error_oss.push_str("Error decoding client message length\n");
            return None;
        }
    };
    // Ciphertext length.
    let ciphertext_length = match r.u64() {
        Some(v) => v,
        None => {
            error_oss.push_str(ERR_SINGLE_OBJECT);
            error_oss.push_str("Error decoding client ciphertext length\n");
            return None;
        }
    };
    // Ciphertext — the C++ resizes to `ciphertextLength` first (a hostile
    // length `terminate`s); bounds-check instead, and report the failure at
    // the index the C++ read loop would reach (the first byte past the end).
    if ciphertext_length > r.p.available() as u64 {
        error_oss.push_str(&format!(
            "Error decoding client ciphertext at index '{}'\n",
            r.p.available()
        ));
        return None;
    }
    let mut ciphertext = Vec::with_capacity(ciphertext_length as usize);
    for _ in 0..ciphertext_length {
        match r.u8() {
            Some(b) => ciphertext.push(b),
            None => unreachable!("bounds-checked above"),
        }
    }
    match secretbox_open(&ciphertext, &nonce, key_receive) {
        Some(message) => {
            // C++: `message.resize(messageLength)` then
            // `decryptedPacket.append(message.data(), messageLength)`
            // (`Shared.cpp:674-684`) — exactly `messageLength` bytes, zero
            // padding when the declared length exceeds the decrypted one.
            let mut inner = message;
            if (inner.len() as u64) < message_length {
                inner.resize(message_length as usize, 0);
            }
            let mut packet = Packet::new();
            packet.append(&inner[..message_length as usize]);
            Some(packet)
        }
        None => {
            error_oss.push_str("Failure decrypting encrypted client message\n");
            None
        }
    }
}

/// C++ `decodeClientToServerPacketInner`
/// (`Shared.cpp:844-855` + the `INJECT_COMMON_PACKET_HANDLING_CODE`
/// macro at `Shared.cpp:773-792`). `key_receive == None` only fails the encrypted
/// branch (C++ passes the pointer straight to `decodeEncryptedPacket`).
fn decode_cts_inner(
    key_receive: Option<&SodiumReceiveKeyArray>,
    r: &mut Reader,
    error_oss: &mut String,
) -> ClientToServer {
    let pt = match extract_packet_type(r, error_oss) {
        Some(t) => t,
        None => {
            return ClientToServer::PInvalid {
                error: error_oss.clone(),
            }
        }
    };

    if pt == ClientToServer::P_ENCRYPTED_MSG {
        let Some(key) = key_receive else {
            error_oss.push_str("Cannot decode encrypted message without receive key\n");
            return ClientToServer::PInvalid {
                error: error_oss.clone(),
            };
        };
        // C++ re-decodes the decrypted packet via the *inner* function
        // (no preamble re-check — the inner packet is type byte + fields
        // only, `Shared.cpp:782-790`).
        match decrypt_packet(r, error_oss, key) {
            Some(mut decrypted) => {
                let mut r2 = Reader::new(&mut decrypted);
                decode_cts_inner(Some(key), &mut r2, error_oss)
            }
            None => ClientToServer::PInvalid {
                error: error_oss.clone(),
            },
        }
    } else {
        // The `HANDLE_PACKET` arms (`Shared.cpp:758-771`, dispatched at 850-851`): every concrete
        // CTSP type; `PInvalid` has no arm (type 0 falls through to
        // "unknown type"). `makeExtractAllMembers` (`Shared.cpp:801-821`)
        // reads every field, appending `Error decoding field #i \n` per
        // failed field, and KEEPS READING after a failure (the packet is
        // now poisoned, so the later fields fail too).
        let mut success = true;
        let mut steam_id: u64 = 0;
        let mut login_token: u64 = 0;
        let mut name = String::new();
        let mut password_hash = String::new();
        let mut level_validator = String::new();
        let mut key = [0u8; crate::sodium::SODIUM_PUBLIC_KEY_BYTES];
        let mut replay_file = oh_replay::ReplayFile::default();
        let mut compressed_replay_file = oh_replay::CompressedReplayFile::default();

        match pt {
            ClientToServer::CTSP_HEARTBEAT | ClientToServer::CTSP_DISCONNECT => {}
            ClientToServer::CTS_PUBLIC_KEY => {
                for (i, slot) in key.iter_mut().enumerate() {
                    match r.u8() {
                        Some(b) => *slot = b,
                        None => {
                            err_array_element(error_oss, i);
                            err_field(error_oss, 0);
                            success = false;
                            break;
                        }
                    }
                }
            }
            ClientToServer::CTSP_REGISTER | ClientToServer::CTSP_LOGIN => {
                steam_id = match r.u64() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        0
                    }
                };
                name = match r.string() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 1);
                        success = false;
                        String::new()
                    }
                };
                password_hash = match r.string() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 2);
                        success = false;
                        String::new()
                    }
                };
            }
            ClientToServer::CTSP_LOGOUT => {
                steam_id = match r.u64() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        0
                    }
                };
            }
            ClientToServer::CTSP_DELETE_ACCOUNT => {
                steam_id = match r.u64() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        0
                    }
                };
                password_hash = match r.string() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 1);
                        success = false;
                        String::new()
                    }
                };
            }
            ClientToServer::CTSP_REQUEST_TOP_SCORES
            | ClientToServer::CTSP_REQUEST_OWN_SCORE
            | ClientToServer::CTSP_REQUEST_TOP_SCORES_AND_OWN_SCORE
            | ClientToServer::CTSP_STARTED_GAME => {
                login_token = match r.u64() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        0
                    }
                };
                level_validator = match r.string() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 1);
                        success = false;
                        String::new()
                    }
                };
            }
            ClientToServer::CTSP_REPLAY => {
                login_token = match r.u64() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        0
                    }
                };
                // C++ `Extractor<hg::replay_file>` (`Shared.cpp:268-283`);
                // `deserialize_from_packet` (`Replay.cpp:397-423`):
                // u64-BE count + raw file bytes.
                if !replay_file.deserialize_from_packet(r.p) {
                    error_oss.push_str("Error deserializing replay\n");
                    err_field(error_oss, 1);
                    success = false;
                    r.valid = false;
                }
            }
            ClientToServer::CTSP_COMPRESSED_REPLAY => {
                login_token = match r.u64() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        0
                    }
                };
                // C++ `Extractor<hg::compressed_replay_file>`
                // (`Shared.cpp:286-301`).
                if !compressed_replay_file.deserialize_from_packet(r.p) {
                    error_oss.push_str("Error deserializing compressed replay\n");
                    err_field(error_oss, 1);
                    success = false;
                    r.valid = false;
                }
            }
            ClientToServer::CTSP_REQUEST_SERVER_STATUS | ClientToServer::CTSP_READY => {
                login_token = match r.u64() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        0
                    }
                };
            }
            _ => {
                // `Shared.cpp:850-853`: unknown type byte.
                error_oss.push_str(&format!("Unknown packet type '{pt}'\n"));
                return ClientToServer::PInvalid {
                    error: error_oss.clone(),
                };
            }
        }

        if !success {
            return ClientToServer::PInvalid {
                error: error_oss.clone(),
            };
        }

        match pt {
            ClientToServer::CTSP_HEARTBEAT => ClientToServer::CTSPHeartbeat,
            ClientToServer::CTSP_DISCONNECT => ClientToServer::CTSPDisconnect,
            ClientToServer::CTS_PUBLIC_KEY => ClientToServer::CTSPPublicKey { key },
            ClientToServer::CTSP_REGISTER => ClientToServer::CTSPRegister {
                steam_id,
                name,
                password_hash,
            },
            ClientToServer::CTSP_LOGIN => ClientToServer::CTSPLogin {
                steam_id,
                name,
                password_hash,
            },
            ClientToServer::CTSP_LOGOUT => ClientToServer::CTSPLogout { steam_id },
            ClientToServer::CTSP_DELETE_ACCOUNT => ClientToServer::CTSPDeleteAccount {
                steam_id,
                password_hash,
            },
            ClientToServer::CTSP_REQUEST_TOP_SCORES => ClientToServer::CTSPRequestTopScores {
                login_token,
                level_validator,
            },
            ClientToServer::CTSP_REPLAY => ClientToServer::CTSPReplay {
                login_token,
                replay_file,
            },
            ClientToServer::CTSP_REQUEST_OWN_SCORE => ClientToServer::CTSPRequestOwnScore {
                login_token,
                level_validator,
            },
            ClientToServer::CTSP_REQUEST_TOP_SCORES_AND_OWN_SCORE => {
                ClientToServer::CTSPRequestTopScoresAndOwnScore {
                    login_token,
                    level_validator,
                }
            }
            ClientToServer::CTSP_STARTED_GAME => ClientToServer::CTSPStartedGame {
                login_token,
                level_validator,
            },
            ClientToServer::CTSP_COMPRESSED_REPLAY => ClientToServer::CTSPCompressedReplay {
                login_token,
                compressed_replay_file,
            },
            ClientToServer::CTSP_REQUEST_SERVER_STATUS => {
                ClientToServer::CTSPRequestServerStatus { login_token }
            }
            ClientToServer::CTSP_READY => ClientToServer::CTSPReady { login_token },
            _ => unreachable!("handled above"),
        }
    }
}

/// C++ `decodeServerToClientPacketInner`
/// (`Shared.cpp:907-918` + the shared macro).
fn decode_stc_inner(
    key_receive: Option<&SodiumReceiveKeyArray>,
    r: &mut Reader,
    error_oss: &mut String,
) -> ServerToClient {
    let pt = match extract_packet_type(r, error_oss) {
        Some(t) => t,
        None => {
            return ServerToClient::PInvalid {
                error: error_oss.clone(),
            }
        }
    };

    if pt == ServerToClient::P_ENCRYPTED_MSG {
        let Some(key) = key_receive else {
            error_oss.push_str("Cannot decode encrypted message without receive key\n");
            return ServerToClient::PInvalid {
                error: error_oss.clone(),
            };
        };
        match decrypt_packet(r, error_oss, key) {
            Some(mut decrypted) => {
                let mut r2 = Reader::new(&mut decrypted);
                decode_stc_inner(Some(key), &mut r2, error_oss)
            }
            None => ServerToClient::PInvalid {
                error: error_oss.clone(),
            },
        }
    } else {
        let mut success = true;
        let mut error = String::new();
        let mut login_token: u64 = 0;
        let mut login_name = String::new();
        let mut level_validator = String::new();
        let mut key = [0u8; crate::sodium::SODIUM_PUBLIC_KEY_BYTES];
        let mut scores: Vec<ProcessedScore> = Vec::new();
        let mut score = ProcessedScore {
            position: 0,
            user_name: String::new(),
            score_timestamp: 0,
            score_value: 0.0,
        };
        let mut own_score: Option<ProcessedScore> = None;
        let mut protocol_version: u8 = 0;
        let mut game_version = GameVersion::default();
        let mut validators: Vec<String> = Vec::new();

        match pt {
            ServerToClient::STC_KICK
            | ServerToClient::STC_REGISTRATION_SUCCESS
            | ServerToClient::STC_LOGOUT_SUCCESS
            | ServerToClient::STC_LOGOUT_FAILURE
            | ServerToClient::STC_DELETE_ACCOUNT_SUCCESS => {}
            ServerToClient::STC_PUBLIC_KEY => {
                for (i, slot) in key.iter_mut().enumerate() {
                    match r.u8() {
                        Some(b) => *slot = b,
                        None => {
                            err_array_element(error_oss, i);
                            err_field(error_oss, 0);
                            success = false;
                            break;
                        }
                    }
                }
            }
            ServerToClient::STC_REGISTRATION_FAILURE
            | ServerToClient::STC_LOGIN_FAILURE
            | ServerToClient::STC_DELETE_ACCOUNT_FAILURE => {
                error = match r.string() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        String::new()
                    }
                };
            }
            ServerToClient::STC_LOGIN_SUCCESS => {
                login_token = match r.u64() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        0
                    }
                };
                login_name = match r.string() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 1);
                        success = false;
                        String::new()
                    }
                };
            }
            ServerToClient::STC_TOP_SCORES | ServerToClient::STC_TOP_SCORES_AND_OWN_SCORE => {
                level_validator = match r.string() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        String::new()
                    }
                };
                match extract_scores(r, error_oss) {
                    Some(s) => scores = s,
                    None => {
                        err_field(error_oss, 1);
                        success = false;
                    }
                }
                if pt == ServerToClient::STC_TOP_SCORES_AND_OWN_SCORE {
                    match extract_optional_score(r, error_oss) {
                        Some(v) => own_score = v,
                        None => {
                            err_field(error_oss, 2);
                            success = false;
                        }
                    }
                }
            }
            ServerToClient::STC_OWN_SCORE => {
                level_validator = match r.string() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        String::new()
                    }
                };
                match extract_score(r, error_oss) {
                    Some(s) => score = s,
                    None => {
                        err_field(error_oss, 1);
                        success = false;
                    }
                }
            }
            ServerToClient::STC_SERVER_STATUS => {
                protocol_version = match r.u8() {
                    Some(v) => v,
                    None => {
                        error_oss.push_str(ERR_SINGLE_OBJECT);
                        err_field(error_oss, 0);
                        success = false;
                        0
                    }
                };
                match extract_game_version(r, error_oss) {
                    Some(v) => game_version = v,
                    None => {
                        err_field(error_oss, 1);
                        success = false;
                    }
                }
                // `std::vector<std::string>` — u64-BE count + strings.
                let count = match r.u64() {
                    Some(n) => n,
                    None => {
                        error_oss.push_str("Error extracting vector size\n");
                        err_field(error_oss, 2);
                        success = false;
                        0
                    }
                };
                if count > r.p.available() as u64 {
                    error_oss.push_str("Error extracting vector size\n");
                    err_field(error_oss, 2);
                    success = false;
                } else {
                    for i in 0..count {
                        match r.string() {
                            Some(s) => validators.push(s),
                            None => {
                                error_oss.push_str(&format!(
                                    "Error extracting vector element at index '{i}'\n"
                                ));
                                err_field(error_oss, 2);
                                success = false;
                                break;
                            }
                        }
                    }
                }
            }
            _ => {
                error_oss.push_str(&format!("Unknown packet type '{pt}'\n"));
                return ServerToClient::PInvalid {
                    error: error_oss.clone(),
                };
            }
        }

        if !success {
            return ServerToClient::PInvalid {
                error: error_oss.clone(),
            };
        }

        match pt {
            ServerToClient::STC_KICK => ServerToClient::STCPKick,
            ServerToClient::STC_PUBLIC_KEY => ServerToClient::STCPPublicKey { key },
            ServerToClient::STC_REGISTRATION_SUCCESS => ServerToClient::STCPRegistrationSuccess,
            ServerToClient::STC_REGISTRATION_FAILURE => {
                ServerToClient::STCPRegistrationFailure { error }
            }
            ServerToClient::STC_LOGIN_SUCCESS => ServerToClient::STCPLoginSuccess {
                login_token,
                login_name,
            },
            ServerToClient::STC_LOGIN_FAILURE => ServerToClient::STCPLoginFailure { error },
            ServerToClient::STC_LOGOUT_SUCCESS => ServerToClient::STCPLogoutSuccess,
            ServerToClient::STC_LOGOUT_FAILURE => ServerToClient::STCPLogoutFailure,
            ServerToClient::STC_DELETE_ACCOUNT_SUCCESS => ServerToClient::STCPDeleteAccountSuccess,
            ServerToClient::STC_DELETE_ACCOUNT_FAILURE => {
                ServerToClient::STCPDeleteAccountFailure { error }
            }
            ServerToClient::STC_TOP_SCORES => ServerToClient::STCPTopScores {
                level_validator,
                scores,
            },
            ServerToClient::STC_OWN_SCORE => ServerToClient::STCPOwnScore {
                level_validator,
                score,
            },
            ServerToClient::STC_TOP_SCORES_AND_OWN_SCORE => {
                ServerToClient::STCPTopScoresAndOwnScore {
                    level_validator,
                    scores,
                    own_score,
                }
            }
            ServerToClient::STC_SERVER_STATUS => ServerToClient::STCPServerStatus {
                protocol_version,
                game_version,
                supported_level_validators: validators,
            },
            _ => unreachable!("handled above"),
        }
    }
}

/// C++ `decodeClientToServerPacket` (`Shared.cpp:857-868`): the preamble
/// check, then the inner dispatch. `key_receive` is the C++
/// `const SodiumReceiveKeyArray*` (absent = `None`); `error_oss` is the
/// C++ `std::ostringstream&` (pre-existing content is preserved — the
/// whole content becomes the `PInvalid.error`).
pub fn decode_client_to_server_packet(
    key_receive: Option<&SodiumReceiveKeyArray>,
    p: &mut Packet,
    error_oss: &mut String,
) -> ClientToServer {
    let mut r = Reader::new(p);
    if !verify_preamble(&mut r, error_oss) {
        return ClientToServer::PInvalid {
            error: error_oss.clone(),
        };
    }
    decode_cts_inner(key_receive, &mut r, error_oss)
}

/// C++ `decodeServerToClientPacket` (`Shared.cpp:920-931`).
pub fn decode_server_to_client_packet(
    key_receive: Option<&SodiumReceiveKeyArray>,
    p: &mut Packet,
    error_oss: &mut String,
) -> ServerToClient {
    let mut r = Reader::new(p);
    if !verify_preamble(&mut r, error_oss) {
        return ServerToClient::PInvalid {
            error: error_oss.clone(),
        };
    }
    decode_stc_inner(key_receive, &mut r, error_oss)
}
