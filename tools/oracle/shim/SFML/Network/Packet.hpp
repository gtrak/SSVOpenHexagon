#pragma once
// Minimal shim of the SFML fork's sf::Packet for SSVOpenHexagon's
// Replay.cpp / Online/Shared.cpp.
// ENCODING (verified against the fork's src/SFML/Network/Packet.cpp):
//   bool/int8/uint8  -> 1 raw byte
//   int16/uint16     -> 2 bytes BIG-ENDIAN (htons)
//   int32/uint32     -> 4 bytes BIG-ENDIAN (htonl)
//   int64/uint64     -> 8 bytes BIG-ENDIAN (manual)
//   float/double     -> raw host bytes (memcpy, little-endian on x86!)
//   std::string      -> u32 BIG-ENDIAN length, then raw bytes
//   append(raw,n)    -> raw bytes
// Read-failure semantics (fork Packet::checkSize, :589): a read that does
// not fit sets the invalid flag; the value is left unchanged and every
// subsequent read fails (the probe relies on this "poison" behavior).
#include <vector>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
namespace sf {

class Packet {
    std::vector<char> d;
    std::size_t r = 0;
    bool valid = true;
    void put_be(std::uint64_t v, int n) {
        for (int i = n - 1; i >= 0; --i) { char c = char((v >> (8*i)) & 0xFF); d.push_back(c); }
    }
    bool get_be(std::uint64_t& out, int n) {
        if (!valid || r + std::size_t(n) > d.size()) { valid = false; return false; }
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
    std::size_t getDataSize() const { return d.size(); }
    const char* data() const { return d.data(); }
    const void* getData() const { return d.empty() ? nullptr : d.data(); }
    std::size_t getReadPosition() const { return r; }
    bool endOfPacket() const { return r >= d.size(); }
    void clear() { d.clear(); r = 0; valid = true; }
    operator bool() const { return valid; }

    Packet& operator<<(bool v)          { return *this << std::uint8_t(v ? 1 : 0); }
    Packet& operator<<(std::int8_t v)   { return *this << std::uint8_t(std::uint8_t(v)); }
    Packet& operator<<(std::uint8_t v)  { append(&v, 1); return *this; }
    Packet& operator<<(std::int16_t v)  { return *this << std::uint16_t(std::uint16_t(v)); }
    Packet& operator<<(std::uint16_t v) { put_be(v, 2); return *this; }
    Packet& operator<<(std::int32_t v)  { return *this << std::uint32_t(std::uint32_t(v)); }
    Packet& operator<<(std::uint32_t v) { put_be(v, 4); return *this; }
    Packet& operator<<(std::int64_t v)  { return *this << std::uint64_t(std::uint64_t(v)); }
    Packet& operator<<(std::uint64_t v) { put_be(v, 8); return *this; }
    Packet& operator<<(float v)         { append(&v, sizeof(v)); return *this; }
    Packet& operator<<(double v)        { append(&v, sizeof(v)); return *this; }
    Packet& operator<<(const char* s)   { return *this << std::string(s ? s : ""); }
    Packet& operator<<(const std::string& s) {
        *this << std::uint32_t(s.size());
        if (!s.empty()) append(s.data(), s.size());
        return *this;
    }

    Packet& operator>>(bool& v) {
        std::uint8_t u;
        if ((*this >> u)) v = (u != 0);
        return *this;
    }
    Packet& operator>>(std::int8_t& v)  { std::uint8_t u; if ((*this >> u)) v = std::int8_t(u); return *this; }
    Packet& operator>>(std::uint8_t& v) { if (r + 1 > d.size()) { valid = false; return *this; } v = std::uint8_t(d[r++]); return *this; }
    Packet& operator>>(std::int16_t& v) { std::uint16_t u; if ((*this >> u)) v = std::int16_t(u); return *this; }
    Packet& operator>>(std::uint16_t& v){ std::uint64_t t; if (get_be(t, 2)) v = std::uint16_t(t); return *this; }
    Packet& operator>>(std::int32_t& v) { std::uint32_t u; if ((*this >> u)) v = std::int32_t(u); return *this; }
    Packet& operator>>(std::uint32_t& v){ std::uint64_t t; if (get_be(t, 4)) v = std::uint32_t(t); return *this; }
    Packet& operator>>(std::int64_t& v) { std::uint64_t t; if (get_be(t, 8)) v = std::int64_t(t); return *this; }
    Packet& operator>>(std::uint64_t& v){ get_be(v, 8); return *this; }
    Packet& operator>>(float& v)        { if (!valid || r + sizeof(v) > d.size()) { valid = false; return *this; } std::memcpy(&v, &d[r], sizeof(v)); r += sizeof(v); return *this; }
    Packet& operator>>(double& v)       { if (!valid || r + sizeof(v) > d.size()) { valid = false; return *this; } std::memcpy(&v, &d[r], sizeof(v)); r += sizeof(v); return *this; }
    Packet& operator>>(std::string& s)  {
        std::uint32_t len = 0;
        *this >> len;
        s.clear();
        if (len > 0) {
            if (!valid || r + len > d.size()) { valid = false; return *this; }
            s.assign(&d[r], len);
            r += len;
        }
        return *this;
    }
};
} // namespace sf
