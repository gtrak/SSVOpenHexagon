#pragma once
#include <SFML/System/Vector2.hpp>
#include <SFML/Graphics/Color.hpp>
namespace sf {
struct Vertex {
    Vector2f position;
    Color color;
    Vector2f texCoords;
    Vertex() = default;
    Vertex(const Vector2f& p) : position(p) {}
    Vertex(const Vector2f& p, const Color& c) : position(p), color(c) {}
    Vertex(const Vector2f& p, const Color& c, const Vector2f& t) : position(p), color(c), texCoords(t) {}
};
}
