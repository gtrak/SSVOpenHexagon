#pragma once
#include <cstdint>
#include <algorithm>
namespace sf {
struct Color {
    std::uint8_t r{}, g{}, b{}, a{255};
    Color() = default;
    Color(std::uint8_t r_, std::uint8_t g_, std::uint8_t b_, std::uint8_t a_=255) : r(r_),g(g_),b(b_),a(a_) {}
    static const Color Black, White, Red, Green, Blue, Transparent;
};
}
