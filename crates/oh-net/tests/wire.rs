//! Byte-exact wire vectors for `oh_net::shared` — the C++ oracle output of
//! `tools/oracle/net_probe.cpp` (the REAL `Online/Shared.cpp` +
//! `Online/Sodium.cpp` compiled against the SFML `Packet` shim, libsodium
//! 1.0.18; recorded in `.agents/plans/001-rust-port/mapping/oracle-vectors.md`,
//! §"Net wire vectors"). Every encode assert pins the full packet bytes;
//! every decode assert pins the variant index + fields; the negative tests
//! pin the exact `PInvalid.error` strings (byte-for-byte, including the
//! NUL byte in the protocol-version mismatch and the trailing space in
//! `Error decoding field #i \n`).

use oh_net::packet::Packet;
use oh_net::shared::ClientToServer;
use oh_net::shared::{
    decode_client_to_server_packet, decode_server_to_client_packet,
    make_client_to_server_encrypted_packet, make_client_to_server_packet,
    make_server_to_client_encrypted_packet, make_server_to_client_packet, ProcessedScore,
    ServerToClient,
};
use oh_net::sodium::{
    calculate_client_session_sodium_rt_keys, calculate_server_session_sodium_rt_keys,
    secretbox_seal, sodium_hash, sodium_key_to_string, SodiumPsKeys,
};

// ---- the probe's fixed fixtures (tools/oracle/net_probe.cpp) ----

const STEAM_ID: u64 = 0x0A0B0C0D0E0F1011;
const LOGIN_TOKEN: u64 = 0x1112131415161718;
const NAME: &str = "tester";
const PWHASH: &str = "hashedpw";
const LV: &str = "cube_1_apeirogon_m_1.6";
const LV2: &str = "cube_1_apeirogon_m_2";
const PUBKEY: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
];
fn score_1() -> ProcessedScore {
    ProcessedScore {
        position: 1,
        user_name: "tester".into(),
        score_timestamp: 1_700_000_000_000,
        score_value: 123.45,
    }
}

fn score_2() -> ProcessedScore {
    ProcessedScore {
        position: 2,
        user_name: "other".into(),
        score_timestamp: 1_700_000_000_001,
        score_value: 99.5,
    }
}

/// The oracle-vectors replay fixture 1 (the same `replay_file` whose
/// 84-byte file bytes are the pinned `HEX`).
fn replay_fixture1() -> oh_replay::ReplayFile {
    let mut data = oh_replay::ReplayData::default();
    data.record_input(false, false, false, false);
    data.record_input(false, true, false, false);
    data.record_input(true, false, true, false);
    data.record_input(false, false, false, false);
    data.record_input(false, true, false, false);
    data.record_input(true, false, false, true);
    oh_replay::ReplayFile {
        version: 5,
        player_name: "tester".into(),
        seed: 12345,
        data,
        pack_id: "ohvrvanilla".into(),
        level_id: "cube_1_apeirogon".into(),
        first_play: true,
        difficulty_mult: 1.6f32,
        played_score: 3600.0,
    }
}

/// The oracle-vectors pinned 76-byte zlib stream (replay fixture 1).
fn compressed_fixture1() -> oh_replay::CompressedReplayFile {
    oh_replay::CompressedReplayFile {
        data: hex(
            "78da636560606003e292d4e292d4224b03063060638002265606264e6e20233fa3aca82c312f3327275100c84d2e4d4a8d378c4f2c48cd2cca4fcfcf633c7be68c3d5887c21a070042d611e8",
        ),
    }
}

fn hex(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| c.to_digit(16).unwrap() as u8)
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|p| p[0] * 16 + p[1])
        .collect()
}

fn hexs(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn cts(packet: &ClientToServer) -> Vec<u8> {
    let mut p = Packet::new();
    make_client_to_server_packet(&mut p, packet);
    p.data().to_vec()
}

fn stc(packet: &ServerToClient) -> Vec<u8> {
    let mut p = Packet::new();
    make_server_to_client_packet(&mut p, packet);
    p.data().to_vec()
}

// ---------------------------------------------------------------------------
// CTS encoding — byte-exact against the C++ oracle
// ---------------------------------------------------------------------------

#[test]
fn cts_packets_are_byte_exact() {
    // Header for every CTS packet: 6f 68 | 00 | 02 01 07, then the type
    // byte. (preamble "oh", protocol 0, game version 2.1.7.)
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPHeartbeat)),
        "6f680002010702",
        "cts_heartbeat"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPDisconnect)),
        "6f680002010703",
        "cts_disconnect"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPPublicKey { key: PUBKEY })),
        "6f680002010704000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        "cts_public_key"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPRegister {
            steam_id: STEAM_ID,
            name: NAME.into(),
            password_hash: PWHASH.into(),
        })),
        "6f6800020107050a0b0c0d0e0f101100000006746573746572000000086861736865647077",
        "cts_register"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPLogin {
            steam_id: STEAM_ID,
            name: NAME.into(),
            password_hash: PWHASH.into(),
        })),
        "6f6800020107060a0b0c0d0e0f101100000006746573746572000000086861736865647077",
        "cts_login"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPLogout { steam_id: STEAM_ID })),
        "6f6800020107070a0b0c0d0e0f1011",
        "cts_logout"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPDeleteAccount {
            steam_id: STEAM_ID,
            password_hash: PWHASH.into(),
        })),
        "6f6800020107080a0b0c0d0e0f1011000000086861736865647077",
        "cts_delete_account"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPRequestTopScores {
            login_token: LOGIN_TOKEN,
            level_validator: LV.into(),
        })),
        "6f680002010709111213141516171800000016637562655f315f61706569726f676f6e5f6d5f312e36",
        "cts_request_top_scores"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPReplay {
            login_token: LOGIN_TOKEN,
            replay_file: replay_fixture1(),
        })),
        // u64-BE replay byte count (0x54 = 84) + the pinned replay file bytes.
        "6f68000201070a111213141516171800000000000000540500000006000000746573746572393000000000000006000000000000000002050002090b0000006f68767276616e696c6c6110000000637562655f315f61706569726f676f6e01cdcccc3f000000000020ac40",
        "cts_replay"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPRequestOwnScore {
            login_token: LOGIN_TOKEN,
            level_validator: LV.into(),
        })),
        "6f68000201070b111213141516171800000016637562655f315f61706569726f676f6e5f6d5f312e36",
        "cts_request_own_score"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPRequestTopScoresAndOwnScore {
            login_token: LOGIN_TOKEN,
            level_validator: LV.into(),
        })),
        "6f68000201070c111213141516171800000016637562655f315f61706569726f676f6e5f6d5f312e36",
        "cts_request_top_scores_and_own_score"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPStartedGame {
            login_token: LOGIN_TOKEN,
            level_validator: LV.into(),
        })),
        "6f68000201070d111213141516171800000016637562655f315f61706569726f676f6e5f6d5f312e36",
        "cts_started_game"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPCompressedReplay {
            login_token: LOGIN_TOKEN,
            compressed_replay_file: compressed_fixture1(),
        })),
        // u64-BE count (0x4C = 76) + the pinned zlib stream.
        "6f68000201070e1112131415161718000000000000004c78da636560606003e292d4e292d4224b03063060638002265606264e6e20233fa3aca82c312f3327275100c84d2e4d4a8d378c4f2c48cd2cca4fcfcf633c7be68c3d5887c21a070042d611e8",
        "cts_compressed_replay"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPRequestServerStatus {
            login_token: LOGIN_TOKEN,
        })),
        "6f68000201070f1112131415161718",
        "cts_request_server_status"
    );
    assert_eq!(
        hexs(&cts(&ClientToServer::CTSPReady {
            login_token: LOGIN_TOKEN,
        })),
        "6f6800020107101112131415161718",
        "cts_ready"
    );
}

// ---------------------------------------------------------------------------
// STC encoding — byte-exact against the C++ oracle
// ---------------------------------------------------------------------------

#[test]
fn stc_packets_are_byte_exact() {
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPKick)),
        "6f680002010702",
        "stc_kick"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPPublicKey { key: PUBKEY })),
        "6f680002010703000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        "stc_public_key"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPRegistrationSuccess)),
        "6f680002010704",
        "stc_registration_success"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPRegistrationFailure {
            error: "registration failed".into(),
        })),
        "6f68000201070500000013726567697374726174696f6e206661696c6564",
        "stc_registration_failure"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPLoginSuccess {
            login_token: LOGIN_TOKEN,
            login_name: NAME.into(),
        })),
        "6f680002010706111213141516171800000006746573746572",
        "stc_login_success"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPLoginFailure {
            error: "login failed".into(),
        })),
        "6f6800020107070000000c6c6f67696e206661696c6564",
        "stc_login_failure"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPLogoutSuccess)),
        "6f680002010708",
        "stc_logout_success"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPLogoutFailure)),
        "6f680002010709",
        "stc_logout_failure"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPDeleteAccountSuccess)),
        "6f68000201070a",
        "stc_delete_account_success"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPDeleteAccountFailure {
            error: "delete failed".into(),
        })),
        "6f68000201070b0000000d64656c657465206661696c6564",
        "stc_delete_account_failure"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPTopScores {
            level_validator: LV.into(),
            scores: vec![score_1(), score_2()],
        })),
        // ProcessedScore = u32-BE position + string + u64-BE timestamp +
        // raw host (LE) double.
        "6f68000201070c00000016637562655f315f61706569726f676f6e5f6d5f312e36000000000000000200000001000000067465737465720000018bcfe56800cdccccccccdc5e4000000002000000056f746865720000018bcfe568010000000000e05840",
        "stc_top_scores"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPOwnScore {
            level_validator: LV.into(),
            score: score_1(),
        })),
        "6f68000201070d00000016637562655f315f61706569726f676f6e5f6d5f312e3600000001000000067465737465720000018bcfe56800cdccccccccdc5e40",
        "stc_own_score"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPTopScoresAndOwnScore {
            level_validator: LV.into(),
            scores: vec![score_1(), score_2()],
            own_score: Some(score_1()),
        })),
        // The own-score block: 1-byte set flag (01) + the score.
        "6f68000201070e00000016637562655f315f61706569726f676f6e5f6d5f312e36000000000000000200000001000000067465737465720000018bcfe56800cdccccccccdc5e4000000002000000056f746865720000018bcfe568010000000000e058400100000001000000067465737465720000018bcfe56800cdccccccccdc5e40",
        "stc_top_scores_and_own_score_some"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPTopScoresAndOwnScore {
            level_validator: LV.into(),
            scores: vec![score_1(), score_2()],
            own_score: None,
        })),
        // Same but the set flag is 00 and no score follows.
        "6f68000201070e00000016637562655f315f61706569726f676f6e5f6d5f312e36000000000000000200000001000000067465737465720000018bcfe56800cdccccccccdc5e4000000002000000056f746865720000018bcfe568010000000000e0584000",
        "stc_top_scores_and_own_score_none"
    );
    assert_eq!(
        hexs(&stc(&ServerToClient::STCPServerStatus {
            protocol_version: 0,
            game_version: oh_net::protocol::GAME_VERSION,
            supported_level_validators: vec![LV.into(), LV2.into()],
        })),
        // game version = three i32-BE; the validator vector = u64-BE count
        // (02) + the two strings.
        "6f68000201070f00000000020000000100000007000000000000000200000016637562655f315f61706569726f676f6e5f6d5f312e3600000014637562655f315f61706569726f676f6e5f6d5f32",
        "stc_server_status"
    );
}

// ---------------------------------------------------------------------------
// Decode round trips (Rust encode → Rust decode, full-value equality)
// ---------------------------------------------------------------------------

fn cts_round_trip(packet: &ClientToServer) {
    let mut p = Packet::new();
    make_client_to_server_packet(&mut p, packet);
    let mut error = String::new();
    let got = decode_client_to_server_packet(None, &mut p, &mut error);
    assert_eq!(got, *packet, "round trip failed; error_oss: {error:?}");
}

fn stc_round_trip(packet: &ServerToClient) {
    let mut p = Packet::new();
    make_server_to_client_packet(&mut p, packet);
    let mut error = String::new();
    let got = decode_server_to_client_packet(None, &mut p, &mut error);
    assert_eq!(got, *packet, "round trip failed; error_oss: {error:?}");
}

#[test]
fn cts_decode_round_trips() {
    cts_round_trip(&ClientToServer::CTSPHeartbeat);
    cts_round_trip(&ClientToServer::CTSPDisconnect);
    cts_round_trip(&ClientToServer::CTSPPublicKey { key: PUBKEY });
    cts_round_trip(&ClientToServer::CTSPRegister {
        steam_id: STEAM_ID,
        name: NAME.into(),
        password_hash: PWHASH.into(),
    });
    cts_round_trip(&ClientToServer::CTSPLogin {
        steam_id: STEAM_ID,
        name: NAME.into(),
        password_hash: PWHASH.into(),
    });
    cts_round_trip(&ClientToServer::CTSPLogout { steam_id: STEAM_ID });
    cts_round_trip(&ClientToServer::CTSPDeleteAccount {
        steam_id: STEAM_ID,
        password_hash: PWHASH.into(),
    });
    cts_round_trip(&ClientToServer::CTSPRequestTopScores {
        login_token: LOGIN_TOKEN,
        level_validator: LV.into(),
    });
    cts_round_trip(&ClientToServer::CTSPReplay {
        login_token: LOGIN_TOKEN,
        replay_file: replay_fixture1(),
    });
    cts_round_trip(&ClientToServer::CTSPRequestOwnScore {
        login_token: LOGIN_TOKEN,
        level_validator: LV.into(),
    });
    cts_round_trip(&ClientToServer::CTSPRequestTopScoresAndOwnScore {
        login_token: LOGIN_TOKEN,
        level_validator: LV.into(),
    });
    cts_round_trip(&ClientToServer::CTSPStartedGame {
        login_token: LOGIN_TOKEN,
        level_validator: LV.into(),
    });
    cts_round_trip(&ClientToServer::CTSPCompressedReplay {
        login_token: LOGIN_TOKEN,
        compressed_replay_file: compressed_fixture1(),
    });
    cts_round_trip(&ClientToServer::CTSPRequestServerStatus {
        login_token: LOGIN_TOKEN,
    });
    cts_round_trip(&ClientToServer::CTSPReady {
        login_token: LOGIN_TOKEN,
    });
}

#[test]
fn stc_decode_round_trips() {
    stc_round_trip(&ServerToClient::STCPKick);
    stc_round_trip(&ServerToClient::STCPPublicKey { key: PUBKEY });
    stc_round_trip(&ServerToClient::STCPRegistrationSuccess);
    stc_round_trip(&ServerToClient::STCPRegistrationFailure {
        error: "registration failed".into(),
    });
    stc_round_trip(&ServerToClient::STCPLoginSuccess {
        login_token: LOGIN_TOKEN,
        login_name: NAME.into(),
    });
    stc_round_trip(&ServerToClient::STCPLoginFailure {
        error: "login failed".into(),
    });
    stc_round_trip(&ServerToClient::STCPLogoutSuccess);
    stc_round_trip(&ServerToClient::STCPLogoutFailure);
    stc_round_trip(&ServerToClient::STCPDeleteAccountSuccess);
    stc_round_trip(&ServerToClient::STCPDeleteAccountFailure {
        error: "delete failed".into(),
    });
    stc_round_trip(&ServerToClient::STCPTopScores {
        level_validator: LV.into(),
        scores: vec![score_1(), score_2()],
    });
    stc_round_trip(&ServerToClient::STCPOwnScore {
        level_validator: LV.into(),
        score: score_1(),
    });
    stc_round_trip(&ServerToClient::STCPTopScoresAndOwnScore {
        level_validator: LV.into(),
        scores: vec![score_1(), score_2()],
        own_score: Some(score_1()),
    });
    stc_round_trip(&ServerToClient::STCPTopScoresAndOwnScore {
        level_validator: LV.into(),
        scores: vec![score_1(), score_2()],
        own_score: None,
    });
    stc_round_trip(&ServerToClient::STCPServerStatus {
        protocol_version: 0,
        game_version: oh_net::protocol::GAME_VERSION,
        supported_level_validators: vec![LV.into(), LV2.into()],
    });
}

// ---------------------------------------------------------------------------
// Sodium KATs — cross-implementation against the C++ oracle
// ---------------------------------------------------------------------------

/// The probe's seed pattern: `seed[i] = offset + i` (server: 0..31,
/// client: 32..63).
fn seeded_ps_keys(offset: u8) -> SodiumPsKeys {
    let mut seed = [0u8; 32];
    for (i, slot) in seed.iter_mut().enumerate() {
        *slot = offset.wrapping_add(i as u8);
    }
    let (public_key, secret_key) =
        sodiumoxide::crypto::kx::keypair_from_seed(&sodiumoxide::crypto::kx::Seed(seed));
    SodiumPsKeys {
        key_public: public_key.as_ref().try_into().unwrap(),
        key_secret: secret_key.as_ref().try_into().unwrap(),
    }
}

#[test]
fn kx_seed_keypairs_and_session_keys_match_the_cpp_oracle() {
    // The probe seeds the PS keypairs with crypto_kx_seed_keypair (seed
    // patterns 0..31 / 32..63) — sodiumoxide's keypair_from_seed issues
    // the same call, so the public halves and the session keys must equal
    // the pinned C++ values.
    let server = seeded_ps_keys(0);
    let client = seeded_ps_keys(32);

    assert_eq!(
        hexs(&server.key_public),
        "0e0216223f147143d32615a91189c288c1728cba3cc5f9f621b1026e03d83129",
        "kx_server_pk"
    );
    assert_eq!(
        hexs(&client.key_public),
        "99f4674ecc87c0b8e712f192b8f49e7442a9376b4875967ababa28471019a93e",
        "kx_client_pk"
    );

    let client_rt = calculate_client_session_sodium_rt_keys(&client, &server.key_public)
        .expect("valid key material");
    let server_rt = calculate_server_session_sodium_rt_keys(&server, &client.key_public)
        .expect("valid key material");

    assert_eq!(
        hexs(&client_rt.key_receive),
        "6d3e872064c89c0eac73f4713ffbbedcc286a65f7af9e993fe9dfced2404ec9c",
        "kx_client_rx"
    );
    assert_eq!(
        hexs(&client_rt.key_transmit),
        "4fa03a98a7376a06f043151bbc4ae5c10af48b625daafccda647a83b9d27b2ea",
        "kx_client_tx"
    );
    assert_eq!(
        hexs(&server_rt.key_receive),
        "4fa03a98a7376a06f043151bbc4ae5c10af48b625daafccda647a83b9d27b2ea",
        "kx_server_rx"
    );
    assert_eq!(
        hexs(&server_rt.key_transmit),
        "6d3e872064c89c0eac73f4713ffbbedcc286a65f7af9e993fe9dfced2404ec9c",
        "kx_server_tx"
    );
    // The protocol invariant (probe `kx_symmetry_ok=1`).
    assert_eq!(client_rt.key_receive, server_rt.key_transmit);
    assert_eq!(client_rt.key_transmit, server_rt.key_receive);
}

const FIXED_TRANSMIT_KEY: [u8; 32] = [0x5A; 32];

/// The probe's CTS envelope: inner packet = the `cts_started_game`
/// type byte + fields (no header), nonce = 00..17 (the fake
/// randombytes), transmit key = 0x5A x 32.
#[test]
fn cts_encrypted_matches_the_cpp_oracle() {
    let nonce: [u8; 24] = (0..24).collect::<Vec<_>>().try_into().unwrap();
    let inner = hex("0d111213141516171800000016637562655f315f61706569726f676f6e5f6d5f312e36");

    // Cross-implementation check: the C++ (libsodium 1.0.18, real
    // crypto_secretbox_easy) ciphertext must equal the Rust seal.
    let ciphertext = secretbox_seal(&inner, &nonce, &FIXED_TRANSMIT_KEY);
    assert_eq!(
        hexs(&ciphertext),
        "06577fb0b335b9c32899f58f2f018811822ee812d506b672f3448e5d557b3d7a7e6bf6ea4adde79aed307497f60c4955c87542",
        "cts_encrypted ciphertext"
    );

    let envelope = ClientToServer::PEncryptedMsg {
        nonce,
        message_length: 35,
        ciphertext_length: 51,
        ciphertext,
    };
    assert_eq!(
        hexs(&cts(&envelope)),
        "6f680002010701000102030405060708090a0b0c0d0e0f10111213141516170000000000000023000000000000003306577fb0b335b9c32899f58f2f018811822ee812d506b672f3448e5d557b3d7a7e6bf6ea4adde79aed307497f60c4955c87542",
        "cts_encrypted envelope"
    );

    let mut p = Packet::new();
    make_client_to_server_packet(&mut p, &envelope);
    let mut error = String::new();
    let got = decode_client_to_server_packet(Some(&FIXED_TRANSMIT_KEY), &mut p, &mut error);
    assert_eq!(
        got,
        ClientToServer::CTSPStartedGame {
            login_token: LOGIN_TOKEN,
            level_validator: LV.into(),
        },
        "encrypted decode; error_oss: {error:?}"
    );
}

/// The probe's STC envelope: inner = `stc_login_success` type byte +
/// fields, nonce = 18..2F, same fixed transmit key.
#[test]
fn stc_encrypted_matches_the_cpp_oracle() {
    let nonce: [u8; 24] = (24..48).collect::<Vec<_>>().try_into().unwrap();
    let inner = hex("06111213141516171800000006746573746572");

    let ciphertext = secretbox_seal(&inner, &nonce, &FIXED_TRANSMIT_KEY);
    assert_eq!(
        hexs(&ciphertext),
        "2f999e3fda2c21bdb759f1e8ab00a5550aa19d5372a060fbb80e996d835654f7aa39f5",
        "stc_encrypted ciphertext"
    );

    let envelope = ServerToClient::PEncryptedMsg {
        nonce,
        message_length: 19,
        ciphertext_length: 35,
        ciphertext,
    };
    assert_eq!(
        hexs(&stc(&envelope)),
        "6f68000201070118191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f000000000000001300000000000000232f999e3fda2c21bdb759f1e8ab00a5550aa19d5372a060fbb80e996d835654f7aa39f5",
        "stc_encrypted envelope"
    );

    let mut p = Packet::new();
    make_server_to_client_packet(&mut p, &envelope);
    let mut error = String::new();
    let got = decode_server_to_client_packet(Some(&FIXED_TRANSMIT_KEY), &mut p, &mut error);
    assert_eq!(
        got,
        ServerToClient::STCPLoginSuccess {
            login_token: LOGIN_TOKEN,
            login_name: NAME.into(),
        },
        "encrypted decode; error_oss: {error:?}"
    );
}

/// The `make*EncryptedPacket` entry points with a FRESH random nonce (the
/// non-deterministic path): encode → decode must round-trip.
#[test]
fn encrypted_make_functions_round_trip() {
    let mut p = Packet::new();
    make_client_to_server_encrypted_packet(
        &FIXED_TRANSMIT_KEY,
        &mut p,
        &ClientToServer::CTSPStartedGame {
            login_token: LOGIN_TOKEN,
            level_validator: LV.into(),
        },
    );
    let mut error = String::new();
    let got = decode_client_to_server_packet(Some(&FIXED_TRANSMIT_KEY), &mut p, &mut error);
    assert_eq!(
        got,
        ClientToServer::CTSPStartedGame {
            login_token: LOGIN_TOKEN,
            level_validator: LV.into(),
        },
        "error_oss: {error:?}"
    );

    let mut q = Packet::new();
    make_server_to_client_encrypted_packet(
        &FIXED_TRANSMIT_KEY,
        &mut q,
        &ServerToClient::STCPTopScores {
            level_validator: LV.into(),
            scores: vec![score_1(), score_2()],
        },
    );
    let mut error = String::new();
    let got = decode_server_to_client_packet(Some(&FIXED_TRANSMIT_KEY), &mut q, &mut error);
    assert_eq!(
        got,
        ServerToClient::STCPTopScores {
            level_validator: LV.into(),
            scores: vec![score_1(), score_2()],
        },
        "error_oss: {error:?}"
    );
}

#[test]
fn sodium_hash_kats_match_the_cpp_oracle() {
    // crypto_generichash (keyed BLAKE2b, 32-byte zero key, 32-byte output)
    // — cross-implementation against the C++ oracle.
    assert_eq!(
        hexs(&sodium_hash(b"oh-net")),
        "73e1c8626cbe2ed83771e3a7eaf1fc7d74c2b8fc9587c36311f453f4387de45c",
        "sodium_hash_oh_net"
    );
    assert_eq!(
        hexs(&sodium_hash(b"")),
        "fd7b3e075ab7e245506323f1d1b55330ccd606fb3301f4caba25022a1de50b47",
        "sodium_hash_empty"
    );
    // The decimal-concatenation key string (probe-independent unit pin).
    assert_eq!(sodium_key_to_string(&[3, 200, 1]), "32001");
}

// ---------------------------------------------------------------------------
// Negative decode cases — exact PInvalid.error strings from the oracle
// ---------------------------------------------------------------------------

fn decode_cts_invalid(bytes: &[u8], key: Option<&[u8; 32]>) -> (u8, String) {
    let mut p = Packet::new();
    p.append(bytes);
    let mut error = String::new();
    let got = decode_client_to_server_packet(key, &mut p, &mut error);
    let idx = got_index(&got);
    match got {
        ClientToServer::PInvalid { error } => (idx, error),
        other => panic!("expected PInvalid, got {other:?}"),
    }
}

fn got_index(got: &ClientToServer) -> u8 {
    match got {
        ClientToServer::PInvalid { .. } => ClientToServer::P_INVALID,
        ClientToServer::PEncryptedMsg { .. } => ClientToServer::P_ENCRYPTED_MSG,
        ClientToServer::CTSPHeartbeat => ClientToServer::CTSP_HEARTBEAT,
        ClientToServer::CTSPDisconnect => ClientToServer::CTSP_DISCONNECT,
        ClientToServer::CTSPPublicKey { .. } => ClientToServer::CTS_PUBLIC_KEY,
        ClientToServer::CTSPRegister { .. } => ClientToServer::CTSP_REGISTER,
        ClientToServer::CTSPLogin { .. } => ClientToServer::CTSP_LOGIN,
        ClientToServer::CTSPLogout { .. } => ClientToServer::CTSP_LOGOUT,
        ClientToServer::CTSPDeleteAccount { .. } => ClientToServer::CTSP_DELETE_ACCOUNT,
        ClientToServer::CTSPRequestTopScores { .. } => ClientToServer::CTSP_REQUEST_TOP_SCORES,
        ClientToServer::CTSPReplay { .. } => ClientToServer::CTSP_REPLAY,
        ClientToServer::CTSPRequestOwnScore { .. } => ClientToServer::CTSP_REQUEST_OWN_SCORE,
        ClientToServer::CTSPRequestTopScoresAndOwnScore { .. } => {
            ClientToServer::CTSP_REQUEST_TOP_SCORES_AND_OWN_SCORE
        }
        ClientToServer::CTSPStartedGame { .. } => ClientToServer::CTSP_STARTED_GAME,
        ClientToServer::CTSPCompressedReplay { .. } => ClientToServer::CTSP_COMPRESSED_REPLAY,
        ClientToServer::CTSPRequestServerStatus { .. } => {
            ClientToServer::CTSP_REQUEST_SERVER_STATUS
        }
        ClientToServer::CTSPReady { .. } => ClientToServer::CTSP_READY,
    }
}

fn header(extra: &[u8]) -> Vec<u8> {
    let mut v = vec![0x6f, 0x68, 0x00, 0x02, 0x01, 0x07];
    v.extend_from_slice(extra);
    v
}

#[test]
fn negative_cts_bad_preamble_1st() {
    // 'x' 'h' + valid protocol/version + heartbeat type.
    let (idx, error) = decode_cts_invalid(&[0x78, 0x68, 0x00, 0x02, 0x01, 0x07, 0x02], None);
    assert_eq!(idx, ClientToServer::P_INVALID);
    assert_eq!(
        error,
        "Error, preamble 1st byte has value 'x, which doesn't match \
         expected value 'o'\n"
    );
}

#[test]
fn negative_cts_bad_protocol() {
    // protocol byte 0x01 — the expected value (uint8_t 0) streams as a NUL
    // character, so the error string contains a raw \0.
    let (idx, error) = decode_cts_invalid(&[0x6f, 0x68, 0x01, 0x02, 0x01, 0x07, 0x02], None);
    assert_eq!(idx, ClientToServer::P_INVALID);
    let mut expected = String::from(
        "Error, protocol version has value '\u{1}, which doesn't match \
         expected value '",
    );
    expected.push('\0');
    expected.push_str("'\n");
    assert_eq!(error, expected);
}

#[test]
fn negative_cts_truncated_preamble() {
    let (idx, error) = decode_cts_invalid(&[0x6f], None);
    assert_eq!(idx, ClientToServer::P_INVALID);
    assert_eq!(
        error,
        "Error extracting single object\nError extracting preamble 2st byte\n"
    );
}

#[test]
fn negative_cts_truncated_game_version() {
    let (idx, error) = decode_cts_invalid(&[0x6f, 0x68, 0x00], None);
    assert_eq!(idx, ClientToServer::P_INVALID);
    assert_eq!(
        error,
        "Error extracting single object\nError extracting major version\n"
    );
}

#[test]
fn negative_cts_unknown_type() {
    // The probe's type byte is 99 *decimal* (0x63); the C++ prints the
    // type via `static_cast<int>` (Shared.cpp:850), so the message is the
    // decimal value.
    let (idx, error) = decode_cts_invalid(&header(&[99]), None);
    assert_eq!(idx, ClientToServer::P_INVALID);
    assert_eq!(error, "Unknown packet type '99'\n");
}

#[test]
fn negative_cts_type0_not_decodable() {
    // Type 0 (PInvalid) is NOT a decodable type — the C++ dispatch has no
    // PInvalid arm, so a hand-crafted type-0 packet fails as "unknown".
    let (idx, error) = decode_cts_invalid(
        &header(&[0x00, 0x00, 0x00, 0x00, 0x07])
            .into_iter()
            .chain("invalid".bytes())
            .collect::<Vec<_>>(),
        None,
    );
    assert_eq!(idx, ClientToServer::P_INVALID);
    assert_eq!(error, "Unknown packet type '0'\n");
}

#[test]
fn negative_cts_truncated_register() {
    // CTSPRegister: steamId OK, `name` declares 10 bytes but only 3 are
    // present. The C++ keeps extracting the remaining field after the
    // failure (poisoned packet → the 3rd field fails too), so the error
    // carries lines for BOTH fields (note the trailing space in
    // `field #1 \n`).
    let mut bytes = header(&[0x05]);
    bytes.extend_from_slice(&STEAM_ID.to_be_bytes());
    bytes.extend_from_slice(&10u32.to_be_bytes());
    bytes.extend_from_slice(b"tes");
    let (idx, error) = decode_cts_invalid(&bytes, None);
    assert_eq!(idx, ClientToServer::P_INVALID);
    assert_eq!(
        error,
        "Error extracting single object\n\
         Error decoding field #1 \n\
         Error extracting single object\n\
         Error decoding field #2 \n"
    );
}

#[test]
fn negative_cts_encrypted_no_key() {
    let (idx, error) = decode_cts_invalid(&header(&[0x01]), None);
    assert_eq!(idx, ClientToServer::P_INVALID);
    assert_eq!(
        error,
        "Cannot decode encrypted message without receive key\n"
    );
}

#[test]
fn negative_cts_encrypted_bad_key() {
    let mut bytes = header(&[0x01]);
    bytes.extend_from_slice(&(0..24).map(|i| i as u8).collect::<Vec<_>>());
    bytes.extend_from_slice(&32u64.to_be_bytes()); // messageLength
    bytes.extend_from_slice(&48u64.to_be_bytes()); // ciphertextLength
    bytes.extend_from_slice(&(0..48).map(|i| (i * 7) as u8).collect::<Vec<_>>());
    let bad_key = [0x5B; 32];
    let (idx, error) = decode_cts_invalid(&bytes, Some(&bad_key));
    assert_eq!(idx, ClientToServer::P_INVALID);
    assert_eq!(error, "Failure decrypting encrypted client message\n");
}

#[test]
fn negative_cts_encrypted_truncated() {
    // Declares a 36-byte ciphertext, only 10 present: the per-byte read
    // loop fails at index 10.
    let mut bytes = header(&[0x01]);
    bytes.extend_from_slice(&(0..24).map(|i| i as u8).collect::<Vec<_>>());
    bytes.extend_from_slice(&20u64.to_be_bytes()); // messageLength
    bytes.extend_from_slice(&36u64.to_be_bytes()); // ciphertextLength
    bytes.extend_from_slice(&(0..10).map(|i| (i * 3) as u8).collect::<Vec<_>>());
    let (idx, error) = decode_cts_invalid(&bytes, Some(&[0x5A; 32]));
    assert_eq!(idx, ClientToServer::P_INVALID);
    assert_eq!(error, "Error decoding client ciphertext at index '10'\n");
}

#[test]
fn negative_stc_unknown_type() {
    let mut p = Packet::new();
    p.append(&header(&[99]));
    let mut error = String::new();
    let got = decode_server_to_client_packet(None, &mut p, &mut error);
    match got {
        ServerToClient::PInvalid { error } => {
            assert_eq!(error, "Unknown packet type '99'\n");
        }
        other => panic!("expected PInvalid, got {other:?}"),
    }
}

// ---- the raw-byte string fields (issue 18 interop) ----

/// C++ `std::string` holds raw bytes: the `passwordHash` wire field is the
/// raw 32-byte `sodiumHash` digest (a BLAKE2b output — almost never valid
/// UTF-8). The encode side carries it byte-exactly (the client wraps the
/// digest `from_utf8_unchecked`, `client.rs:604-613`); the decode side must
/// read it back byte-exactly (no UTF-8 validation — the C++
/// `extractInto(std::string)` never validates, `Shared.cpp:130-172`).
#[test]
fn raw_byte_password_hash_round_trips() {
    // The C++'s exact field value: the keyed-BLAKE2b digest of the
    // salted password (`HexagonClient.cpp:799-811`, `Sodium.cpp:62-74`).
    let digest = sodium_hash(b"saltpassword");
    assert!(
        std::str::from_utf8(&digest).is_err(),
        "the fixture digest is NOT valid UTF-8"
    );
    let pwhash = unsafe { String::from_utf8_unchecked(digest.to_vec()) };

    let mut p = Packet::new();
    make_client_to_server_packet(
        &mut p,
        &ClientToServer::CTSPRegister {
            steam_id: STEAM_ID,
            name: NAME.to_string(),
            password_hash: pwhash.clone(),
        },
    );

    let mut q = Packet::new();
    q.append(p.data());
    let mut error = String::new();
    let got = decode_client_to_server_packet(None, &mut q, &mut error);
    match got {
        ClientToServer::CTSPRegister {
            steam_id,
            name,
            password_hash,
        } => {
            assert_eq!(steam_id, STEAM_ID);
            assert_eq!(name, NAME);
            assert_eq!(
                password_hash.as_bytes(),
                &digest[..],
                "the raw 32-byte digest must round-trip byte-exactly"
            );
        }
        other => panic!("expected CTSPRegister, got {other:?} (error: {error:?})"),
    }
}
