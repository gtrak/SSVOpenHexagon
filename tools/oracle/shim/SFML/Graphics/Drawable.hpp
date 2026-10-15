#pragma once
#include <SFML/Graphics/RenderTarget.hpp>
namespace sf {
class Drawable { public: virtual ~Drawable() = default; protected: virtual void draw(RenderTarget&, const RenderStates&) const; }; }
