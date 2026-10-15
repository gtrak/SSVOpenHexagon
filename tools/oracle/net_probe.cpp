// Compiles the REAL Online/Shared.cpp + Online/Sodium.cpp + Core/Replay.cpp
// against the SFML Packet shim (tools/oracle/shim) and the cached libsodium
// 1.0.18, emitting byte-exact wire vectors for the Rust port
// (crates/oh-net/tests/wire.rs). See mapping/oracle-vectors.md §"Net wire
// vectors" for the recorded output.
//
// DETERMINISM:
// - `randombytes_buf` is overridden below (the probe's own definition wins
//   at link time for the statically linked Shared.cpp/Sodium.cpp objects),
//   filling every request from a fixed offset pattern `byte_i = offset + i`.
//   The call order in main() fixes the offsets:
//     offset  0 (24 B): CTS encrypted nonce   -> bytes 00..17
//     offset 24 (24 B): STC encrypted nonce   -> bytes 18..2F
//     offset 48 (8 B):  randomUInt64 pin      -> bytes 30..37
//   (Ubuntu's libsodium.so binds its internal randombytes_buf calls directly
//   — no PLT — so it is NOT interposable; the key exchange below therefore
//   seeds the keypairs with crypto_kx_seed_keypair instead of going through
//   the (random) generateSodiumPSKeys. That is the same primitive the C++
//   generateSodiumPSKeys uses internally: crypto_kx_keypair =
//   randombytes(seed) + crypto_kx_seed_keypair, so a Rust test can
//   reproduce these keypairs with sodiumoxide kx::keypair_from_seed.)
// - The encrypted packets use a fixed transmit key (the hg API takes the
//   key explicitly), so the whole envelope is reproducible.
// - The session-key and secretbox calls use no randomness.

#include <SFML/Network/Packet.hpp>
#include "SSVOpenHexagon/Online/Shared.hpp"
#include "SSVOpenHexagon/Online/Sodium.hpp"
#include "SSVOpenHexagon/Core/Replay.hpp"

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <optional>
#include <sstream>
#include <string>
#include <vector>

namespace {

std::size_t rb_offset = 0;

std::string hexArr(const unsigned char* b, std::size_t n) {
    std::string s;
    for(std::size_t i = 0; i < n; ++i)
    { char buf[3]; snprintf(buf, 3, "%02x", (unsigned)b[i]); s += buf; }
    return s;
}

void printHex(const char* name, const sf::Packet& p) {
    printf("%s=%s\n", name, hexArr((const unsigned char*)p.getData(),
        p.getDataSize()).c_str());
}

void printErr(const char* name, const std::string& e) {
    printf("%s err=", name);
    for(const char c : e)
    {
        if(c == '\n') printf("\\n");
        else if(c == '\0') printf("\\0");
        else if((unsigned char)c < 32) printf("\\x%02x", (unsigned char)c);
        else putchar(c);
    }
    printf("\n");
}

// Decode check: the variant index must equal `idx` and the fields must
// match the fixture (verified by `same`). `decodeFn` is either
// hg::decodeClientToServerPacket or hg::decodeServerToClientPacket.
template <typename T, typename DecodeFn>
void checkDecode(sf::Packet& p, const char* name, std::size_t idx,
    DecodeFn&& decodeFn, auto&& same)
{
    std::ostringstream oss;
    auto got = decodeFn(nullptr, oss, p);
    bool ok = got.index() == idx;
    if(ok)
    {
        T g = std::get<T>(got);
        ok = same(g);
    }
    printf("decode_%s idx=%zu ok=%d\n", name, got.index(), (int)ok);
    if(!ok && got.index() == 0)
        printErr(name, std::get<hg::PInvalid>(got).error);
}

template <typename T, typename DecodeFn>
void checkDecodeKey(sf::Packet& p, const char* name, std::size_t idx,
    const hg::SodiumReceiveKeyArray* key, DecodeFn&& decodeFn, auto&& same)
{
    std::ostringstream oss;
    auto got = decodeFn(key, oss, p);
    bool ok = got.index() == idx;
    if(ok)
    {
        T g = std::get<T>(got);
        ok = same(g);
    }
    printf("decode_%s idx=%zu ok=%d\n", name, got.index(), (int)ok);
    if(!ok)
    {
        if(got.index() == 0)
            printErr(name, std::get<hg::PInvalid>(got).error);
        else
            printf("decode_%s type-mismatch\n", name);
    }
}

// Decode check for the negative (PInvalid) cases: index must be 0 and the
// error string must equal `expected` byte-for-byte.
void checkInvalid(sf::Packet& p, const char* name, const std::string& expected)
{
    std::ostringstream oss;
    auto got = hg::decodeClientToServerPacket(nullptr, oss, p);
    bool ok = got.index() == 0 && std::get<hg::PInvalid>(got).error == expected;
    printf("neg_%s ok=%d\n", name, (int)ok);
    if(!ok)
        printErr(name, std::get<hg::PInvalid>(got).error);
}

void appendRaw(sf::Packet& p, const std::vector<unsigned char>& b)
{
    p.append(b.data(), b.size());
}

} // namespace

extern "C" void randombytes_buf(void* p, std::size_t n)
{
    auto* b = static_cast<unsigned char*>(p);
    for(std::size_t i = 0; i < n; ++i)
        b[i] = static_cast<unsigned char>(rb_offset + i);
    rb_offset += n;
}

int main()
{
    // ---- fixed fixtures (one representative value per field) ----
    constexpr std::uint64_t STEAM_ID = 0x0A0B0C0D0E0F1011ull;
    constexpr std::uint64_t LOGIN_TOKEN = 0x1112131415161718ull;
    const std::string NAME = "tester";
    const std::string PWHASH = "hashedpw";
    const std::string LV = "cube_1_apeirogon_m_1.6";
    const std::string LV2 = "cube_1_apeirogon_m_2";
    hg::SodiumPublicKeyArray PUBKEY{};
    for(int i = 0; i < 32; ++i) PUBKEY[i] = (unsigned char)i;

    hg::Database::ProcessedScore s1{1, "tester", 1700000000000ull, 123.45};
    hg::Database::ProcessedScore s2{2, "other", 1700000000001ull, 99.5};
    auto sameScore = [](const hg::Database::ProcessedScore& a,
        const hg::Database::ProcessedScore& b)
    { return a.position == b.position && a.userName == b.userName &&
             a.scoreTimestamp == b.scoreTimestamp && a.scoreValue == b.scoreValue; };

    // The oracle-vectors replay fixture 1 (replay_probe.cpp): the same
    // replay_file whose 84-byte file bytes are the recorded HEX.
    hg::replay_file rf;
    rf._version = 5;
    rf._player_name = "tester";
    rf._seed = 12345ull;
    rf._pack_id = "ohvrvanilla";
    rf._level_id = "cube_1_apeirogon";
    rf._first_play = true;
    rf._difficulty_mult = 1.6f;
    rf._played_score = 3600.0;
    rf._data.record_input(false, false, false, false);
    rf._data.record_input(false, true, false, false);
    rf._data.record_input(true, false, true, false);
    rf._data.record_input(false, false, false, false);
    rf._data.record_input(false, true, false, false);
    rf._data.record_input(true, false, false, true);

    // The oracle-vectors pinned 76-byte zlib stream (embedded verbatim so
    // this probe is independent of the local zlib version).
    const std::string CZHEX =
        "78da636560606003e292d4e292d4224b03063060638002265606264e6e20233f"
        "a3aca82c312f3327275100c84d2e4d4a8d378c4f2c48cd2cca4fcfcf633c7be6"
        "8c3d5887c21a070042d611e8";
    std::vector<char> CZ(76);
    for(int i = 0; i < 76; ++i)
    {
        int v = 0;
        sscanf(CZHEX.c_str() + 2 * i, "%2x", &v);
        CZ[i] = (char)v;
    }
    hg::compressed_replay_file crf;
    crf._data = CZ;

    // ------------------------------------------------------------------
    // CTS encoding (makeClientToServerPacket)
    // ------------------------------------------------------------------
    sf::Packet p;

    // NOTE: PInvalid is NOT encodable via the C++ API (Shared.cpp only
    // explicitly instantiates make*Packet for PEncryptedMsg and the concrete
    // CTSP/STCP types). Type 0 therefore exists only as a decode-side value;
    // the hand-crafted type-0 vector below pins its decode behavior.

    hg::makeClientToServerPacket(p, hg::CTSPHeartbeat{});
    printHex("cts_heartbeat", p);

    hg::makeClientToServerPacket(p, hg::CTSPDisconnect{});
    printHex("cts_disconnect", p);

    hg::makeClientToServerPacket(p, hg::CTSPPublicKey{PUBKEY});
    printHex("cts_public_key", p);

    hg::CTSPRegister cRegister{STEAM_ID, NAME, PWHASH};
    hg::makeClientToServerPacket(p, cRegister);
    printHex("cts_register", p);

    hg::makeClientToServerPacket(p, hg::CTSPLogin{STEAM_ID, NAME, PWHASH});
    printHex("cts_login", p);

    hg::makeClientToServerPacket(p, hg::CTSPLogout{STEAM_ID});
    printHex("cts_logout", p);

    hg::makeClientToServerPacket(p, hg::CTSPDeleteAccount{STEAM_ID, PWHASH});
    printHex("cts_delete_account", p);

    hg::CTSPRequestTopScores cReqTop{LOGIN_TOKEN, LV};
    hg::makeClientToServerPacket(p, cReqTop);
    printHex("cts_request_top_scores", p);

    hg::CTSPReplay cReplay{LOGIN_TOKEN, rf};
    hg::makeClientToServerPacket(p, cReplay);
    printHex("cts_replay", p);

    hg::CTSPRequestOwnScore cReqOwn{LOGIN_TOKEN, LV};
    hg::makeClientToServerPacket(p, cReqOwn);
    printHex("cts_request_own_score", p);

    hg::CTSPRequestTopScoresAndOwnScore cReqBoth{LOGIN_TOKEN, LV};
    hg::makeClientToServerPacket(p, cReqBoth);
    printHex("cts_request_top_scores_and_own_score", p);

    hg::CTSPStartedGame cStarted{LOGIN_TOKEN, LV};
    hg::makeClientToServerPacket(p, cStarted);
    printHex("cts_started_game", p);

    hg::CTSPCompressedReplay cCReplay{LOGIN_TOKEN, crf};
    hg::makeClientToServerPacket(p, cCReplay);
    printHex("cts_compressed_replay", p);

    hg::makeClientToServerPacket(p, hg::CTSPRequestServerStatus{LOGIN_TOKEN});
    printHex("cts_request_server_status", p);

    hg::makeClientToServerPacket(p, hg::CTSPReady{LOGIN_TOKEN});
    printHex("cts_ready", p);

    // ------------------------------------------------------------------
    // STC encoding (makeServerToClientPacket)
    // ------------------------------------------------------------------
    hg::makeServerToClientPacket(p, hg::STCPKick{});
    printHex("stc_kick", p);

    hg::makeServerToClientPacket(p, hg::STCPPublicKey{PUBKEY});
    printHex("stc_public_key", p);

    hg::makeServerToClientPacket(p, hg::STCPRegistrationSuccess{});
    printHex("stc_registration_success", p);

    hg::STCPRegistrationFailure sRegFail{.error = "registration failed"};
    hg::makeServerToClientPacket(p, sRegFail);
    printHex("stc_registration_failure", p);

    hg::STCPLoginSuccess sLoginOk{LOGIN_TOKEN, NAME};
    hg::makeServerToClientPacket(p, sLoginOk);
    printHex("stc_login_success", p);

    hg::STCPLoginFailure sLoginFail{.error = "login failed"};
    hg::makeServerToClientPacket(p, sLoginFail);
    printHex("stc_login_failure", p);

    hg::makeServerToClientPacket(p, hg::STCPLogoutSuccess{});
    printHex("stc_logout_success", p);

    hg::makeServerToClientPacket(p, hg::STCPLogoutFailure{});
    printHex("stc_logout_failure", p);

    hg::makeServerToClientPacket(p, hg::STCPDeleteAccountSuccess{});
    printHex("stc_delete_account_success", p);

    hg::STCPDeleteAccountFailure sDelFail{.error = "delete failed"};
    hg::makeServerToClientPacket(p, sDelFail);
    printHex("stc_delete_account_failure", p);

    hg::STCPTopScores sTop{LV, {s1, s2}};
    hg::makeServerToClientPacket(p, sTop);
    printHex("stc_top_scores", p);

    hg::STCPOwnScore sOwn{LV, s1};
    hg::makeServerToClientPacket(p, sOwn);
    printHex("stc_own_score", p);

    hg::STCPTopScoresAndOwnScore sBothSome{LV, {s1, s2},
        std::optional<hg::Database::ProcessedScore>{s1}};
    hg::makeServerToClientPacket(p, sBothSome);
    printHex("stc_top_scores_and_own_score_some", p);

    hg::STCPTopScoresAndOwnScore sBothNone{LV, {s1, s2}, std::nullopt};
    hg::makeServerToClientPacket(p, sBothNone);
    printHex("stc_top_scores_and_own_score_none", p);

    hg::STCPServerStatus sStatus{hg::PROTOCOL_VERSION, hg::GAME_VERSION,
        {LV, LV2}};
    hg::makeServerToClientPacket(p, sStatus);
    printHex("stc_server_status", p);

    // ------------------------------------------------------------------
    // CTS decode (plaintext, no receive key)
    // ------------------------------------------------------------------
    const decltype(&hg::decodeClientToServerPacket) ctsDecode =
        &hg::decodeClientToServerPacket;
    const decltype(&hg::decodeServerToClientPacket) stcDecode =
        &hg::decodeServerToClientPacket;

    hg::makeClientToServerPacket(p, cRegister);
    checkDecode<hg::CTSPRegister>(
        p, "cts_register", 5, ctsDecode,
        [&](const hg::CTSPRegister& g)
        { return g.steamId == STEAM_ID && g.name == NAME &&
                 g.passwordHash == PWHASH; });

    hg::makeClientToServerPacket(p, hg::CTSPHeartbeat{});
    checkDecode<hg::CTSPHeartbeat>(
        p, "cts_heartbeat", 2, ctsDecode,
        [&](const hg::CTSPHeartbeat&) { return true; });

    hg::makeClientToServerPacket(p, hg::CTSPDisconnect{});
    checkDecode<hg::CTSPDisconnect>(
        p, "cts_disconnect", 3, ctsDecode,
        [&](const hg::CTSPDisconnect&) { return true; });

    hg::makeClientToServerPacket(p, hg::CTSPPublicKey{PUBKEY});
    checkDecode<hg::CTSPPublicKey>(
        p, "cts_public_key", 4, ctsDecode,
        [&](const hg::CTSPPublicKey& g)
        { return g.key == PUBKEY; });

    hg::makeClientToServerPacket(p, hg::CTSPLogin{STEAM_ID, NAME, PWHASH});
    checkDecode<hg::CTSPLogin>(
        p, "cts_login", 6, ctsDecode,
        [&](const hg::CTSPLogin& g)
        { return g.steamId == STEAM_ID && g.name == NAME &&
                 g.passwordHash == PWHASH; });

    hg::makeClientToServerPacket(p, hg::CTSPLogout{STEAM_ID});
    checkDecode<hg::CTSPLogout>(
        p, "cts_logout", 7, ctsDecode,
        [&](const hg::CTSPLogout& g) { return g.steamId == STEAM_ID; });

    hg::makeClientToServerPacket(p, hg::CTSPDeleteAccount{STEAM_ID, PWHASH});
    checkDecode<hg::CTSPDeleteAccount>(
        p, "cts_delete_account", 8, ctsDecode,
        [&](const hg::CTSPDeleteAccount& g)
        { return g.steamId == STEAM_ID && g.passwordHash == PWHASH; });

    hg::makeClientToServerPacket(p, cReqTop);
    checkDecode<hg::CTSPRequestTopScores>(
        p, "cts_request_top_scores", 9, ctsDecode,
        [&](const hg::CTSPRequestTopScores& g)
        { return g.loginToken == LOGIN_TOKEN && g.levelValidator == LV; });

    hg::makeClientToServerPacket(p, cReplay);
    checkDecode<hg::CTSPReplay>(
        p, "cts_replay", 10, ctsDecode,
        [&](const hg::CTSPReplay& g)
        { return g.loginToken == LOGIN_TOKEN && g.replayFile == rf; });

    hg::makeClientToServerPacket(p, cReqOwn);
    checkDecode<hg::CTSPRequestOwnScore>(
        p, "cts_request_own_score", 11, ctsDecode,
        [&](const hg::CTSPRequestOwnScore& g)
        { return g.loginToken == LOGIN_TOKEN && g.levelValidator == LV; });

    hg::makeClientToServerPacket(p, cReqBoth);
    checkDecode<hg::CTSPRequestTopScoresAndOwnScore>(
        p, "cts_request_top_scores_and_own_score", 12, ctsDecode,
        [&](const hg::CTSPRequestTopScoresAndOwnScore& g)
        { return g.loginToken == LOGIN_TOKEN && g.levelValidator == LV; });

    hg::makeClientToServerPacket(p, cStarted);
    checkDecode<hg::CTSPStartedGame>(
        p, "cts_started_game", 13, ctsDecode,
        [&](const hg::CTSPStartedGame& g)
        { return g.loginToken == LOGIN_TOKEN && g.levelValidator == LV; });

    hg::makeClientToServerPacket(p, cCReplay);
    checkDecode<hg::CTSPCompressedReplay>(
        p, "cts_compressed_replay", 14, ctsDecode,
        [&](const hg::CTSPCompressedReplay& g)
        { return g.loginToken == LOGIN_TOKEN &&
                 g.compressedReplayFile._data == crf._data; });

    hg::makeClientToServerPacket(p, hg::CTSPRequestServerStatus{LOGIN_TOKEN});
    checkDecode<hg::CTSPRequestServerStatus>(
        p, "cts_request_server_status", 15, ctsDecode,
        [&](const hg::CTSPRequestServerStatus& g)
        { return g.loginToken == LOGIN_TOKEN; });

    hg::makeClientToServerPacket(p, hg::CTSPReady{LOGIN_TOKEN});
    checkDecode<hg::CTSPReady>(
        p, "cts_ready", 16, ctsDecode,
        [&](const hg::CTSPReady& g) { return g.loginToken == LOGIN_TOKEN; });

    // ------------------------------------------------------------------
    // STC decode (plaintext, no receive key)
    // ------------------------------------------------------------------
    hg::makeServerToClientPacket(p, hg::STCPKick{});
    checkDecode<hg::STCPKick>(
        p, "stc_kick", 2, stcDecode,
        [&](const hg::STCPKick&) { return true; });

    hg::makeServerToClientPacket(p, hg::STCPPublicKey{PUBKEY});
    checkDecode<hg::STCPPublicKey>(
        p, "stc_public_key", 3, stcDecode,
        [&](const hg::STCPPublicKey& g)
        { return g.key == PUBKEY; });

    hg::makeServerToClientPacket(p, hg::STCPRegistrationSuccess{});
    checkDecode<hg::STCPRegistrationSuccess>(
        p, "stc_registration_success", 4, stcDecode,
        [&](const hg::STCPRegistrationSuccess&) { return true; });

    hg::makeServerToClientPacket(p, sRegFail);
    checkDecode<hg::STCPRegistrationFailure>(
        p, "stc_registration_failure", 5, stcDecode,
        [&](const hg::STCPRegistrationFailure& g)
        { return g.error == "registration failed"; });

    hg::makeServerToClientPacket(p, sLoginOk);
    checkDecode<hg::STCPLoginSuccess>(
        p, "stc_login_success", 6, stcDecode,
        [&](const hg::STCPLoginSuccess& g)
        { return g.loginToken == LOGIN_TOKEN && g.loginName == NAME; });

    hg::makeServerToClientPacket(p, sLoginFail);
    checkDecode<hg::STCPLoginFailure>(
        p, "stc_login_failure", 7, stcDecode,
        [&](const hg::STCPLoginFailure& g) { return g.error == "login failed"; });

    hg::makeServerToClientPacket(p, hg::STCPLogoutSuccess{});
    checkDecode<hg::STCPLogoutSuccess>(
        p, "stc_logout_success", 8, stcDecode,
        [&](const hg::STCPLogoutSuccess&) { return true; });

    hg::makeServerToClientPacket(p, hg::STCPLogoutFailure{});
    checkDecode<hg::STCPLogoutFailure>(
        p, "stc_logout_failure", 9, stcDecode,
        [&](const hg::STCPLogoutFailure&) { return true; });

    hg::makeServerToClientPacket(p, hg::STCPDeleteAccountSuccess{});
    checkDecode<hg::STCPDeleteAccountSuccess>(
        p, "stc_delete_account_success", 10, stcDecode,
        [&](const hg::STCPDeleteAccountSuccess&) { return true; });

    hg::makeServerToClientPacket(p, sDelFail);
    checkDecode<hg::STCPDeleteAccountFailure>(
        p, "stc_delete_account_failure", 11, stcDecode,
        [&](const hg::STCPDeleteAccountFailure& g)
        { return g.error == "delete failed"; });

    hg::makeServerToClientPacket(p, sTop);
    checkDecode<hg::STCPTopScores>(
        p, "stc_top_scores", 12, stcDecode,
        [&](const hg::STCPTopScores& g)
        { return g.levelValidator == LV && g.scores.size() == 2 &&
                 sameScore(g.scores[0], s1) && sameScore(g.scores[1], s2); });

    hg::makeServerToClientPacket(p, sOwn);
    checkDecode<hg::STCPOwnScore>(
        p, "stc_own_score", 13, stcDecode,
        [&](const hg::STCPOwnScore& g)
        { return g.levelValidator == LV && sameScore(g.score, s1); });

    hg::makeServerToClientPacket(p, sBothSome);
    checkDecode<hg::STCPTopScoresAndOwnScore>(
        p, "stc_top_scores_and_own_score_some", 14, stcDecode,
        [&](const hg::STCPTopScoresAndOwnScore& g)
        { return g.levelValidator == LV && g.scores.size() == 2 &&
                 sameScore(g.scores[0], s1) && sameScore(g.scores[1], s2) &&
                 g.ownScore.has_value() && sameScore(*g.ownScore, s1); });

    hg::makeServerToClientPacket(p, sBothNone);
    checkDecode<hg::STCPTopScoresAndOwnScore>(
        p, "stc_top_scores_and_own_score_none", 14, stcDecode,
        [&](const hg::STCPTopScoresAndOwnScore& g)
        { return g.levelValidator == LV && g.scores.size() == 2 &&
                 sameScore(g.scores[0], s1) && sameScore(g.scores[1], s2) &&
                 !g.ownScore.has_value(); });

    hg::makeServerToClientPacket(p, sStatus);
    checkDecode<hg::STCPServerStatus>(
        p, "stc_server_status", 15, stcDecode,
        [&](const hg::STCPServerStatus& g)
        { return g.protocolVersion == hg::PROTOCOL_VERSION &&
                 g.gameVersion == hg::GAME_VERSION &&
                 g.supportedLevelValidators.size() == 2 &&
                 g.supportedLevelValidators[0] == LV &&
                 g.supportedLevelValidators[1] == LV2; });

    // ------------------------------------------------------------------
    // Sodium KAT: seeded key exchange + the encrypted round trips
    // ------------------------------------------------------------------
    // crypto_kx_seed_keypair (what generateSodiumPSKeys does after its
    // randombytes_buf call): seed = the fixed pattern, secret + public
    // halves derived (libsodium >= 1.0.14 split: the stored secret key is
    // NOT the seed; sodiumoxide's kx::keypair_from_seed issues the same
    // call, so the Rust test reproduces these keypairs exactly).
    hg::SodiumPSKeys serverPs{};
    hg::SodiumPSKeys clientPs{};
    unsigned char seedS[32], seedC[32];
    for(int i = 0; i < 32; ++i) { seedS[i] = (unsigned char)i; seedC[i] = (unsigned char)(32 + i); }
    crypto_kx_seed_keypair(serverPs.keyPublic.data(), serverPs.keySecret.data(), seedS);
    crypto_kx_seed_keypair(clientPs.keyPublic.data(), clientPs.keySecret.data(), seedC);
    printf("kx_server_pk=%s\n",
        hexArr(serverPs.keyPublic.data(), 32).c_str());
    printf("kx_client_pk=%s\n",
        hexArr(clientPs.keyPublic.data(), 32).c_str());
    auto serverRt =
        hg::calculateServerSessionSodiumRTKeys(serverPs, clientPs.keyPublic)
            .value();
    auto clientRt =
        hg::calculateClientSessionSodiumRTKeys(clientPs, serverPs.keyPublic)
            .value();
    printf("kx_client_rx=%s\n",
        hexArr(clientRt.keyReceive.data(), 32).c_str());
    printf("kx_client_tx=%s\n",
        hexArr(clientRt.keyTransmit.data(), 32).c_str());
    printf("kx_server_rx=%s\n",
        hexArr(serverRt.keyReceive.data(), 32).c_str());
    printf("kx_server_tx=%s\n",
        hexArr(serverRt.keyTransmit.data(), 32).c_str());
    printf("kx_symmetry_ok=%d\n",
        (int)(clientRt.keyReceive == serverRt.keyTransmit &&
            clientRt.keyTransmit == serverRt.keyReceive));

    // Fixed transmit key for both directions (the hg API takes the key
    // explicitly; the nonce is the fixed fake-randombytes value).
    hg::SodiumTransmitKeyArray FIXED_KEY{};
    for(int i = 0; i < 32; ++i) FIXED_KEY[i] = 0x5A;

    const bool ctsEncOk =
        hg::makeClientToServerEncryptedPacket(FIXED_KEY, p, cStarted);
    printf("make_cts_encrypted_ok=%d\n", (int)ctsEncOk);
    printHex("cts_encrypted", p);
    checkDecodeKey<hg::CTSPStartedGame>(
        p, "cts_encrypted", 13, &FIXED_KEY, ctsDecode,
        [&](const hg::CTSPStartedGame& g)
        { return g.loginToken == LOGIN_TOKEN && g.levelValidator == LV; });

    const bool stcEncOk =
        hg::makeServerToClientEncryptedPacket(FIXED_KEY, p, sLoginOk);
    printf("make_stc_encrypted_ok=%d\n", (int)stcEncOk);
    printHex("stc_encrypted", p);
    checkDecodeKey<hg::STCPLoginSuccess>(
        p, "stc_encrypted", 6, &FIXED_KEY, stcDecode,
        [&](const hg::STCPLoginSuccess& g)
        { return g.loginToken == LOGIN_TOKEN && g.loginName == NAME; });

    printf("sodium_hash_oh_net=%s\n",
        hexArr((const unsigned char*)hg::sodiumHash("oh-net").data(), 32)
            .c_str());
    printf("sodium_hash_empty=%s\n",
        hexArr((const unsigned char*)hg::sodiumHash("").data(), 32).c_str());
    printf("random_u64=0x%016llx\n",
        (unsigned long long)hg::randomUInt64());

    // ------------------------------------------------------------------
    // Negative decode cases (exact PInvalid error strings)
    // ------------------------------------------------------------------
    {
        // "xh" + protocol 0 + version + heartbeat: bad 1st preamble byte.
        std::vector<unsigned char> b = {'x', 'h', 0, 2, 1, 7, 2};
        sf::Packet q;
        appendRaw(q, b);
        checkInvalid(q, "cts_bad_preamble_1st",
            "Error, preamble 1st byte has value 'x, which doesn't match "
            "expected value 'o'\n");
    }
    {
        // "oh" + protocol 1: bad protocol version (expected value is the
        // uint8_t 0, which the C++ ostream prints as a NUL character).
        std::vector<unsigned char> b = {'o', 'h', 1, 2, 1, 7, 2};
        sf::Packet q;
        appendRaw(q, b);
        // The expected value contains a NUL byte (uint8_t 0 streamed as a
        // char) — build the string in pieces (a string literal would be
        // truncated at the NUL by the strlen-based ctor).
        std::string expected(
            "Error, protocol version has value '\x01, which doesn't match "
            "expected value '");
        expected += '\0';
        expected += "'\n";
        checkInvalid(q, "cts_bad_protocol", expected);
    }
    {
        // "o" only: 2nd preamble byte missing.
        std::vector<unsigned char> b = {'o'};
        sf::Packet q;
        appendRaw(q, b);
        checkInvalid(q, "cts_truncated_preamble",
            "Error extracting single object\n"
            "Error extracting preamble 2st byte\n");
    }
    {
        // Preamble + protocol but the game-version bytes are missing.
        std::vector<unsigned char> b = {'o', 'h', 0};
        sf::Packet q;
        appendRaw(q, b);
        checkInvalid(q, "cts_truncated_game_version",
            "Error extracting single object\n"
            "Error extracting major version\n");
    }
    {
        // Full header, unknown type byte 99.
        std::vector<unsigned char> b = {'o', 'h', 0, 2, 1, 7, 99};
        sf::Packet q;
        appendRaw(q, b);
        checkInvalid(q, "cts_unknown_type", "Unknown packet type '99'\n");
    }
    {
        // Type 0 (PInvalid): no PInvalid arm in the dispatch (and the C++
        // API cannot even encode one) — a hand-crafted type-0 packet falls
        // through to "unknown type" before its fields are read.
        std::vector<unsigned char> b = {
            'o', 'h', 0, 2, 1, 7, 0, 0, 0, 0, 7, 'i', 'n', 'v', 'a', 'l', 'i',
            'd'};
        sf::Packet q;
        appendRaw(q, b);
        checkInvalid(q, "cts_type0_not_decodable",
            "Unknown packet type '0'\n");
    }
    {
        // CTSPRegister with a truncated `name` string (declares 10 bytes,
        // 3 present). The C++ extractAllMembers keeps reading the fields
        // after the first failure (the SFML packet is now invalid), so the
        // error carries the per-field lines for fields #1 AND #2.
        std::vector<unsigned char> b = {
            'o', 'h', 0, 2, 1, 7, 5,
            0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, // steamId
            0, 0, 0, 10, 't', 'e', 's'}; // name: len 10, only 3 bytes
        sf::Packet q;
        appendRaw(q, b);
        checkInvalid(q, "cts_truncated_register",
            "Error extracting single object\n"
            "Error decoding field #1 \n"
            "Error extracting single object\n"
            "Error decoding field #2 \n");
    }
    {
        // Encrypted type without a receive key.
        std::vector<unsigned char> b = {'o', 'h', 0, 2, 1, 7, 1};
        sf::Packet q;
        appendRaw(q, b);
        std::ostringstream oss;
        auto got = hg::decodeClientToServerPacket(nullptr, oss, q);
        bool ok = got.index() == 0 &&
            std::get<hg::PInvalid>(got).error ==
            "Cannot decode encrypted message without receive key\n";
        printf("neg_cts_encrypted_no_key ok=%d\n", (int)ok);
        if(!ok)
            printErr("cts_encrypted_no_key",
                std::get<hg::PInvalid>(got).error);
    }
    {
        // Encrypted type with the wrong receive key.
        hg::SodiumReceiveKeyArray badKey{};
        for(int i = 0; i < 32; ++i) badKey[i] = 0x5B;
        std::vector<unsigned char> b = {'o', 'h', 0, 2, 1, 7, 1};
        for(int i = 0; i < 24; ++i) b.push_back((unsigned char)i);
        b.insert(b.end(), {0, 0, 0, 0, 0, 0, 0, 32}); // messageLength
        b.insert(b.end(), {0, 0, 0, 0, 0, 0, 0, 48}); // ciphertextLength
        for(int i = 0; i < 48; ++i) b.push_back((unsigned char)(i * 7));
        sf::Packet q;
        appendRaw(q, b);
        std::ostringstream oss;
        auto got = hg::decodeClientToServerPacket(&badKey, oss, q);
        bool ok = got.index() == 0 &&
            std::get<hg::PInvalid>(got).error ==
            "Failure decrypting encrypted client message\n";
        printf("neg_cts_encrypted_bad_key ok=%d\n", (int)ok);
        if(!ok)
            printErr("cts_encrypted_bad_key",
                std::get<hg::PInvalid>(got).error);
    }
    {
        // Encrypted type with a truncated ciphertext (declares 36 bytes,
        // 10 present): the per-byte read loop fails at index 10.
        std::vector<unsigned char> b = {'o', 'h', 0, 2, 1, 7, 1};
        for(int i = 0; i < 24; ++i) b.push_back((unsigned char)i);
        b.insert(b.end(), {0, 0, 0, 0, 0, 0, 0, 20}); // messageLength
        b.insert(b.end(), {0, 0, 0, 0, 0, 0, 0, 36}); // ciphertextLength
        for(int i = 0; i < 10; ++i) b.push_back((unsigned char)(i * 3));
        sf::Packet q;
        appendRaw(q, b);
        std::ostringstream oss;
        auto got =
            hg::decodeClientToServerPacket(&FIXED_KEY, oss, q);
        bool ok = got.index() == 0 &&
            std::get<hg::PInvalid>(got).error ==
            "Error decoding client ciphertext at index '10'\n";
        printf("neg_cts_encrypted_truncated ok=%d\n", (int)ok);
        if(!ok)
            printErr("cts_encrypted_truncated",
                std::get<hg::PInvalid>(got).error);
    }
    {
        // STC direction: unknown type.
        std::vector<unsigned char> b = {'o', 'h', 0, 2, 1, 7, 99};
        sf::Packet q;
        appendRaw(q, b);
        std::ostringstream oss;
        auto got = hg::decodeServerToClientPacket(nullptr, oss, q);
        bool ok = got.index() == 0 &&
            std::get<hg::PInvalid>(got).error == "Unknown packet type '99'\n";
        printf("neg_stc_unknown_type ok=%d\n", (int)ok);
        if(!ok)
            printErr("stc_unknown_type",
                std::get<hg::PInvalid>(got).error);
    }
    return 0;
}
