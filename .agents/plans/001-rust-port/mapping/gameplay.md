# Gameplay Mapping (C++ → Rust)

Mechanical transcription of the gameplay-critical source files for the Rust port.
All formulas, constants, and type choices are transcribed verbatim. "FT" is always `float` (see Frametime section).

Source files covered (in this document):

1. `include/SSVOpenHexagon/Components/SpeedData.hpp`
2. `include/SSVOpenHexagon/Components/CWall.hpp` + `src/SSVOpenHexagon/Components/CWall.cpp`
3. `include/SSVOpenHexagon/Components/CPlayer.hpp` + `src/SSVOpenHexagon/Components/CPlayer.cpp`
4. `include/SSVOpenHexagon/Components/CCustomWall.hpp`, `CCustomWallHandle.hpp`, `CCustomWallManager.hpp` + `src/SSVOpenHexagon/Core/CCustomWallManager.cpp`
5. `include/SSVOpenHexagon/Core/HGStatus.hpp` + `src/SSVOpenHexagon/Core/HGStatus.cpp`
6. `include/SSVOpenHexagon/Data/LevelStatus.hpp`
7. `include/SSVOpenHexagon/Core/RandomNumberGenerator.hpp`, `RandomNumberGeneratorTypes.hpp` + `src/SSVOpenHexagon/Core/RandomNumberGenerator.cpp`
8. `include/SSVOpenHexagon/Utils/Ticker.hpp` + `src/SSVOpenHexagon/Utils/Ticker.cpp`
9. `include/SSVOpenHexagon/Utils/Timeline2.hpp` + `src/SSVOpenHexagon/Utils/Timeline2.cpp`
10. `include/SSVOpenHexagon/Core/CustomTimeline.hpp`, `CustomTimelineHandle.hpp`, `CustomTimelineManager.hpp` + `src/SSVOpenHexagon/Core/CustomTimelineManager.cpp`
11. `include/SSVOpenHexagon/Utils/FastVertexVector.hpp`
12. `include/SSVOpenHexagon/Utils/PointInPolygon.hpp`, `Geometry.hpp` + `src/SSVOpenHexagon/Utils/Geometry.cpp`, `Utils.cpp`
13. `src/SSVOpenHexagon/Core/HGUpdate.cpp`
14. `src/SSVOpenHexagon/Core/HexagonGame.cpp`

Conventions used in this document:

- "verbatim" code blocks reproduce the C++ exactly (comments trimmed only when redundant).
- All numeric literals are quoted exactly as in source (e.g. `5.f`, `0.5f`).
- `ssvu::FT` = `float` (see §Frametime). `ssvs` = SSVStart utils, `ssvu` = SSVUtils.
- SFML types: `sf::Vector2f` (float x,y), `sf::Color` (uint8 r,g,b,a; `sf::Color` has `.r/.g/.b/.a`).

---

## 1. SpeedData — `include/SSVOpenHexagon/Components/SpeedData.hpp`

`namespace hg`

```cpp
struct SpeedData
{
    float _speed;
    float _accel;
    float _min;
    float _max;
    float _pingPong;

    explicit SpeedData(float speed = 0, float accel = 0.f, float min = 0.f,
        float max = 0.f, bool pingPong = false) noexcept
        : _speed{speed},
          _accel{accel},
          _min{min},
          _max{max},
          _pingPong{pingPong ? -1.f : 1.f}
    {}

    void update(const ssvu::FT ft) noexcept
    {
        if(_accel == 0.f)
        {
            return;
        }

        _speed += _accel * ft;

        if(_speed > _max)
        {
            _speed = _max;
            _accel *= _pingPong;
        }
        else if(_speed < _min)
        {
            _speed = _min;
            _accel *= _pingPong;
        }
    }
};
```

Notes:
- `_pingPong` stores `-1.f` when the `pingPong` flag is true, else `1.f` (bool → float).
- When `pingPong == false` and speed hits a bound, `_accel *= 1.f` (no-op) — speed stays clamped at the bound every frame.
- Order of checks: `_speed > _max` checked first, then `else if _speed < _min`.
- All members are plain `float`.
- Default arg `speed = 0` (int literal 0, converted to float).

---

## 2. CWall — `include/SSVOpenHexagon/Components/CWall.hpp` + `src/.../CWall.cpp`

`namespace hg`. Includes: `SpeedData.hpp`, `PointInPolygon.hpp`, `FastVertexVector.hpp`, `<SFML/System/Vector2.hpp>`, `<array>`, `<cstdint>`.

### Class fields

| Field | Type | Default |
|---|---|---|
| `_vertexPositions` | `std::array<sf::Vector2f, 4>` | set in ctor |
| `_speed` | `SpeedData` | set in ctor |
| `_curve` | `SpeedData` | set in ctor |
| `_hueMod` | `float` | set in ctor |
| `_killed` | `bool` | `false` (ctor) |

Private methods:
- `void moveTowardsCenter(const float wallSpawnDist, const float radius, const sf::Vector2f& centerPos, const ssvu::FT ft);`
- `void moveCurve(const sf::Vector2f& centerPos, const ssvu::FT ft);`

Public methods (signatures):

```cpp
explicit CWall(const unsigned int sides, const float wallAngleLeft,
    const float wallAngleRight, const float wallSkewLeft,
    const float wallSkewRight, const sf::Vector2f& centerPos,
    const int side, const float thickness, const float distance,
    const SpeedData& speed, const SpeedData& curve, const float hueMod);

void update(const float wallSpawnDist, const float radius,
    const sf::Vector2f& centerPos, const ssvu::FT ft);

[[gnu::always_inline]] void moveVertexAlongCurveImpl(sf::Vector2f& vertex,
    const sf::Vector2f& centerPos, const float xSin, const float xCos) const noexcept;

[[gnu::always_inline]] float getCurveRadians(const ssvu::FT ft) const noexcept;

[[gnu::always_inline]] void moveVertexAlongCurve(sf::Vector2f& vertex,
    const sf::Vector2f& centerPos, const ssvu::FT ft) const noexcept;

void draw(sf::Color color, Utils::FastVertexVectorTris& wallQuads);
void setHueMod(float hueMod) noexcept;

[[nodiscard, gnu::always_inline]] const std::array<sf::Vector2f, 4>& getVertexPositions() const noexcept;
[[nodiscard, gnu::always_inline]] const SpeedData& getSpeed() const noexcept;
[[nodiscard, gnu::always_inline]] const SpeedData& getCurve() const noexcept;
[[nodiscard, gnu::always_inline]] bool isOverlapping(const sf::Vector2f& point) const noexcept;
[[nodiscard, gnu::always_inline]] constexpr bool isCustomWall() const noexcept;   // always false
[[nodiscard, gnu::always_inline]] std::uint8_t getKillingSide() const noexcept;   // always 0u
[[nodiscard, gnu::always_inline]] bool isDead() const noexcept;
```

### Ctor body (verbatim)

```cpp
CWall::CWall(const unsigned int sides, const float wallAngleLeft,
    const float wallAngleRight, const float wallSkewLeft,
    const float wallSkewRight, const sf::Vector2f& centerPos, const int side,
    const float thickness, const float distance, const SpeedData& speed,
    const SpeedData& curve, const float hueMod)
    : _speed{speed}, _curve{curve}, _hueMod{hueMod}, _killed{false}
{
    const float div{ssvu::tau / static_cast<float>(sides) * 0.5f};
    const float angle{div * 2.f * static_cast<float>(side)};

    const float angleN = angle - div;
    const float angleP = angle + div;

    const auto vecFromRad = [](const float rad, const float dist) {
        return sf::Vector2f{dist * std::cos(rad), dist * std::sin(rad)};
    };

    _vertexPositions[0] = centerPos + vecFromRad(angleN, distance);
    _vertexPositions[1] = centerPos + vecFromRad(angleP, distance);
    _vertexPositions[2] = centerPos + vecFromRad(angleP + wallAngleLeft,
                                          distance + thickness + wallSkewLeft);
    _vertexPositions[3] = centerPos + vecFromRad(angleN + wallAngleRight,
                                          distance + thickness + wallSkewRight);
}
```

Key facts:
- `ssvu::tau` = `6.28318530718f` (float).
- `div = tau / sides * 0.5f` (note: left-associative, i.e. `(tau / sides) * 0.5f`).
- `angle = div * 2.f * side`.
- Vertex ordering (0..3): (angleN, distance) → (angleP, distance) → (angleP + wallAngleLeft, distance + thickness + wallSkewLeft) → (angleN + wallAngleRight, distance + thickness + wallSkewRight).
- `wallSkewLeft` is applied to vertex 2 (the "P" side); `wallSkewRight` to vertex 3 (the "N" side).

### `update` (verbatim)

```cpp
void CWall::update(const float wallSpawnDist, const float radius,
    const sf::Vector2f& centerPos, const ssvu::FT ft)
{
    _speed.update(ft);
    _curve.update(ft);

    moveTowardsCenter(wallSpawnDist, radius, centerPos, ft);
    if(_curve._speed != 0.f)
    {
        moveCurve(centerPos, ft);
    }
}
```

### `moveTowardsCenter` (verbatim)

```cpp
void CWall::moveTowardsCenter(const float wallSpawnDist, const float radius,
    const sf::Vector2f& centerPos, const ssvu::FT ft)
{
    const float halfRadius{radius * 0.5f};
    const float outerBounds{wallSpawnDist * 1.1f};

    int pointsOutOfBounds{0};
    int pointsOnCenter{0};

    for(sf::Vector2f& vp : _vertexPositions)
    {
        const float xDistance = std::abs(vp.x - centerPos.x);
        const float yDistance = std::abs(vp.y - centerPos.y);

        if(xDistance < halfRadius && yDistance < halfRadius)
        {
            ++pointsOnCenter;
            continue;
        }

        if(xDistance > outerBounds || yDistance > outerBounds)
        {
            ++pointsOutOfBounds;
        }

        vp += (centerPos - vp).normalized() * _speed._speed * 5.f * ft;
    }

    if(pointsOnCenter == 4 || pointsOutOfBounds == 4)
    {
        _killed = true;
    }
}
```

Key facts:
- `(centerPos - vp).normalized()` — SFML `Vector2f::normalized()` (zero-vector guard: SFML's normalized() leaves vector as-is if magnitude is 0).
- Movement scale constant: `5.f`.
- AABB-ish test: x and y distances compared separately (`&&` for onCenter, `||` for outOfBounds).
- Wall is killed when all 4 vertices are on-center or all 4 out-of-bounds.
- Vertices on center are NOT moved (continue) — but note `pointsOutOfBounds` also not incremented for them.

### `moveCurve` + curve-rotation helpers (verbatim)

```cpp
// hpp:
[[gnu::always_inline]] void moveVertexAlongCurveImpl(sf::Vector2f& vertex,
    const sf::Vector2f& centerPos, const float xSin, const float xCos) const noexcept
{
    const float tempX = vertex.x - centerPos.x;
    const float tempY = vertex.y - centerPos.y;
    vertex.x = tempX * xCos - tempY * xSin + centerPos.x;
    vertex.y = tempX * xSin + tempY * xCos + centerPos.y;
}

[[gnu::always_inline]] float getCurveRadians(const ssvu::FT ft) const noexcept
{
    constexpr float divBy60 = 1.f / 60.f;
    return _curve._speed * divBy60 * ft;
}

[[gnu::always_inline]] void moveVertexAlongCurve(sf::Vector2f& vertex,
    const sf::Vector2f& centerPos, const ssvu::FT ft) const noexcept
{
    const float rad = getCurveRadians(ft);
    moveVertexAlongCurveImpl(vertex, centerPos, std::sin(rad), std::cos(rad));
}

// cpp:
void CWall::moveCurve(const sf::Vector2f& centerPos, const ssvu::FT ft)
{
    const float rad = getCurveRadians(ft);
    const float radSin = std::sin(rad);
    const float radCos = std::cos(rad);

    for(sf::Vector2f& vp : _vertexPositions)
    {
        moveVertexAlongCurveImpl(vp, centerPos, radSin, radCos);
    }
}
```

Curve rotation sign convention: standard 2D rotation with angle `rad = _curve._speed * (1/60) * ft` (per-frame incremental rotation; NOT cumulative — computed fresh each frame from the current `_curve._speed` value). Positive `_curve._speed` rotates counter-clockwise in screen space with y-down coordinates (i.e. mathematically clockwise-looking on screen, same convention as `getOrbitRad`/`getVecFromRad`).

### `draw` (verbatim)

```cpp
void CWall::draw(sf::Color color, Utils::FastVertexVectorTris& wallQuads)
{
    if(_hueMod != 0)
    {
        color = Utils::transformHue(color, _hueMod);
    }

    wallQuads.batch_unsafe_emplace_back_quad(color, _vertexPositions[0],
        _vertexPositions[1], _vertexPositions[2], _vertexPositions[3]);
}
```

(`Utils::transformHue` — see §PointInPolygon/Geometry section.)

### Other methods (verbatim)

```cpp
void CWall::setHueMod(float hueMod) noexcept { _hueMod = hueMod; }
// getVertexPositions -> _vertexPositions
// getSpeed -> _speed
// getCurve -> _curve
bool CWall::isOverlapping(const sf::Vector2f& point) const noexcept
{
    return Utils::pointInFourVertexPolygon(_vertexPositions[0],
        _vertexPositions[1], _vertexPositions[2], _vertexPositions[3], point);
}
constexpr bool CWall::isCustomWall() const noexcept { return false; }
std::uint8_t CWall::getKillingSide() const noexcept { return 0u; }
bool CWall::isDead() const noexcept { return _killed; }
```

---

## 3. CPlayer — `include/SSVOpenHexagon/Components/CPlayer.hpp` + `src/.../CPlayer.cpp`

`namespace hg`. Includes (cpp): CCustomWall.hpp, CWall.hpp, Color.hpp, Easing.hpp, Geometry.hpp, MoveTowards.hpp, PointInPolygon.hpp, Ticker.hpp, SSVStart/Utils/SFML.hpp, SSVUtils Frametime.hpp + Math.hpp, SFML Color/Vector2, `<algorithm>`, `<cmath>`.

### File-local constants (cpp, verbatim)

```cpp
inline constexpr float baseThickness{5.f};
inline constexpr float unfocusedTriangleWidth{3.f};
inline constexpr float focusedTriangleWidth{-1.5f};
inline constexpr float triangleWidthRange{
    unfocusedTriangleWidth - focusedTriangleWidth};   // = 4.5f
// (inside push-related code, at file scope just before the template method:)
inline constexpr float collisionPadding{0.5f};
```

### Class fields

| Field | Type | Default (ctor) |
|---|---|---|
| `_startPos` | `sf::Vector2f` | `pos` (ctor param) |
| `_pos` | `sf::Vector2f` | `pos` |
| `_prePushPos` | `sf::Vector2f` | `pos` — position before push; NOT updated after successful wall push |
| `_lastPos` | `sf::Vector2f` | `pos` — previous-frame position, adjusted to current frame's radius |
| `_hue` | `float` | `0` |
| `_angle` | `float` | `0` |
| `_lastAngle` | `float` | `0` |
| `_size` | `float` | `size` |
| `_speed` | `float` | `speed` |
| `_focusSpeed` | `float` | `focusSpeed` |
| `_dead` | `bool` | `false` |
| `_justSwapped` | `bool` | `false` |
| `_forcedMove` | `bool` | `false` — set when moved via `setPlayerAngle()` |
| `_radius` | `float` | `0.f` — cached radius for current frame |
| `_maxSafeDistance` | `float` | `0.f` — max distance to closest safe (non-colliding) position |
| `_currentSpeed` | `float` | `0.f` — cached player speed this frame |
| `_triangleWidth` | `float` | `unfocusedTriangleWidth` (3.f) |
| `_triangleWidthTransitionTime` | `float` | `0.f` |
| `_swapTimer` | `Ticker` | `Ticker{swapCooldown}` |
| `_swapBlinkTimer` | `Ticker` | `Ticker{6.f}` |
| `_deadEffectTimer` | `Ticker` | `Ticker{80.f, false}` (total 80, not running initially) |
| `_currTiltedAngle` | `float` | `0` |

Private methods:
- `void drawPivot(const unsigned int sides, const sf::Color& colorMain, Utils::FastVertexVectorTris& wallQuads, Utils::FastVertexVectorTris& capTris, const sf::Color& capColor);`
- `void drawDeathEffect(Utils::FastVertexVectorTris& wallQuads);`
- `template <typename Wall> [[nodiscard]] bool checkWallCollisionEscape(const Wall& wall, sf::Vector2f& pos, const float radiusSquared);`
- `void updateTriangleWidthTransition(const bool focused, const ssvu::FT ft);`

Public methods (signatures):

```cpp
explicit CPlayer(const sf::Vector2f& pos, const float swapCooldown,
    const float size, const float speed, const float focusSpeed) noexcept;

[[nodiscard, gnu::always_inline]] const sf::Vector2f& getPosition() const noexcept;  // _pos
[[nodiscard, gnu::always_inline]] float getPlayerAngle() const noexcept;           // _angle
void setPlayerAngle(const float newAng) noexcept;   // sets _angle = newAng; _forcedMove = true
void playerSwap();
void kill(const bool fatal);
void update(const bool focused, const bool swapEnabled, const ssvu::FT ft);
void updateInputMovement(const float movementDir, const float playerSpeedMult,
    const bool focused, const ssvu::FT ft);
void resetSwap(const float swapCooldown);
void setJustSwapped(const bool value);
void updatePosition(const float radius);
[[nodiscard]] sf::Color getColor(const sf::Color& colorPlayer) const;
[[nodiscard]] sf::Color getColorAdjustedForSwap(const sf::Color& colorPlayer) const;
void draw(const unsigned int sides, const sf::Color& colorMain,
    const sf::Color& colorPlayer, Utils::FastVertexVectorTris& wallQuads,
    Utils::FastVertexVectorTris& capTris, Utils::FastVertexVectorTris& playerTris,
    const sf::Color& capColor, const float angleTiltIntensity,
    const bool swapBlinkingEffect);
[[nodiscard]] bool push(const int movementDir, const float radius,
    const CWall& wall, const sf::Vector2f& mCenterPos, const float radiusSquared, ssvu::FT ft);
[[nodiscard]] bool push(const int movementDir, const float radius,
    const hg::CCustomWall& wall, const float radiusSquared, const ssvu::FT ft);
[[nodiscard]] bool getJustSwapped() const noexcept;
[[nodiscard, gnu::always_inline]] bool isReadyToSwap() const noexcept;  // !_swapTimer.isRunning()
[[nodiscard, gnu::always_inline]] bool hasChangedAngle() const noexcept; // _angle != _lastAngle
```

### `getColor` / `getColorAdjustedForSwap` (verbatim)

```cpp
[[nodiscard]] sf::Color CPlayer::getColor(const sf::Color& colorPlayer) const
{
    return !_deadEffectTimer.isRunning() ? colorPlayer
                                         : Utils::getColorFromHue(_hue / 360.f);
}

[[nodiscard]] sf::Color CPlayer::getColorAdjustedForSwap(
    const sf::Color& colorPlayer) const
{
    if(!_swapTimer.isRunning() && !_dead)
    {
        return Utils::getColorFromHue(
            std::fmod(_swapBlinkTimer.getCurrent() / 12.f, 0.2f));
    }

    return getColor(colorPlayer);
}
```

Pulse/beat-pulse math: swap blink hue = `fmod(_swapBlinkTimer.getCurrent() / 12.f, 0.2f)` — cycles hue through [0, 0.2) at 1/12 of the 6-second blink timer period.

### `draw` (verbatim)

```cpp
void CPlayer::draw(const unsigned int sides, const sf::Color& colorMain,
    const sf::Color& colorPlayer, Utils::FastVertexVectorTris& wallQuads,
    Utils::FastVertexVectorTris& capTris,
    Utils::FastVertexVectorTris& playerTris, const sf::Color& capColor,
    const float angleTiltIntensity, const bool swapBlinkingEffect)
{
    drawPivot(sides, colorMain, wallQuads, capTris, capColor);

    if(_deadEffectTimer.isRunning())
    {
        drawDeathEffect(wallQuads);
    }

    const float tiltedAngle =
        _angle + (_currTiltedAngle * ssvu::toRad(24.f) * angleTiltIntensity);

    const sf::Vector2f pLeft = ssvs::getOrbitRad(
        _pos, tiltedAngle - ssvu::toRad(100.f), _size + _triangleWidth);
    const sf::Vector2f pRight = ssvs::getOrbitRad(
        _pos, tiltedAngle + ssvu::toRad(100.f), _size + _triangleWidth);

    playerTris.reserve_more(3);
    playerTris.batch_unsafe_emplace_back(
        swapBlinkingEffect ? getColorAdjustedForSwap(colorPlayer)
                           : getColor(colorPlayer),
        ssvs::getOrbitRad(_pos, tiltedAngle, _size), pLeft, pRight);
}
```

Triangle geometry: apex at orbit radius `_size`, base vertices at `_size + _triangleWidth`, angles `tiltedAngle ± toRad(100.f)`. Tilt: `_currTiltedAngle * toRad(24.f) * angleTiltIntensity` added to `_angle`.

### `drawPivot` (verbatim)

```cpp
void CPlayer::drawPivot(const unsigned int sides, const sf::Color& colorMain,
    Utils::FastVertexVectorTris& wallQuads,
    Utils::FastVertexVectorTris& capTris, const sf::Color& capColor)
{
    const float div{ssvu::tau / sides * 0.5f};
    const float pRadius{_radius * 0.75f};

    for(auto i(0u); i < sides; ++i)
    {
        const float sAngle{div * 2.f * i};

        const sf::Vector2f p1{ssvs::getOrbitRad(_startPos, sAngle - div, pRadius)};
        const sf::Vector2f p2{ssvs::getOrbitRad(_startPos, sAngle + div, pRadius)};
        const sf::Vector2f p3{ssvs::getOrbitRad(_startPos, sAngle + div, pRadius + baseThickness)};
        const sf::Vector2f p4{ssvs::getOrbitRad(_startPos, sAngle - div, pRadius + baseThickness)};

        wallQuads.reserve_more_quad(1);
        wallQuads.batch_unsafe_emplace_back_quad(colorMain, p1, p2, p3, p4);

        capTris.reserve_more(3);
        capTris.batch_unsafe_emplace_back(capColor, p1, p2, _startPos);
    }
}
```

Pivot radius: `_radius * 0.75f` (the classic hexagon pivot is 75% of play radius). Note `ssvu::tau / sides` uses unsigned int `sides` (implicit conversion to float).

### `drawDeathEffect` (verbatim)

```cpp
void CPlayer::drawDeathEffect(Utils::FastVertexVectorTris& wallQuads)
{
    const float div{ssvu::tau / 6 * 0.5f};
    const float dRadius{_hue / 8.f};
    const float thickness{_hue / 20.f};

    const sf::Color colorMain{Utils::getColorFromHue((360.f - _hue) / 360.f)};

    for(auto i(0u); i < 6; ++i)
    {
        const float sAngle{div * 2.f * i};

        const sf::Vector2f p1{ssvs::getOrbitRad(_pos, sAngle - div, dRadius)};
        const sf::Vector2f p2{ssvs::getOrbitRad(_pos, sAngle + div, dRadius)};
        const sf::Vector2f p3{ssvs::getOrbitRad(_pos, sAngle + div, dRadius + thickness)};
        const sf::Vector2f p4{ssvs::getOrbitRad(_pos, sAngle - div, dRadius + thickness)};

        wallQuads.reserve_more_quad(1);
        wallQuads.batch_unsafe_emplace_back_quad(colorMain, p1, p2, p3, p4);
    }
}
```

Death effect: fixed 6 sides; radius grows as `_hue / 8.f` (`_hue` increases `18.f * ft` per frame, resets to 0 at 360); thickness `_hue / 20.f`; hue is inverted: `(360.f - _hue) / 360.f`.

### `playerSwap`, `kill` (verbatim)

```cpp
void CPlayer::playerSwap()
{
    _angle += ssvu::pi;
}

void CPlayer::kill(const bool fatal)
{
    _deadEffectTimer.restart();

    if(fatal)
    {
        _dead = true;

        // Avoid moving back position if the player had just swapped or the
        // player was forcibly moved by a lot via Lua scripting.
        if(!_justSwapped && ssvs::getDistEuclidean(_pos, _lastPos) < 24.f)
        {
            // Move back position to graphically show the tip of the triangle
            // hitting the wall rather than the center of the triangle.
            _pos = ssvs::getOrbitRad(_lastPos, _angle, -_size);
        }
    }
}
```

`ssvu::pi` = `3.14159265359f`. Swap = angle += pi (half turn). Fatal-kill position rollback: only if NOT just swapped AND Euclidean distance `_pos`↔`_lastPos` < `24.f`; new `_pos` = orbit of `_lastPos` at angle `_angle` with radius `-_size` (i.e. behind the triangle tip).

### `checkWallCollisionEscape` (collision-escape algorithm, verbatim)

```cpp
template <typename Wall>
[[nodiscard]] bool CPlayer::checkWallCollisionEscape(
    const Wall& wall, sf::Vector2f& pos, const float radiusSquared)
{
    // To find the closest wall side we intersect the circumference of the
    // possible player positions with the sides of the wall. We use the
    // intersection closest to the player's position as post collision target.
    // If an escape route could not be found player is killed.

    bool saved{false};
    sf::Vector2f vec1, vec2;
    float tempDistance, safeDistance{_maxSafeDistance};
    const unsigned int vxIncrement{wall.isCustomWall() ? 1u : 2u};
    const std::array<sf::Vector2f, 4>& wVertexes{wall.getVertexPositions()};

    // This is actually useless for normal walls, but if we removed
    // getKillingSide() for CWall we would have to write a separate
    // function just to do without this variable,
    const unsigned int killingSide{wall.getKillingSide()};

    const auto assignResult = [&]()
    {
        tempDistance = ssvs::getMagSquared(vec1 - pos);
        if(tempDistance < safeDistance)
        {
            pos = vec1;
            saved = true;
            safeDistance = tempDistance;
        }
    };

    for(unsigned int i{0u}, j{3u}; i < 4u; i += vxIncrement, j = i - 1)
    {
        if(j == killingSide)
        {
            continue;
        }

        switch(Utils::getLineCircleIntersection(
            vec1, vec2, wVertexes[i], wVertexes[j], radiusSquared))
        {
            case 1u:
            {
                assignResult();
                break;
            }
            case 2u:
            {
                if(ssvs::getMagSquared(vec1 - pos) >
                    ssvs::getMagSquared(vec2 - pos))
                {
                    vec1 = vec2;
                }
                assignResult();
                break;
            }
            default:
            {
                break;
            }
        }
    }

    return saved;
}
```

Algorithm details:
- `vxIncrement`: normal CWall walls share vertices with neighbors, so sides are visited with step 2 (pairs (0,3),(2,1) — wait, see loop below); custom walls use step 1 (all 4 sides individually).
- Loop: `i` starts 0, `j` starts 3, increments `i += vxIncrement`, `j = i - 1` (unsigned underflow: when `i==0`, `j = 0-1 = 3` in the first iteration because j is explicitly initialized to 3u; on subsequent iterations with step 2: (i=0,j=3), (i=2,j=1). With step 1: (i=0,j=3), (i=1,j=0), (i=2,j=1), (i=3,j=2)).
- For each wall side segment `(wVertexes[j], wVertexes[i])` — note segment is passed as (wVertexes[i], wVertexes[j]) to `getLineCircleIntersection(vec1, vec2, A=wVertexes[i], B=wVertexes[j], radiusSquared)` — compute circle (centered at `pos`, radius² = `radiusSquared`) intersections.
- 1 intersection → candidate `vec1`; 2 → pick closer of `vec1`/`vec2` (squared mag to `pos`), then candidate.
- Keep the candidate with smallest squared distance to `pos`, bounded by `_maxSafeDistance` (initial `safeDistance`). If no candidate within `_maxSafeDistance`, `saved == false` → death.
- `j == killingSide` side is skipped (custom walls: a side that must kill instead of allow escape).
- `Utils::getLineCircleIntersection` returns `unsigned int`: 0/1/2 (see Geometry section).

### `push` for CWall (verbatim)

```cpp
[[nodiscard]] bool CPlayer::push(const int movementDir, const float radius,
    const CWall& wall, const sf::Vector2f& centerPos, const float radiusSquared,
    const ssvu::FT ft)
{
    if(_dead)
    {
        return false;
    }

    sf::Vector2f testPos{_pos};
    sf::Vector2f pushVel{0.f, 0.f};

    // If it's a rotating wall push player in the direction the
    // wall is rotating by the appropriate amount, but only if the direction
    // of the rotation is different from the direction player is moving.
    // Save the position difference in case we need to do a second attempt
    // at saving player.
    const SpeedData& curveData{wall.getCurve()};
    if(curveData._speed != 0.f &&
        ssvu::getSign(curveData._speed) != movementDir)
    {
        wall.moveVertexAlongCurve(testPos, centerPos, ft);
        pushVel = testPos - _pos;
    }

    // If player is not moving calculate now...
    if(!movementDir && !_forcedMove)
    {
        _pos = testPos + ssvs::getNormalized(testPos - _prePushPos) *
                             (2.f * collisionPadding);
        _angle = ssvs::getRad(_pos);
        updatePosition(radius);
        return wall.isOverlapping(_pos);
    }

    // ...otherwise make testPos the position of the previous frame plus
    // the curving wall's velocity, and check an escape on that position.
    // ... (ASCII diagram in source: high-side-count levels)
    testPos = _lastPos + pushVel;
    if(wall.isOverlapping(testPos) ||
        !checkWallCollisionEscape(wall, testPos, radiusSquared))
    {
        return true;
    }

    // If player survived assign it the saving testPos, but displace it further
    // out the wall border, otherwise player would be lying right on top of the
    // border.
    _pos =
        testPos + ssvs::getNormalized(testPos - _prePushPos) * collisionPadding;
    _angle = ssvs::getRad(_pos);
    updatePosition(radius);
    return false;
}
```

Key facts:
- Return value `true` = player killed (collision not resolvable).
- Curve push only applied when `getSign(curve._speed) != movementDir` (moving against the wall's rotation). `ssvu::getSign` for signed types: `(0 < x) - (x < 0)` → -1/0/+1.
- Static (no input) case: pos = testPos + normalized(testPos - prePushPos) * (2 * collisionPadding) = `* 1.f`; then check overlap → return it.
- Moving case: `testPos = _lastPos + pushVel`; if overlapping OR no escape → death.
- Successful push: pos displaced by `normalized(testPos - _prePushPos) * collisionPadding` (0.5f).
- `_angle = ssvs::getRad(_pos)` — note `getRad(vec)` = `atan2(vec.y, vec.x)` — player angle is relative to ORIGIN (0,0), not center! Because `_pos` is in world coordinates where center is the origin (centerPos is (0,0) in world space — see HexagonGame: walls/player all positioned relative to a zero center; see HGUpdate).
- `ssvs::getRad` (1-arg) = `std::atan2(mVec.y, mVec.x)`.

### `push` for CCustomWall (verbatim)

```cpp
[[nodiscard]] bool CPlayer::push(const int movementDir, const float radius,
    const hg::CCustomWall& wall, const float radiusSquared, const ssvu::FT ft)
{
    (void)ft; // Currently unused.

    if(_dead)
    {
        return false;
    }

    const std::array<sf::Vector2f, 4>& wVertexes{wall.getVertexPositions()};
    const std::array<sf::Vector2f, 4>& wOldVertexes{
        wall.getOldVertexPositions()};
    sf::Vector2f pushVel{0.f, 0.f}, i1, i2;
    const unsigned int killingSide{wall.getKillingSide()};
    constexpr float pushDotThreshold{
        0.15f}; // 0.1 would be enough in most scenarios
                // but we raise it to 0.15 for really fast walls.

    for(unsigned int i{0u}, j{3u}; i < 4; j = i++)
    {
        if(j == killingSide)
        {
            continue;
        }

        const std::array<sf::Vector2f, 4> collisionPolygon{
            wVertexes[i], wOldVertexes[i], wOldVertexes[j], wVertexes[j]};

        if(Utils::pointInPolygon<4>(collisionPolygon, _lastPos.x, _lastPos.y))
        {
            // For a side to be an effective source of push it must have
            // intersected the player's positions circle both now and the
            // previous frame.
            if(Utils::getLineCircleClosestIntersection(i1, _lastPos,
                   wOldVertexes[i], wOldVertexes[j], radiusSquared) &&
                Utils::getLineCircleClosestIntersection(
                    i2, _lastPos, wVertexes[i], wVertexes[j], radiusSquared))
            {
                pushVel = i2 - i1;
                if(std::abs(ssvs::getDotProduct(ssvs::getNormalized(pushVel),
                       ssvs::getNormalized(_lastPos))) > pushDotThreshold)
                {
                    pushVel = {0.f, 0.f};
                }
            }
            break;
        }
    }

    // Player is not moving exit now.
    if(!movementDir && !_forcedMove)
    {
        _pos += pushVel;
        _pos +=
            ssvs::getNormalized(_pos - _prePushPos) * (2.f * collisionPadding);
        _angle = ssvs::getRad(_pos);
        updatePosition(radius);
        return wall.isOverlapping(_pos);
    }

    // If alive try to find a close enough safe position.
    sf::Vector2f testPos{_lastPos + pushVel};
    if(wall.isOverlapping(testPos) ||
        !checkWallCollisionEscape(wall, testPos, radiusSquared))
    {
        return true;
    }

    _pos =
        testPos + ssvs::getNormalized(testPos - _prePushPos) * collisionPadding;
    _angle = ssvs::getRad(_pos);
    updatePosition(radius);

    return false;
}
```

Key facts:
- Loop `for(unsigned int i{0u}, j{3u}; i < 4; j = i++)` — NOTE: loop increment is `j = i++` (i.e. j takes old i, then i increments). So sides visited: (i=0,j=3), (i=1,j=0), (i=2,j=1), (i=3,j=2).
- For each side builds sweep polygon {new_i, old_i, old_j, new_j} and checks `_lastPos` inside it (`pointInPolygon<4>`).
- If inside: requires circle-line closest intersection on BOTH old segment and new segment (centered at `_lastPos`, radius² = radiusSquared). pushVel = i2 - i1. If |dot(normalize(pushVel), normalize(_lastPos))| > `0.15f` → pushVel zeroed (push must be perpendicular to radial direction).
- First matching side `break`s the loop.
- Rest mirrors CWall push (static: 2*collisionPadding; moving: escape from `_lastPos + pushVel`).

### `updateTriangleWidthTransition` (verbatim)

```cpp
void CPlayer::updateTriangleWidthTransition(
    const bool focused, const ssvu::FT ft)
{
    if(focused && _triangleWidthTransitionTime < 1.f)
    {
        Utils::moveTowards(_triangleWidthTransitionTime, 1.f, ft * 0.1f);
    }
    else if(!focused && _triangleWidthTransitionTime > 0.f)
    {
        Utils::moveTowardsZero(_triangleWidthTransitionTime, ft * 0.1f);
    }

    _triangleWidth =
        triangleWidthRange *
        (1.f - Utils::getSmoothStep(0.f, 1.f, _triangleWidthTransitionTime));
}
```

Transition rate: `ft * 0.1f` per frame (full 0→1 transition in 10 "frames" of FT). `_triangleWidth = 4.5f * (1 - smoothstep(0,1,t))` so focused (t=1) → width 0? NO: smoothstep(1)=1 → width = 0 when fully focused?? Actually: unfocused t=0 → width = 4.5 * 1 = 4.5?? Wait — check: unfocused width is 3.f. Let me re-derive in the Notes below.

NOTE (verification): At unfocused `t=0`: `_triangleWidth = 4.5 * (1 - 0) = 4.5`. At focused `t=1`: `4.5 * (1 - 1) = 0`. But the constants say `unfocusedTriangleWidth = 3.f`, `focusedTriangleWidth = -1.5f`, `triangleWidthRange = 4.5f`. The formula yields 4.5→0, NOT 3→-1.5. Transcribe as written; the named constants `unfocusedTriangleWidth`/`focusedTriangleWidth` are only used to compute the range. (Do not "fix" — port exactly.)

### `update` (verbatim)

```cpp
void CPlayer::update(
    const bool focused, const bool swapEnabled, const ssvu::FT ft)
{
    updateTriangleWidthTransition(focused, ft);

    if(_deadEffectTimer.isRunning())
    {
        _deadEffectTimer.update(ft);

        _hue += 18.f * ft;

        if(_hue > 360.f)
        {
            _hue = 0.f;
        }

        if(_dead)
        {
            return;
        }

        if(_deadEffectTimer.getTotal() >= 100)
        {
            _deadEffectTimer.stop();
            _deadEffectTimer.resetAll();
        }
    }

    _swapBlinkTimer.update(ft / 3.f);

    if(swapEnabled && _swapTimer.update(ft))
    {
        _swapTimer.stop();
    }

    _lastAngle = _angle;
    _forcedMove = false;
}
```

Key facts:
- Death effect hue rate: `18.f * ft` per frame; wraps `_hue` 360→0 (strictly `> 360.f`).
- Non-fatal death: deadEffectTimer total >= 100 → stop + resetAll (level restart via Lua; timer total is set to 100 by `kill(false)` callers? — see HGUpdate: non-fatal kill restarts the timer with total 100).
- `_swapBlinkTimer.update(ft / 3.f)` — blink timer advances at 1/3 speed (period effectively 18 FT-units).
- Swap cooldown: `_swapTimer.update(ft)` returns true when the timer reaches its total; then stopped.
- `_lastAngle = _angle` and `_forcedMove = false` at the END of update (after movement).

### `updateInputMovement` (verbatim)

```cpp
void CPlayer::updateInputMovement(const float movementDir,
    const float playerSpeedMult, const bool focused, const ssvu::FT ft)
{
    _currentSpeed = playerSpeedMult * (focused ? _focusSpeed : _speed) * ft;
    _angle += ssvu::toRad(_currentSpeed * movementDir);

    const float inc = ft / 10.f;

    _currTiltedAngle =
        (movementDir == 0.f)
            ? Utils::getMoveTowardsZero(_currTiltedAngle, inc)
            : Utils::getMoveTowards(_currTiltedAngle, movementDir, inc * 2.f);
}
```

Key facts:
- `_currentSpeed` is in DEGREES per FT-unit (`toRad` applied to `_currentSpeed * movementDir` for the angle increment).
- Tilt: moveTowardsZero with `inc = ft / 10.f`; moveTowards `movementDir` with `inc * 2.f` = `ft / 5.f`.

### `resetSwap`, `setJustSwapped`, `updatePosition` (verbatim)

```cpp
void CPlayer::resetSwap(const float swapCooldown)
{
    _swapTimer.restart(swapCooldown);
    _swapBlinkTimer.restart(6.f);
}

void CPlayer::setJustSwapped(const bool value)
{
    _justSwapped = value;
}

void CPlayer::updatePosition(const float radius)
{
    _radius = radius;

    _prePushPos = _pos = ssvs::getOrbitRad(_startPos, _angle, _radius);
    _lastPos = ssvs::getOrbitRad(_startPos, _lastAngle, _radius);

    _maxSafeDistance =
        ssvs::getMagSquared(
            _lastPos - ssvs::getOrbitRad(_startPos,
                           _lastAngle + ssvu::toRad(_currentSpeed), _radius)) +
        32.f;
}
```

Key facts:
- `_prePushPos` and `_pos` both set to orbit of `_startPos` at `_angle`, radius `_radius` (world center = `_startPos`, typically (0,0)).
- `_lastPos` = orbit of `_startPos` at `_lastAngle`, current radius (re-projected previous angle onto current radius).
- `_maxSafeDistance` = squared distance between `_lastPos` and the position that would result from one frame of movement (`_lastAngle + toRad(_currentSpeed)`), plus `32.f`. This bounds how far the escape search will accept.

### `getJustSwapped`
`return _justSwapped;`

---

## 4. CCustomWall, CCustomWallHandle, CCustomWallManager

### 4.1 `include/SSVOpenHexagon/Components/CCustomWallHandle.hpp` (entire file)

```cpp
namespace hg {
using CCustomWallHandle = int;
} // namespace hg
```

### 4.2 `include/SSVOpenHexagon/Components/CCustomWall.hpp`

`namespace hg`. Includes: CCustomWallHandle.hpp, PointInPolygon.hpp, FastVertexVector.hpp, SSVUtils Frametime.hpp, SFML Vector2/Color, `<array>`, `<bitset>`, `<cstdint>`, `<utility>`.

```cpp
class CCustomWall
{
public:
    using Handle = int;

private:
    std::array<sf::Vector2f, 4> _vertexPositions;
    std::array<sf::Vector2f, 4> _oldVertexPositions;
    std::array<sf::Color, 4> _vertexColors;
    std::uint8_t _killingSide{0u};

    enum CWFlags : unsigned int
    {
        NoCollision,
        Deadly,
        CWFlagsCount
    };

    std::bitset<CWFlags::CWFlagsCount> _flags; // Default: collides, not deadly

public:
    // (all inline in header, reproduced below)
};
```

Methods (all `[[gnu::always_inline]]` in source):

```cpp
void reset() { _killingSide = 0u; _flags.reset(); }

void draw(Utils::FastVertexVectorTris& wallQuads)
{
    wallQuads.unsafe_emplace_back_quad(        //
        _vertexPositions[0], _vertexColors[0], //
        _vertexPositions[1], _vertexColors[1], //
        _vertexPositions[2], _vertexColors[2], //
        _vertexPositions[3], _vertexColors[3]);
}

[[nodiscard]] bool isOverlapping(const sf::Vector2f& point) const noexcept
{
    // Cannot use `pointInFourVertexPolygon` here due to vertex ordering
    // requirements.
    return Utils::pointInPolygon<4>(_vertexPositions, point.x, point.y);
}

void setVertexPos(const int vertexIndex, const sf::Vector2f& pos) noexcept
{
    _oldVertexPositions[vertexIndex] =
        std::exchange(_vertexPositions[vertexIndex], pos);
}

void moveVertexPos(const int vertexIndex, const sf::Vector2f& offset) noexcept
{
    _oldVertexPositions[vertexIndex] = _vertexPositions[vertexIndex];
    _vertexPositions[vertexIndex] += offset;
}

void moveVertexPos4Same(const sf::Vector2f& offset) noexcept
{
    _oldVertexPositions = _vertexPositions;
    for(sf::Vector2f& v : _vertexPositions) { v += offset; }
}

void setVertexColor(const int vertexIndex, const sf::Color& color) noexcept
{
    _vertexColors[vertexIndex] = color;
}

void setCanCollide(const bool collide) noexcept { _flags[CWFlags::NoCollision] = !collide; }
void setDeadly(const bool deadly) noexcept { _flags[CWFlags::Deadly] = deadly; }

[[nodiscard]] const sf::Vector2f& getVertexPos(const int vertexIndex) const noexcept;
[[nodiscard]] const std::array<sf::Vector2f, 4>& getVertexPositions() const noexcept;
[[nodiscard]] const std::array<sf::Vector2f, 4>& getOldVertexPositions() const noexcept;
[[nodiscard]] bool getCanCollide() const noexcept { return !_flags[CWFlags::NoCollision]; }
[[nodiscard]] bool getDeadly() const noexcept { return _flags[CWFlags::Deadly]; }
[[nodiscard]] constexpr bool isCustomWall() const noexcept { return true; }
void setKillingSide(const std::uint8_t side) noexcept { _killingSide = side; }
[[nodiscard]] std::uint8_t getKillingSide() const noexcept { return _killingSide; }
```

Key facts:
- `_oldVertexPositions` mirrors the previous frame's positions; `setVertexPos` uses `std::exchange` (old value captured into `_oldVertexPositions[i]`).
- Default flags: empty bitset → collides, not deadly.
- Vertex colors per-vertex (RGBA uint8) — custom walls support per-vertex coloring.

### 4.3 `include/SSVOpenHexagon/Components/CCustomWallManager.hpp`

`namespace hg`. Forward-declares `CPlayer`.

```cpp
class CCustomWallManager
{
    std::vector<CCustomWall> _customWalls;
    std::vector<CCustomWallHandle> _freeHandles;
    std::vector<bool> _handleAvailable;
    CCustomWallHandle _nextFreeHandle{0};
    std::size_t _count{0};
    std::vector<CCustomWallHandle> _tempAliveHandles;

    [[nodiscard]] bool isValidHandle(const CCustomWallHandle h) const noexcept;
    [[nodiscard]] bool checkValidHandle(const CCustomWallHandle h, const char* msg);
    [[nodiscard]] bool checkValidVertexIdx(const CCustomWallHandle h, const int vertexIdx, const char* msg);
    [[nodiscard]] bool checkValidVertexIdxAndHandle(const CCustomWallHandle h, const int vertexIdx, const char* msg);
    void destroyUnchecked(const CCustomWallHandle cwHandle);

public:
    [[nodiscard]] CCustomWallHandle create(void (*fAfterCreate)(CCustomWall&));
    void destroy(const CCustomWallHandle cwHandle);
    void destroyAllOutOfBounds(const sf::Vector2f& bounds);
    void setVertexPos(const CCustomWallHandle cwHandle, const int vertexIdx, const sf::Vector2f& pos);
    void moveVertexPos(const CCustomWallHandle cwHandle, const int vertexIdx, const sf::Vector2f& offset);
    void moveVertexPos4Same(const CCustomWallHandle cwHandle, const sf::Vector2f& offset);
    void setCanCollide(const CCustomWallHandle cwHandle, const bool collide);
    void setDeadly(const CCustomWallHandle cwHandle, const bool deadly);
    void setKillingSide(const CCustomWallHandle cwHandle, const std::uint8_t killingSide);
    void setVertexColor(const CCustomWallHandle cwHandle, const int vertexIdx, const sf::Color& color);
    void setVertexPos4(const CCustomWallHandle cwHandle, const sf::Vector2f& p0, const sf::Vector2f& p1, const sf::Vector2f& p2, const sf::Vector2f& p3);
    void setVertexColor4(const CCustomWallHandle cwHandle, const sf::Color& c0, const sf::Color& c1, const sf::Color& c2, const sf::Color& c3);
    void setVertexColor4Same(const CCustomWallHandle cwHandle, const sf::Color& color);
    [[nodiscard]] const sf::Vector2f& getVertexPos(const CCustomWallHandle cwHandle, const int vertexIdx);
    [[nodiscard]] const std::array<sf::Vector2f, 4>& getVertexPos4(const CCustomWallHandle cwHandle);
    [[nodiscard]] bool getCanCollide(const CCustomWallHandle cwHandle);
    [[nodiscard]] bool getDeadly(const CCustomWallHandle cwHandle);
    [[nodiscard]] std::uint8_t getKillingSide(const CCustomWallHandle cwHandle);
    void clear();
    void draw(Utils::FastVertexVectorTris& wallQuads);
    [[nodiscard]] bool handleCollision(const int movement, const float radius, CPlayer& mPlayer, ssvu::FT mFT);
    [[nodiscard]] std::size_t count() const noexcept { return _count; }
    [[nodiscard]] std::size_t maxHandles() const noexcept { return _customWalls.size(); }
};
```

Note: `destroyAllOutOfBounds` is declared in the header; NOT defined in `CCustomWallManager.cpp` (dead/unused declaration — verify with porter; do not implement unless required).

### 4.4 `src/SSVOpenHexagon/Core/CCustomWallManager.cpp` (all method bodies verbatim)

```cpp
[[nodiscard]] bool CCustomWallManager::isValidHandle(
    const CCustomWallHandle h) const noexcept
{
    return h >= 0 &&                                    //
           h < static_cast<int>(_customWalls.size()) && //
           h < static_cast<int>(_handleAvailable.size());
}

[[nodiscard]] bool CCustomWallManager::checkValidHandle(
    const CCustomWallHandle h, const char* msg)
{
    if(_handleAvailable[h]) [[unlikely]]
    {
        ssvu::lo("CustomWallManager")
            << "Attempted to " << msg << " of invalid custom wall " << h << '\n';
        SSVOH_ASSERT(ssvu::contains(_freeHandles, h));
        return false;
    }
    SSVOH_ASSERT(isValidHandle(h));
    return true;
}

[[nodiscard]] bool CCustomWallManager::checkValidVertexIdx(
    const CCustomWallHandle h, const int vertexIdx, const char* msg)
{
    if(vertexIdx < 0 || vertexIdx > 3) [[unlikely]]
    {
        ssvu::lo("CustomWallManager")
            << "Invalid vertex index " << vertexIdx << " for custom wall " << h
            << " while attempting to " << msg << '\n';
        return false;
    }
    return true;
}

[[nodiscard]] bool CCustomWallManager::checkValidVertexIdxAndHandle(
    const CCustomWallHandle h, const int vertexIdx, const char* msg)
{
    return checkValidVertexIdx(h, vertexIdx, msg) && checkValidHandle(h, msg);
}

[[nodiscard]] CCustomWallHandle CCustomWallManager::create(
    void (*fAfterCreate)(CCustomWall&))
{
    if(_freeHandles.empty()) [[unlikely]]
    {
        const std::size_t reserveSize = 32 + _nextFreeHandle * 2;
        const std::size_t maxHandleIndex = _nextFreeHandle + reserveSize;

        _freeHandles.reserve(maxHandleIndex);
        _customWalls.resize(maxHandleIndex);
        _handleAvailable.resize(maxHandleIndex);

        for(std::size_t i = 0; i < reserveSize; ++i)
        {
            _freeHandles.emplace_back(_nextFreeHandle + i);
            _handleAvailable[_nextFreeHandle + i] = true;
        }

        _nextFreeHandle = maxHandleIndex;
    }

    const auto res = _freeHandles.back();
    _freeHandles.pop_back();
    _handleAvailable[res] = false;
    ++_count;

    // Restore default state
    CCustomWall& cw = _customWalls[res];
    cw.reset();
    fAfterCreate(cw);
    return res;
}

void CCustomWallManager::destroyUnchecked(const CCustomWallHandle cwHandle)
{
    SSVOH_ASSERT(!_handleAvailable[cwHandle]);
    SSVOH_ASSERT(isValidHandle(cwHandle));
    _handleAvailable[cwHandle] = true;
    --_count;
    SSVOH_ASSERT(!ssvu::contains(_freeHandles, cwHandle));
    _freeHandles.emplace_back(cwHandle);
}

void CCustomWallManager::destroy(const CCustomWallHandle cwHandle)
{
    if(_handleAvailable[cwHandle]) [[unlikely]]
    {
        ssvu::lo("CustomWallManager")
            << "Attempted to destroy invalid wall " << cwHandle << '\n';
        return;
    }
    destroyUnchecked(cwHandle);
}
```

Simple setters/getters (each: validate handle/vertexIdx, forward to `CCustomWall`; invalid → no-op / sentinel return):

```cpp
void setVertexPos(h, idx, pos)  { if(!checkValidVertexIdxAndHandle(h, idx, "set vertex pos")) return; _customWalls[h].setVertexPos(idx, pos); }
void moveVertexPos(h, idx, off) { if(!checkValidVertexIdxAndHandle(h, idx, "add vertex pos")) return; _customWalls[h].moveVertexPos(idx, off); }
void moveVertexPos4Same(h, off) { if(!checkValidHandle(h, "add four vertex pos same")) return; _customWalls[h].moveVertexPos4Same(off); }
void setCanCollide(h, collide)  { if(!checkValidHandle(h, "set collision")) return; _customWalls[h].setCanCollide(collide); }
void setDeadly(h, deadly)       { if(!checkValidHandle(h, "set deadly status")) return; _customWalls[h].setDeadly(deadly); }

void setKillingSide(h, side)
{
    if(side > 3u) [[unlikely]]
    {
        ssvu::lo("CustomWallManager")
            << "Attempted to set killing side with invalid value " << side
            << ", acceptable values are 0 to 3\n";
        return;
    }
    if(!checkValidHandle(h, "set killing side")) { return; }
    _customWalls[h].setKillingSide(side);
}

const sf::Vector2f& getVertexPos(h, idx)
{
    if(!checkValidVertexIdxAndHandle(h, idx, "get vertex pos")) { return ssvs::zeroVec2f; }
    return _customWalls[h].getVertexPos(idx);
}

static const std::array<sf::Vector2f, 4> zeroArr{
    ssvs::zeroVec2f, ssvs::zeroVec2f, ssvs::zeroVec2f, ssvs::zeroVec2f};

const std::array<sf::Vector2f, 4>& getVertexPos4(h)
{
    if(!checkValidHandle(h, "get four vertex pos")) { return zeroArr; }
    return _customWalls[h].getVertexPositions();
}

bool getCanCollide(h) { if(!checkValidHandle(h, "get collision")) return false; return _customWalls[h].getCanCollide(); }
bool getDeadly(h)     { if(!checkValidHandle(h, "get deadly status")) return false; return _customWalls[h].getDeadly(); }
std::uint8_t getKillingSide(h) { if(!checkValidHandle(h, "get killing side")) return -1; return _customWalls[h].getKillingSide(); }

void setVertexColor(h, idx, color) { if(!checkValidVertexIdxAndHandle(h, idx, "set vertex color")) return; _customWalls[h].setVertexColor(idx, color); }
void setVertexPos4(h, p0, p1, p2, p3) { if(!checkValidHandle(h, "set four vertex pos")) return; cw.setVertexPos(0,p0); ... (1..3); }
void setVertexColor4(h, c0, c1, c2, c3) { if(!checkValidHandle(h, "set four vertex color")) return; cw.setVertexColor(0,c0); ... }
void setVertexColor4Same(h, color) { if(!checkValidHandle(h, "set four vertex color same")) return; set all 4 to color; }

void clear()
{
    _freeHandles.clear();
    _customWalls.clear();
    _handleAvailable.clear();
    _nextFreeHandle = 0;
    _count = 0;
}

void draw(Utils::FastVertexVectorTris& wallQuads)
{
    for(CCustomWallHandle h = 0; h < static_cast<int>(_customWalls.size()); ++h)
    {
        if(!_handleAvailable[h])
        {
            _customWalls[h].draw(wallQuads);
        }
    }
}
```

Sentinel returns for invalid handles: `getVertexPos` → `ssvs::zeroVec2f` (0,0); `getVertexPos4` → `zeroArr`; `getCanCollide`/`getDeadly` → false; `getKillingSide` → `-1` (as uint8, wraps to 255).

`handleCollision` (verbatim, important 3-phase algorithm):

```cpp
[[nodiscard]] bool CCustomWallManager::handleCollision(
    const int movement, const float radius, CPlayer& mPlayer, ssvu::FT mFT)
{
    // ------------------------------------------------------------------------
    // Get all alive walls
    _tempAliveHandles.clear();

    for(CCustomWallHandle h = 0; h < static_cast<int>(_customWalls.size()); ++h)
    {
        if(!_handleAvailable[h] && _customWalls[h].getCanCollide())
        {
            _tempAliveHandles.emplace_back(h);
        }
    }

    const float radiusSquared{radius * radius};
    const sf::Vector2f& pPos{mPlayer.getPosition()};

    {
        bool collided{false};
        for(const CCustomWallHandle h : _tempAliveHandles)
        {
            if(!_customWalls[h].isOverlapping(pPos))
            {
                continue;
            }

            if(mPlayer.getJustSwapped() || _customWalls[h].getDeadly() ||
                mPlayer.push(
                    movement, radius, _customWalls[h], radiusSquared, mFT))
            {
                return true;
            }

            collided = true;
        }

        if(!collided)
        {
            return false;
        }
    }

    // Recheck collision on all walls.
    {
        bool collided{false};
        for(const CCustomWallHandle h : _tempAliveHandles)
        {
            if(!_customWalls[h].isOverlapping(pPos))
            {
                continue;
            }

            if(mPlayer.push(
                   movement, radius, _customWalls[h], radiusSquared, mFT))
            {
                return true;
            }

            collided = true;
        }

        if(!collided)
        {
            return false;
        }
    }

    // Last round with no push.
    for(const CCustomWallHandle h : _tempAliveHandles)
    {
        if(!_customWalls[h].isOverlapping(pPos))
        {
            continue;
        }

        return true;
    }

    return false;
}
```

Algorithm summary:
- Phase 1: for each alive, collidable wall overlapping player pos: kill if justSwapped OR wall deadly OR push() fails. (`pPos` is a REFERENCE to player position — updated as pushes move the player; note `pPos` reference is taken once but points at `_pos` so later phases see updated position.)
- Phase 2: recheck all walls, push (no justSwapped/deadly short-circuit this time).
- Phase 3: final overlap check WITHOUT pushing — any overlap → death (true).
- Returns true if player must die.

---
## 5. HexagonGameStatus — `include/SSVOpenHexagon/Core/HGStatus.hpp` + `src/.../HGStatus.cpp`

`namespace hg`. Includes Clock.hpp (defines `HRClock = std::chrono::high_resolution_clock`, `HRTimePoint = std::chrono::time_point<HRClock>`), `<SFML/Graphics/Color.hpp>` (only for FlashColor? actually unused in header body), `<array>`, `<chrono>`, `<cstddef>`, `<optional>`, `<string>`.

### Enums (verbatim)

```cpp
enum class StateChange
{
    None,
    MustRestart,
    MustReplay
};

enum class RenderStage : std::size_t
{
    BackgroundTris = 0,
    WallQuads3D = 1,
    PivotQuads3D = 2,
    PlayerTris3D = 3,
    WallQuads = 4,
    CapTris = 5,
    PivotQuads = 6,
    PlayerTris = 7,
    Text = 8,
    Count = 9
};
```

### struct HexagonGameStatus

Public nested struct:

```cpp
struct FlashColor
{
    int r;
    int g;
    int b;
};
```

Private fields (all `double` unless noted):

| Field | Type | Default |
|---|---|---|
| `totalFrametimeAccumulator` | `double` | `0` (value-init `{}`) |
| `playedFrametimeAccumulator` | `double` | `0` |
| `pausedFrametimeAccumulator` | `double` | `0` |
| `currentPause` | `double` | `0.1 * 60` (i.e. 6.0) |
| `currentIncrementTime` | `double` | `0` |
| `customScore` | `float` | `0` |

Public fields:

| Field | Type | Default |
|---|---|---|
| `pulse` | `float` | `75` |
| `pulseDirection` | `float` | `1` |
| `pulseDelay` | `float` | `0` |
| `beatPulse` | `float` | `0` |
| `beatPulseDelay` | `float` | `0` |
| `pulse3D` | `float` | `1.f` |
| `pulse3DDirection` | `float` | `1` |
| `flashEffect` | `float` | `0` |
| `radius` | `float` | `75` |
| `fastSpin` | `float` | `0` |
| `cameraShake` | `float` | `0` |
| `hasDied` | `bool` | `false` |
| `mustStateChange` | `StateChange` | `StateChange::None` |
| `scoreInvalid` | `bool` | `false` |
| `invalidReason` | `std::string` | `""` |
| `started` | `bool` | `false` |
| `restartInput` | `std::string` | (default) |
| `replayInput` | `std::string` | (default) |
| `showPlayerTrail` | `bool` | `true` |
| `fragmentShaderIds` | `std::array<std::optional<std::size_t>, RenderStage::Count>` (9 entries) | all nullopt |

Method signatures (see cpp for bodies):
- `void start() noexcept;`
- `[[nodiscard]] double getIncrementTimeSeconds() const noexcept;`
- `[[nodiscard]] double getTimeSeconds() const noexcept;`
- `[[nodiscard]] HRTimePoint getCurrentTP() const noexcept;`
- `[[nodiscard]] HRTimePoint getTimeTP() const noexcept;`
- `[[nodiscard]] HRTimePoint getLevelStartTP() const noexcept;`
- `[[nodiscard]] bool isTimePaused() const noexcept;`
- `void pauseTime(const double seconds) noexcept;`
- `void resetIncrementTime() noexcept;`
- `void resetTime();`
- `void accumulateFrametime(const double ft) noexcept;`
- `void updateCustomScore(const float score) noexcept;`
- `[[nodiscard]] double getTotalAccumulatedFrametime() const noexcept;`
- `[[nodiscard]] double getTotalAccumulatedFrametimeInSeconds() const noexcept;`
- `[[nodiscard]] double getPlayedAccumulatedFrametime() const noexcept;`
- `[[nodiscard]] double getPlayedAccumulatedFrametimeInSeconds() const noexcept;`
- `[[nodiscard]] double getPausedAccumulatedFrametime() const noexcept;`
- `[[nodiscard]] double getPausedAccumulatedFrametimeInSeconds() const noexcept;`
- `[[nodiscard]] float getCustomScore() const noexcept;`

### Bodies (verbatim)

```cpp
void HexagonGameStatus::start() noexcept
{
    // Reset time and custom score:
    resetTime();
    customScore = 0;
    // Signal that we started:
    started = true;
}

double HexagonGameStatus::getIncrementTimeSeconds() const noexcept
{
    return currentIncrementTime / 60.0;
}

double HexagonGameStatus::getTimeSeconds() const noexcept
{
    return getPlayedAccumulatedFrametimeInSeconds();
}

HRTimePoint HexagonGameStatus::getCurrentTP() const noexcept
{
    return HRTimePoint{std::chrono::milliseconds{
        (int64_t)(getTotalAccumulatedFrametimeInSeconds() * 1000.0)}};
}

HRTimePoint HexagonGameStatus::getTimeTP() const noexcept
{
    return HRTimePoint{std::chrono::milliseconds{
        (int64_t)(getPlayedAccumulatedFrametimeInSeconds() * 1000.0)}};
}

HRTimePoint HexagonGameStatus::getLevelStartTP() const noexcept
{
    return HRTimePoint{};
}

bool HexagonGameStatus::isTimePaused() const noexcept
{
    return currentPause > 0.0;
}

void HexagonGameStatus::pauseTime(const double seconds) noexcept
{
    currentPause += seconds * 60.0;
}

void HexagonGameStatus::resetIncrementTime() noexcept
{
    currentIncrementTime = 0.0;
}

void HexagonGameStatus::resetTime()
{
    totalFrametimeAccumulator = 0.0;
    playedFrametimeAccumulator = 0.0;
    pausedFrametimeAccumulator = 0.0;
    currentPause = 0.1 * 60;
    currentIncrementTime = 0.0;
}

void HexagonGameStatus::accumulateFrametime(const double ft) noexcept
{
    // TODO (P2): double-check what to do with remainder
    totalFrametimeAccumulator += ft;

    if(currentPause > 0.0)
    {
        currentPause -= ft;
        // if(currentPause < 0.0) { pauseRemainder = -currentPause; }  (commented out)
    }
    else
    {
        playedFrametimeAccumulator += ft;
        currentIncrementTime += ft;
        // playedFrametimeAccumulator += pauseRemainder;  (commented out)
    }
}

void HexagonGameStatus::updateCustomScore(const float score) noexcept
{
    customScore = score;
}

double getTotalAccumulatedFrametime()        { return totalFrametimeAccumulator; }
double getTotalAccumulatedFrametimeInSeconds(){ return getTotalAccumulatedFrametime() / 60.0; }
double getPlayedAccumulatedFrametime()       { return playedFrametimeAccumulator; }
double getPlayedAccumulatedFrametimeInSeconds(){ return getPlayedAccumulatedFrametime() / 60.0; }
double getPausedAccumulatedFrametime()       { return pausedFrametimeAccumulator; }
double getPausedAccumulatedFrametimeInSeconds(){ return getPausedAccumulatedFrametime() / 60.0; }
float getCustomScore() const noexcept        { return customScore; }
```

Key facts:
- **double vs float**: all time accumulators and pause values are `double`; `ft` (frametime) passed in as `double`. Conversion factor is always `60.0` (60 FT-units per second).
- **Pulse fields are `float`** (`pulse`, `pulseDirection`, `pulseDelay`, `beatPulse`, `beatPulseDelay`, `pulse3D`, `pulse3DDirection`, `radius`).
- `resetTime()` re-primes `currentPause` to `0.1 * 60` = 6.0 FT (a 0.1 s initial pause).
- Pause over-run is DROPPED: when `ft` exceeds `currentPause`, the excess does NOT count as played time (remainder code is commented out). The entire frame is treated as paused.
- `pausedFrametimeAccumulator` is accumulated NOWHERE in this file (always 0) — dead accumulator; getter still exists.
- `getCurrentTP`/`getTimeTP` return time points at duration `milliseconds((int64)(seconds * 1000.0))` from epoch.

---

## 6. LevelStatus — `include/SSVOpenHexagon/Data/LevelStatus.hpp`

`namespace hg`. Includes `<string>`, `<unordered_map>`. All bodies inline except ctor (defined elsewhere, not in scope of this task).

```cpp
struct LevelStatus
{
    std::unordered_map<std::string, std::string> trackedVariables;

    bool scoreOverridden{false};
    std::string scoreOverride;

    // Music and sound related attributes
    bool syncMusicToDM{true};
    float musicPitch{1.f};
    std::string beepSound{"beep.ogg"};
    std::string levelUpSound{"increment.ogg"};
    std::string swapSound{"swap.ogg"};
    std::string deathSound{"death.ogg"};

    float speedMult{1.f};
    float playerSpeedMult{1.f};
    float speedInc{0.f};
    float speedMax{0.f};
    float rotationSpeed{0.f};
    float rotationSpeedInc{0.f};
    float rotationSpeedMax{0.f};
    float delayMult{1.f};
    float delayInc{0.f};
    float delayMin{0.f};
    float delayMax{0.f};
    float fastSpin{0.f};
    float incTime{15.f};
    float pulseMin{75.f};
    float pulseMax{80.f};
    float pulseSpeed{0.f};
    float pulseSpeedR{0.f};
    float pulseDelayMax{0.f};
    float pulseInitialDelay{0.f};
    float swapCooldownMult{1.f};

    // Beat pulse controls the size of the center polygon; matches music beat.
    float beatPulseInitialDelay{0.f};
    float beatPulseMax{0.f};
    float beatPulseDelayMax{0.f};
    float beatPulseSpeedMult{1.f};

    float radiusMin{72.f};
    float wallSkewLeft{0.f};
    float wallSkewRight{0.f};
    float wallAngleLeft{0.f};
    float wallAngleRight{0.f};
    float wallSpawnDistance{1600.f};

    float cameraShake{0};

    unsigned int sides{6};
    unsigned int sidesMax{6};
    unsigned int sidesMin{6};

    bool swapEnabled{false};
    bool tutorialMode{false};
    bool _3DRequired{false};
    bool shadersRequired{false};
    bool incEnabled{true};
    bool rndSideChangesEnabled{true};
    bool darkenUnevenBackgroundChunk{true};

    bool manualPulseControl{false};
    bool manualBeatPulseControl{false};

    unsigned long currentIncrements{0u};

    explicit LevelStatus(const bool mSyncMusicToDM, const float mWallSpawnDistance);
    [[nodiscard]] bool hasSpeedMaxLimit() const noexcept;   // (body: speedMax > 0, defined in other TU)
    [[nodiscard]] bool hasDelayMaxLimit() const noexcept;   // (body: delayMax > 0, defined in other TU)
};
```

Notes:
- All numeric fields are `float` EXCEPT `sides/sidesMax/sidesMin` (`unsigned int`) and `currentIncrements` (`unsigned long`).
- Defaults: `incTime = 15.f` (increment every 15 s), `pulseMin = 75.f`, `pulseMax = 80.f`, `radiusMin = 72.f`, `wallSpawnDistance = 1600.f`.
- `hasSpeedMaxLimit` / `hasDelayMaxLimit` bodies live in another TU (Data/LevelStatus.cpp is not in this task's file list); they are simple predicates over `speedMax` / `delayMax`.

---

## 7. random_number_generator — `include/SSVOpenHexagon/Core/RandomNumberGenerator.hpp`, `RandomNumberGeneratorTypes.hpp` + `src/.../RandomNumberGenerator.cpp`

### Types

```cpp
// RandomNumberGeneratorTypes.hpp (entire file body)
namespace hg {
using random_number_generator_seed_type = unsigned long long;   // u64
}

// RandomNumberGenerator.hpp
#include <SSVUtils/Internal/PCG/PCG.hpp>   // wraps pcg_random.hpp, declared as system header
#include <random>

class random_number_generator
{
public:
    using engine_type = pcg32_fast;                 // = pcg_engines::mcg_xsh_rs_64_32
    using seed_type = random_number_generator_seed_type;  // u64
    using state_type = engine_type::state_type;     // uint64_t

private:
    seed_type _seed;
    engine_type _rng;

public:
    explicit random_number_generator(const seed_type seed) noexcept;
    [[nodiscard]] seed_type seed() const noexcept;

    template <typename T>
    [[nodiscard, gnu::always_inline]] inline T get_int(const T min, const T max) noexcept
    {
        SSVOH_ASSERT(min <= max);
        return std::uniform_int_distribution<T>{min, max}(_rng);
    }

    template <typename T>
    [[nodiscard, gnu::always_inline]] inline T get_real(const T min, const T max) noexcept
    {
        SSVOH_ASSERT(min <= max);
        return std::uniform_real_distribution<T>{min, max}(_rng);
    }

    inline void advance(const state_type delta) noexcept { _rng.advance(delta); }
};
```

Ctor (cpp, verbatim):

```cpp
random_number_generator::random_number_generator(const seed_type seed) noexcept
    : _seed{seed}, _rng{seed}
{
    advance(1);   // NOTE: one LCG step is burned AFTER seeding; first output is from state (seed|3)*M
}

seed_type random_number_generator::seed() const noexcept { return _seed; }
```

### The engine: PCG-XSH-RS 64/32 (mcg variant) — EXACT transcription

`pcg32_fast` = `pcg_engines::mcg_xsh_rs_64_32` = `pcg_detail::engine<uint32_t /*xtype result*/, uint64_t /*itype state*/, xsh_rs_mixin<uint32_t, uint64_t>, true /*output_previous*/, no_stream<uint64_t>, default_multiplier<uint64_t>>`.

LCG core (verbatim from pcg_random.hpp):

```cpp
// multiplier for uint64_t (default_multiplier<uint64_t>::multiplier()):
6364136223846793005ULL
// no_stream (mcg): increment() = 0, is_mcg = true

itype bump(itype state) { return state * multiplier() + increment(); }  // state * 6364136223846793005 (mod 2^64)

itype base_generate0()   // used because output_previous == true
{
    itype old_state = state_;
    state_ = bump(state_);
    return old_state;
}

result_type operator()()
{
    if(output_previous)  // true for 64-bit state (sizeof(itype) <= 8)
        return this->output(base_generate0());
    else
        return this->output(base_generate());
}
```

Seeding (engine ctor, verbatim):

```cpp
engine(itype state = itype(0xcafef00dd15ea5e5ULL))
    : state_(this->is_mcg ? state | state_type(3U)
                          : bump(state + this->increment()))
{
}
```

So: `state_ = seed | 3` (guarantees state odd; period 2^62; MCG low 2 bits fixed at 3).

Output function `xsh_rs_mixin<uint32_t, uint64_t>::output(internal)` (verbatim + precomputed constants for this instantiation):

```cpp
static xtype output(itype internal)
{
    constexpr bitcount_t bits = 64;            // sizeof(uint64_t)*8
    constexpr bitcount_t xtypebits = 32;       // sizeof(uint32_t)*8
    constexpr bitcount_t sparebits = 32;       // bits - xtypebits
    constexpr bitcount_t opbits =
        sparebits - 5 >= 64  ? 5
        : sparebits - 4 >= 32 ? 4
        : sparebits - 3 >= 16 ? 3   // <-- TAKEN (29 >= 16)
        : sparebits - 2 >= 4 ? 2
        : sparebits - 1 >= 1 ? 1 : 0;
    constexpr bitcount_t mask = (1 << opbits) - 1;          // 7
    constexpr bitcount_t maxrandshift = mask;               // 7
    constexpr bitcount_t topspare = opbits;                 // 3
    constexpr bitcount_t bottomspare = sparebits - topspare; // 29
    constexpr bitcount_t xshift = topspare + (xtypebits + maxrandshift) / 2; // 3 + 19 = 22
    bitcount_t rshift = opbits ? bitcount_t(internal >> (bits - opbits)) & mask : 0;  // (internal >> 61) & 7
    internal ^= internal >> xshift;                     // internal ^= internal >> 22
    xtype result = xtype(internal >> (bottomspare - maxrandshift + rshift)); // internal >> (22 + rshift)
    return result;
}
```

In plain Rust terms:
```
let rshift = ((state >> 61) & 7) as u32;
let x = state ^ (state >> 22);
(x >> (22 + rshift)) as u32
```

`advance(delta)` (fast-forward, verbatim):

```cpp
static itype advance(itype state, itype delta, itype cur_mult, itype cur_plus)
{
    constexpr itype ZERO = 0u;
    constexpr itype ONE = 1u;
    itype acc_mult = 1;
    itype acc_plus = 0;
    while(delta > ZERO)
    {
        if(delta & ONE)
        {
            acc_mult *= cur_mult;
            acc_plus = acc_plus * cur_mult + cur_plus;
        }
        cur_plus = (cur_mult + ONE) * cur_plus;
        cur_mult *= cur_mult;
        delta >>= 1;
    }
    return acc_mult * state + acc_plus;
}
// For MCG cur_plus starts 0 and stays 0 => state = state * (6364136223846793005 ^ delta mod 2^64).
void advance(itype delta) { state_ = advance(state_, delta, this->multiplier(), this->increment()); }
```

`bounded_rand` / `operator()(result_type upper_bound)` exist in the engine but are NOT used by the game (game always uses std distributions).

### std distribution algorithms (libstdc++ — VERIFY build toolchain!)

The game relies on `std::uniform_int_distribution<T>` and `std::uniform_real_distribution<T>` run on a uint32-output engine. **These are implementation-defined.** Transcribed below is libstdc++ 15 (system g++ 15.2 on this machine); confirm which GCC the actual build uses before finalizing Rust RNG parity.

`uniform_int_distribution<T>::operator()(urng)` (libstdc++ 15, `bits/uniform_int_dist.h`):

```cpp
// __uctype = common_type<uint32 (engine result), make_unsigned<T>>
constexpr __uctype __urngmin = 0, __urngmax = UINT32_MAX;
constexpr __uctype __urngrange = __urngmax - __urngmin;   // 0xFFFFFFFF
const __uctype __urange = (T)param.b() - (T)param.a();    // unsigned arithmetic

if (__urngrange > __urange) {
    const __uctype __uerange = __urange + 1;
    // __urngrange == UINT32_MAX (our case) => Lemire nearly-divisionless (128-bit path):
    //   _Wp __product = _Wp(__g()) * _Wp(__range);      // u64 * u64
    //   _Up __low = _Up(__product);                      // low 32 bits
    //   if (__low < __range) {
    //       _Up __threshold = -__range % __range;
    //       while (__low < __threshold) {
    //           __product = _Wp(__g()) * _Wp(__range);   // re-draw
    //           __low = _Up(__product);
    //       }
    //   }
    //   return __product >> 32;                          // high 32 bits
}
else if (__urngrange < __urange) {
    // upscaling (only if T is wider than u32 and range > 2^32-1):
    do {
        const __uctype __uerngrange = __urngrange + 1;
        __tmp = (__uerngrange * operator()(__urng, param_type(0, __urange / __uerngrange)));
        __ret = __tmp + ((__uctype(__urng()) - __urngmin));
    } while (__ret > __urange || __ret < __tmp);
}
else
    __ret = (__uctype(__urng()) - __urngmin);
return __ret + param.a();
```

Special case of note: `get_int<T>(a, b)` with `b == a` (`__urange == 0`) → `__uerange == 1` → product = x*1, low = x; threshold = -1 % 1 = 0; while x < 0 never true; returns x >> 32 = 0 → result = a. (Always consumes ONE engine draw even for zero-width range.)

`uniform_real_distribution<T>::operator()(urng)` (libstdc++ 15, `bits/random.h` + `random.tcc`):

```cpp
// via __detail::_Adaptor:
//   value = std::generate_canonical<T, std::numeric_limits<T>::digits>(urng)
// result = value * (b - a) + a

// generate_canonical<float, 24> (libstdc++ 15):
//   __b  = min(digits(float) = 24, 24) = 24
//   __r  = (long double)(max - min + 1) = 4294967296.0L   (2^32)
//   __log2r = log(2^32)/log(2) = 32
//   __m  = max(1, (24 + 32 - 1) / 32) = 1
//   loop (k=1): __sum += float(urng() - 0) * 1.0f; __tmp *= 2^32
//   __ret = __sum / __tmp     // float division by 4294967296.0f
//   if (__ret >= 1.0f) __ret = std::nextafter(1.0f, 0.0f);  // = 0.99999994f
//   return __ret
//
// Consequence for float: EXACTLY ONE engine draw per get_real<float> call.
// value = float(draw) / 4294967296.0f (with clamp; draw == UINT32_MAX rounds float(draw)
// to exactly 2^32, triggering the clamp to nextafter(1,0)).
//
// For double (digits=53): __m = (53+31)/32 = 2 draws;
//   sum = (float)?? NO: sum computed in double:
//   k=2: sum += double(x1) * 1.0; tmp = 2^32
//   k=1: sum += double(x0) * 2^32; tmp = 2^64
//   ret = sum / 2^64
```

### RNG usage in game code (collected in §HGUpdate/§HexagonGame)

Every call site must record: the exact `T` (int type or float), the min/max expressions, and the ORDER of draws (deterministic sequence).

---

## 8. Ticker — `include/SSVOpenHexagon/Utils/Ticker.hpp` + `src/.../Ticker.cpp`

`namespace hg`.

Fields:

| Field | Type | Default |
|---|---|---|
| `target` | `ssvu::FT` (float) | ctor param |
| `current` | `float` | `0.f` |
| `total` | `float` | `0.f` |
| `running` | `bool` | `true` (or ctor param) |
| `loop` | `bool` | `true` |
| `ticks` | `std::size_t` | `0` |

Ctor: `Ticker(ssvu::FT mTarget, bool mRunning = true) : target{mTarget}, running{mRunning} {}` — note `current/total` are value-initialized (0.f) regardless of ctor; `running` from param, `loop` stays default `true`.

All method bodies (verbatim):

```cpp
bool Ticker::update(ssvu::FT mFT) noexcept
{
    const float increment = mFT * static_cast<float>(running);

    current += increment;
    total += increment;

    if(current < target)
    {
        return false;
    }

    ++ticks;
    resetCurrent();      // current = 0.f
    running = loop;
    return true;
}

bool Ticker::update(ssvu::FT mFT, ssvu::FT mTarget) noexcept
{
    target = mTarget;
    return update(mFT);
}

void pause()   { running = false; }
void resume()  { running = true; }
void stop()    { resetCurrent(); pause(); }
void restart() { resetCurrent(); resume(); }
void restart(ssvu::FT mTarget) { target = mTarget; restart(); }

void resetCurrent() { current = 0.f; }
void resetTicks()   { ticks = 0; }
void resetTotal()   { total = 0.f; }
void resetAll()     { resetCurrent(); resetTicks(); resetTotal(); }
void setLoop(bool mX) { loop = mX; }

bool getLoop() const { return loop; }
bool isRunning() const { return running; }
float getTarget() const { return target; }
float getCurrent() const { return current; }
float getTotal() const { return total; }
std::size_t getTicks() const { return ticks; }

template <typename T = ssvu::FT> T getTotalSecs() const noexcept { return static_cast<T>(ssvu::getFTToSeconds(total)); }
template <typename T = ssvu::FT> T getCurrentSecs() const noexcept { return static_cast<T>(ssvu::getFTToSeconds(current)); }
```

Key facts:
- `update` when NOT running: increment = 0 → returns `current < target` (usually true → false returned) — i.e. paused tickers never fire and don't advance.
- On fire: `current` RESET to 0 (overflow beyond target is DROPPED — no carry), `ticks` incremented, `running` set to `loop`.
- `running`/`loop` are the two booleans controlling behavior; `stop()` resets current and pauses.
- `ssvu::getFTToSeconds(x)` = `x / 60.f`.
- NOTE `Ticker{80.f, false}` (CPlayer `_deadEffectTimer`) → starts NOT running; `kill()` calls `_deadEffectTimer.restart()` which resumes it. For non-fatal kill, HGUpdate restarts it with a different target (see HGUpdate).
---

## 9. timeline2 / timeline2_runner — `include/SSVOpenHexagon/Utils/Timeline2.hpp` + `src/.../Timeline2.cpp`

`namespace hg::Utils`.

Types:
- `clock = std::chrono::high_resolution_clock` (WALL-CLOCK, not game time)
- `time_point = clock::time_point`
- `duration = clock::duration`

Action structs (verbatim):

```cpp
struct action_do          { std::function<void()> _func; };
struct action_wait_for    { duration _duration; };
struct action_wait_until  { time_point _time_point; };
struct action_wait_until_fn { std::function<time_point()> _time_point_fn; };
struct action
{
    std::variant<action_do, action_wait_for, action_wait_until,
        action_wait_until_fn>
        _inner;
};
```

`class timeline2`:
- field: `std::vector<action> _actions;`
- methods:
```cpp
void clear();                                   // _actions.clear()
void append_do(const std::function<void()>& func);
void append_wait_for(const duration d);
void append_wait_for_seconds(const double s);   // append_wait_for(milliseconds((int)(s * 1000.0)))  -- TRUNCATES ms
void append_wait_for_sixths(const double s);    // append_wait_for_seconds(s / 60.0)
void append_wait_until(const time_point tp);
void append_wait_until_fn(const std::function<time_point()>& tp_fn);
[[nodiscard]] std::size_t size() const noexcept;        // _actions.size()
[[nodiscard]] action& action_at(const std::size_t i) noexcept;  // assert i < size; _actions[i]
```

`class timeline2_runner`:
- `enum class outcome { proceed, waiting, finished };`
- fields: `std::size_t _current_idx{0};` and `std::optional<time_point> _wait_start_tp;`
- `outcome update(timeline2& timeline, const time_point tp);`

`update` (verbatim logic):

```cpp
outcome timeline2_runner::update(timeline2& timeline, const time_point tp)
{
    if(_current_idx >= timeline.size()) { return outcome::finished; }

    while(_current_idx < timeline.size())
    {
        action& a = timeline.action_at(_current_idx);
        const outcome o = match(a._inner,
            [&](const action_do& x)            { x._func(); return outcome::proceed; },
            [&](const action_wait_for& x)      {
                if(!_wait_start_tp.has_value()) { _wait_start_tp = tp; }
                const auto elapsed = tp - _wait_start_tp.value();
                if(elapsed < x._duration) { return outcome::waiting; }
                _wait_start_tp.reset();
                return outcome::proceed;
            },
            [&](const action_wait_until& x)    { if(tp < x._time_point) { return outcome::waiting; } return outcome::proceed; },
            [&](const action_wait_until_fn& x) { if(tp < x._time_point_fn()) { return outcome::waiting; } return outcome::proceed; }
        );
        if(o == outcome::proceed) { ++_current_idx; continue; }
        if(o == outcome::waiting) { return o; }
        SSVOH_ASSERT(false);
    }
    return outcome::finished;
}
```

Key facts:
- A single runner instance tracks ONE active `action_wait_for` at a time (`_wait_start_tp` optional). Only one wait_for can be "in flight"; the runner processes actions sequentially and halts at the first waiting action.
- `wait_for` compares elapsed wall-clock delta against the duration; `wait_until` compares absolute time points.
- When a timeline finishes, the manager clears it and resets the runner (see §10).

---

## 10. CustomTimeline / CustomTimelineHandle / CustomTimelineManager

### CustomTimeline (`include/.../Core/CustomTimeline.hpp`)

```cpp
struct CustomTimeline
{
    Utils::timeline2 _timeline;
    Utils::timeline2_runner _runner;
};
```

### CustomTimelineHandle (`include/.../Core/CustomTimelineHandle.hpp`)

```cpp
using CustomTimelineHandle = int;
```

### CustomTimelineManager (`include/.../Core/CustomTimelineManager.hpp` + `src/.../CustomTimelineManager.cpp`)

Field: `std::vector<CustomTimeline> _timelines;`

Methods (bodies verbatim):

```cpp
CustomTimelineManager() = default;
~CustomTimelineManager() = default;

bool isHandleValid(const CustomTimelineHandle h) const noexcept
{
    return h >= 0 && h < static_cast<CustomTimelineHandle>(_timelines.size());
}

void clear() noexcept { _timelines.clear(); }

void updateAllTimelines(const HRTimePoint tp)
{
    for(CustomTimeline& t : _timelines)
    {
        if(const auto o = t._runner.update(t._timeline, tp);
            o == Utils::timeline2_runner::outcome::finished)
        {
            t._timeline.clear();
            t._runner = {};
        }
    }
}

CustomTimelineHandle create()
{
    _timelines.emplace_back();
    const CustomTimelineHandle h = _timelines.size() - 1;
    SSVOH_ASSERT(isHandleValid(h));
    return h;
}

CustomTimeline& get(const CustomTimelineHandle h) noexcept
{
    SSVOH_ASSERT(isHandleValid(h));
    return _timelines.at(static_cast<std::size_t>(h));
}

const CustomTimeline& get(const CustomTimelineHandle h) const noexcept
{
    SSVOH_ASSERT(isHandleValid(h));
    return _timelines.at(static_cast<std::size_t>(h));
}
```

Notes:
- Handles are simple indices into a `std::vector` (no free-list, unlike CCustomWallManager). `create()` appends and returns the new index.
- `updateAllTimelines` uses the WALL-CLOCK `HRTimePoint` (from `Clock.hpp`, high_resolution_clock). On finish it clears the timeline and value-resets the runner.
---

## 11. FastVertexVector — `include/SSVOpenHexagon/Utils/FastVertexVector.hpp`

`namespace hg::Utils`.

`template <sf::PrimitiveType TPrimitive> struct FastVertexVector : public sf::Drawable`

Storage: a raw growable array of `sf::Vertex` via a `union VertexUnion { sf::Vertex _v; VertexUnion() {} }` (skips sf::Vertex default ctor; size/alignment asserted equal to `sf::Vertex`).

Fields:
- `Utils::UniquePtrArray<VertexUnion> _data{nullptr};`
- `std::size_t _size{};`
- `std::size_t _capacity{};`

Methods (bodies verbatim in essence):

```cpp
void reserve_more(const std::size_t n) { reserve(_size * 2 + n); }

void reserve(const std::size_t n)
{
    if(_capacity >= n) { return; }
    auto new_data = Utils::makeUniqueArray<VertexUnion>(n);
    if(_data != nullptr) { std::memcpy(new_data.get(), _data.get(), sizeof(sf::Vertex) * _size); }
    else { SSVOH_ASSERT(_size == 0); SSVOH_ASSERT(_capacity == 0); }
    _data = SSVOH_MOVE(new_data);
    _capacity = n;
}

void unsafe_emplace_other(const FastVertexVector& rhs) noexcept
{
    SSVOH_ASSERT(_size + rhs._size <= _capacity);
    if(rhs.size() == 0) { return; }
    SSVOH_ASSERT(_data != nullptr);
    std::memcpy(_data.get() + _size, rhs._data.get(), sizeof(sf::Vertex) * rhs._size);
    _size += rhs._size;
}

void clear() noexcept { _size = 0; }   // capacity retained

std::size_t size() const noexcept { return _size; }

template <typename... Ts> void unsafe_emplace_back(Ts&&... xs)
{
    SSVOH_ASSERT(_size <= _capacity);
    SSVOH_ASSERT(_data != nullptr);
    new(&_data[_size++]._v) sf::Vertex{SSVOH_FWD(xs)...};
}

template <typename... Ts> void batch_unsafe_emplace_back(const sf::Color& color, Ts&&... positions)
{
    SSVOH_ASSERT(_size + sizeof...(positions) <= _capacity);
    SSVOH_ASSERT(_data != nullptr);
    ((new(&_data[_size++]._v) sf::Vertex{positions, color}), ...);
}

void draw(sf::RenderTarget&, const sf::RenderStates&) const override
{
    if(_data == nullptr) { SSVOH_ASSERT(_size == 0); SSVOH_ASSERT(_capacity == 0); return; }
    mRenderTarget.draw(reinterpret_cast<const sf::Vertex*>(_data.get()), _size, TPrimitive, mRenderStates);
}

sf::Vertex& operator[](std::size_t i) noexcept / const overload;   // assert i < _size
sf::Vertex* begin()/end(); const overloads; end() = begin() + _size;
```

`class FastVertexVectorTris : public FastVertexVector<sf::PrimitiveType::Triangles>`:

```cpp
void batch_unsafe_emplace_back_quad(const sf::Color& color,
    const sf::Vector2f& nw, const sf::Vector2f& sw, const sf::Vector2f& se, const sf::Vector2f& ne)
{
    batch_unsafe_emplace_back(color,
        nw, sw, se,          //
        nw, se, ne);
}
// quad = two triangles: (nw,sw,se) and (nw,se,ne)

void unsafe_emplace_back_quad( // per-vertex colors (custom walls)
    const sf::Vector2f& nw, const sf::Color& colorNW,
    const sf::Vector2f& sw, const sf::Color& colorSW,
    const sf::Vector2f& se, const sf::Color& colorSE,
    const sf::Vector2f& ne, const sf::Color& colorNE)
{
    unsafe_emplace_back(nw, colorNW);
    unsafe_emplace_back(sw, colorSW);
    unsafe_emplace_back(se, colorSE);
    unsafe_emplace_back(nw, colorNW);
    unsafe_emplace_back(se, colorSE);
    unsafe_emplace_back(ne, colorNE);
}

void reserve_more_quad(const std::size_t n) { reserve_more(n * 6); }
void reserve_quad(const std::size_t n) { reserve(n * 6); }
```

Corner naming convention used throughout: **nw = vertex 0, sw = vertex 1, se = vertex 2, ne = vertex 3** (i.e. `_vertexPositions[0..3]` map to nw/sw/se/ne). Diagonal split: nw→sw→se, then nw→se→ne (the (0,2) diagonal).

`sf::Vertex` layout (SFML): `{ sf::Vector2f position; sf::Color color; sf::Vector2f texCoords; }` — position is Vector2f (2 floats), color is 4 uint8.

---

## 12. PointInPolygon / Geometry — `PointInPolygon.hpp`, `Geometry.hpp` + `Geometry.cpp`

### PointInPolygon.hpp (entire file, verbatim)

```cpp
namespace hg::Utils {

template <std::size_t N, typename TC, typename T>
[[gnu::always_inline, gnu::pure, nodiscard]] inline bool pointInPolygon(
    const TC& mVertices, const T x, const T y) noexcept
{
    bool result{false};

    for(std::size_t i{0}, j{N - 1}; i < N; j = i++)
    {
        const auto& vI{mVertices[i]};
        const auto& vJ{mVertices[j]};

        if(((vI.y > y) != (vJ.y > y)) &&
            (x < (vJ.x - vI.x) * (y - vI.y) / (vJ.y - vI.y) + vI.x))
        {
            result = !result;
        }
    }

    return result;
}

inline bool pointInFourVertexPolygon(const sf::Vector2f& a, const sf::Vector2f& b,
    const sf::Vector2f& c, const sf::Vector2f& d, const sf::Vector2f& point) noexcept
{
    const sf::Vector2f ab = b - a;
    const sf::Vector2f bc = c - b;
    const sf::Vector2f cd = d - c;
    const sf::Vector2f da = a - d;

    const sf::Vector2f ap_ab = point - a;
    const sf::Vector2f bp_bc = point - b;
    const sf::Vector2f cp_cd = point - c;
    const sf::Vector2f dp_da = point - d;

    const float ab_x_ap = ab.cross(ap_ab);
    const float bc_x_bp = bc.cross(bp_bc);
    const float cd_x_cp = cd.cross(cp_cd);
    const float da_x_dp = da.cross(dp_da);

    return (ab_x_ap <= 0.f && bc_x_bp <= 0.f && cd_x_cp <= 0.f &&
               da_x_dp <= 0.f) ||
           (ab_x_ap >= 0.f && bc_x_bp >= 0.f && cd_x_cp >= 0.f &&
               da_x_dp >= 0.f);
}
}
```

Key facts:
- `pointInPolygon<N>` is the classic even-odd ray-cast (Paul Bourke's algorithm). Loop `for(i=0, j=N-1; i<N; j=i++)` — j tracks the PREVIOUS vertex (j = i-1 mod N, via `j = i++` where i++ is post-increment: j gets old i). Edge case: horizontal edges (vI.y == vJ.y) never satisfy `(vI.y > y) != (vJ.y > y)`.
- `pointInFourVertexPolygon` (used by `CWall::isOverlapping`): sign-of-cross-product test. ALL four cross products must have the same sign (all <= 0 or all >= 0) — i.e. point must be on the same side of every directed edge. This REQUIRES the 4 vertices to be in consistent CCW (or CW) order. `sf::Vector2f::cross(o)` = `x*o.y - y*o.x`.
- CCustomWall uses `pointInPolygon<4>` instead because its vertices can be in arbitrary (Lua-defined) order (comment in CCustomWall: "Cannot use pointInFourVertexPolygon here due to vertex ordering requirements").

### Geometry.hpp + Geometry.cpp (verbatim)

```cpp
// Geometry.hpp
namespace hg::Utils {
inline constexpr float epsilon{1.0e-4};

[[nodiscard]] unsigned int getLineCircleIntersection(sf::Vector2f& i1,
    sf::Vector2f& i2, const sf::Vector2f& p1, const sf::Vector2f& p2,
    const float mRadiusSquared);

[[nodiscard]] bool getLineCircleClosestIntersection(sf::Vector2f& mIntersection,
    const sf::Vector2f& mPos, const sf::Vector2f& p1, const sf::Vector2f& p2,
    const float mRadiusSquared);
}

// Geometry.cpp
unsigned int getLineCircleIntersection(sf::Vector2f& i1, sf::Vector2f& i2,
    const sf::Vector2f& p1, const sf::Vector2f& p2, const float mRadiusSquared)
{
    const float dx{p2.x - p1.x};
    const float dy{p2.y - p1.y};
    const float a{dx * dx + dy * dy};
    const float b{2.f * (dx * p1.x + dy * p1.y)};
    const float c{p1.x * p1.x + p1.y * p1.y - mRadiusSquared};
    const float delta{b * b - 4.f * a * c};

    // No intersections.
    if(delta < 0.f)
    {
        return 0u;
    }

    float t;
    const float twoA{2.f * a};

    // One intersection.
    if(delta < epsilon)     // epsilon = 1.0e-4
    {
        t = -b / twoA;
        i1 = {p1.x + t * dx, p1.y + t * dy};
        return 1u;
    }

    // Two intersections.
    const float sqrtDelta{std::sqrt(delta)};
    t = (-b + sqrtDelta) / twoA;
    i1 = {p1.x + t * dx, p1.y + t * dy};
    t = (-b - sqrtDelta) / twoA;
    i2 = {p1.x + t * dx, p1.y + t * dy};
    return 2u;
}

bool getLineCircleClosestIntersection(sf::Vector2f& mIntersection,
    const sf::Vector2f& mPos, const sf::Vector2f& p1, const sf::Vector2f& p2,
    const float mRadiusSquared)
{
    sf::Vector2f v1, v2;

    switch(getLineCircleIntersection(v1, v2, p1, p2, mRadiusSquared))
    {
        case 1u: mIntersection = v1; return true;
        case 2u:
            if(ssvs::getMagSquared(v1 - mPos) > ssvs::getMagSquared(v2 - mPos))
                mIntersection = v2;
            else
                mIntersection = v1;
            return true;
        default: return false;
    }
}
```

Key facts:
- Quadratic: `a t² + b t + c = 0` with a=|p2-p1|², b=2·(p2-p1)·p1, c=|p1|² - r² (circle centered at ORIGIN (0,0) with squared radius mRadiusSquared — world-space center is always origin).
- Return codes: 0 (no intersection), 1 (tangent-ish, delta in [0, 1.0e-4)), 2.
- `i1` always = intersection with t = (-b + sqrt(delta)) / (2a); `i2` = t = (-b - sqrt(delta)) / (2a).
- **NOTE: line SEGMENT, not infinite line — the function does NOT clamp t to [0,1]; intersections outside the segment are still returned.** The wall side usage relies on this.
- All float arithmetic.

---

## 13. `src/SSVOpenHexagon/Utils/Utils.cpp`

`namespace hg::Utils`. This file contains NO gameplay math — it is Lua/asset plumbing:

- `void runLuaCode(Lua::LuaContext& mLua, const std::string& mCode)` — wraps `mLua.executeCode(mCode)`; on `std::runtime_error` logs via `ssvu::lo("hg::Utils::runLuaCode")` with code + error, rethrows; on `...` logs "Fatal unknown Lua error", rethrows.
- `bool runLuaFileCached(HGAssets& assets, Lua::LuaContext& mLua, const std::string& mFileName)` — caches file contents in `assets.getLuaFileCache()` (unordered_map<string,string>); reads file binary; `static std::string buffer` (FUNCTION-LOCAL STATIC, shared across calls); returns `found` (true if the file was already cached).
- `void runLuaFile(Lua::LuaContext& mLua, const std::string& mFileName)` — throws `std::runtime_error` if file can't open ("Fatal Lua error\nCould not open file: <name>\n").
- `const PackData& findDependencyPackDataOrThrow(assets, currentPack, disambiguator, name, author)` — validates dependency exists + version >= minVersion, else throws.
- `getDependentAssetFilename(assetSubfolder, execScriptPackPathContext, currentPackPath, assetName)` → `concat(context, assetSubfolder, '/', assetName)`; context = last entry of execScriptPackPathContext, or currentPackPath if empty.
- `withDependencyAssetFilename(assetSubfolder, f, execScriptPackPathContext, assets, currentPack, disamb, name, author, assetName)` — pushes dependency folder onto context, calls `f(concat(folder, subfolder, '/', assetName))`, pops context via `HG_SCOPE_GUARD`; rethrows runtime errors with logging.
- `withDependencyScriptFilename` / `getDependentScriptFilename` → subfolder `"Scripts"`.
- `withDependencyShaderFilename` / `getDependentShaderFilename` → subfolder `"Shaders"`.

For the Rust port these belong to the Lua/pack subsystem (see mapping 06/07/14), not gameplay core. Transcribed for completeness only.

---

## 13b. Referenced helper formulas (exact bodies, from non-listed headers)

These are referenced by the gameplay code above and MUST match bit-for-bit.

### Easing (`include/SSVOpenHexagon/Utils/Easing.hpp`)

```cpp
inline float getSaturated(const float mValue)
{
    if(mValue < 0.f) { return 0.f; }
    if(mValue > 1.f) { return 1.f; }
    return mValue;
}

inline float getSmoothStep(const float edge0, const float edge1, float x)
{
    x = getSaturated((x - edge0) / (edge1 - edge0));
    return x * x * (3.f - 2.f * x);   // smoothstep, Hermite
}

inline float getSmootherStep(const float edge0, const float edge1, float x)
{
    x = getSaturated((x - edge0) / (edge1 - edge0));
    return x * x * x * (x * (x * 6.f - 15.f) + 10.f);
}
```

### MoveTowards (`include/SSVOpenHexagon/Utils/MoveTowards.hpp`)

```cpp
inline constexpr float getMoveTowards(float value, const float target, const float step) noexcept
{
    SSVOH_ASSERT(step >= 0);
    if(value < target) { value += step; if(value > target) { value = target; } }
    else if(value > target) { value -= step; if(value < target) { value = target; } }
    return value;
}
inline constexpr float getMoveTowardsZero(const float value, const float step) noexcept
{ return getMoveTowards(value, 0.f, step); }
inline constexpr void moveTowards(float& value, const float target, const float step) noexcept
{ value = getMoveTowards(value, target, step); }
inline constexpr void moveTowardsZero(float& value, const float step) noexcept
{ value = getMoveTowardsZero(value, step); }
```

### Color (`include/SSVOpenHexagon/Utils/Color.hpp`)

```cpp
inline sf::Color getColorDarkened(sf::Color mColor, const float mMultiplier) noexcept
{
    if(mMultiplier != 0.f)
    {
        mColor.r = static_cast<float>(mColor.r) / mMultiplier;
        mColor.b = static_cast<float>(mColor.b) / mMultiplier;
        mColor.g = static_cast<float>(mColor.g) / mMultiplier;
    }
    return mColor;
}

inline sf::Color transformHue(const sf::Color& in, const float H) noexcept
{
    const float u{std::cos(H * 3.14f / 180.f)};
    const float w{std::sin(H * 3.14f / 180.f)};

    return sf::Color{
        static_cast<std::uint8_t>((.701 * u + .168 * w) * in.r +
                                  (-.587 * u + .330 * w) * in.g +
                                  (-.114 * u - .497 * w) * in.b),
        static_cast<std::uint8_t>((-.299 * u - .328 * w) * in.r +
                                  (.413 * u + .035 * w) * in.g +
                                  (-.114 * u + .292 * w) * in.b),
        static_cast<std::uint8_t>((-.3 * u + 1.25 * w) * in.r +
                                  (-.588 * u - 1.05 * w) * in.g +
                                  (.886 * u - .203 * w) * in.b),
        static_cast<std::uint8_t>(255) //
    };
}
// NOTE: alpha is FORCED to 255 (opaque) after transform.
// NOTE: input components are promoted to float for the matrix math, then truncated to uint8 (C++ cast = truncation toward zero, NOT rounding).

inline sf::Color getColorFromHue(const float hue) noexcept
{
    const int i = std::floor(hue * 6.f);
    const float f = (hue * 6.f) - i;
    const float q = 1.f - f;
    const float t = f;

    const auto ret = [](const float r, const float g, const float b)
    { return sf::Color(r * 255.f, g * 255.f, b * 255.f); };

    switch(i)
    {
        case 0: return ret(1.f, t, 0.f);
        case 1: return ret(q, 1.f, 0.f);
        case 2: return ret(0.f, 1.f, t);
        case 3: return ret(0.f, q, 1.f);
        case 4: return ret(t, 0.f, 1.f);
    }
    return ret(1.f, 0.f, q);   // case 5 / default
}
// NOTE: hue assumed in [0,1). sf::Color(r*255.f, g*255.f, b*255.f) — SFML ctor takes int; truncation semantics.

inline constexpr std::uint8_t componentClamp(const float value) noexcept
{
    if(value > 255.f) { return std::uint8_t(255); }
    if(value < 0.f)   { return std::uint8_t(0); }
    return static_cast<std::uint8_t>(value);
}

inline sf::Glsl::Vec3 toGLSLVec3(const sf::Color& color) noexcept
{ return {static_cast<float>(color.r) / 255.f, static_cast<float>(color.g) / 255.f, static_cast<float>(color.b) / 255.f}; }
inline sf::Glsl::Vec4 toGLSLVec4(const sf::Color& color) noexcept
{ return {static_cast<float>(color.r) / 255.f, static_cast<float>(color.g) / 255.f, static_cast<float>(color.b) / 255.f, static_cast<float>(color.a) / 255.f}; }
```

### SSVStart/SSVUtils vector+math helpers used by gameplay (verbatim)

From `SSVStart/Utils/Vector2.hpp`:

```cpp
static Vec2f zeroVec2f{0.f, 0.f};

template <typename T> inline T getMagSquared(const Vec2<T>& mVec) noexcept
{ return mVec.x * mVec.x + mVec.y * mVec.y; }

template <typename T> inline T getMag(const Vec2<T>& mVec) noexcept
{ return std::sqrt(getMagSquared(mVec)); }

// normalize (safe): mVec /= getMag(mVec); skips if mag == 0 (see normalize below)
template <typename T> inline void normalize(Vec2<T>& mVec) noexcept
{ normalize(mVec, getMag(mVec)); }
// normalize with known magnitude:
template <typename T1, typename T2> inline void normalize(Vec2<T1>& mVec, const T2& mMag) noexcept
{ if(mMag != 0) { mVec /= mMag; } }     // guard: divides only when mag != 0
template <typename T1, typename T2> inline Vec2<CT<T1,T2>> getNormalized(Vec2<T1> mVec, const T2& mMag) noexcept
{ normalize(mVec, mMag); return mVec; }
template <typename T> inline auto getNormalized(Vec2<T> mVec) noexcept
{ normalize(mVec); return mVec; }       // SAFE: zero vector stays zero (NOT normalized)

template <typename T> inline T getRad(const Vec2<T>& mVec) noexcept
{ return std::atan2(mVec.y, mVec.x); }

template <typename T1, typename T2 = float>
inline auto getVecFromRad(const T1& mRad, const T2& mMag = 1.f)
{ return Vec2<CT<T1,T2>>(mMag * std::cos(mRad), mMag * std::sin(mRad)); }

template <typename T1, typename T2, typename T3>
inline auto getOrbitRad(const Vec2<T1>& mVec, const T2& mRad, const T3& mRadius)
{ return Vec2<CT<T1,T2,T3>>(mVec) + getVecFromRad(mRad, mRadius); }

template <typename T1, typename T2>
inline CT<T1, T2> getDotProduct(const Vec2<T1>& mA, const Vec2<T2>& mB) noexcept
{ return mA.x * mB.x + mA.y * mB.y; }

template <typename T1, typename T2, typename T3, typename T4>
inline constexpr auto getDistSquaredEuclidean(const T1& mX1, const T2& mY1, const T3& mX2, const T4& mY2) noexcept
{ return (mX1 - mX2) * (mX1 - mX2) + (mY1 - mY2) * (mY1 - mY2); }
template <typename T1, typename T2, typename T3, typename T4>
inline constexpr auto getDistEuclidean(const T1& mX1, const T2& mY1, const T3& mX2, const T4& mY2) noexcept
{ return std::sqrt(getDistSquaredEuclidean(mX1, mY1, mX2, mY2)); }
```

From `SSVUtils/Core/Utils/Math.hpp`:

```cpp
constexpr float pi{3.14159265359f};
constexpr float piHalf{1.57079632679f};
constexpr float tau{6.28318530718f};
constexpr float radDegRatio{0.01745329251f};

template <typename T> inline constexpr T toRad(const T& mX) noexcept { return mX * radDegRatio; }
template <typename T> inline constexpr T toDeg(const T& mX) noexcept { return mX / radDegRatio; }

template <typename T> inline constexpr int getSign(const T& mX) noexcept   // signed overload
{ return (0 < mX) - (mX < 0); }    // +1 / 0 / -1
```

SFML `sf::Vector2f::normalized()` (used in CWall::moveTowardsCenter): normalizes in place; if the vector's magnitude is 0 it leaves it unchanged (SFML guards the zero case). `sf::Vector2f::cross(o)` = `x * o.y - y * o.x`.
---

## 14. `src/SSVOpenHexagon/Core/HGUpdate.cpp`

`namespace hg`. Methods of `HexagonGame` (header: `include/SSVOpenHexagon/Core/HexagonGame.hpp` — see §15 for fields). Includes: HexagonGame.hpp, CWall.hpp, LevelData.hpp, Assert/Assets/Audio/Config, Clock/Concat/Easing/LevelValidator/MoveTowards/Split/String, Discord/HexagonClient/Joystick/LuaScripting/Steam, SSVUtils Rnd.hpp, imgui (non-Android), SSVStart Vector2, SSVUtils Frametime+Containers, SFML Color/Vector2, `<array>`, `<optional>`, `<stdexcept>`, `<cstring>`, `<cstdint>`.

### `fastForwardTo(const double target)` (verbatim)

```cpp
void HexagonGame::fastForwardTo(const double target)
{
    const HRTimePoint tpBegin = HRClock::now();

    const auto exceededProcessingTime = [&]
    {
        constexpr int maxProcessingSeconds = 3;
        return hrSecondsSince(tpBegin) > maxProcessingSeconds;
    };

    while(!status.hasDied && status.getTimeSeconds() < target &&
          !exceededProcessingTime())
    {
        update(Config::TIME_STEP, 1.0f /* timescale */);
        postUpdate();
    }
}
```

(`Config::TIME_STEP` — see §15; fixed 1.0f FT per tick in the current codebase — verify in Config.hpp.)

### `advanceByTicks(const int nTicks)` (verbatim)

```cpp
void HexagonGame::advanceByTicks(const int nTicks)
{
    for(int i = 0; i < nTicks; ++i)
    {
        update(Config::TIME_STEP, 1.0f /* timescale */);
        postUpdate();
    }
}
```

### `update(ssvu::FT mFT, const float timescale)` — MAIN UPDATE (verbatim)

```cpp
void HexagonGame::update(ssvu::FT mFT, const float timescale)
{
    // Fast-forwarding for level testing
    if(fastForwardTarget.has_value())
    {
        const double target = fastForwardTarget.value();
        fastForwardTarget.reset();

        fastForwardTo(target);

        if(audio != nullptr)
        {
            audio->setMusicPlayingOffsetSeconds(
                audio->getMusicPlayingOffsetSeconds() +
                status.getTimeSeconds());
        }
        return;
    }

    // Advance by ticks for level testing
    if(advanceTickCount.has_value())
    {
        const int nTicks = advanceTickCount.value();
        advanceTickCount.reset();

        const bool wasPaused = debugPause;
        debugPause = false;
        advanceByTicks(nTicks);
        debugPause = wasPaused;
        return;
    }

    // Update client
    if(hexagonClient != nullptr)
    {
        hexagonClient->update();
    }

    // Scale simulation delta frame time
    mFT *= timescale;

    // Discord/Steam rich presence
    if(window != nullptr)
    {
        std::string nameStr = levelData->name;
        nameFormat(nameStr);
        const std::string diffStr = diffFormat(difficultyMult);
        const std::string timeStr = timeFormat(status.getTimeSeconds());

        constexpr float DELAY_TO_UPDATE = 5.f; // X seconds
        timeUntilRichPresenceUpdate -= ssvu::getFTToSeconds(mFT);

        if(timeUntilRichPresenceUpdate <= 0.f)
        {
            if(steamManager != nullptr)
            {
                steamManager->set_rich_presence_in_game(nameStr, diffStr, timeStr);
            }
            timeUntilRichPresenceUpdate = DELAY_TO_UPDATE;
        }
        updateRichPresenceCallbacks();
    }

    if(mustStart)
    {
        mustStart = false;
        start();
    }

    if(!debugPause)
    {
        updateFlash(mFT);
        effectTimelineManager.update(mFT);

        if(!mustReplayInput())
        {
            updateInput();
        }
        else
        {
            SSVOH_ASSERT(activeReplay.has_value());
            if(!status.started)
            {
                if(window != nullptr && window->hasTimer())
                {
                    window->getTimerBase().reset();   // avoid initial speedup when viewing replays
                }
                mustStart = true;
            }
            else
            {
                const input_bitset ib =
                    activeReplay->replayPlayer.get_current_and_move_forward();

                if(ib[static_cast<unsigned int>(input_bit::left)])    { inputMovement = -1; }
                else if(ib[static_cast<unsigned int>(input_bit::right)]) { inputMovement = 1; }
                else                                                    { inputMovement = 0; }

                inputSwap = ib[static_cast<unsigned int>(input_bit::swap)];
                inputFocused = ib[static_cast<unsigned int>(input_bit::focus)];
            }
        }

        if(Config::getShowKeyIcons() || mustShowReplayUI()) { updateKeyIcons(); }
        if(Config::getShowLevelInfo() || mustShowReplayUI()) { updateLevelInfo(); }

        // Update input leniency time after death to avoid accidental restart.
        if(deathInputIgnore > 0.f)
        {
            deathInputIgnore -= mFT;
        }

        if(status.started)
        {
            styleData.computeColors();

            player.update(getInputFocused(), getLevelStatus().swapEnabled, mFT);

            if(!status.hasDied)
            {
                const std::optional<bool> preventPlayerInput =
                    runLuaFunctionIfExists<bool, float, int, bool, bool>(
                        "onInput", mFT, getInputMovement(), getInputFocused(),
                        getInputSwap());

                if(!preventPlayerInput.has_value() || !(*preventPlayerInput))
                {
                    player.updateInputMovement(getInputMovement(),
                        getPlayerSpeedMult(), getInputFocused(), mFT);

                    // Play "swap ready blip" sound and create particles
                    if(!playerNowReadyToSwap && player.isReadyToSwap())
                    {
                        playerNowReadyToSwap = true;
                        if(Config::getPlaySwapReadySound())
                        {
                            playSoundOverride("swapBlip.ogg");
                        }
                        swapParticlesSpawnInfo =
                            SwapParticleSpawnInfo{.ready{true},
                                .position{player.getPosition()},
                                .angle{player.getPlayerAngle()}};
                    }

                    // Create particles after swap
                    if(getLevelStatus().swapEnabled && getInputSwap() &&
                        player.isReadyToSwap())
                    {
                        swapParticlesSpawnInfo =
                            SwapParticleSpawnInfo{.ready{false},
                                .position{player.getPosition()},
                                .angle{player.getPlayerAngle()}};
                        performPlayerSwap(true /* mPlaySound */);
                        player.resetSwap(getSwapCooldown());
                        player.setJustSwapped(true);
                        playerNowReadyToSwap = false;
                    }
                    else
                    {
                        player.setJustSwapped(false);
                    }
                }

                status.accumulateFrametime(mFT);
                if(levelStatus.scoreOverridden)
                {
                    status.updateCustomScore(
                        lua.readVariable<float>(levelStatus.scoreOverride));
                }

                updateEvents(mFT);
                updateIncrement();

                if(mustChangeSides && walls.empty())
                {
                    sideChange(rng.get_int(
                        levelStatus.sidesMin, levelStatus.sidesMax));
                }

                updateLevel(mFT);
                updateCustomTimelines();

                if(Config::getBeatPulse())
                {
                    updateBeatPulse(mFT);
                }

                updatePulse(mFT);

                if(!Config::getBlackAndWhite())
                {
                    styleData.update(mFT, std::pow(difficultyMult, 0.8f));
                }

                player.updatePosition(getRadius());

                updateWalls(mFT);
                ssvu::eraseRemoveIf(
                    walls, [](const CWall& w) { return w.isDead(); });

                updateCustomWalls(mFT);
            }
            else
            {
                levelStatus.rotationSpeed *= 0.99f;
            }

            // This is done even with 3D disabled as it's used to affect the RNG
            // state for replay validation.
            updatePulse3D(mFT);

            if(!Config::getNoRotation())
            {
                updateRotation(mFT);
            }

            updateCameraShake(mFT);

            if(!status.hasDied)
            {
                const auto fixup =
                    [](const float x) -> random_number_generator::state_type
                {
                    // Avoid UB when converting to unsigned type:
                    return x < 0.f ? -x : x;
                };

                rng.advance(fixup(status.pulse));
                rng.advance(fixup(status.pulse3D));
                rng.advance(fixup(status.fastSpin));
                rng.advance(fixup(status.flashEffect));
                rng.advance(fixup(levelStatus.rotationSpeed));
                // TODO (P1): stuff from style?
            }
        }

        if(window != nullptr)
        {
            SSVOH_ASSERT(overlayCamera.has_value());
            SSVOH_ASSERT(backgroundCamera.has_value());

            updateParticles(mFT);

            if(Config::getShowPlayerTrail() && status.showPlayerTrail)
            {
                updateTrailParticles(mFT);
            }
            if(Config::getShowSwapParticles())
            {
                updateSwapParticles(mFT);
            }
            overlayCamera->update(mFT);
            backgroundCamera->update(mFT);
        }
    }

    updateText(mFT);

    if(status.started)
    {
        if(status.mustStateChange != StateChange::None)
        {
            const bool executeLastReplay =
                status.mustStateChange == StateChange::MustReplay;

            if(!executeLastReplay && !assets.anyLocalProfileActive())
            {
                goToMenu();
                return;
            }

            newGame(getPackId(), restartId, restartFirstTime, difficultyMult,
                executeLastReplay);
        }

#if 0  // TODO (P2): score invalidation due to performance
        if(!status.scoreInvalid && Config::getOfficial())
        {
            invalidateScore("PERFORMANCE ISSUES");
        }
#endif

        if(!Config::get3D() && levelStatus._3DRequired)
        {
            invalidateScore("3D REQUIRED");
        }

        if(!Config::getShaders() && levelStatus.shadersRequired)
        {
            invalidateScore("SHADERS REQUIRED");
        }
    }
}
```

**Frame order (deterministic, must match exactly):**
1. fast-forward / tick-advance test hooks (early return).
2. `hexagonClient->update()`.
3. `mFT *= timescale`.
4. Rich presence bookkeeping (seconds; `DELAY_TO_UPDATE = 5.f`).
5. `mustStart` → `start()`.
6. If `!debugPause`:
   a. `updateFlash(mFT)`
   b. `effectTimelineManager.update(mFT)`
   c. input: `updateInput()` OR replay input extraction
   d. key icons / level info (UI)
   e. `deathInputIgnore -= mFT` (if > 0)
   f. if `status.started`:
      - `styleData.computeColors()`
      - `player.update(focused, swapEnabled, mFT)`
      - if alive:
        - Lua `onInput` (may veto input; returns optional<bool>)
        - `player.updateInputMovement(movementDir, playerSpeedMult, focused, mFT)`
        - swap-ready blip + particles; swap execution (`performPlayerSwap`, `resetSwap(getSwapCooldown())`, `setJustSwapped(true)`) or `setJustSwapped(false)`
        - `status.accumulateFrametime(mFT)`
        - custom score via Lua variable if `scoreOverridden`
        - `updateEvents(mFT)`
        - `updateIncrement()`
        - `mustChangeSides && walls.empty()` → `sideChange(rng.get_int(sidesMin, sidesMax))`
        - `updateLevel(mFT)` (Lua `onUpdate` + level timeline + `onStep`)
        - `updateCustomTimelines()`
        - `updateBeatPulse(mFT)` if `Config::getBeatPulse()`
        - `updatePulse(mFT)`
        - `styleData.update(mFT, pow(difficultyMult, 0.8f))` unless black-and-white
        - `player.updatePosition(getRadius())`
        - `updateWalls(mFT)` → erase dead walls → `updateCustomWalls(mFT)`
      - else (dead): `levelStatus.rotationSpeed *= 0.99f`
      - `updatePulse3D(mFT)` (always — affects RNG state!)
      - `updateRotation(mFT)` unless `Config::getNoRotation()`
      - `updateCameraShake(mFT)`
      - if alive: **RNG state fix-up** (see below)
   g. particles / trail / swap particles; camera updates.
7. `updateText(mFT)`.
8. `mustStateChange` → `newGame(...)` or `goToMenu()`; score invalidation checks.

**RNG advance fix-up (replay determinism, verbatim core):**
```cpp
const auto fixup = [](const float x) -> random_number_generator::state_type
{ return x < 0.f ? -x : x; };   // abs(); note: -(-0.f) == +0.f
rng.advance(fixup(status.pulse));
rng.advance(fixup(status.pulse3D));
rng.advance(fixup(status.fastSpin));
rng.advance(fixup(status.flashEffect));
rng.advance(fixup(levelStatus.rotationSpeed));
```
RNG is advanced by |float values| (truncated to u64) EVERY frame while alive — these advances are NOT consumed by draws, they exist purely to mix state so replay validation works. `state_type` = u64. Negative-zero: `x < 0.f` is false for -0.f, so fixup(-0.f) = -0.f → converted to 0.

### `updateWalls(ssvu::FT mFT)` (verbatim)

```cpp
void HexagonGame::updateWalls(ssvu::FT mFT)
{
    bool collided{false};
    const float radiusSquared{status.radius * status.radius + 8.f};
    const sf::Vector2f& pPos{player.getPosition()};

    for(CWall& w : walls)
    {
        w.update(levelStatus.wallSpawnDistance, getRadius(), centerPos, mFT);

        if(!w.isOverlapping(pPos))
        {
            continue;
        }

        // Kill after a swap or if player could not be pushed out to safety.
        if(player.getJustSwapped())
        {
            performPlayerKill();
            if(steamManager != nullptr) { steamManager->unlock_achievement("a22_swapdeath"); }
        }
        else if(player.push(getInputMovement(), getRadius(), w, centerPos,
                    radiusSquared, mFT))
        {
            performPlayerKill();
        }

        collided = true;
    }

    // There was no collision, so we can stop here.
    if(!collided)
    {
        return;
    }

    // Second round, always deadly...
    for(CWall& w : walls)
    {
        if(!w.isOverlapping(pPos))
        {
            continue;
        }
        if(player.getJustSwapped())
        {
            if(steamManager != nullptr) { steamManager->unlock_achievement("a22_swapdeath"); }
        }
        performPlayerKill();
    }
}
```

Key facts:
- Collision radius squared = `status.radius * status.radius + 8.f` (8.f extra "forgiveness" added to the SQUARED radius).
- Two-phase like CCustomWallManager: phase 1 (per wall: overlap? justSwapped→kill; else push→kill if push returns true); phase 2 "always deadly": any remaining overlap → kill.
- `pPos` is a reference to `player._pos` — updated during pushes.
- Note: in phase 1, if `getJustSwapped()` is true, `player.push` is NOT called (short-circuit) — swap-immunity only lasts that frame; phase 2 kills on any overlap regardless of swap.
- `getInputMovement()` returns `inputMovement` (-1/0/+1).

### `updateCustomWalls(ssvu::FT mFT)` (verbatim)

```cpp
void HexagonGame::updateCustomWalls(ssvu::FT mFT)
{
    if(cwManager.handleCollision(getInputMovement(), getRadius(), player, mFT))
    {
        performPlayerKill();
        if(player.getJustSwapped())
        {
            if(steamManager != nullptr) { steamManager->unlock_achievement("a22_swapdeath"); }
        }
    }
}
```

### `start()` (verbatim)

```cpp
void HexagonGame::start()
{
    status.start();
    messageText.setString("");
    playSoundOverride("go.ogg");

    if(!mustReplayInput())
    {
        std::string nameStr = levelData->name;
        nameFormat(nameStr);
        std::string packStr = getPackName();
        nameFormat(packStr);
        const std::string diffStr = diffFormat(difficultyMult);

        if(discordManager != nullptr)
        {
            discordManager->set_rich_presence_in_game(
                nameStr + " [x" + diffStr + "]", packStr);
        }

        const std::string& validator = levelData->getValidator(difficultyMult);

        if(hexagonClient != nullptr &&
            hexagonClient->getState() == HexagonClient::State::LoggedIn_Ready &&
            Config::getOfficial() && !levelData->unscored &&
            hexagonClient->isLevelSupportedByServer(validator))
        {
            hexagonClient->trySendStartedGame(validator);
        }
    }
    else
    {
        if(discordManager != nullptr)
        {
            discordManager->set_rich_presence_on_replay();
        }
    }

    if(audio != nullptr && !Config::getNoMusic())
    {
        audio->resumeMusic();
    }

    runLuaFunctionIfExists<void>("onLoad");
}
```

### Input handling (verbatim)

```cpp
static void setInputImplIfFalse(bool& var, const bool x)
{
    if(!var) { var = x; }
}

void HexagonGame::updateInput_UpdateJoystickControls()
{
    if(window == nullptr) { return; }
    Joystick::update(Config::getJoystickDeadzone());
    setInputImplIfFalse(inputImplCCW, Joystick::pressed(Joystick::Jdir::Left));
    setInputImplIfFalse(inputImplCW,  Joystick::pressed(Joystick::Jdir::Right));
    setInputImplIfFalse(inputSwap,    Joystick::pressed(Joystick::Jid::Swap));
    setInputImplIfFalse(inputFocused, Joystick::pressed(Joystick::Jid::Focus));

    if(Joystick::risingEdge(Joystick::Jid::Exit)) { goToMenu(); }
    else if(Joystick::risingEdge(Joystick::Jid::ForceRestart) ||
            (status.hasDied && Joystick::risingEdge(Joystick::Jid::Restart)))
    {
        status.mustStateChange = StateChange::MustRestart;
    }
    else if(status.hasDied && Joystick::risingEdge(Joystick::Jid::Replay))
    {
        status.mustStateChange = StateChange::MustReplay;
    }
}

void HexagonGame::updateInput_UpdateTouchControls()
{
    if(window == nullptr) { return; }
    for(const auto& p : window->getFingerDownPositions())
    {
        if(p.x < window->getRenderWindow().getSize().x / 2.f) { setInputImplIfFalse(inputImplCCW, true); }
        else                                                 { setInputImplIfFalse(inputImplCW,  true); }
    }
}

void HexagonGame::updateInput_ResolveInputImplToInputMovement()
{
    if(inputImplCW && !inputImplCCW)
    {
        inputMovement = inputImplLastMovement = 1;
        return;
    }
    if(!inputImplCW && inputImplCCW)
    {
        inputMovement = inputImplLastMovement = -1;
        return;
    }
    if(inputImplCW && inputImplCCW)
    {
        inputMovement = -inputImplLastMovement;   // BOTH pressed: flip direction
        return;
    }
    inputMovement = inputImplLastMovement = 0;
}

void HexagonGame::updateInput_RecordCurrentInputToLastReplayData()
{
    if(!status.started || status.hasDied) { return; }
    const bool left = getInputMovement() == -1;
    const bool right = getInputMovement() == 1;
    const bool swap = getInputSwap();
    const bool focus = getInputFocused();
    lastReplayData.record_input(left, right, swap, focus);
}

void HexagonGame::updateInput()
{
    if(imguiLuaConsoleHasInput()) { return; }

    if(!status.started &&
        (!Config::getRotateToStart() || inputImplCCW || inputImplCW))
    {
        mustStart = true;
    }

    if(alwaysSpinRight)
    {
        inputImplCCW = false;
        inputImplCW = true;
        inputSwap = false;
        inputFocused = false;
    }
    else if(executeRandomInputs) // TODO (P2): For testing
    {
        static std::random_device rd;
        static std::mt19937 en(rd());
        inputImplCCW = std::uniform_int_distribution<int>{0, 1}(en);
        inputImplCW  = std::uniform_int_distribution<int>{0, 1}(en);
        inputSwap    = std::uniform_int_distribution<int>{0, 1}(en);
        inputFocused = std::uniform_int_distribution<int>{0, 1}(en);
    }
    else
    {
        updateInput_UpdateJoystickControls(); // Joystick state.
        updateInput_UpdateTouchControls();    // Touchscreen state.
    }

    updateInput_ResolveInputImplToInputMovement();
    updateInput_RecordCurrentInputToLastReplayData();
}
```

Notes:
- sideChange logic: `inputMovement` in {-1,0,+1}; both directions pressed → `inputMovement = -inputImplLastMovement` (toggle to the other direction; `inputImplLastMovement` retains its value in that branch — NOT updated to 0).
- Random-input testing path uses `std::mt19937` seeded by `std::random_device` (NOT the game RNG) — test-only, no replay impact.
- Keyboard state: "handled by callbacks set in the constructor" (setKeyCallbacks → sets inputImplCCW/CW etc.; see §15 HexagonGame ctor).

### `updateEvents` / `updateCustomTimelines` / `updateIncrement` / `updateLevel` (verbatim)

```cpp
void HexagonGame::updateEvents(ssvu::FT)
{
    if(const auto o = eventTimelineRunner.update(eventTimeline, status.getTimeTP());
        o == Utils::timeline2_runner::outcome::finished)
    {
        eventTimeline.clear();
        eventTimelineRunner = {};
    }

    if(const auto o = messageTimelineRunner.update(messageTimeline, status.getCurrentTP());
        o == Utils::timeline2_runner::outcome::finished)
    {
        messageTimeline.clear();
        messageTimelineRunner = {};
    }
}

void HexagonGame::updateCustomTimelines()
{
    _customTimelineManager.updateAllTimelines(status.getCurrentTP());
}

void HexagonGame::updateIncrement()
{
    if(!levelStatus.incEnabled) { return; }
    if(status.getIncrementTimeSeconds() < levelStatus.incTime) { return; }
    ++levelStatus.currentIncrements;
    incrementDifficulty();
    status.resetIncrementTime();
    mustChangeSides = true;
}

void HexagonGame::updateLevel(ssvu::FT mFT)
{
    if(status.isTimePaused()) { return; }
    runLuaFunctionIfExists<float>("onUpdate", mFT);
    const auto o = timelineRunner.update(timeline, status.getTimeTP());
    if(o == Utils::timeline2_runner::outcome::finished && !mustChangeSides)
    {
        timeline.clear();
        runLuaFunctionIfExists<void>("onStep");
        timelineRunner = {};
    }
}
```

Notes:
- **Difficulty increment rule**: every `levelStatus.incTime` seconds (default 15.f) of PLAYED time, if `incEnabled`: `currentIncrements++`, `incrementDifficulty()` (see §15 for exact speed/delay/rotation increments), reset increment timer, set `mustChangeSides`.
- Level timeline (`timeline`) runs on GAME time (`status.getTimeTP()`); event timeline on game time; message + custom timelines on ABSOLUTE accumulated time (`status.getCurrentTP()` — includes pauses!).
- `onUpdate` Lua is skipped while time paused; `onStep` fires when the level timeline finishes AND `!mustChangeSides` (multi-step levels: step only between side changes).

### Pulse / beat-pulse / rotation / shake / flash / pulse3D (verbatim)

```cpp
void HexagonGame::updatePulse(ssvu::FT mFT)
{
    if(!levelStatus.manualPulseControl)
    {
        if(status.pulseDelay <= 0)
        {
            const float pulseAdd{status.pulseDirection > 0
                                     ? levelStatus.pulseSpeed
                                     : -levelStatus.pulseSpeedR};

            const float pulseLimit{status.pulseDirection > 0
                                       ? levelStatus.pulseMax
                                       : levelStatus.pulseMin};

            status.pulse += pulseAdd * mFT * getMusicDMSyncFactor();

            if((status.pulseDirection > 0 && status.pulse >= pulseLimit) ||
                (status.pulseDirection < 0 && status.pulse <= pulseLimit))
            {
                status.pulse = pulseLimit;
                status.pulseDirection *= -1;

                if(status.pulseDirection < 0)
                {
                    status.pulseDelay = levelStatus.pulseDelayMax;
                }
            }
        }
        status.pulseDelay -= mFT * getMusicDMSyncFactor();
    }
    refreshPulse();
}

void HexagonGame::refreshPulse()
{
    if(window != nullptr)
    {
        SSVOH_ASSERT(backgroundCamera.has_value());
        const float p{Config::getNoPulse() ? 1.f : (status.pulse / levelStatus.pulseMin)};
        const float rotation{backgroundCamera->getRotation()};

        backgroundCamera->setView(sf::View{ssvs::zeroVec2f,
            {(Config::getWidth() * Config::getZoomFactor()) * p,
             (Config::getHeight() * Config::getZoomFactor()) * p}});

        backgroundCamera->setRotation(rotation);
    }
}

void HexagonGame::updateBeatPulse(ssvu::FT mFT)
{
    if(!levelStatus.manualBeatPulseControl)
    {
        if(status.beatPulseDelay <= 0)
        {
            status.beatPulse = levelStatus.beatPulseMax;
            status.beatPulseDelay = levelStatus.beatPulseDelayMax;
        }
        else
        {
            status.beatPulseDelay -= mFT * getMusicDMSyncFactor();
        }

        if(status.beatPulse > 0)
        {
            status.beatPulse -= (2.f * mFT * getMusicDMSyncFactor()) *
                                levelStatus.beatPulseSpeedMult;
        }
    }
    refreshBeatPulse();
}

void HexagonGame::refreshBeatPulse()
{
    const float radiusMin{Config::getBeatPulse() ? levelStatus.radiusMin : 75};
    status.radius =
        radiusMin * (status.pulse / levelStatus.pulseMin) + status.beatPulse;
}

void HexagonGame::updateRotation(ssvu::FT mFT)
{
    auto nextRotation(getRotationSpeed() * 10.f);
    if(status.fastSpin > 0)
    {
        nextRotation += std::abs((Utils::getSmootherStep(0,
                                      levelStatus.fastSpin, status.fastSpin) /
                                     3.5f) *
                                 17.f) *
                        ssvu::getSign(nextRotation);

        status.fastSpin -= mFT;
    }

    if(window != nullptr)
    {
        SSVOH_ASSERT(backgroundCamera.has_value());
        backgroundCamera->turn(nextRotation);
    }
}
```

(`backgroundCamera`/`overlayCamera` are `std::optional` members — see §15.)

Pulse math summary:
- Pulse ping-pongs between `pulseMin` (75.f default) and `pulseMax` (80.f default); rising speed = `levelStatus.pulseSpeed`, falling speed = `levelStatus.pulseSpeedR` (negative direction).
- When turning from up→down, `pulseDelay = pulseDelayMax` (pause at the top only; no delay at the bottom — `pulseDelay` is decremented every frame regardless, including while `pulseDelay > 0`).
- `status.pulseDelay` initial value 0 (see HGStatus).
- All scaled by `getMusicDMSyncFactor()` (music/sound desync compensation; see §15).
- Beat pulse: when `beatPulseDelay <= 0` → set `beatPulse = beatPulseMax`, `beatPulseDelay = beatPulseDelayMax`; each frame while positive: `beatPulse -= (2.f * mFT * dmSync) * beatPulseSpeedMult`.
- **Radius formula**: `status.radius = radiusMin * (pulse / pulseMin) + beatPulse` where `radiusMin = Config::getBeatPulse() ? levelStatus.radiusMin : 75`. (So with beat pulse disabled the radius is `75 * pulse/pulseMin`.)
- `getRadius()` presumably returns `status.radius` (see §15).

Rotation math:
- Base rotation speed: `getRotationSpeed() * 10.f` degrees/FT (camera `turn` takes degrees).
- fastSpin: adds `abs(smoothstep(0, levelStatus.fastSpin, status.fastSpin)/3.5f * 17.f) * sign(nextRotation)`; `status.fastSpin` counts down by `mFT` (seconds-like, but FT units) each frame while > 0.
- `getSmootherStep(edge0=0, edge1=levelStatus.fastSpin, x=status.fastSpin)` — 5th-order smootherstep (see §13b). Note when `levelStatus.fastSpin == 0` this would divide by zero — but the guard `status.fastSpin > 0` with `status.fastSpin` set only when `levelStatus.fastSpin > 0` (see §15 setFastSpin) prevents it; STILL: if levelStatus.fastSpin is 0 and status.fastSpin > 0 (Lua-set) → div-by-zero → NaN. (Latent C++ behavior; replicate or guard per orchestrator decision — flagging.)

### `updateCameraShake` (verbatim)

```cpp
void HexagonGame::updateCameraShake(ssvu::FT mFT)
{
    if(!backgroundCamera.has_value() || !overlayCamera.has_value()) { return; }

    if(status.cameraShake <= 0.f)
    {
        if(preShakeCenters.has_value())
        {
            backgroundCamera->setCenter(preShakeCenters->background);
            overlayCamera->setCenter(preShakeCenters->overlay);
            preShakeCenters.reset();
        }
        return;
    }

    status.cameraShake -= mFT;

    if(!preShakeCenters.has_value())
    {
        preShakeCenters = PreShakeCenters{
            backgroundCamera->getCenter(), overlayCamera->getCenter()};
    }

    const auto makeShakeVec = [this]
    {
        const float i = status.cameraShake;
        return sf::Vector2f(rng.get_real(-i, i), rng.get_real(-i, i));
    };

    backgroundCamera->setCenter(preShakeCenters->background + makeShakeVec());
    overlayCamera->setCenter(preShakeCenters->overlay + makeShakeVec());
}
```

RNG use: TWO `rng.get_real<float>(-i, i)` draws per shaken frame (x then y), `i = status.cameraShake` (counts down by mFT). NOTE: `get_real(-i, i)` with i possibly < 1.0f — uniform_real_distribution<float> handles narrow ranges; when i == 0 the call is not made (guard `<= 0.f`).

### `updateFlash` / `updatePulse3D` (verbatim)

```cpp
void HexagonGame::updateFlash(ssvu::FT mFT)
{
    if(status.flashEffect > 0)
    {
        status.flashEffect -= 3 * mFT;
    }
    status.flashEffect = ssvu::getClamped(status.flashEffect, 0.f, 255.f);

    for(sf::Vertex& vertex : flashPolygon)
    {
        vertex.color.a = status.flashEffect;
    }
}

void HexagonGame::updatePulse3D(ssvu::FT mFT)
{
    status.pulse3D += styleData._3dPulseSpeed * status.pulse3DDirection * mFT;
    if(status.pulse3D > styleData._3dPulseMax)
    {
        status.pulse3DDirection = -1.f;
    }
    else if(status.pulse3D < styleData._3dPulseMin)
    {
        status.pulse3DDirection = 1.f;
    }
}
```

- Flash decays at `3 * mFT` per frame (rate 3 FT-units), clamped [0.f, 255.f]; `flashPolygon` vertices' alpha = flashEffect (float assigned to uint8 truncates).
- pulse3D ping-pong between `styleData._3dPulseMin`/`Max` at speed `styleData._3dPulseSpeed` (StyleData — see §15).

### Particles (verbatim)

```cpp
void HexagonGame::updateParticles(ssvu::FT mFT)
{
    const auto isOutOfBounds = [](const Particle& p)
    {
        const sf::Sprite& sp = p.sprite;
        const sf::Vector2f& pos = sp.getPosition();
        constexpr float padding = 256.f;
        return (pos.x < 0 - padding || pos.x > Config::getWidth() + padding ||
                pos.y < 0 - padding || pos.y > Config::getHeight() + padding);
    };

    const auto makePBParticle = [this]
    {
        Particle p;
        p.sprite.setTexture(*txStarParticle);
        p.sprite.setPosition(
            {ssvu::getRndR(-64.f, Config::getWidth() + 64.f), -64.f});
        p.sprite.setRotation(sf::degrees(ssvu::getRndR(0.f, 360.f)));

        const float scale = ssvu::getRndR(0.75f, 1.35f);
        p.sprite.setScale({scale, scale});

        sf::Color c = getColorMain();
        c.a = ssvu::getRndI(90, 145);
        p.sprite.setColor(c);

        p.velocity = {ssvu::getRndR(-12.f, 12.f), ssvu::getRndR(4.f, 18.f)};
        p.angularVelocity = ssvu::getRndR(-6.f, 6.f);
        return p;
    };

    ssvu::eraseRemoveIf(particles, isOutOfBounds);

    for(Particle& p : particles)
    {
        sf::Sprite& sp = p.sprite;
        sp.setPosition(sp.getPosition() + p.velocity * mFT);
        sp.setRotation(sp.getRotation() + sf::degrees(p.angularVelocity * mFT));
    }

    if(mustSpawnPBParticles)
    {
        nextPBParticleSpawn -= mFT;
        if(nextPBParticleSpawn <= 0.f)
        {
            particles.emplace_back(makePBParticle());
            nextPBParticleSpawn = 2.75f;
        }
    }
}
```

Note: "PB" particles use `ssvu::getRndR/getRndI` — the GLOBAL SSVUtils RNG (thread-local), NOT the game's `random_number_generator`. Test/cosmetic only (power-bar particles). `ssvu::getRndR(a,b)` = uniform_real_distribution<float>; `ssvu::getRndI(a,b)` = uniform_int_distribution<int> on SSVUtils' internal engine (see SSVUtils/Core/Utils/Rnd.hpp — not gameplay-deterministic).

Spawn interval `nextPBParticleSpawn = 2.75f`.

```cpp
void HexagonGame::updateTrailParticles(ssvu::FT mFT)
{
    const auto isDead = [&](const TrailParticle& p)
    { return p.sprite.getColor().a <= 3; };

    const auto makeTrailParticle = [this]
    {
        TrailParticle p;
        p.sprite.setTexture(*txSmallCircle);
        p.sprite.setPosition(player.getPosition());
        p.sprite.setOrigin(sf::Vector2f{txSmallCircle->getSize()} / 2.f);

        const float scale = Config::getPlayerTrailScale();
        p.sprite.setScale({scale, scale});

        sf::Color c = getColorPlayerTrail();
        c.a = Config::getPlayerTrailAlpha();
        p.sprite.setColor(c);

        p.angle = player.getPlayerAngle();
        return p;
    };

    ssvu::eraseRemoveIf(trailParticles, isDead);

    for(TrailParticle& p : trailParticles)
    {
        sf::Color color = p.sprite.getColor();
        const float newAlpha = Utils::getMoveTowardsZero(
            static_cast<float>(color.a), Config::getPlayerTrailDecay() * mFT);
        color.a = static_cast<std::uint8_t>(newAlpha);
        p.sprite.setColor(color);

        p.sprite.setScale(p.sprite.getScale() * 0.98f);

        p.sprite.setPosition(
            ssvs::getVecFromRad(p.angle, status.radius + 2.4f));
    }

    if(player.hasChangedAngle())
    {
        trailParticles.emplace_back(makeTrailParticle());
    }
}
```

Trail: one particle spawned per frame where player angle changed; alpha decays toward 0 at `getPlayerTrailDecay() * mFT` per frame; scale shrinks `* 0.98f` per frame; position re-projected every frame onto radius `status.radius + 2.4f` at stored angle (so trail follows the circle even as radius pulses).

```cpp
void HexagonGame::updateSwapParticles(ssvu::FT mFT)
{
    const auto isDead = [&](const SwapParticle& p)
    { return p.sprite.getColor().a <= 3; };

    const auto makeSwapParticle = [this](const SwapParticleSpawnInfo& si,
                                      const float expand, const float speedMult,
                                      const float scaleMult, const float alpha)
    {
        SwapParticle p;
        p.sprite.setTexture(*txSmallCircle);
        p.sprite.setPosition(si.position);
        p.sprite.setOrigin(sf::Vector2f{txSmallCircle->getSize()} / 2.f);

        const float scale = ssvu::getRndR(0.65f, 1.35f) * scaleMult;
        p.sprite.setScale({scale, scale});

        sf::Color c = getColorPlayerTrail();
        c.a = alpha;
        p.sprite.setColor(c);

        p.velocity =
            ssvs::getVecFromRad(si.angle + ssvu::getRndR(-expand, expand),
                ssvu::getRndR(0.1f, 10.f) * speedMult);
        return p;
    };

    ssvu::eraseRemoveIf(swapParticles, isDead);

    for(SwapParticle& p : swapParticles)
    {
        sf::Color color = p.sprite.getColor();
        const float newAlpha =
            Utils::getMoveTowardsZero(static_cast<float>(color.a), 3.5f * mFT);
        color.a = static_cast<std::uint8_t>(newAlpha);
        p.sprite.setColor(color);

        p.sprite.setScale(p.sprite.getScale() * 0.98f);
        p.sprite.setPosition(p.sprite.getPosition() + p.velocity * mFT);
    }

    if(swapParticlesSpawnInfo.has_value())
    {
        if(swapParticlesSpawnInfo->ready == false)
        {
            // after-swap burst:
            for(int i = 0; i < 20; ++i)
            {
                swapParticles.emplace_back(
                    makeSwapParticle(*swapParticlesSpawnInfo,
                        0.45f /* expand */, 1.f /* speedMult */,
                        1.f /* scaleMult */, 45.f /* alpha */));
            }
            for(int i = 0; i < 10; ++i)
            {
                swapParticles.emplace_back(
                    makeSwapParticle(*swapParticlesSpawnInfo,
                        3.14f /* expand */, 0.45f /* speedMult */,
                        0.75f /* scaleMult */, 35.f /* alpha */));
            }
        }
        else
        {
            // ready blip:
            for(int i = 0; i < 14; ++i)
            {
                swapParticles.emplace_back(
                    makeSwapParticle(*swapParticlesSpawnInfo,
                        3.14f /* expand */, 1.3f /* speedMult */,
                        0.4f /* scaleMult */, 140.f /* alpha */));
            }
        }
        swapParticlesSpawnInfo.reset();
    }
}
```

Note: swap/trail particles use `ssvu::getRndR` (global RNG) for spread/speed/scale — cosmetic; NOT the game RNG.

### ImGui Lua console (test tooling — condensed but complete)

`ilcTextEditCallback(ImGuiInputTextCallbackData*)` — autocomplete (CallbackCompletion: word boundary at ' ', '\t', ',', ';'; skip leading '?'; match against `LuaScripting::getAllFunctionNames()` via case-insensitive prefix `Strnicmp`; 0 matches → log `No match for "..."!`; 1 match → replace word + append " "; N matches → complete common prefix (case-insensitive) and list "Possible matches:\n- name\n..." ) and history (CallbackHistory: up/down arrows walk `ilcHistory`, `-1` = new-entry position).

`postUpdate_ImguiLuaConsole()`:
- Active only when `Config::getDebug() && !Config::getOfficial()` (Android builds compiled out entirely via `#ifndef SSVOH_ANDROID`).
- Window "Lua Console", initial size `ImVec2(600, 700)` (FirstUseEver).
- Log line colors: `[error]` → (1.0,0.4,0.4,1.0); `[warning]` → (1.0,0.4,1.0,1.0); `[lua]` → (0.4,1.0,0.4,1.0); prefix `# ` → (1.0,0.8,0.6,1.0); `[?]` → (0.4,0.4,1.0,1.0). Lines wrapped at 80 chars (`lineLimit = 80`, substr chunks of 80).
- Commands: `!CLEAR` clears log; `!HELP` shows help text (verbatim in source: `!clear`, `!help`, `!ff <seconds>`, `!advt <ticks>`, `?fn`); `!ff <seconds>` → `std::stod`, sets `fastForwardTarget` (double); `!advt <ticks>` → `std::stoi`, sets `advanceTickCount` (clamped `>= 0`); `?fn` → `LuaScripting::getDocsForFunction`; anything else: first tries `u_log(<cmd>)\n`, on `std::runtime_error` falls back to executing `<cmd>\n` directly; errors logged as `[error]: <what>\n` or `[error]: unknown\n`.
- Command history: dedup (remove previous identical entry before appending), max unbounded.
- Footer: `update ms: %.2f` / `draw ms: %.2f`; `ImGui::DragFloat("Timescale", &simSpeed, 0.005f)` writes `Config::setTimescale(simSpeed)`; `ImGui::Checkbox("Invincible")` → `Config::setInvincible`.
- "Track" input: appends `u_impl_addTrackedResult(<code>)\n` to `ilcLuaTracked`; "Untrack All" clears; each frame tracked codes are re-executed and `ilcLuaTrackedResults` displayed in a 2-column table (name, value).

`postUpdate()` = `postUpdate_ImguiLuaConsole();` only.

---
---

## 15. HexagonGame — `src/SSVOpenHexagon/Core/HexagonGame.cpp` (+ `include/.../HexagonGame.hpp`)

`namespace hg`.

### Simulation tick rate (Config)

```cpp
// Config.hpp:
inline constexpr float TICKS_PER_SECOND = 240.f;
inline constexpr float TIME_STEP = 60.f / TICKS_PER_SECOND;   // = 0.25f FT per tick
```
So the loop calls `update(0.25f, timescale)` 240×/sec at timescale 1.0.

### Anonymous-namespace helpers (verbatim)

```cpp
[[nodiscard]] double getReplayScore(const HexagonGameStatus& status)
{
    return status.getCustomScore() != 0.f
               ? status.getCustomScore()
               : status.getPlayedAccumulatedFrametime();
}
// NOTE: custom score (FLOAT) returned as double; fallback = played FT (double).

[[nodiscard]] random_number_generator initializeRng()
{
    thread_local pcg32_fast seed_rng = []
    {
        pcg_extras::seed_seq_from<std::random_device> seed_source;
        return pcg32_fast{seed_source};
    }();

    return random_number_generator{seed_rng()};
}
```

RNG facts:
- A FRESH `random_number_generator` is created per `newGame` (unless executing last replay).
- Its u64 seed = ONE 32-bit draw from a THREAD-LOCAL `pcg32_fast` (itself a full PCG state seeded by `seed_seq_from<std::random_device>` — 4 random u32s). The seed is thus a uniform u32 stored in a u64 (high 32 bits zero).
- The seed is saved as `lastSeed` / `replay_file._seed`; replays re-seed `rng = random_number_generator{replayFile._seed}`.
- Remember the ctor does `advance(1)` (one LCG burn step) — replay validation must include it.

### `createWall` (verbatim)

```cpp
void HexagonGame::createWall(int mSide, float mThickness,
    const SpeedData& mSpeed, const SpeedData& mCurve, float mHueMod)
{
    walls.emplace_back(getSides(), getWallAngleLeft(), getWallAngleRight(),
        getWallSkewLeft(), getWallSkewRight(), centerPos, mSide, mThickness,
        levelStatus.wallSpawnDistance, mSpeed, mCurve, mHueMod);
}
```

- `centerPos` is `const sf::Vector2f{0.f, 0.f}` — the world origin; ALL positions (player, walls) are relative to it. `CPlayer`'s `_startPos` is `ssvs::zeroVec2f`.
- Wall spawn distance = `levelStatus.wallSpawnDistance` (default 1600.f, overridable via `Config::getSpawnDistance()` at level load).

### Class fields (HexagonGame.hpp, with types/defaults)

```cpp
// Platform/integration
Steam::steam_manager* steamManager;
Discord::discord_manager* discordManager;
bool discordHung{false};
bool steamHung{false};
std::int8_t discordAttempt{1};
std::int8_t steamAttempt{1};

HGAssets& assets;
sf::Font& font;          // "OpenSquare-Regular.ttf"
sf::Font& fontBold;      // "OpenSquare-Bold.ttf"
Audio* audio;
const LevelData* levelData;
ssvs::GameState game;
ssvs::GameWindow* window;
HexagonClient* hexagonClient;

// IMGUI Lua Console (see §14)
sf::Clock ilcDeltaClock;
std::vector<std::string> ilcCmdLog;
std::vector<std::string> ilcHistory;
int ilcHistoryPos{-1};
std::string ilcCmdBuffer;
std::string ilcTrackBuffer;
bool ilcShowConsole{false};
bool ilcShowConsoleNext{false};
std::vector<std::string> ilcLuaTracked;
std::vector<std::string> ilcLuaTrackedNames;
std::vector<std::string> ilcLuaTrackedResults;
bool debugPause{false};
std::vector<std::string> execScriptPackPathContext;

// Core gameplay (public)
CPlayer player;
std::vector<CWall> walls;
CCustomWallManager cwManager;
float timeUntilRichPresenceUpdate = 0.f;

// Cameras
std::optional<ssvs::Camera> backgroundCamera;   // world space, center (0,0)
std::optional<ssvs::Camera> overlayCamera;      // screen space, center (w/2,h/2)

struct PreShakeCenters { sf::Vector2f background; sf::Vector2f overlay; };
std::optional<PreShakeCenters> preShakeCenters;

ssvu::TimelineManager effectTimelineManager;      // SSVUtils Timeline (legacy, FT-based)

const sf::Vector2f centerPos{0.f, 0.f};

Lua::LuaContext lua;
std::unordered_set<std::string> calledDeprecatedFunctions;

LevelStatus levelStatus;
MusicData musicData;
StyleData styleData;

Utils::timeline2 timeline;            // level timeline (game time)
Utils::timeline2_runner timelineRunner;
Utils::timeline2 eventTimeline;       // event timeline (game time)
Utils::timeline2_runner eventTimelineRunner;
Utils::timeline2 messageTimeline;     // message timeline (absolute time)
Utils::timeline2_runner messageTimelineRunner;
CustomTimelineManager _customTimelineManager;

sf::Text messageText;
sf::Text pbText;
Utils::FastVertexVectorTris flashPolygon;

struct Particle      { sf::Sprite sprite; sf::Vector2f velocity; float angularVelocity; };
struct TrailParticle { sf::Sprite sprite; float angle; };
struct SwapParticle  { sf::Sprite sprite; sf::Vector2f velocity; };

sf::Texture* txStarParticle;     // "starParticle.png"
sf::Texture* txSmallCircle;      // "smallCircle.png"

std::vector<Particle> particles;
std::vector<TrailParticle> trailParticles;
std::vector<SwapParticle> swapParticles;
bool mustSpawnPBParticles{false};

struct SwapParticleSpawnInfo { bool ready; sf::Vector2f position; float angle; };
std::optional<SwapParticleSpawnInfo> swapParticlesSpawnInfo;
float nextPBParticleSpawn{0.f};
float pbTextGrowth{0.f};

// UI sprites / level info
sf::Sprite keyIconLeft, keyIconRight, keyIconFocus, keyIconSwap, replayIcon;
sf::RectangleShape levelInfoRectangle;
sf::Text levelInfoTextLevel, levelInfoTextPack, levelInfoTextAuthor, levelInfoTextBy, levelInfoTextDM;

// Flags
bool firstPlay{true};
bool restartFirstTime{true};
bool inputFocused{false};
bool inputSwap{false};
bool mustTakeScreenshot{false};
bool mustChangeSides{false};
bool mustStart{false};

random_number_generator rng;
HexagonGameStatus status;
float deathInputIgnore{0.f};

struct ActiveReplay
{
    replay_file replayFile;
    replay_player replayPlayer;
    std::string replayPackName;
    std::string replayLevelName;
    explicit ActiveReplay(const replay_file& mReplayFile);
    // ctor: replayFile{mReplayFile}, replayPlayer{replayFile._data}
};
std::optional<ActiveReplay> activeReplay;

random_number_generator::seed_type lastSeed{};
replay_data lastReplayData{};
bool lastFirstPlay{};
double lastPlayedScore{};

std::string restartId;
float difficultyMult{1};
int inputImplLastMovement{0};
int inputMovement{0};
bool inputImplCW{false};
bool inputImplCCW{false};
bool playerNowReadyToSwap{false};

std::ostringstream os;
sf::Text fpsText, timeText, text, replayText;
CapColor capColor;               // tinyvariant of CapColorMode enum (Main/MainDarkened/ByIndex/...), see Data/CapColor.hpp
std::string packId;
std::string levelId;

// Render buffers (FastVertexVectorTris)
Utils::FastVertexVectorTris backgroundTris, wallQuads, pivotQuads, playerTris, capTris,
                            wallQuads3D, pivotQuads3D, playerTris3D;

// Fast-forward / tick-advance test hooks
std::optional<double> fastForwardTarget;
std::optional<int> advanceTickCount;

// Test utilities
std::function<void(const replay_file&)> onDeathReplayCreated;
bool executeRandomInputs{false};
bool alwaysSpinRight{false};
```

### Ctor (key parts, verbatim)

```cpp
HexagonGame::HexagonGame(Steam::steam_manager* mSteamManager,
    Discord::discord_manager* mDiscordManager, HGAssets& mAssets, Audio* mAudio,
    ssvs::GameWindow* mGameWindow, HexagonClient* mHexagonClient)
    : steamManager(mSteamManager),
      discordManager(mDiscordManager),
      assets(mAssets),
      font{assets.getFontOrNullFont("OpenSquare-Regular.ttf")},
      fontBold{assets.getFontOrNullFont("OpenSquare-Bold.ttf")},
      audio(mAudio),
      window(mGameWindow),
      hexagonClient{mHexagonClient},
      player{ssvs::zeroVec2f, getSwapCooldown(), Config::getPlayerSize(),
          Config::getPlayerSpeed(), Config::getPlayerFocusSpeed()},
      levelStatus{Config::getMusicSpeedDMSync(), Config::getSpawnDistance()},
      messageText{initText("", font, 38.f)},
      pbText{initText("", fontBold, 65.f)},
      txStarParticle{nullptr},
      txSmallCircle{nullptr},
      levelInfoTextLevel{"", font},
      levelInfoTextPack{"", font},
      levelInfoTextAuthor{"", font},
      levelInfoTextBy{"", font},
      levelInfoTextDM{"", font},
      rng{initializeRng()},
      fpsText{initText("0", font, 25.f)},
      timeText{initText("0", fontBold, 70.f)},
      text{initText("", font, 25.f)},
      replayText{initText("", font, 20.f)}
{
    if(window != nullptr)
    {
        const float width = Config::getWidth();
        const float height = Config::getHeight();
        const float zoomFactor = Config::getZoomFactor();

        backgroundCamera.emplace(sf::View{ssvs::zeroVec2f,
            sf::Vector2f{width * zoomFactor, height * zoomFactor}});

        overlayCamera.emplace(sf::View{sf::Vector2f{width / 2.f, height / 2.f},
            sf::Vector2f{width, height}});

        txStarParticle = &assets.getTextureOrNullTexture("starParticle.png");
        txSmallCircle = &assets.getTextureOrNullTexture("smallCircle.png");
    }

    game.onUpdate += [this](ssvu::FT mFT) { update(mFT, Config::getTimescale()); };
    game.onPostUpdate += [this] { postUpdate(); };
    game.onDraw += [this] { draw(); };
    game.onAnyEvent += Imgui::processEvent;

    if(window != nullptr) { window->onRecreation += [this] { initKeyIcons(); }; }

    Config::keyboardBindsSanityCheck();
    using Tid = Config::Tid;

    addTid2StateInput(Tid::RotateCCW, inputImplCCW);   // 2-state (held) input
    addTid2StateInput(Tid::RotateCW,  inputImplCW);
    addTid2StateInput(Tid::Focus,     inputFocused);
    addTid2StateInput(Tid::Swap,      inputSwap);

    // Escape hardcoded → goToMenu (Always)
    // Tid::Exit (Always) → goToMenu
    // Tid::ForceRestart (Once) → status.mustStateChange = MustRestart
    // Tid::Restart (Once) → if(deathInputIgnore <= 0.f && status.hasDied) mustStateChange = MustRestart
    // Tid::Replay (Once) → if((deathInputIgnore <= 0.f && status.hasDied) || inReplay()) mustStateChange = MustReplay
    // Tid::Screenshot (Once) → mustTakeScreenshot = true
    // Tid::LuaConsole (Once) → if Config::getDebug(): ilcShowConsoleNext = true
    // Tid::Pause (Once) → if debug: toggle debugPause; pause/resume music accordingly
    // All console-sensitive binds wrapped in notInConsole(...) = skip if imguiLuaConsoleHasInput()

    Config::loadAllJoystickBinds();
    initKeyIcons();
}
```

`ssvs::Camera` semantics (SSVStart, needed for rotation/pulse port):
- Deferred: `turn(mDeg)` adds to `nextRotation` (deg per FT unit); `pan` adds to `nextPan`; `zoomIn/zoomOut` adjust `nextZoomFactor`.
- On `update(ft)`: `center += nextPan`; `view.zoom(pow(nextZoomFactor, ft))`; `view.rotate(degrees(nextRotation * ft))`; then reset deltas.
- `setView/setCenter/setRotation/setSkew` apply IMMEDIATELY.
- `skew` default `{1.f, 1.f}`; applied as view size multipliers in `apply()`.

### `newGame` (verbatim, condensed — full body in source)

```cpp
void HexagonGame::newGame(const std::string& mPackId, const std::string& mId,
    bool mFirstPlay, float mDifficultyMult, bool executeLastReplay)
{
    if(!mFirstPlay) { saveReplay(); }   // save replay when restarting without having died

    packId = mPackId;
    levelId = mId;

    if(executeLastReplay && activeReplay.has_value())
    {
        firstPlay = activeReplay->replayFile._first_play;
    }
    else
    {
        firstPlay = mFirstPlay;
    }

    setLevelData(assets.getLevelData(mId), mFirstPlay);
    difficultyMult = mDifficultyMult;

    const double tempReplayScore = getReplayScore(status);
    status = HexagonGameStatus{};        // FRESH status (pulse=75, radius=75, etc.)

    if(!executeLastReplay)
    {
        rng = initializeRng();           // NEW random seed
        lastSeed = rng.seed();
        lastReplayData = replay_data{};
        lastFirstPlay = mFirstPlay;
        activeReplay.reset();
    }
    else
    {
        if(!activeReplay.has_value())
        {
            lastPlayedScore = tempReplayScore;
            activeReplay.emplace(replay_file{
                ._version{0},
                ._player_name{assets.getCurrentLocalProfile().getName()},
                ._seed{lastSeed},
                ._data{lastReplayData},
                ._pack_id{mPackId},
                ._level_id{mId},
                ._first_play{lastFirstPlay},
                ._difficulty_mult{mDifficultyMult},
                ._played_score{lastPlayedScore},
            });
        }
        activeReplay->replayPlayer.reset();
        activeReplay->replayPackName = Utils::toUppercase(assets.getPackData(mPackId).name);
        activeReplay->replayLevelName = Utils::toUppercase(levelData->name);
        rng = random_number_generator{activeReplay->replayFile._seed};  // RESPAWN same seed
        firstPlay = activeReplay->replayFile._first_play;
    }

    // Audio: stopSounds, stopLevelMusic; if music enabled: playLevelMusic + pauseMusic + refreshMusicPitch; else stopMusic.
    debugPause = false;

    messageText.setString("");  pbText.setString("");
    eventTimeline.clear();      eventTimelineRunner = {};
    messageTimeline.clear();    messageTimelineRunner = {};
    _customTimelineManager.clear();

    walls.clear();
    cwManager.clear();
    player = CPlayer{ssvs::zeroVec2f, getSwapCooldown(), Config::getPlayerSize(),
        Config::getPlayerSpeed(), Config::getPlayerFocusSpeed()};

    timeline.clear();  timelineRunner = {};
    effectTimelineManager.clear();
    mustChangeSides = false;
    mustStart = false;

    pbTextGrowth = 0.f;
    mustSpawnPBParticles = false;
    swapParticlesSpawnInfo.reset();
    nextPBParticleSpawn = 0.f;
    particles.clear(); trailParticles.clear(); swapParticles.clear();

    initFlashEffect(255, 255, 255);

    if(window != nullptr)
    {
        // Reset zoom
        overlayCamera->setView(sf::View{{Config::getWidth() / 2.f, Config::getHeight() / 2.f},
            sf::Vector2f(Config::getWidth(), Config::getHeight())});
        backgroundCamera->setView(sf::View{ssvs::zeroVec2f,
            {Config::getWidth() * Config::getZoomFactor(),
             Config::getHeight() * Config::getZoomFactor()}});
        backgroundCamera->setRotation(0);
        overlayCamera->setSkew(sf::Vector2f{1.f, 1.f});
        backgroundCamera->setSkew(sf::Vector2f{1.f, 1.f});
    }

    inputImplCCW = inputImplCW = false;
    playerNowReadyToSwap = false;

    if(!firstPlay) runLuaFunctionIfExists<void>("onPreUnload");
    lua = Lua::LuaContext{};
    calledDeprecatedFunctions.clear();
    initLua();
    runLuaFile(levelData->luaScriptPath);

    if(!firstPlay)
    {
        runLuaFunctionIfExists<void>("onUnload");
        playSoundOverride("restart.ogg");
    }
    else
    {
        playSoundOverride("select.ogg");
    }

    runLuaFunctionIfExists<void>("onInit");

    restartId = mId;
    restartFirstTime = false;
    setSides(levelStatus.sides);   // plays beep + clamps sides >= 3

    // Set initial values for some status fields from Lua
    status.pulseDelay += levelStatus.pulseInitialDelay;
    status.beatPulseDelay += levelStatus.beatPulseInitialDelay;
    timeUntilRichPresenceUpdate = -1.f; // immediate update

    // ... (restartInput/replayInput hint strings, see source: builds "PRESS <key> TO RESTART\n" etc.,
    //      trims at first comma, appends " OR JOYSTICK <btn>" when joystick bind set;
      //      "NO RESTART BUTTON SET\n" / "NO REPLAY BUTTON SET\n" fallbacks)
}
```

Key facts:
- `status = HexagonGameStatus{}` — full default re-init: pulse 75, pulseDirection 1, radius 75, currentPause 6.0, hasDied false, started false, etc.
- `setSides(levelStatus.sides)` after `onInit` (Lua may have changed sides in onInit).
- `status.pulseDelay`/`beatPulseDelay` initialized from Lua-set `levelStatus.pulseInitialDelay`/`beatPulseInitialDelay` AFTER status reset.
- Music: `playLevelMusic()` picks a random segment via `musicData.playRandomSegment` and `status.beatPulseDelay += segment.beatPulseDelayOffset` (see TODO comment: "can this actually de-sync replays? beatpulse affects gameplay" — YES it does: beat pulse is gameplay).

### Death path (verbatim)

```cpp
void HexagonGame::death(bool mForce)
{
    if(status.hasDied) { return; }

    deathInputIgnore = 10.f;

    playSoundAbort(levelStatus.deathSound);
    runLuaFunctionIfExists<void>("onPreDeath");

    if(!mForce && (Config::getInvincible() || levelStatus.tutorialMode))
    {
        return;
    }

    runLuaFunctionIfExists<void>("onDeath");
    death_shakeCamera();
    death_updateRichPresence();
    stopLevelMusic();
    death_flashEffect();

    status.hasDied = true;

    if(!inReplay())
    {
        const replay_file rf = death_createReplayFile();
        if(onDeathReplayCreated) { onDeathReplayCreated(rf); }
        death_sendAndSaveReplay(rf);
    }

    death_saveScoreIfNeededAndShowPBEffects(); // Saves local best

    if(window != nullptr && Config::getAutoRestart())
    {
        status.mustStateChange = StateChange::MustRestart;
    }
}
```

- `deathInputIgnore = 10.f` (FT units) blocks restart/replay key input right after death (decayed in `update`).
- Invincible/tutorial → `onPreDeath` runs but NOT `onDeath`, no shake/flash, `hasDied` stays false.

```cpp
void HexagonGame::death_shakeCamera()
{
    overlayCamera->setView(sf::View{{Config::getWidth() / 2.f, Config::getHeight() / 2.f},
        sf::Vector2f(Config::getWidth(), Config::getHeight())});
    backgroundCamera->setCenter(ssvs::zeroVec2f);
    status.cameraShake = 45.f * Config::getCameraShakeMultiplier();
}

void HexagonGame::death_flashEffect()
{
    initFlashEffect(255, 255, 255);
    status.flashEffect = 255;
}
```

Camera shake on death: `45.f * Config::getCameraShakeMultiplier()`.

```cpp
replay_file HexagonGame::death_createReplayFile()
{
    const std::string rfName = assets.anyLocalProfileActive()
                                   ? assets.getCurrentLocalProfile().getName()
                                   : "no_profile";
    return replay_file{
        ._version{0},
        ._player_name{rfName},
        ._seed{lastSeed},
        ._data{lastReplayData},
        ._pack_id{packId},
        ._level_id{levelId},
        ._first_play{firstPlay},
        ._difficulty_mult{difficultyMult},
        ._played_score{getReplayScore(status)},
    };
}
```

`death_sendAndSaveReplay`: compress via `compress_replay_file`; send to server if `death_sendReplay` (requires hexagonClient LoggedIn_Ready + official); save to `Replays/<levelId>/<diff>x/<filename>.z` (invalid chars `: * ? " < > |` replaced by `_`).

`death_saveScoreIfNeeded`: local score = `status.getTimeSeconds()` (double, seconds); is PB if `> assets.getLocalScore(validator)`; saves via `assets.setLocalScore`. PB effects: `pbText = "NEW PERSONAL BEST!"`, `mustSpawnPBParticles = true`, sound "personalBest.ogg"; else "gameOver.ogg".

### Headless execution (test/replay tooling, verbatim)

```cpp
std::optional<HexagonGame::GameExecutionResult>
HexagonGame::executeGameUntilDeath(const int maxProcessingSeconds, const float timescale)
{
    const HRTimePoint tpBegin = HRClock::now();
    const auto exceededProcessingTime = [&] { return hrSecondsSince(tpBegin) > maxProcessingSeconds; };

    while(!status.hasDied)
    {
        update(Config::TIME_STEP, timescale);
        postUpdate();
        if(exceededProcessingTime()) { return std::nullopt; }
    }

    return GameExecutionResult{
        .playedTimeSeconds = status.getPlayedAccumulatedFrametimeInSeconds(),
        .pausedTimeSeconds = status.getPausedAccumulatedFrametimeInSeconds(),
        .totalTimeSeconds  = status.getTotalAccumulatedFrametimeInSeconds(),
        .customScore       = status.getCustomScore()
    };
}

std::optional<HexagonGame::GameExecutionResult>
HexagonGame::runReplayUntilDeathAndGetScore(const replay_file& mReplayFile,
    const int maxProcessingSeconds, const float timescale)
{
    setLastReplay(mReplayFile);
    newGame(mReplayFile._pack_id, mReplayFile._level_id,
        mReplayFile._first_play, mReplayFile._difficulty_mult,
        /* mExecuteLastReplay */ true);
    return executeGameUntilDeath(maxProcessingSeconds, timescale);
}

struct GameExecutionResult
{
    double playedTimeSeconds;
    double pausedTimeSeconds;
    double totalTimeSeconds;
    float customScore;
};

void HexagonGame::setLastReplay(const replay_file& mReplayFile)
{
    lastSeed = mReplayFile._seed;
    lastReplayData = mReplayFile._data;
    lastFirstPlay = mReplayFile._first_play;
    lastPlayedScore = mReplayFile._played_score;
    activeReplay.emplace(mReplayFile);
}
```

### Difficulty increment rules — `incrementDifficulty` + `sideChange` (verbatim)

```cpp
void HexagonGame::incrementDifficulty()
{
    playSoundOverride("levelUp.ogg");

    const float signMult = (levelStatus.rotationSpeed > 0.f) ? 1.f : -1.f;

    levelStatus.rotationSpeed += levelStatus.rotationSpeedInc * signMult;

    const auto& rotationSpeedMax(levelStatus.rotationSpeedMax);
    if(std::abs(levelStatus.rotationSpeed) > rotationSpeedMax)
    {
        levelStatus.rotationSpeed = rotationSpeedMax * signMult;
    }

    levelStatus.rotationSpeed *= -1.f;
    status.fastSpin = levelStatus.fastSpin;
}

void HexagonGame::sideChange(unsigned int mSideNumber)
{
    levelStatus.speedMult += levelStatus.speedInc;
    levelStatus.delayMult += levelStatus.delayInc;

    if(levelStatus.rndSideChangesEnabled)
    {
        setSides(mSideNumber);
    }

    mustChangeSides = false;

    playSoundOverride(levelStatus.levelUpSound);
    runLuaFunctionIfExists<void>("onIncrement");
}
```

Difficulty rules:
- **Speed**: `speedMult` accumulates `+ speedInc` on EACH side change (i.e. every increment). Effective wall speed = `getSpeedMultDM() = speedMult * pow(difficultyMult, 0.65f)`, clamped to `speedMax` if `hasSpeedMaxLimit()` (`speedMax > 0.f`).
- **Delay**: `delayMult` accumulates `+ delayInc`. Effective = `getDelayMultDM() = delayMult / pow(difficultyMult, 0.10f)`, clamped to `delayMax` if `hasDelayMaxLimit()` (`delayMax > 0.f`). (Division — higher DM makes delay multiplier SMALLER.)
- **Rotation**: on each increment: `signMult = rotationSpeed > 0 ? 1 : -1` (NOTE: zero → -1); `rotationSpeed += rotationSpeedInc * signMult` (i.e. moves AWAY from zero by rotationSpeedInc); clamp |rotationSpeed| to `rotationSpeedMax` (keeping signMult's sign); then `rotationSpeed *= -1.f` (DIRECTION FLIPS every increment). Also `status.fastSpin = levelStatus.fastSpin` (triggers the fast-spin ramp effect; counts down in updateRotation).
- **Sides**: `sideChange(n)` called with `rng.get_int(sidesMin, sidesMax)` — only applied when `rndSideChangesEnabled` (else sides stay; mustChangeSides still cleared). `setSides` clamps to minimum 3 and plays the beep.
- Trigger: `updateIncrement()` every `incTime` seconds → `incrementDifficulty()` then `mustChangeSides = true`; the actual `sideChange` happens in `update` only when `walls.empty()` (wait for walls to despawn).

### Multipliers / getters (verbatim)

```cpp
float HexagonGame::getMusicDMSyncFactor() const { return std::pow(difficultyMult, 0.12f); }
float HexagonGame::getOptionalMusicDMSyncFactor() const
{ return levelStatus.syncMusicToDM ? getMusicDMSyncFactor() : 1.f; }

void HexagonGame::refreshMusicPitch()
{
    if(audio != nullptr)
    {
        audio->setCurrentMusicPitch(getOptionalMusicDMSyncFactor() *
                                    Config::getMusicSpeedMult() *
                                    levelStatus.musicPitch);
    }
}

void HexagonGame::setSides(unsigned int mSides)
{
    playSoundOverride(levelStatus.beepSound);
    if(mSides < 3) { mSides = 3; }
    levelStatus.sides = mSides;
}

float HexagonGame::getRadius() const noexcept { return status.radius; }

float HexagonGame::getSpeedMultDM() const noexcept
{
    const auto res = levelStatus.speedMult * (std::pow(difficultyMult, 0.65f));
    if(!levelStatus.hasSpeedMaxLimit()) { return res; }
    return (res < levelStatus.speedMax) ? res : levelStatus.speedMax;
}

float HexagonGame::getDelayMultDM() const noexcept
{
    const auto res = levelStatus.delayMult / (std::pow(difficultyMult, 0.10f));
    if(!levelStatus.hasDelayMaxLimit()) { return res; }
    return (res < levelStatus.delayMax) ? res : levelStatus.delayMax;
}

float HexagonGame::getRotationSpeed() const noexcept { return levelStatus.rotationSpeed; }
unsigned int HexagonGame::getSides() const noexcept { return levelStatus.sides; }
// getWallSkewLeft/Right, getWallAngleLeft/Right → levelStatus fields
float HexagonGame::getPlayerSpeedMult() const { return levelStatus.playerSpeedMult; }
bool HexagonGame::getInputFocused() const { return inputFocused; }
bool HexagonGame::getInputSwap() const { return inputSwap; }
int HexagonGame::getInputMovement() const { return inputMovement; }

bool HexagonGame::inReplay() const noexcept { return activeReplay.has_value(); }
bool HexagonGame::mustReplayInput() const noexcept { return inReplay() && !activeReplay->replayPlayer.done(); }
bool HexagonGame::mustShowReplayUI() const noexcept { return inReplay(); }

float HexagonGame::getSwapCooldown() const noexcept
{
    return std::max(36.f * levelStatus.swapCooldownMult, 8.f);
}

void HexagonGame::performPlayerSwap(const bool mPlaySound)
{
    player.playerSwap();
    runLuaFunctionIfExists<void>("onCursorSwap");
    if(mPlaySound) { playSoundOverride(getLevelStatus().swapSound); }
}

void HexagonGame::performPlayerKill()
{
    const bool fatal = !Config::getInvincible() && !getLevelStatus().tutorialMode;
    player.kill(fatal);
    death();
}
```

- **Swap cooldown**: `max(36.f * swapCooldownMult, 8.f)` FT (60 FT = 1 s, so base 0.6 s; floor 8 FT ≈ 0.133 s).
- Player speed: `Config::getPlayerSpeed()` × `levelStatus.playerSpeedMult` (see CPlayer::updateInputMovement — `_currentSpeed = playerSpeedMult * speed * ft`, degrees/FT).
- Colors: `getColorMain/Player/Text/Wall/Cap` return white (alpha preserved) when `Config::getBlackAndWhite()`; `getColorPlayerAdjustedForSwap` → `player.getColorAdjustedForSwap(...)` only when `Config::getShowSwapBlinkingEffect()`; `getColorPlayerTrail` → swap-adjusted color when `Config::getPlayerTrailHasSwapColor()`.

### `setLevelData` / music (verbatim)

```cpp
void HexagonGame::setLevelData(const LevelData& mLevelData, bool mMusicFirstPlay)
{
    levelData = &mLevelData;
    levelStatus = LevelStatus{Config::getMusicSpeedDMSync(), Config::getSpawnDistance()};
    styleData = assets.getStyleData(levelData->packId, levelData->styleId);
    styleData.computeColors();
    musicData = assets.getMusicData(levelData->packId, levelData->musicId);
    musicData.firstPlay = mMusicFirstPlay;
}

void HexagonGame::playLevelMusic()
{
    if(shouldPlayMusic())
    {
        const MusicData::Segment segment = musicData.playRandomSegment(getPackId(), *audio);
        // TODO (P0): can this actually de-sync replays? beatpulse affects gameplay
        // TODO (P1): problems with addHash in headless mode:
        status.beatPulseDelay += segment.beatPulseDelayOffset;
    }
}

void HexagonGame::playLevelMusicAtTime(float mSeconds)
{
    if(shouldPlayMusic()) { musicData.playSeconds(getPackId(), *audio, mSeconds); }
}

void HexagonGame::stopLevelMusic()
{
    if(shouldPlayMusic()) { audio->stopMusic(); }
}
```

### `addMessage` (verbatim)

```cpp
void HexagonGame::addMessage(std::string mMessage, double mDuration, bool mSoundToggle)
{
    if(!Config::getShowMessages()) { return; }
    Utils::uppercasify(mMessage);

    messageTimeline.append_do(
        [this, mSoundToggle, mMessage]
        {
            if(mSoundToggle) { playSoundOverride(levelStatus.beepSound); }
            messageText.setString(mMessage);
        });

    messageTimeline.append_wait_for_sixths(mDuration);   // mDuration is in SECONDS
    messageTimeline.append_do([this] { messageText.setString(""); });
}

void HexagonGame::clearMessages() { messageTimeline.clear(); }
```

### `invalidateScore` / `shouldSaveScore` / `goToMenu` / `raiseWarning`

- `invalidateScore(reason)`: one-shot; sets `status.scoreInvalid = true`, `status.invalidReason = reason`; logs.
- `shouldSaveScore()`: false (with log) when: no local profile; `!Config::isEligibleForScore()` (reason from `Config::getUneligibilityReason()`); `status.scoreInvalid`; `levelStatus.tutorialMode`; `levelData->unscored`; `inReplay()`.
- `goToMenu(sendScores=true, error=false)`: stopSounds; clear tracked; beep unless error; `saveReplay()` if sendScores && !error; `onUnload` unless error; call `fnGoToMenu(error)`.
- `raiseWarning(fn, info)`: once-per-function-name (via `calledDeprecatedFunctions` set); prints `[Lua] WARNING: The function "<fn>" (used in level "<level>") is deprecated. <info>` and logs `[warning]: ...`.

### `saveReplay` (verbatim)

```cpp
void HexagonGame::saveReplay()
{
    if(shouldSaveScore() && !status.hasDied)
    {
        (void)death_saveScoreIfNeeded(); // Saves local best
        const replay_file rf = death_createReplayFile();
        ssvu::lo("Replay") << "Attempting to send and save replay...\n";
        death_sendAndSaveReplay(rf);
    }
}
```

### UI update helpers (key constants)

- `updateKeyIcons`: icons halfSize 32.f; left arrow rotated 180°; scale = `getKeyIconsScale()/getZoomFactor()`; padding `8.f * scaling`; layout chain swap → focus → right → left stepping left by `scaledSize + padding` from bottom-right corner; replay icon at top-right, scale `scaling / 2.f`.
- `updateLevelInfo`: rectangle `325.f x 75.f`, outline thickness `3.f`, scaling `1.f/getZoomFactor()`, padding 8; level name font 20, pack 14, author 20 (SE-anchored), "BY" 12, DM 14; names trimmed to 26 chars + uppercased; shown only when `levelData->difficultyMults.size() > 1`.
- `nameFormat`: capitalizes first char (`name[0] = toupper`).
- `diffFormat`: `snprintf("%g", diff)`; `timeFormat`: `snprintf("%.3f", time)`.

### RNG usage summary (game `random_number_generator` only)

| Call site | Expression | T | Notes |
|---|---|---|---|
| `update` (side change) | `rng.get_int(levelStatus.sidesMin, levelStatus.sidesMax)` | `unsigned int` | only when `mustChangeSides && walls.empty()` |
| `updateCameraShake` | `rng.get_real(-i, i)` × 2 (x, y) | `float` | per shaken frame; `i = status.cameraShake` |
| `update` (RNG fix-up) | `rng.advance(abs(x))` × 5 | u64 deltas | pulse, pulse3D, fastSpin, flashEffect, rotationSpeed (abs, u64 truncation) |
| `newGame` | `random_number_generator{seed}` / `initializeRng()` | u64 seed | fresh seed per level/restart; replay uses saved seed |
| (not game RNG) | `ssvu::getRndR/getRndI` (particles, PB spawn) | — | thread-local global pcg32_fast; cosmetic |
| (not game RNG) | `std::mt19937` + `std::random_device` (executeRandomInputs) | int | test-only |
| (not game RNG) | `musicData.playRandomSegment` | — | see MusicData (06-lua mapping / assets) |

---

## 16. Cross-cutting notes for the Rust porter

1. **World space**: origin-centered. `centerPos = (0,0)`. Player angle `atan2(y, x)` is from world origin. All wall/geometry math assumes circle centered at (0,0).
2. **FT units**: 60 FT-units per second. `ssvu::FT = float`. Fixed tick 0.25 FT (240 Hz) in the real loop; test paths use `update(0.25, timescale)`.
3. **Time bookkeeping is double-precision** (HGStatus), while all gameplay quantities (positions, speeds, radii) are float.
4. **`status.radius`** is the gameplay collision radius (player orbit radius); `radiusSquared` for wall collision adds `+ 8.f` to the SQUARED value.
5. **Wall quads**: 4 verts nw/sw/se/ne (indices 0/1/2/3), split on (0,2) diagonal; CWall verts generated at `distance` (inner edge = spawn distance) — wait: inner edge is at `distance`, outer at `distance + thickness (+skew)`. The wall is created at spawn distance and moves inward.
6. **Replay determinism chain**: seed → PCG state (seed|3, burn 1 step) → per-frame `rng.advance(|float|)` ×5 + event-driven draws (sides, camera shake) + Lua `onIncrement`/`onUpdate` (Lua RNG? — levels calling `rnd()` use the game RNG via Lua API — see 06-lua-game-api mapping) must ALL be reproduced in exact order.
7. **`updatePulse3D` runs even when 3D disabled** solely to advance its state (and hence the RNG fix-up) — do not gate it.
8. **`getLineCircleIntersection` is not segment-clamped** (no t ∈ [0,1] check) — intentional.
9. **Ticker overflow**: when a tick exceeds the target, the excess time is dropped (current resets to 0).
10. **`ssvu::getRndI(min, max)` is EXCLUSIVE of max** (uses `max - 1`); the game `random_number_generator::get_int(min, max)` is INCLUSIVE (std distribution). Do not confuse.
11. **`std::uniform_int_distribution` / `uniform_real_distribution` are libstdc++-specific** — the Rust port must replicate the exact distribution algorithm of the build toolchain (Lemire 128-bit path for u32 engines on libstdc++ ≥ 14; see §7) or replay parity will break.
12. **Pulse/beat-pulse affect the radius** (`refreshBeatPulse`) — they are gameplay, not just visual. `musicData.playRandomSegment`'s `beatPulseDelayOffset` is part of the deterministic state (but the segment choice is RNG via MusicData — verify its RNG source in the music mapping).
13. **`levelStatus.rotationSpeed` starts at 0** → `signMult = -1` on the first increment (zero is treated as negative).
14. **`fixup(-0.f)`** → 0 (the `x < 0.f` check is false for -0.f).
15. **CCustomWall vertex ordering is arbitrary** (Lua-defined); `pointInPolygon<4>` (even-odd) is used for overlap, NOT the cross-product test.
16. **`_prePushPos`** is updated in `updatePosition` (every frame, from `_angle` before this frame's movement) and is the reference for push displacement direction.


---

## 17. Out-of-scope references (exact locations, for follow-up mappings)

- `HexagonGame::draw()` and all `draw*` methods: `src/SSVOpenHexagon/Core/HGGraphics.cpp` (graphics mapping 10/11).
- `HexagonGame::initFlashEffect(int r, int g, int b)` (HGGraphics.cpp, verbatim):

```cpp
void HexagonGame::initFlashEffect(int r, int g, int b)
{
    flashPolygon.clear();
    flashPolygon.reserve(6);

    const sf::Color color{static_cast<std::uint8_t>(r),
        static_cast<std::uint8_t>(g), static_cast<std::uint8_t>(b), 0};

    const auto width = static_cast<float>(Config::getWidth());
    const auto height = static_cast<float>(Config::getHeight());
    const float offset = 100.f;

    const sf::Vector2f nw{-offset, -offset};
    const sf::Vector2f sw{-offset, height + offset};
    const sf::Vector2f se{width + offset, height + offset};
    const sf::Vector2f ne{width + offset, -offset};

    flashPolygon.batch_unsafe_emplace_back_quad(color, nw, sw, se, ne);
}
```
(Screen-space quad covering the window plus a 100.f margin; initial alpha 0; `updateFlash` sets `vertex.color.a = status.flashEffect` each frame.)

- `HexagonGame::updateText(ssvu::FT mFT)` (HGGraphics.cpp): builds `os` (ostringstream) lines: debug pause, tutorial/official mode, debug + `CUSTOM WALLS: count / maxHandles`; when started: `SWAP ENABLED`, `INVINCIBILITY ON`, time text `timeFormat(status.getTimeSeconds())` + "s" (70pt bold, center), FPS text; then sets `text`/`timeText`/`fpsText`. PB text animation: `pbTextGrowth += 0.08f * mFT`, wraps at `ssvu::pi * 2.f` (→ period 78.54 FT ≈ 1.31 s cycle). (Full body in source; UI only.)
- `HexagonGame::executeEvents(ssvuj::Obj&, float)`: `src/SSVOpenHexagon/Core/HGScripting.cpp` (Lua event scheduling from `executeEvents` JSON — see mapping 07).
- `StyleData` fields used: `float _3dPulseMax{}`, `float _3dPulseMin{}`, `float _3dPulseSpeed{}` (StyleData.hpp lines 85-87), plus `computeColors()` / `update(ssvu::FT, float mult = 1.f)` (styles mapping 11).
- `ssvs::getDistEuclidean(const Vec2<T1>&, const Vec2<T2>&)` overload (SSVStart/Utils/Vector2.hpp line 573): `return ssvu::getDistEuclidean(mA.x, mA.y, mB.x, mB.y);` — used by `CPlayer::kill`.
- `ssvs::Input` two-state inputs (`add2StateInput`) set the bool on press/release; once-triggers fire on rising edge; see SSVStart Input mapping (12-menu or 09-game-loop).
- `LevelStatus` ctor (src/SSVOpenHexagon/Data/LevelStatus.cpp, verbatim):

```cpp
LevelStatus::LevelStatus(const bool mSyncMusicToDM, const float mWallSpawnDistance)
    : syncMusicToDM{mSyncMusicToDM}, wallSpawnDistance{mWallSpawnDistance}
{}
bool LevelStatus::hasSpeedMaxLimit() const noexcept { return speedMax > 0.f; }
bool LevelStatus::hasDelayMaxLimit() const noexcept { return delayMax > 0.f; }
```

