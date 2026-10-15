#pragma once
// Minimal shim of the SFML fork's sf::Packet for SSVOpenHexagon's Replay.cpp.
// ENCODING (verified against the fork's src/SFML/Network/Packet.cpp):
//   bool/int8/uint8  -> 1 raw byte
//   int16/uint16     -> 2 bytes BIG-ENDIAN (htons)
//   int32/uint32     -> 4 bytes BIG-ENDIAN (htonl)
//   int64/uint64     -> 8 bytes BIG-ENDIAN (manual)
//   float/double     -> raw host bytes (memcpy, little-endian on x86!)
//   std::string      -> u32 BIG-ENDIAN length, then raw bytes
//   append(raw,n)    -> raw bytes
#include <vector>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
namespace sf {

class Packet {
    std::vector<char> d; std::size_t r = 0;
    void put_be(std::uint64_t v, int n) {
        for (int i = n - 1; i >= 0; --i) { char c = char((v >> (8*i)) & 0xFF); d.push_back(c); }
    }
    bool get_be(std::uint64_t& out, int n) {
        if (r + std::size_t(n) > d.size()) return false;
        out = 0;
        for (int i = 0; i < n; ++i) out = (out << 8) | (unsigned char)d[r++];
        return true;
    }
public:
    void append(const void* p, std::size_t n) {
        const char* c = static_cast<const char*>(p);
        d.insert(d.end(), c, c + n);
    }
    std::size_t size() const { return d.size(); }
    const char* data() const { return d.data(); }

    Packet& operator<<(bool v)          { return *this << std::uint8_t(v ? 1 : 0); }
    Packet& operator<<(std::uint8_t v)  { append(&v, 1); return *this; }
    Packet& operator<<(std::uint16_t v) { put_be(v, 2); return *this; }
    Packet& operator<<(std::uint32_t v) { put_be(v, 4); return *this; }
    Packet& operator<<(std::uint64_t v) { put_be(v, 8); return *this; }
    Packet& operator<<(const std::string& s) { *this << std::uint32_t(s.size()); if(!s.empty()) append(s.data(), s.size()); return *this; }

    bool operator>>(std::uint8_t& v)  { if (r+1 > d.size()) return false; v = std::uint8_t(d[r++]); return true; }
    bool operator>>(std::uint32_t& v) { std::uint64_t t; if(!get_be(t,4)) return false; v = std::uint32_t(t); return true; }
    bool operator>>(std::uint64_t& v) { return get_be(v, 8); }
};
} // namespace sf
