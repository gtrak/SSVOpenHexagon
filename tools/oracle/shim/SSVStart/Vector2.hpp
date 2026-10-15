#pragma once
// Minimal subset of SSVStart/Utils/Vector2.hpp for the gameplay oracle.
#include <SFML/System/Vector2.hpp>
#include <SSVUtils/Core/Utils/Math.hpp>
#include <cmath>
namespace ssvs {
inline sf::Vector2f getVecFromRad(float rad, float dist){ return {dist*std::cos(rad), dist*std::sin(rad)}; }
inline sf::Vector2f getOrbitRad(const sf::Vector2f& center, float rad, float dist){ return center + getVecFromRad(rad, dist); }
inline float getMagSquared(const sf::Vector2f& v){ return v.x*v.x + v.y*v.y; }
inline float getDotProduct(const sf::Vector2f& a, const sf::Vector2f& b){ return a.x*b.x + a.y*b.y; }
inline float getDistEuclidean(const sf::Vector2f& a, const sf::Vector2f& b){ return std::sqrt(getMagSquared(b-a)); }
inline sf::Vector2f getNormalized(const sf::Vector2f& v){ const float l=std::sqrt(getMagSquared(v)); return l? sf::Vector2f{v.x/l,v.y/l} : sf::Vector2f{}; }
inline float getRad(const sf::Vector2f& v){ return std::atan2(v.y, v.x); }
}
