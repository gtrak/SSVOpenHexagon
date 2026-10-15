#pragma once
#include <SFML/System/Vector2.hpp>
#include <SFML/Graphics/Color.hpp>
namespace sf { namespace Glsl {
struct Vec2 { float x,y; Vec2()=default; Vec2(float a,float b):x(a),y(b){} Vec2(const Vector2f& v):x(v.x),y(v.y){} };
struct Vec3 { float x,y,z; Vec3()=default; Vec3(float a,float b,float c):x(a),y(b),z(c){} Vec3(const Color& c):x(c.r/255.f),y(c.g/255.f),z(c.b/255.f){} };
struct Vec4 { float x,y,z,w; Vec4()=default; Vec4(float a,float b,float c,float d):x(a),y(b),z(c),w(d){} Vec4(const Color& c):x(c.r/255.f),y(c.g/255.f),z(c.b/255.f),w(c.a/255.f){} };
}}
