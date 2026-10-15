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
