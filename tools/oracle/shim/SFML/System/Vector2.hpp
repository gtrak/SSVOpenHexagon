#pragma once
#include <cmath>
namespace sf {
template <typename T> struct Vector2 {
    T x{}, y{};
    Vector2() = default;
    Vector2(T a, T b) : x(a), y(b) {}
    template <typename U> Vector2(const Vector2<U>& v) : x(T(v.x)), y(T(v.y)) {}
    Vector2& operator+=(const Vector2& r){ x+=r.x; y+=r.y; return *this; }
    Vector2& operator-=(const Vector2& r){ x-=r.x; y-=r.y; return *this; }
    Vector2& operator*=(T s){ x*=s; y*=s; return *this; }
    T cross(const Vector2& r) const { return x*r.y - y*r.x; }
    T dot(const Vector2& r) const { return x*r.x + y*r.y; }
    // SFML normalized()
    Vector2 normalized() const {
        const float len = std::sqrt(float(x)*float(x)+float(y)*float(y));
        return len ? Vector2(x/len, y/len) : Vector2{};
    }
};
template <typename T> Vector2<T> operator+(Vector2<T> l, const Vector2<T>& r){ return {l.x+r.x,l.y+r.y}; }
template <typename T> Vector2<T> operator-(Vector2<T> l, const Vector2<T>& r){ return {l.x-r.x,l.y-r.y}; }
template <typename T> Vector2<T> operator*(Vector2<T> l, T s){ return {l.x*s,l.y*s}; }
template <typename T> Vector2<T> operator*(T s, Vector2<T> r){ return {r.x*s,r.y*s}; }
using Vector2f = Vector2<float>;
using Vector2i = Vector2<int>;
using Vector2u = Vector2<unsigned>;
} // namespace sf
