#pragma once
#include <SFML/Graphics/Vertex.hpp>
#include <SFML/Graphics/PrimitiveType.hpp>
#include <cstddef>
namespace sf { struct RenderStates {}; }
namespace sf {
class RenderTarget { public:
  void draw(const Vertex*, std::size_t, PrimitiveType, const RenderStates& = {}) {}
}; }
