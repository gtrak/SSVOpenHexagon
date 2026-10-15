// Compiles the REAL Replay.cpp against a minimal SFML::Packet shim, emitting
// golden bytes for the Rust port to match.
#include <SFML/Network/Packet.hpp>
#include "SSVOpenHexagon/Core/Replay.hpp"
#include <cstdio>
#include <cstring>
#include <cstdint>

int main(){
    hg::replay_file rf;
    rf._version = 5;
    rf._player_name = "tester";
    rf._seed = 12345ull;
    rf._pack_id = "ohvrvanilla";
    rf._level_id = "cube_1_apeirogon";
    rf._first_play = true;
    rf._difficulty_mult = 1.6f;
    rf._played_score = 3600.0;
    rf._data.record_input(false,false,false,false);
    rf._data.record_input(false,true ,false,false);
    rf._data.record_input(true ,false,true ,false);
    rf._data.record_input(false,false,false,false);
    rf._data.record_input(false,true ,false,false);
    rf._data.record_input(true ,false,false,true );

    static std::byte buf[1<<20];
    auto sr = rf.serialize(buf, sizeof(buf));
    printf("serialized_bytes=%zu success=%d\n", sr.written_bytes(), (int)(bool)sr);
    printf("HEX=");
    for(std::size_t i=0;i<sr.written_bytes();++i) printf("%02x", (unsigned)(unsigned char)buf[i]);
    printf("\n");
    printf("PLAYED_SECONDS=%.9g\n", rf.played_seconds());
    auto crf = hg::compress_replay_file(rf);
    if(crf){
      printf("compressed_bytes=%zu\n", crf->_data.size());
      printf("CZHEX=");
      for(unsigned char c : crf->_data) printf("%02x", (unsigned)c);
      printf("\n");
      auto back = hg::decompress_replay_file(*crf);
      printf("roundtrip_equal=%d\n", (back && (*back == rf)) ? 1 : 0);
    } else printf("compression_failed\n");
    sf::Packet p; (void)rf.serialize_to_packet(p);
    printf("packet_bytes=%zu\n", p.size());
    printf("PHEX=");
    for(std::size_t i=0;i<p.size();++i) printf("%02x",(unsigned)(unsigned char)p.data()[i]);
    printf("\n");

    // ---- fixture 2 (issue 009c): the `alwaysSpinRight` input stream ----
    // Every recorded tick is record_input(false,true,false,false) — the
    // C++ policy block (HGUpdate.cpp:660-666) + the CW-only resolve branch
    // (movement = 1 = right bit) — so each input byte is 0x02 (bit order
    // pinned by fixture 1's note). Same envelope the Rust determinism
    // harness records (version 0, C++ test profile name).
    const unsigned NSTREAM = 12;
    hg::replay_file rf2;
    rf2._version = 0;
    rf2._player_name = "testProfile";
    rf2._seed = 987654321ull;
    rf2._pack_id = "ohvrvanilla_vittorio_romeo_cube_1";
    rf2._level_id = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";
    rf2._first_play = true;
    rf2._difficulty_mult = 1.0f;
    rf2._played_score = 720.0;
    for(unsigned i = 0; i < NSTREAM; ++i)
        rf2._data.record_input(false, true, false, false);
    auto sr2 = rf2.serialize(buf, sizeof(buf));
    printf("stream_serialized_bytes=%zu success=%d\n", sr2.written_bytes(), (int)(bool)sr2);
    printf("STREAMHEX=");
    for(std::size_t i=0;i<sr2.written_bytes();++i) printf("%02x", (unsigned)(unsigned char)buf[i]);
    printf("\n");
    auto crf2 = hg::compress_replay_file(rf2);
    if(crf2){
      printf("stream_compressed_bytes=%zu\n", crf2->_data.size());
      printf("STREAMCZHEX=");
      for(unsigned char c : crf2->_data) printf("%02x", c);
      printf("\n");
    } else printf("stream_compression_failed\n");
    return 0;
}
