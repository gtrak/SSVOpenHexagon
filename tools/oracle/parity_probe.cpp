// Parity corpus probe (issue 23). Compiles the REAL C++ sources
// (Core/RandomNumberGenerator.cpp, Core/Replay.cpp, Utils/BuildPackId.cpp,
// Utils/LevelValidator.cpp) against the repo's minimal SFML shims and emits
// the C++-derived JSON corpus files pinned under
// .agents/plans/001-rust-port/parity/ — byte-identical regeneration via
// tools/parity.sh (the committed script).
//
// Emits three documents, each introduced by a marker line:
//   === corpus: rng_streams.json ===
//   === corpus: replay_serialization.json ===
//   === corpus: pack_ids.json ===
//
// The corpus fixtures (seeds, replay envelopes, input counts/scores) mirror
// the pinned corpus: the level-run entries (parity/level_runs.json,
// rust-self-consistent) fix the alwaysSpinRight input count N and score S
// per level pair; this probe pins the C++ serialization of exactly those
// replay files (seed 987654321, N inputs of 0x02, score S).
#include "SSVOpenHexagon/Core/Replay.hpp"
#include "SSVOpenHexagon/Core/RandomNumberGenerator.hpp"
#include "SSVOpenHexagon/Utils/BuildPackId.hpp"
#include "SSVOpenHexagon/Utils/LevelValidator.hpp"
#include <SFML/Network/Packet.hpp>

#include <cstdio>
#include <cstdint>
#include <cstring>
#include <string>

namespace {

void hexOf(const unsigned char* p, const std::size_t n)
{
    for(std::size_t i = 0; i < n; ++i)
        printf("%02x", (unsigned)p[i]);
}

void f32Bits(const float f)
{
    std::uint32_t b;
    std::memcpy(&b, &f, 4);
    printf("\"0x%08x\"", b);
}

void f64Bits(const double d)
{
    std::uint64_t b;
    std::memcpy(&b, &d, 8);
    printf("\"0x%016llx\"", (unsigned long long)b);
}

/// Serializes one corpus replay fixture (file + compressed + packet bytes).
void replayFixture(const char* name, const hg::replay_file& rf,
    const bool last)
{
    static std::byte buf[1 << 20];
    const auto sr = rf.serialize(buf, sizeof(buf));
    if(!sr)
    {
        printf("SERIALIZE_FAILURE %s\n", name);
        std::exit(1);
    }
    printf("    \"%s\": {\n", name);
    printf("      \"version\": %u,\n", (unsigned)rf._version);
    printf("      \"player_name\": \"%s\",\n", rf._player_name.c_str());
    printf("      \"seed\": %llu,\n", (unsigned long long)rf._seed);
    printf("      \"pack_id\": \"%s\",\n", rf._pack_id.c_str());
    printf("      \"level_id\": \"%s\",\n", rf._level_id.c_str());
    printf("      \"first_play\": %s,\n", rf._first_play ? "true" : "false");
    printf("      \"difficulty_mult_bits\": ");
    f32Bits(rf._difficulty_mult);
    printf(",\n");
    printf("      \"played_score_bits\": ");
    f64Bits(rf._played_score);
    printf(",\n");
    printf("      \"input_count\": %zu,\n", rf._data.size());
    // The exact input bytes the Rust harness records: uniform (a single
    // byte repeated — the alwaysSpinRight fixtures) or the explicit list.
    {
        const std::uint8_t first =
            static_cast<std::uint8_t>(rf._data.at(0).to_ulong());
        bool uniform = true;
        for(std::size_t k = 1; k < rf._data.size(); ++k)
        {
            if(static_cast<std::uint8_t>(rf._data.at(k).to_ulong()) != first)
            {
                uniform = false;
                break;
            }
        }
        if(uniform)
        {
            printf("      \"input_byte_uniform\": %u,\n", (unsigned)first);
        }
        else
        {
            printf("      \"inputs\": [");
            for(std::size_t k = 0; k < rf._data.size(); ++k)
                printf("%s%u", k ? ", " : "",
                    (unsigned)rf._data.at(k).to_ulong());
            printf("],\n");
        }
    }
    printf("      \"serialized_bytes\": %zu,\n", sr.written_bytes());
    printf("      \"serialized_hex\": \"");
    hexOf(reinterpret_cast<const unsigned char*>(buf), sr.written_bytes());
    printf("\",\n");
    const auto crf = hg::compress_replay_file(rf);
    if(!crf)
    {
        printf("COMPRESSION_FAILURE %s\n", name);
        std::exit(1);
    }
    printf("      \"compressed_bytes\": %zu,\n", crf->_data.size());
    printf("      \"compressed_hex\": \"");
    for(const unsigned char c : crf->_data)
        printf("%02x", c);
    printf("\",\n");
    printf("      \"roundtrip_equal\": %d,\n",
        (hg::decompress_replay_file(*crf) == std::optional<hg::replay_file>{
             rf})
            ? 1
            : 0);
    sf::Packet p;
    if(!rf.serialize_to_packet(p))
    {
        printf("PACKET_FAILURE %s\n", name);
        std::exit(1);
    }
    printf("      \"packet_bytes\": %zu,\n", p.size());
    printf("      \"packet_hex\": \"");
    hexOf(reinterpret_cast<const unsigned char*>(p.data()), p.size());
    printf("\"\n    }%s\n", last ? "" : ",");
}

/// The corpus level pairs (parity/level_runs.json): the SEEDED REPLAY run
/// of the alwaysSpinRight stream per pair — N inputs (0x02 each), corpus
/// seed 987654321, score S (the replay run's played time; the live
/// alwaysSpinRight run's own count/score varies with its entropy-seeded RNG
/// — the onIncrement side-change draws, C++ HGUpdate.cpp:300-303 — so only
/// the seeded-replay outcome is pinned; see level_runs.json `evidence`).
struct PairFixture
{
    const char* key;
    const char* packId;
    const char* levelId;
    unsigned inputCount;
    double score;
};

} // namespace

int main()
{
    // ------------------------------------------------------------------
    // corpus 1: rng_streams.json — the `hg::random_number_generator`
    // wrapper streams (the ctor's advance(1) shift included): 8 int draws,
    // 16 f32 draws, 8 f64 draws per seed, each stream from a FRESH wrapper
    // (get_real<float> vs get_real<double> consume different numbers of
    // engine draws — the streams are not interleavable).
    // ------------------------------------------------------------------
    printf("=== corpus: rng_streams.json ===\n");
    printf("{\n");
    printf("  \"evidence\": \"cpp-probe: tools/oracle/parity_probe.cpp "
           "(real src/SSVOpenHexagon/Core/RandomNumberGenerator.cpp, "
           "libstdc++ uniform distributions)\",\n");
    printf("  \"note\": \"each stream is a fresh "
           "hg::random_number_generator{seed} (the ctor's advance(1) "
           "wrapper shift); real values are bit-exact (hex of the "
           "f32/f64 bits); MCG state pinning: seed | 3 — seeds 0..3 "
           "collapse to the same state (the corpus uses 0 and 4 to "
           "expose that)\",\n");
    printf("  \"seeds\": [\n");
    const unsigned long long seeds[7]
        = {0ULL, 4ULL, 12345ULL, 987654321ULL, 4294967295ULL, 4294967296ULL,
            18446744073709551615ULL};
    for(int s = 0; s < 7; ++s)
    {
        printf("    {\n      \"seed\": %llu,\n", seeds[s]);
        printf("      \"get_int_int_0_5\": [");
        {
            hg::random_number_generator rng{seeds[s]};
            for(int i = 0; i < 8; ++i)
                printf("%s%d", i ? ", " : "", rng.get_int<int>(0, 5));
        }
        printf("],\n");
        printf("      \"get_real_float_0_1_bits\": [");
        {
            hg::random_number_generator rng{seeds[s]};
            for(int i = 0; i < 16; ++i)
            {
                if(i)
                    printf(", ");
                f32Bits(rng.get_real<float>(0.f, 1.f));
            }
        }
        printf("],\n");
        printf("      \"get_real_double_0_1_bits\": [");
        {
            hg::random_number_generator rng{seeds[s]};
            for(int i = 0; i < 8; ++i)
            {
                if(i)
                    printf(", ");
                f64Bits(rng.get_real<double>(0.0, 1.0));
            }
        }
        printf("]\n    }%s\n", s == 6 ? "" : ",");
    }
    printf("  ]\n");
    printf("}\n");

    // ------------------------------------------------------------------
    // corpus 2: replay_serialization.json — byte-exact replay file /
    // compressed / packet forms (the real Replay.cpp) + the
    // `replay_player` cursor walk.
    // ------------------------------------------------------------------
    printf("=== corpus: replay_serialization.json ===\n");
    printf("{\n");
    printf("  \"evidence\": \"cpp-probe: tools/oracle/parity_probe.cpp "
           "(real src/SSVOpenHexagon/Core/Replay.cpp, zlib level 9)\",\n");
    printf("  \"note\": \"the alwaysSpinRight fixtures mirror "
           "parity/level_runs.json: seed 987654321, N inputs all 0x02 "
           "(record_input(false,true,false,false) — C++ alwaysSpinRight "
           "policy HGUpdate.cpp:660-666 + CW-only resolve branch), "
           "difficulty 1.0, first_play, the run's score\",\n");
    printf("  \"fixtures\": {\n");

    // ---- fixture A: the mixed keyboard stream (all four input bits
    // exercised; the oracle-vectors.md fixture-1 content) ----
    {
        hg::replay_file rf;
        rf._version = 5;
        rf._player_name = "tester";
        rf._seed = 12345ULL;
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
        replayFixture("mixed_keyboard_stream", rf, false);
    }

    // ---- fixtures B..E: the four corpus level pairs (alwaysSpinRight) ----
    const PairFixture pairs[4] = {
        {"always_spin_right_cube_1_apeirogon",
            "ohvrvanilla_vittorio_romeo_cube_1",
            "ohvrvanilla_vittorio_romeo_cube_1_apeirogon", 439, 1.725},
        {"always_spin_right_experimental_1_autotest0",
            "ohvrvanilla_vittorio_romeo_experimental_1",
            "ohvrvanilla_vittorio_romeo_experimental_1_autotest0", 1180,
            4.8125},
        {"always_spin_right_orthoplex_1_arcadia",
            "ohvrvanilla_vittorio_romeo_orthoplex_1",
            "ohvrvanilla_vittorio_romeo_orthoplex_1_arcadia", 456,
            1.7958333333333334},
        {"always_spin_right_orthoplex_1_bipolarity",
            "ohvrvanilla_vittorio_romeo_orthoplex_1",
            "ohvrvanilla_vittorio_romeo_orthoplex_1_bipolarity", 358,
            1.3875},
    };
    for(int i = 0; i < 4; ++i)
    {
        const PairFixture& pf = pairs[i];
        hg::replay_file rf;
        rf._version = 0;
        rf._player_name = "testProfile";
        rf._seed = 987654321ULL;
        rf._pack_id = pf.packId;
        rf._level_id = pf.levelId;
        rf._first_play = true;
        rf._difficulty_mult = 1.0f;
        rf._played_score = pf.score;
        for(unsigned k = 0; k < pf.inputCount; ++k)
            rf._data.record_input(false, true, false, false);
        replayFixture(pf.key, rf, false);
    }

    // ---- the `replay_player` cursor walk (the playback-cursor class) ----
    {
        hg::replay_data rd;
        rd.record_input(false, false, false, false);
        rd.record_input(false, true, false, false);
        rd.record_input(true, false, true, false);
        rd.record_input(false, false, false, false);
        rd.record_input(false, true, false, false);
        rd.record_input(true, false, false, true);
        hg::replay_player rp{rd};
        printf("    \"replay_player_walk\": {\n");
        printf("      \"stream\": [0, 2, 5, 0, 2, 9],\n");
        printf("      \"walk\": [");
        std::size_t consumed = 0;
        while(!rp.done())
        {
            const auto ib = rp.get_current_and_move_forward();
            printf("%s%u", consumed ? ", " : "",
                (unsigned)ib.to_ulong());
            ++consumed;
        }
        printf("],\n");
        printf("      \"done_after\": %zu,\n", consumed);
        rp.reset();
        printf("      \"after_reset_first\": %u,\n",
            (unsigned)rp.get_current_and_move_forward().to_ulong());
        printf("      \"after_reset_size_unchanged\": %zu\n", rd.size());
        printf("    }\n");
    }
    printf("  }\n");
    printf("}\n");

    // ------------------------------------------------------------------
    // corpus 3: pack_ids.json — `buildPackId` + `getLevelValidator` for
    // every shipped _RELEASE pack (the real BuildPackId.cpp +
    // LevelValidator.cpp). The base pack ships NO levels: its validator
    // pin uses the documented synthetic level id
    // `<packId>_nolevels` (format coverage only).
    // ------------------------------------------------------------------
    printf("=== corpus: pack_ids.json ===\n");
    printf("{\n");
    printf("  \"evidence\": \"cpp-probe: tools/oracle/parity_probe.cpp "
           "(real src/SSVOpenHexagon/Utils/BuildPackId.cpp + "
           "Utils/LevelValidator.cpp)\",\n");
    printf("  \"packs\": [\n");
    struct PackFixture
    {
        const char* folder;
        const char* disambiguator;
        const char* author;
        const char* name;
        int version;
        const char* firstLevel; // nullptr = no levels (synthetic id)
    };
    const PackFixture packs[8] = {
        {"artwork", "thing", "Synth Morxemplum", "Artwork", 1,
            "pointless"},
        {"base", "ohvrvanilla", "vittorio romeo", "base", 1, nullptr},
        {"cube", "ohvrvanilla", "vittorio romeo", "cube", 1, "apeirogon"},
        {"experimental", "ohvrvanilla", "vittorio romeo", "experimental", 1,
            "3D_required"},
        {"hypercube", "ohvrvanilla", "vittorio romeo", "hypercube", 1,
            "acceleradiant"},
        {"orthoplex", "ohvrvanilla", "vittorio romeo", "orthoplex", 1,
            "arcadia"},
        {"tutorial", "ohvrvanilla", "vittorio romeo", "tutorial", 1,
            "babysteps"},
        {"workshopexample", "ohvrworkshopexample", "vittorio romeo",
            "example", 1, "examplelevel"},
    };
    for(int i = 0; i < 8; ++i)
    {
        const PackFixture& pk = packs[i];
        const std::string packId
            = hg::Utils::buildPackId(pk.disambiguator, pk.author, pk.name,
                pk.version);
        const std::string levelId
            = pk.firstLevel
                ? (packId + "_" + std::string(pk.firstLevel))
                : (packId + "_nolevels");
        printf("    {\n");
        printf("      \"folder\": \"%s\",\n", pk.folder);
        printf("      \"disambiguator\": \"%s\",\n", pk.disambiguator);
        printf("      \"author\": \"%s\",\n", pk.author);
        printf("      \"name\": \"%s\",\n", pk.name);
        printf("      \"version\": %d,\n", pk.version);
        printf("      \"pack_id\": \"%s\",\n", packId.c_str());
        printf("      \"validator_level_id\": \"%s\",\n", levelId.c_str());
        printf("      \"validator_diff_mult\": 1.6,\n");
        printf("      \"validator\": \"%s\"\n",
            hg::Utils::getLevelValidator(levelId, 1.6f).c_str());
        printf("    }%s\n", i == 7 ? "" : ",");
    }
    printf("  ]\n");
    printf("}\n");
    return 0;
}
