# Rendering & Menu Mapping (C++ → Rust)

Mechanical transcription of the rendering + menu source files for the Rust port
(issues 10–14). All formulas, layout constants, pixel values, and user-visible
strings are transcribed verbatim. "FT" is always `ssvu::FT` = `float`.

Source files covered (in this document):

1. `include/SSVOpenHexagon/Core/HGStatus.hpp` — `RenderStage` enum (render-stage order + shader binding)
2. `extlibs/SSVStart/include/SSVStart/Camera/Camera.hpp` — `ssvs::Camera`
3. `src/SSVOpenHexagon/Core/HGGraphics.cpp` — `draw()`, `render()`, `drawText_*`, `drawKeyIcons`, `drawLevelInfo`, `drawParticles`, `drawTrailParticles`, `drawSwapParticles`, `drawImguiLuaConsole`, `updateText`, `initFlashEffect`
4. `include/SSVOpenHexagon/Core/HexagonGame.hpp` + `src/SSVOpenHexagon/Core/HexagonGame.cpp` — camera setup, color getters, key-icon & level-info updates, `initText`, `diffFormat`/`timeFormat`
5. `src/SSVOpenHexagon/Core/HGUpdate.cpp` — `update()` (render-relevant calls), pulse/beat-pulse/rotation/camera-shake/flash/pulse3D updates, particle updates, `ilcTextEditCallback`, `postUpdate_ImguiLuaConsole`, `postUpdate`
6. `include/SSVOpenHexagon/Utils/FastVertexVector.hpp`, `Utils/Easing.hpp`, `Utils/Color.hpp`, `Utils/FontHeight.hpp` (+ `src/.../Utils/FontHeight.cpp`)
7. `include/SSVOpenHexagon/Data/StyleData.hpp` + `src/SSVOpenHexagon/Data/StyleData.cpp`, `Data/ColorData.hpp/.cpp`, `Data/CapColor.hpp/.cpp`
8. `include/SSVOpenHexagon/Global/Imgui.hpp` + `src/SSVOpenHexagon/Global/Imgui.cpp`
9. Shader system: `_RELEASE/Packs/experimental/Shaders/*.frag`, `_RELEASE/Packs/artwork/Shaders/*.frag`, `Global/Assets.cpp` (shader load/lookup), `Core/LuaScripting.cpp` (`shdr_*`), `_RELEASE/Packs/base/Scripts/utils.lua` (`RenderStage` table)
10. `extlibs/SSVStart/include/SSVStart/Utils/SFML.hpp` + `Utils/Vector2.hpp` — geometry helpers used by all draw code
11. `src/SSVOpenHexagon/Core/MenuGame.cpp` + `include/SSVOpenHexagon/Core/MenuGame.hpp` — all states, menus, items, drawing
12. `src/SSVOpenHexagon/Core/HexagonDialogBox.cpp` + `.hpp`

Conventions used in this document:

- "verbatim" code blocks reproduce the C++ exactly (comments trimmed only when redundant; trimming noted inline with `//` markers only where the comment itself matters).
- All numeric literals are quoted exactly as in source (e.g. `32.f`, `0.18f`).
- `ssvu::FT` = `float`. `ssvu` = SSVUtils, `ssvs` = SSVStart, `ssvuj` = JSON wrapper.
- SFML types: `sf::Vector2f` (float x,y), `sf::Color` (uint8 r,g,b,a),
  `sf::View` (center + size + rotation), `sf::Text` (string, font, char size,
  position, origin, scale, fill/outline color+thickness), `sf::Sprite`
  (texture, position, origin, scale, rotation, color).
- `getGlobalBounds()` of an `sf::Text` accounts for origin+position+scale;
  `getLocalBounds()` does not (it is the untransformed glyph-bounds rect,
  usually with `left/top` slightly negative due to font metrics).
- `Config::` references are to `Global/Config.hpp` (mapped in 04-config.md).
  `levelStatus`/`status` fields: see gameplay.md (LevelStatus.hpp,
  HexagonGameStatus in HGStatus.hpp).

---

## 1. RenderStage — `include/SSVOpenHexagon/Core/HGStatus.hpp`

`enum class RenderStage : std::size_t` — **this is the exact render order for
the per-stage fragment-shader binding** (and matches the `RenderStage` Lua
table in `_RELEASE/Packs/base/Scripts/utils.lua`):

```cpp
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

`HexagonGameStatus` (in the same header) holds the per-stage binding:

```cpp
    // Shaders
    std::array<std::optional<std::size_t>,
        static_cast<std::size_t>(RenderStage::Count)>
        fragmentShaderIds;
```

(other `HexagonGameStatus` fields — `pulse`, `pulseDirection`, `pulseDelay`,
`beatPulse`, `beatPulseDelay`, `pulse3D`, `pulse3DDirection`, `flashEffect`,
`radius`, `fastSpin`, `cameraShake`, `hasDied`, `started`, `showPlayerTrail`,
timers — are covered in gameplay.md §HGStatus.)

Lua table (verbatim from `_RELEASE/Packs/base/Scripts/utils.lua` lines 29–40,
loaded for every level so scripts can write `RenderStage.TEXT` etc.):

```lua
-- Render stages for shaders
RenderStage = {
    BACKGROUNDTRIS = 0,
    WALLQUADS3D = 1,
    PIVOTQUADS3D = 2,
    PLAYERTRIS3D = 3,
    WALLQUADS = 4,
    CAPTRIS = 5,
    PIVOTQUADS = 6,
    PLAYERTRIS = 7,
    TEXT = 8
}
```

## 2. `ssvs::Camera` — `extlibs/SSVStart/include/SSVStart/Camera/Camera.hpp`

```cpp
class Camera
{
private:
    sf::View view, computedView;
    sf::Vector2f nextPan, offset, skew{1.f, 1.f};
    float nextZoomFactor{1.f}, nextRotation{0.f};
    bool invalid{true}, mustRecompute{true};

public:
    Camera(const sf::View& mView) : view{mView} {}

    Camera(GameWindow& mGameWindow, const sf::Vector2f& mCenter,
        float mZoomFactor = 1.f)
        : view{mCenter, {mGameWindow.getWidth() / mZoomFactor,
                            mGameWindow.getHeight() / mZoomFactor}}
    {
        assert(mZoomFactor != 0);
    }

    Camera(GameWindow& mGameWindow, float mZoomFactor = 1.f)
        : view{{mGameWindow.getWidth() / 2.f / mZoomFactor,
                   mGameWindow.getHeight() / 2.f / mZoomFactor},
              {mGameWindow.getWidth() / mZoomFactor,
                  mGameWindow.getHeight() / mZoomFactor}}
    {
        assert(mZoomFactor != 0);
    }

    template <typename T = float>
    [[nodiscard]] sf::View apply()
    {
        if(mustRecompute)
        {
            computedView = view;

            computedView.setSize(
                {ssvu::toNum<T>(computedView.getSize().x * skew.x),
                    ssvu::toNum<T>(computedView.getSize().y * skew.y)});

            if(getMag(offset) != 0)
            {
                computedView.setCenter(
                    {view.getCenter() -
                        getVecFromDeg(
                            view.getRotation().asDegrees() + getDeg(offset),
                            getMag(offset))});
            }

            computedView.setCenter({ssvu::toNum<T>(computedView.getCenter().x),
                ssvu::toNum<T>(computedView.getCenter().y)});

            mustRecompute = false;
        }

        return computedView;
    }

    // These methods change the view ON NEXT UPDATE
    void pan(const sf::Vector2f& mVec)   { nextPan += mVec;   invalid = true; }
    void pan(float mX, float mY)         { nextPan.x += mX; nextPan.y += mY; invalid = true; }
    void zoomIn(float mFactor)           { nextZoomFactor /= mFactor; invalid = true; }
    void zoomOut(float mFactor)          { nextZoomFactor *= mFactor; invalid = true; }
    void turn(float mDeg)                { nextRotation += mDeg; invalid = true; }

    template <typename T = float>
    void update(ssvu::FT mFT)
    {
        if(!invalid) return;

        view.setCenter(view.getCenter() + nextPan);
        view.zoom(std::pow(nextZoomFactor, mFT));
        view.rotate(sf::degrees(nextRotation * mFT));

        mustRecompute = true;
        nullify(nextPan);
        nextZoomFactor = 1.f;
        nextRotation = 0.f;
        invalid = false;
    }

    // These properties IMMEDIATELY change the view
    void setView(const sf::View& mView) noexcept { view = mView; mustRecompute = true; }
    void setRotation(float mDeg) noexcept { view.setRotation(sf::degrees(mDeg)); mustRecompute = true; }
    void setSkew(const sf::Vector2f& mSkew) noexcept { skew = mSkew; mustRecompute = true; }
    void setOffset(const sf::Vector2f& mOffset) noexcept { offset = mOffset; mustRecompute = true; }
    void setCenter(const sf::Vector2f& mPosition) noexcept { view.setCenter(mPosition); mustRecompute = true; }

    // This property changes the view ON NEXT UPDATE
    void setNextZoomFactor(float mFactor) noexcept { nextZoomFactor = mFactor; invalid = true; }

    const auto& getView() const noexcept { return view; }
    float getRotation() const noexcept { return view.getRotation().asDegrees(); }
    const auto& getSkew() const noexcept { return skew; }
    const auto& getOffset() const noexcept { return offset; }
    const auto& getCenter() const noexcept { return view.getCenter(); }
    float getNextZoomFactor() const noexcept { return nextZoomFactor; }

    auto getMousePosition(const sf::RenderWindow& mRenderWindow) const
    {
        return mRenderWindow.mapPixelToCoords(
            sf::Mouse::getPosition(mRenderWindow), view);
    }

    auto getConvertedCoords(
        const sf::RenderWindow& mRenderWindow, const sf::Vector2i& mPos) const
    {
        return mRenderWindow.mapPixelToCoords(mPos, view);
    }

    bool isInView(const sf::Vector2f& mPos) const
    {
        return mPos.x <= view.getCenter().x + view.getSize().x &&
               (mPos.x >= view.getCenter().x - view.getSize().x &&
                   (mPos.y <= view.getCenter().y + view.getSize().y &&
                       mPos.y >= view.getCenter().y - view.getSize().y));
    }
};
```

Notes:

- `apply()` is what `window->setView(camera->apply())` consumes: it bakes the
  `skew` (size multiplier) and `offset` (polar offset subtracted from center,
  rotated by the view rotation) into a copy `computedView`, cached until
  `mustRecompute`.
- `view.zoom(z)` divides the view size by `z` (SFML semantics);
  `update()` applies `pow(nextZoomFactor, mFT)` — exponential zoom easing per
  frame.
- `sf::View::rotate` is additive in degrees; `sf::View::setRotation` replaces.
- MenuGame uses the default ctor `Camera(GameWindow&)` (center = window
  center, size = window size, zoom factor 1) plus `setView`/`pan`/`zoomIn`/
  `zoomOut`/`turn`/`update` — see §11.

## 3. `src/SSVOpenHexagon/Core/HGGraphics.cpp`

### 3.1 `formatTime` (file-static)

```cpp
[[nodiscard]] static std::string formatTime(const double x)
{
    return ssvu::toStr(std::floor(x * 1000) / 1000.f);
}
```

(`ssvu::toStr` → `std::to_string`. Note: `floor(x*1000)` is a double, divided
by `1000.f` → double; stringified with default `std::to_string` precision,
i.e. 6 decimal digits, trailing zeros kept, e.g. `"12.500000"`.)

### 3.2 `render` (single-drawable submission)

```cpp
void HexagonGame::render(
    sf::Drawable& mDrawable, const sf::RenderStates& mStates)
{
    if(window == nullptr)
    {
        ssvu::lo("hg::HexagonGame::render")
            << "Attempted to render without a game window\n";

        return;
    }

    window->draw(mDrawable, mStates);
}
```

### 3.3 `draw` (full frame, exact order)

```cpp
void HexagonGame::draw()
{
    if(window == nullptr || Config::getDisableGameRendering())
    {
        return;
    }

    const auto getRenderStates = [this](
                                     const RenderStage rs) -> sf::RenderStates
    {
        if(!Config::getShaders())
        {
            return sf::RenderStates::Default;
        }

        const std::optional<std::size_t> fragmentShaderId =
            status.fragmentShaderIds[static_cast<std::size_t>(rs)];

        if(!fragmentShaderId.has_value())
        {
            return sf::RenderStates::Default;
        }

        runLuaFunctionIfExists<int, float>(
            "onRenderStage", static_cast<int>(rs), 60.f / window->getFPS());
        return sf::RenderStates{assets.getShaderByShaderId(*fragmentShaderId)};
    };

    SSVOH_ASSERT(backgroundCamera.has_value());
    SSVOH_ASSERT(overlayCamera.has_value());

    window->clear(sf::Color::Black);

    if(!status.hasDied)
    {
        if(levelStatus.cameraShake > 0.f)
        {
            const sf::Vector2f shake(ssvu::getRndR(-levelStatus.cameraShake,
                                         levelStatus.cameraShake),
                ssvu::getRndR(
                    -levelStatus.cameraShake, levelStatus.cameraShake));

            backgroundCamera->setCenter(shake);
            overlayCamera->setCenter(
                shake + sf::Vector2f{Config::getWidth() / 2.f,
                            Config::getHeight() / 2.f});
        }
        else
        {
            backgroundCamera->setCenter(ssvs::zeroVec2f);
            overlayCamera->setCenter(sf::Vector2f{
                Config::getWidth() / 2.f, Config::getHeight() / 2.f});
        }
    }

    if(!Config::getNoBackground())
    {
        window->setView(backgroundCamera->apply());

        backgroundTris.clear();

        styleData.drawBackground(backgroundTris, ssvs::zeroVec2f,
            levelStatus.sides,
            Config::getDarkenUnevenBackgroundChunk() &&
                levelStatus.darkenUnevenBackgroundChunk,
            Config::getBlackAndWhite());

        render(backgroundTris, getRenderStates(RenderStage::BackgroundTris));
    }

    window->setView(backgroundCamera->apply());

    wallQuads3D.clear();
    pivotQuads3D.clear();
    playerTris3D.clear();
    wallQuads.clear();
    pivotQuads.clear();
    playerTris.clear();
    capTris.clear();

    // Reserve right amount of memory for all walls and custom walls
    wallQuads.reserve_more_quad(walls.size() + cwManager.count());

    for(CWall& w : walls)
    {
        w.draw(getColorWall(), wallQuads);
    }

    cwManager.draw(wallQuads);

    if(status.started)
    {
        player.draw(getSides(), getColorMain(), getColorPlayer(), pivotQuads,
            capTris, playerTris, getColorCap(), Config::getAngleTiltIntensity(),
            Config::getShowSwapBlinkingEffect());
    }

    if(Config::get3D())
    {
        const float depth(styleData._3dDepth);
        const std::size_t numWallQuads(wallQuads.size());
        const std::size_t numPivotQuads(pivotQuads.size());
        const std::size_t numPlayerTris(playerTris.size());

        wallQuads3D.reserve(numWallQuads * depth);
        pivotQuads3D.reserve(numPivotQuads * depth);
        playerTris3D.reserve(numPlayerTris * depth);

        const float pulse3D{Config::getNoPulse() ? 1.f : status.pulse3D};
        const float effect{
            styleData._3dSkew * Config::get3DMultiplier() * pulse3D};

        const sf::Vector2f skew{1.f, 1.f + effect};
        backgroundCamera->setSkew(skew);

        const float radRot(
            ssvu::toRad(backgroundCamera->getRotation()) + (ssvu::pi / 2.f));
        const float sinRot(std::sin(radRot));
        const float cosRot(std::cos(radRot));

        for(std::size_t i = 0; i < depth; ++i)
        {
            wallQuads3D.unsafe_emplace_other(wallQuads);
            pivotQuads3D.unsafe_emplace_other(pivotQuads);
            playerTris3D.unsafe_emplace_other(playerTris);
        }

        const auto adjustAlpha = [&](sf::Color& c, const float i)
        {
            if(styleData._3dAlphaMult == 0.f)
            {
                return;
            }

            const float newAlpha =
                (static_cast<float>(c.a) / styleData._3dAlphaMult) -
                i * styleData._3dAlphaFalloff;

            c.a = Utils::componentClamp(newAlpha);
        };

        for(int j(0); j < static_cast<int>(depth); ++j)
        {
            const float i(depth - j - 1);

            const float offset(styleData._3dSpacing *
                               (float(i + 1.f) * styleData._3dPerspectiveMult) *
                               (effect * 3.6f) * 1.4f);

            const sf::Vector2f newPos(offset * cosRot, offset * sinRot);

            sf::Color overrideColor;

            if(!Config::getBlackAndWhite())
            {
                overrideColor = Utils::getColorDarkened(
                    styleData.get3DOverrideColor(), styleData._3dDarkenMult);
            }
            else
            {
                overrideColor = Utils::getColorDarkened(
                    sf::Color(255, 255, 255, styleData.getMainColor().a),
                    styleData._3dDarkenMult);
            }
            adjustAlpha(overrideColor, i);

            // Draw pivot layers
            for(std::size_t k = j * numPivotQuads; k < (j + 1) * numPivotQuads;
                ++k)
            {
                pivotQuads3D[k].position += newPos;
                pivotQuads3D[k].color = overrideColor;
            }

            if(styleData.get3DOverrideColor() == styleData.getMainColor())
            {
                overrideColor = Utils::getColorDarkened(
                    getColorWall(), styleData._3dDarkenMult);

                adjustAlpha(overrideColor, i);
            }

            // Draw wall layers
            for(std::size_t k = j * numWallQuads; k < (j + 1) * numWallQuads;
                ++k)
            {
                wallQuads3D[k].position += newPos;
                wallQuads3D[k].color = overrideColor;
            }

            // Apply player color if no 3D override is present.
            if(styleData.get3DOverrideColor() == styleData.getMainColor())
            {
                overrideColor = Utils::getColorDarkened(
                    getColorPlayer(), styleData._3dDarkenMult);

                adjustAlpha(overrideColor, i);
            }

            // Draw player layers
            for(std::size_t k = j * numPlayerTris; k < (j + 1) * numPlayerTris;
                ++k)
            {
                playerTris3D[k].position += newPos;
                playerTris3D[k].color = overrideColor;
            }
        }
    }

    render(wallQuads3D, getRenderStates(RenderStage::WallQuads3D));
    render(pivotQuads3D, getRenderStates(RenderStage::PivotQuads3D));
    render(playerTris3D, getRenderStates(RenderStage::PlayerTris3D));

    if(Config::getShowPlayerTrail() && status.showPlayerTrail)
    {
        drawTrailParticles();
    }

    if(Config::getShowSwapParticles())
    {
        drawSwapParticles();
    }

    render(wallQuads, getRenderStates(RenderStage::WallQuads));
    render(capTris, getRenderStates(RenderStage::CapTris));
    render(pivotQuads, getRenderStates(RenderStage::PivotQuads));
    render(playerTris, getRenderStates(RenderStage::PlayerTris));

    window->setView(overlayCamera->apply());

    drawParticles();
    drawText(getRenderStates(RenderStage::Text));

    // ------------------------------------------------------------------------
    // Draw key icons.
    if(Config::getShowKeyIcons() || mustShowReplayUI())
    {
        drawKeyIcons();
    }

    // ------------------------------------------------------------------------
    // Draw level info.
    if(Config::getShowLevelInfo() || mustShowReplayUI())
    {
        drawLevelInfo(getRenderStates(RenderStage::Text));
    }

    // ------------------------------------------------------------------------
    if(Config::getFlash())
    {
        render(flashPolygon);
    }

    if(mustTakeScreenshot)
    {
        if(window != nullptr)
        {
            window->saveScreenshot("screenshot.png");
        }

        mustTakeScreenshot = false;
    }

    drawImguiLuaConsole();
}
```

**Exact frame order (draw() pipeline):**

1. Clear window to `sf::Color::Black`.
2. Set camera centers (only when `!status.hasDied`): shake-based or
   `(0,0)` background / `(width/2, height/2)` overlay.
3. Background (if `!Config::getNoBackground()`): view = backgroundCamera;
   `styleData.drawBackground` → render `backgroundTris` with
   `RenderStage::BackgroundTris` states.
4. View = backgroundCamera. Rebuild `wallQuads` (each `CWall::draw` +
   `cwManager.draw`), `pivotQuads`/`capTris`/`playerTris` (if
   `status.started`, via `player.draw(...)`), then build the 3D copies
   (`wallQuads3D`, `pivotQuads3D`, `playerTris3D`) if `Config::get3D()`.
5. Render `wallQuads3D` (WallQuads3D) → `pivotQuads3D` (PivotQuads3D) →
   `playerTris3D` (PlayerTris3D).
6. **Still in backgroundCamera view**: `drawTrailParticles()` (if
   `Config::getShowPlayerTrail() && status.showPlayerTrail`), then
   `drawSwapParticles()` (if `Config::getShowSwapParticles()`).
7. Render `wallQuads` (WallQuads) → `capTris` (CapTris) → `pivotQuads`
   (PivotQuads) → `playerTris` (PlayerTris).
8. Switch to overlayCamera. `drawParticles()` (sprites), `drawText`
   (Text stage states).
9. Key icons (if `Config::getShowKeyIcons() || mustShowReplayUI()`).
10. Level info (if `Config::getShowLevelInfo() || mustShowReplayUI()`), same
    Text render states.
11. Flash polygon (if `Config::getFlash()`), default render states.
12. Screenshot (`"screenshot.png"` via `window->saveScreenshot`) if
    `mustTakeScreenshot`, then clear the flag.
13. `drawImguiLuaConsole()` (ImGui, in the window's default view — see 3.14).

**3D math notes (verbatim in code above):**

- `depth = styleData._3dDepth` (float, default 15; loop runs `i < depth` so
  the float truncates to an int count of layers).
- `effect = styleData._3dSkew * Config::get3DMultiplier() * pulse3D` where
  `pulse3D = Config::getNoPulse() ? 1.f : status.pulse3D`.
- `backgroundCamera->setSkew({1.f, 1.f + effect})` — vertical-only skew,
  applied through `Camera::apply()` (view size * skew).
- `radRot = toRad(backgroundCamera->getRotation()) + pi/2`; per-layer offset
  `offset = _3dSpacing * (float(i + 1) * _3dPerspectiveMult) * (effect * 3.6f) * 1.4f`
  with `i = depth - j - 1` (layer 0 = farthest back, largest offset);
  direction `(offset*cosRot, offset*sinRot)` — i.e. "up" in rotated camera
  space.
- Per-layer alpha: `newAlpha = (a / _3dAlphaMult) - i * _3dAlphaFalloff`,
  clamped to [0,255] (`Utils::componentClamp`); skipped entirely when
  `_3dAlphaMult == 0.f`.
- Override color: darkened `get3DOverrideColor()` (or white with main-color
  alpha in B&W mode) by `_3dDarkenMult`. If `get3DOverrideColor() ==
  getMainColor()` (i.e. no explicit override set, see StyleData::computeColors)
  the wall layers use darkened `getColorWall()` and the player layers darkened
  `getColorPlayer()` instead.

### 3.4 `drawImguiLuaConsole`

```cpp
void HexagonGame::drawImguiLuaConsole()
{
    if(window == nullptr)
    {
        return;
    }

    if(!ilcShowConsole)
    {
        return;
    }

    SSVOH_ASSERT(overlayCamera.has_value());

    sf::RenderWindow& renderWindow = window->getRenderWindow();
    window->setView(renderWindow.getDefaultView());

    Imgui::render(*window);
}
```

(Note: asserts `overlayCamera` exists but renders in the *default* window
view, not the overlay camera.)

### 3.5 `initFlashEffect(r, g, b)`

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

Flash color alpha is animated by `updateFlash` (§5.9): all 6 triangle-vertex
colors' alpha set to `status.flashEffect` each frame. Called from Lua
`u_setFlashColor(r,g,b)`; initial call with the level's flash color happens in
`HexagonGame::start()` (gameplay.md).

### 3.6 `drawKeyIcons`

```cpp
void HexagonGame::drawKeyIcons()
{
    constexpr std::uint8_t offOpacity = 90;
    constexpr std::uint8_t onOpacity = 255;

    const sf::Color colorText = getColorText();

    const sf::Color offColor{colorText.r, colorText.g, colorText.b, offOpacity};
    const sf::Color onColor{colorText.r, colorText.g, colorText.b, onOpacity};

    keyIconLeft.setColor((getInputMovement() == -1) ? onColor : offColor);
    keyIconRight.setColor((getInputMovement() == 1) ? onColor : offColor);
    keyIconFocus.setColor(getInputFocused() ? onColor : offColor);
    keyIconSwap.setColor(getInputSwap() ? onColor : offColor);

    render(keyIconLeft);
    render(keyIconRight);
    render(keyIconFocus);
    render(keyIconSwap);

    // ------------------------------------------------------------------------

    if(mustShowReplayUI())
    {
        replayIcon.setColor(onColor);
        render(replayIcon);
    }
}
```

(sprite setup/positioning in `updateKeyIcons`, §4.2; textures:
`keyArrow.png` ×2, `keyFocus.png`, `keySwap.png`, `replayIcon.png`.)

### 3.7 `drawLevelInfo`

```cpp
void HexagonGame::drawLevelInfo(const sf::RenderStates& mStates)
{
    render(levelInfoRectangle, mStates);
    render(levelInfoTextLevel, mStates);
    render(levelInfoTextPack, mStates);
    render(levelInfoTextAuthor, mStates);
    render(levelInfoTextBy, mStates);
    render(levelInfoTextDM, mStates);
}
```

(text/rect setup in `updateLevelInfo`, §4.3.)

### 3.8 `drawParticles` / `drawTrailParticles` / `drawSwapParticles`

```cpp
void HexagonGame::drawParticles()
{
    for(Particle& p : particles)
    {
        render(p.sprite);
    }
}

void HexagonGame::drawTrailParticles()
{
    for(TrailParticle& p : trailParticles)
    {
        render(p.sprite);
    }
}

void HexagonGame::drawSwapParticles()
{
    for(SwapParticle& p : swapParticles)
    {
        render(p.sprite);
    }
}
```

### 3.9 `updateText` (called every frame from `update()`, after the big
sim block; even while `debugPause`)

```cpp
void HexagonGame::updateText(ssvu::FT mFT)
{
    if(window == nullptr)
    {
        return;
    }

    // ------------------------------------------------------------------------
    // Update "personal best" text animation.
    pbTextGrowth += 0.08f * mFT;
    if(pbTextGrowth > ssvu::pi * 2.f)
    {
        pbTextGrowth = 0;
    }

    // ------------------------------------------------------------------------
    os.str("");

    if(debugPause)
    {
        os << "(!) PAUSED (!)\n";
    }

    if(levelStatus.tutorialMode)
    {
        os << "TUTORIAL MODE\n";
    }
    else if(Config::getOfficial())
    {
        os << "OFFICIAL MODE\n";
    }

    if(Config::getDebug())
    {
        os << "DEBUG MODE\n";

        os << "CUSTOM WALLS: " << cwManager.count() << " / "
           << cwManager.maxHandles() << '\n';
    }

    if(status.started)
    {
        if(levelStatus.swapEnabled)
        {
            os << "SWAP ENABLED\n";
        }

        if(Config::getInvincible())
        {
            os << "INVINCIBILITY ON\n";
        }

        if(const float timescale = Config::getTimescale(); timescale != 1.f)
        {
            os << "TIMESCALE " << timescale << '\n';
        }

        if(status.scoreInvalid)
        {
            os << "SCORE INVALIDATED (" << status.invalidReason << ")\n";
        }

        if(status.hasDied)
        {
            os << status.restartInput;
            os << status.replayInput;
        }

        if(calledDeprecatedFunctions.size() > 1)
        {
            os << calledDeprecatedFunctions.size()
               << " WARNINGS RAISED (CHECK CONSOLE)\n";
        }
        else if(calledDeprecatedFunctions.size() > 0)
        {
            os << "1 WARNING RAISED (CHECK CONSOLE)\n";
        }

        const auto& trackedVariables(levelStatus.trackedVariables);
        if(Config::getShowTrackedVariables() && !trackedVariables.empty())
        {
            os << '\n';
            for(const auto& [variableName, display] : trackedVariables)
            {
                if(!lua.doesVariableExist(variableName))
                {
                    continue;
                }

                const std::string value{
                    lua.readVariable<std::string>(variableName)};

                os << Utils::toUppercase(display) << ": "
                   << Utils::toUppercase(value) << '\n';
            }
        }
    }
    else if(Config::getRotateToStart())
    {
        os << "ROTATE TO START\n";
        messageText.setString("ROTATE TO START");
    }

    os.flush();

    // Set in game timer text
    if(!levelStatus.scoreOverridden)
    {
        // By default, use the timer for scoring
        if(status.started)
        {
            timeText.setString(formatTime(status.getTimeSeconds()));
        }
        else
        {
            timeText.setString("0");
        }
    }
    else
    {
        // Alternative scoring
        timeText.setString(
            lua.readVariable<std::string>(levelStatus.scoreOverride));
    }

    const auto getScaledCharacterSize = [&](const float size)
    {
        return ssvu::toNum<unsigned int>(
            size / Config::getZoomFactor() * Config::getTextScaling());
    };

    timeText.setCharacterSize(getScaledCharacterSize(70.f));

    // Set information text
    text.setString(os.str());
    text.setCharacterSize(getScaledCharacterSize(20.f));
    text.setOrigin({0.f, 0.f});

    // Set FPS Text, if option is enabled.
    if(Config::getShowFPS())
    {
        fpsText.setString(ssvu::toStr(window->getFPS()));
        fpsText.setCharacterSize(getScaledCharacterSize(20.f));
    }

    messageText.setCharacterSize(getScaledCharacterSize(32.f));
    messageText.setOrigin({ssvs::getGlobalWidth(messageText) / 2.f, 0.f});

    const float growth = std::sin(pbTextGrowth);
    pbText.setCharacterSize(getScaledCharacterSize(64.f) + growth * 10.f);
    pbText.setOrigin({ssvs::getGlobalWidth(pbText) / 2.f, 0.f});

    // ------------------------------------------------------------------------
    if(mustShowReplayUI())
    {
        const replay_file& rf = activeReplay->replayFile;

        os.str("");

        if(!levelStatus.scoreOverridden)
        {
            os << formatTime(rf.played_seconds()) << "s";
        }
        else
        {
            os << formatTime(rf._played_score);
        }

        os << " BY " << rf._player_name;

        os.flush();

        replayText.setCharacterSize(getScaledCharacterSize(16.f));
        replayText.setString(os.str());
    }
    else
    {
        replayText.setString("");
    }
}
```

**Font/text sizing math (exact):**

- `getScaledCharacterSize(size) = unsigned(size / Config::getZoomFactor() * Config::getTextScaling())` —
  `ssvu::toNum<unsigned int>` is a static_cast (truncation).
- Base sizes: timeText `70.f` (bold), text `20.f`, fpsText `20.f`,
  messageText `32.f`, pbText `64.f + 10.f * sin(pbTextGrowth)` (bold),
  replayText `16.f`.
- `pbTextGrowth` advances by `0.08f * mFT` per frame, wraps when `> 2*pi`
  (set to 0, not modulo — one extra frame of sawtooth).
- Initial `sf::Text` char sizes (constructor `initText`, before any
  scaling): messageText 38 (regular), pbText 65 (bold), fpsText 25 "0"
  (regular), timeText 70 "0" (bold), text 25 "" (regular), replayText 20
  "" (regular). Fonts: `OpenSquare-Regular.ttf`, `OpenSquare-Bold.ttf`.

### 3.10 `drawText_TimeAndStatus`

```cpp
void HexagonGame::drawText_TimeAndStatus(
    const sf::Color& offsetColor, const sf::RenderStates& mStates)
{
    if(Config::getDrawTextOutlines())
    {
        timeText.setOutlineColor(offsetColor);
        text.setOutlineColor(offsetColor);
        fpsText.setOutlineColor(offsetColor);
        replayText.setOutlineColor(offsetColor);

        timeText.setOutlineThickness(2.f);
        text.setOutlineThickness(1.f);
        fpsText.setOutlineThickness(1.f);
        replayText.setOutlineThickness(1.f);
    }
    else
    {
        timeText.setOutlineThickness(0.f);
        text.setOutlineThickness(0.f);
        fpsText.setOutlineThickness(0.f);
        replayText.setOutlineThickness(0.f);
    }

    const float padding =
        (Config::getTextPadding() * Config::getTextScaling()) /
        Config::getZoomFactor();

    const sf::Color colorText = getColorText();

    if(Config::getShowTimer())
    {
        timeText.setFillColor(colorText);
        timeText.setOrigin(ssvs::getLocalNW(timeText));
        timeText.setPosition({padding, padding});

        render(timeText, mStates);
    }

    if(Config::getShowStatusText())
    {
        text.setFillColor(colorText);
        text.setOrigin(ssvs::getLocalNW(text));
        text.setPosition({padding, ssvs::getGlobalBottom(timeText) + padding});

        render(text, mStates);
    }

    if(Config::getShowFPS())
    {
        fpsText.setFillColor(colorText);
        fpsText.setOrigin(ssvs::getLocalSW(fpsText));

        if(Config::getShowLevelInfo() || mustShowReplayUI())
        {
            fpsText.setPosition(
                {padding, ssvs::getGlobalTop(levelInfoRectangle) - padding});
        }
        else
        {
            fpsText.setPosition({padding, Config::getHeight() - padding});
        }

        render(fpsText, mStates);
    }

    if(mustShowReplayUI())
    {
        const float scaling =
            Config::getKeyIconsScale() / Config::getZoomFactor();

        const float replayPadding = 8.f * scaling;

        replayText.setFillColor(colorText);
        replayText.setOrigin(ssvs::getLocalCenterE(replayText));
        replayText.setPosition(ssvs::getGlobalCenterW(replayIcon) -
                               sf::Vector2f{replayPadding, 0});
        render(replayText, mStates);
    }
}
```

Layout summary: timer top-left at `(padding, padding)` with local-NW origin;
status text directly below the timer's global bottom, same x, local-NW origin;
FPS bottom-left with local-SW origin at y = `height - padding` (or
`globalTop(levelInfoRectangle) - padding` when level info / replay UI shown);
replay text anchored west-center of the replay icon, shifted left by
`8.f * keyIconsScale / zoomFactor`.
`padding = textPadding * textScaling / zoomFactor`.

### 3.11 `drawTextMessagePBImpl` / `drawText_Message` / `drawText_PersonalBest`

```cpp
template <typename FRender>
static void drawTextMessagePBImpl(sf::Text& text, const sf::Color& offsetColor,
    const sf::Vector2f& pos, const sf::Color& color, float outlineThickness,
    FRender&& fRender)
{
    if(text.getString().isEmpty())
    {
        return;
    }

    if(Config::getDrawTextOutlines())
    {
        text.setOutlineColor(offsetColor);
        text.setOutlineThickness(outlineThickness);
    }
    else
    {
        text.setOutlineThickness(0.f);
    }

    text.setPosition(pos);
    text.setFillColor(color);

    fRender(text);
}

void HexagonGame::drawText_Message(
    const sf::Color& offsetColor, const sf::RenderStates& mStates)
{
    drawTextMessagePBImpl(messageText, offsetColor,
        {Config::getWidth() / 2.f, Config::getHeight() / 5.5f}, getColorText(),
        1.f /* outlineThickness */,
        [this, &mStates](sf::Text& t) { render(t, mStates); });
}

void HexagonGame::drawText_PersonalBest(
    const sf::Color& offsetColor, const sf::RenderStates& mStates)
{
    drawTextMessagePBImpl(pbText, offsetColor,
        {Config::getWidth() / 2.f,
            Config::getHeight() - Config::getHeight() / 4.f},
        getColorText(), 4.f /* outlineThickness */,
        [this, &mStates](sf::Text& t) { render(t, mStates); });
}
```

- Message position: `(width/2, height/5.5)`, origin =
  `(globalWidth/2, 0)` (set in updateText) → horizontally centered, 1/5.5 of
  the way down. Outline thickness 1.f.
- PB position: `(width/2, height - height/4)` = `(width/2, 3*height/4)`,
  same horizontal-center origin. Outline thickness 4.f.
- Both skip drawing when the string is empty.

### 3.12 `drawText`

```cpp
void HexagonGame::drawText(const sf::RenderStates& mStates)
{
    const sf::Color offsetColor{
        Config::getBlackAndWhite() || styleData.getColors().empty()
            ? sf::Color::Black
            : getColor(0)};

    drawText_TimeAndStatus(offsetColor, mStates);
    drawText_Message(offsetColor, mStates);
    drawText_PersonalBest(offsetColor, mStates);
}
```

### 3.13 Member layout (from HexagonGame.hpp, graphics-related)

```cpp
    std::optional<ssvs::Camera> backgroundCamera;
    std::optional<ssvs::Camera> overlayCamera;

    struct PreShakeCenters
    {
        sf::Vector2f background;
        sf::Vector2f overlay;
    };
    std::optional<PreShakeCenters> preShakeCenters;

    sf::Text messageText;
    sf::Text pbText;
    Utils::FastVertexVectorTris flashPolygon;

    struct Particle   { sf::Sprite sprite; sf::Vector2f velocity; float angularVelocity; };
    struct TrailParticle { sf::Sprite sprite; float angle; };
    struct SwapParticle { sf::Sprite sprite; sf::Vector2f velocity; };

    sf::Texture* txStarParticle;   // "starParticle.png"
    sf::Texture* txSmallCircle;    // "smallCircle.png"
    std::vector<Particle> particles;
    std::vector<TrailParticle> trailParticles;
    std::vector<SwapParticle> swapParticles;
    bool mustSpawnPBParticles{false};
    float nextPBParticleSpawn{0.f};
    float pbTextGrowth{0.f};

    sf::Sprite keyIconLeft;    // keyArrow.png, rotated 180°
    sf::Sprite keyIconRight;   // keyArrow.png
    sf::Sprite keyIconFocus;   // keyFocus.png
    sf::Sprite keyIconSwap;    // keySwap.png
    sf::Sprite replayIcon;     // replayIcon.png

    sf::RectangleShape levelInfoRectangle;
    sf::Text levelInfoTextLevel;
    sf::Text levelInfoTextPack;
    sf::Text levelInfoTextAuthor;
    sf::Text levelInfoTextBy;
    sf::Text levelInfoTextDM;

    sf::Text fpsText;
    sf::Text timeText;
    sf::Text text;
    sf::Text replayText;

    Utils::FastVertexVectorTris backgroundTris;
    Utils::FastVertexVectorTris wallQuads;
    Utils::FastVertexVectorTris pivotQuads;
    Utils::FastVertexVectorTris playerTris;
    Utils::FastVertexVectorTris capTris;
    Utils::FastVertexVectorTris wallQuads3D;
    Utils::FastVertexVectorTris pivotQuads3D;
    Utils::FastVertexVectorTris playerTris3D;
```

## 4. `HexagonGame.cpp` — camera setup, colors, key icons, level info

### 4.1 Constructor: camera + text setup (relevant parts)

```cpp
      font{assets.getFontOrNullFont("OpenSquare-Regular.ttf")},
      fontBold{assets.getFontOrNullFont("OpenSquare-Bold.ttf")},
      ...
      messageText{initText("", font, 38.f)},
      pbText{initText("", fontBold, 65.f)},
      ...
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

    game.onUpdate +=
        [this](ssvu::FT mFT) { update(mFT, Config::getTimescale()); };

    game.onPostUpdate += [this] { postUpdate(); };

    game.onDraw += [this] { draw(); };

    game.onAnyEvent += Imgui::processEvent;

    if(window != nullptr)
    {
        window->onRecreation += [this] { initKeyIcons(); };
    }
```

**Camera initial state (exact):**

- `backgroundCamera`: center `(0, 0)`, size `(width * zoomFactor,
  height * zoomFactor)` — i.e. the world units visible equal the window
  pixels divided by zoomFactor (zoomed-in by `zoomFactor`). Rotation 0, no
  skew/offset.
- `overlayCamera`: center `(width/2, height/2)`, size `(width, height)` —
  1:1 pixel coordinates.
- `Config::getZoomFactor()` defaults to 1 (see 04-config.md); typical window
  is 800×450 (see Config defaults in 04-config.md).

`initText` (file-static, used above):

```cpp
[[nodiscard]] static sf::Text initText(
    const char* text, const sf::Font& font, const float characterSize)
{
    return sf::Text{text, font,
        ssvu::toNum<unsigned int>(characterSize / Config::getZoomFactor())};
}
```

Input registration relevant to rendering/console (the `notInConsole`
wrapper defers any action while ImGui has keyboard/mouse capture):

```cpp
    const auto notInConsole = [this](auto&& f)
    {
        return [this, f](ssvu::FT /*unused*/)
        {
            if(!imguiLuaConsoleHasInput())
            {
                f();
            }
        };
    };

    game.addInput({{sf::Keyboard::Key::Escape}},
        notInConsole([this] { goToMenu(); }), // hardcoded
        ssvs::Input::Type::Always);
```

(Additions via `notInConsole`: `Tid::Exit` → `goToMenu` (Always),
`Tid::ForceRestart`, `Tid::Restart`, `Tid::Replay`, `Tid::Screenshot` →
`mustTakeScreenshot = true`; `Tid::LuaConsole` → `ilcShowConsoleNext = true`
only if `Config::getDebug()`; `Tid::Pause` → toggles `debugPause` if debug,
with music pause/resume side effects. `imguiLuaConsoleHasInput()` =
`ilcShowConsole && (Imgui::wantCaptureKeyboard() || Imgui::wantCaptureMouse())`.)

### 4.2 `initKeyIcons` / `updateKeyIcons`

```cpp
void HexagonGame::initKeyIcons()
{
    if(window == nullptr)
    {
        return;
    }

    for(const auto& t :
        {"keyArrow.png", "keyFocus.png", "keySwap.png", "replayIcon.png"})
    {
        assets.getTextureOrNullTexture(t).setSmooth(true);
    }

    keyIconLeft.setTexture(assets.getTextureOrNullTexture("keyArrow.png"));
    keyIconRight.setTexture(assets.getTextureOrNullTexture("keyArrow.png"));
    keyIconFocus.setTexture(assets.getTextureOrNullTexture("keyFocus.png"));
    keyIconSwap.setTexture(assets.getTextureOrNullTexture("keySwap.png"));
    replayIcon.setTexture(assets.getTextureOrNullTexture("replayIcon.png"));

    updateKeyIcons();
}

void HexagonGame::updateKeyIcons()
{
    if(window == nullptr)
    {
        return;
    }

    constexpr float halfSize = 32.f;
    constexpr float size = halfSize * 2.f;

    keyIconLeft.setOrigin({halfSize, halfSize});
    keyIconRight.setOrigin({halfSize, halfSize});
    keyIconFocus.setOrigin({halfSize, halfSize});
    keyIconSwap.setOrigin({halfSize, halfSize});

    keyIconLeft.setRotation(sf::degrees(180));

    const float scaling = Config::getKeyIconsScale() / Config::getZoomFactor();

    keyIconLeft.setScale({scaling, scaling});
    keyIconRight.setScale({scaling, scaling});
    keyIconFocus.setScale({scaling, scaling});
    keyIconSwap.setScale({scaling, scaling});

    const float scaledHalfSize = halfSize * scaling;
    const float scaledSize = size * scaling;
    const float padding = 8.f * scaling;
    const float finalPadding = scaledSize + padding;
    const sf::Vector2f finalPaddingX{finalPadding, 0.f};

    const sf::Vector2f bottomRight{
        Config::getWidth() - padding - scaledHalfSize,
        Config::getHeight() - padding - scaledHalfSize};

    keyIconSwap.setPosition(bottomRight);
    keyIconFocus.setPosition(keyIconSwap.getPosition() - finalPaddingX);
    keyIconRight.setPosition(keyIconFocus.getPosition() - finalPaddingX);
    keyIconLeft.setPosition(keyIconRight.getPosition() - finalPaddingX);

    // ------------------------------------------------------------------------

    replayIcon.setOrigin({size, size});
    replayIcon.setScale({scaling / 2.f, scaling / 2.f});

    const sf::Vector2f topRight{Config::getWidth() - padding - scaledHalfSize,
        padding + scaledHalfSize};

    replayIcon.setPosition(topRight);
}
```

Layout: four 64×64 px (halfSize 32, origin = center) icons in a row along the
bottom-right; swap icon's center at
`(width - padding - scaledHalf, height - padding - scaledHalf)`; each icon to
the left is shifted left by `scaledSize + padding` (64·s + 8·s = 72·s px).
Left arrow icon rotated 180°. Replay icon: 64×64 texture, origin `(64,64)`
(bottom-right of texture), scale `s/2`, center at top-right
`(width - padding - scaledHalf, padding + scaledHalf)`.
`s = KeyIconsScale / ZoomFactor`. `updateKeyIcons` is called every frame from
`update()` when key icons are shown (§5.1).

### 4.3 `updateLevelInfo`

```cpp
void HexagonGame::updateLevelInfo()
{
    if(window == nullptr)
    {
        return;
    }

    const float levelInfoScaling = 1.f;
    const float scaling = levelInfoScaling / Config::getZoomFactor();
    const float padding = 8.f * scaling;

    const sf::Vector2f size{325.f, 75.f};
    const sf::Vector2f halfSize{size / 2.f};
    const sf::Vector2f scaledHalfSize{halfSize * scaling};

    levelInfoRectangle.setSize(size);
    levelInfoRectangle.setScale({scaling, scaling});

    const sf::Color offsetColor{
        Config::getBlackAndWhite() || styleData.getColors().empty()
            ? sf::Color::Black
            : styleData.getColor(0)};

    levelInfoRectangle.setFillColor(offsetColor);
    levelInfoRectangle.setOutlineColor(styleData.getMainColor());
    levelInfoRectangle.setOrigin(halfSize);
    levelInfoRectangle.setOutlineThickness(3.f);

    const sf::Vector2f bottomLeft{padding + scaledHalfSize.x,
        Config::getHeight() - padding - scaledHalfSize.y};

    levelInfoRectangle.setPosition(bottomLeft);

    const float tPadding = padding;

    const auto trim = [](std::string s)
    {
        if(s.size() > 26)
        {
            return s.substr(0, 26);
        }

        return s;
    };

    levelInfoTextLevel.setFillColor(getColorText());
    levelInfoTextLevel.setCharacterSize(20.f / Config::getZoomFactor());
    levelInfoTextLevel.setString(trim(Utils::toUppercase(levelData->name)));
    levelInfoTextLevel.setOrigin(ssvs::getLocalNW(levelInfoTextLevel));
    levelInfoTextLevel.setPosition(ssvs::getGlobalNW(levelInfoRectangle) +
                                   sf::Vector2f{tPadding, tPadding});

    const auto prepareText = [&](sf::Text& text, const float characterSize,
                                 const std::string& string)
    {
        text.setFillColor(getColorText());
        text.setCharacterSize(characterSize / Config::getZoomFactor());
        text.setString(string);
    };

    prepareText(
        levelInfoTextPack, 14.f, trim(Utils::toUppercase(getPackName())));
    levelInfoTextPack.setOrigin(ssvs::getLocalNW(levelInfoTextPack));
    levelInfoTextPack.setPosition(
        ssvs::getGlobalSW(levelInfoTextLevel) + sf::Vector2f{0.f, tPadding});

    SSVOH_ASSERT(levelData != nullptr);

    prepareText(
        levelInfoTextAuthor, 20.f, trim(Utils::toUppercase(levelData->author)));
    levelInfoTextAuthor.setOrigin(ssvs::getLocalSE(levelInfoTextAuthor));
    levelInfoTextAuthor.setPosition(ssvs::getGlobalSE(levelInfoRectangle) -
                                    sf::Vector2f{tPadding, tPadding});

    prepareText(levelInfoTextBy, 12.f, "BY");
    levelInfoTextBy.setOrigin(ssvs::getLocalSE(levelInfoTextBy));
    levelInfoTextBy.setPosition(
        ssvs::getGlobalSW(levelInfoTextAuthor) - sf::Vector2f{tPadding, 0.f});

    if(levelData->difficultyMults.size() > 1)
    {
        prepareText(levelInfoTextDM, 14.f, diffFormat(difficultyMult) + "x");
        levelInfoTextDM.setOrigin(ssvs::getLocalSW(levelInfoTextDM));
        levelInfoTextDM.setPosition(ssvs::getGlobalSW(levelInfoRectangle) +
                                    sf::Vector2f{tPadding, -tPadding});
    }
    else
    {
        levelInfoTextDM.setString("");
    }
}
```

Layout constants: box `325×75` px (scale `1/zoomFactor`), origin at center
(halfSize), 3.f outline thickness in `getMainColor()`, fill =
`getColor(0)` (or Black in B&W / no colors). Box center positioned so its
bottom-left is `(padding + halfW·s, height - padding - halfH·s)` → box sits in
the bottom-left corner with 8·s px gap. Inner text padding 8·s px; char sizes
20/14/20/12/14 divided by zoomFactor; strings uppercased and trimmed to 26
chars; DM line only shown when `levelData->difficultyMults.size() > 1`, text
`diffFormat(difficultyMult) + "x"`. All text fill `getColorText()`.
Positions (in overlay-camera pixels): level text NW at box-NW + (8s,8s);
pack text NW at level-text-SW + (0,8s); author text SE at box-SE − (8s,8s);
"BY" SE at author-SW − (8s,0); DM text SW at box-SW + (8s,−8s).
Called every frame from `update()` when `Config::getShowLevelInfo() ||
mustShowReplayUI()`.

### 4.4 Formatting helpers

```cpp
void HexagonGame::nameFormat(std::string& name)
{
    name[0] = std::toupper(name[0]);
}

[[nodiscard]] std::string HexagonGame::diffFormat(float diff)
{
    char buf[255];
    std::snprintf(buf, sizeof(buf), "%g", diff);
    return buf;
}

[[nodiscard]] std::string HexagonGame::timeFormat(float time)
{
    char buf[255];
    std::snprintf(buf, sizeof(buf), "%.3f", time);
    return buf;
}
```

### 4.5 Color getters (exact, from HexagonGame.cpp)

```cpp
auto HexagonGame::getColorMain() const -> sf::Color
{
    if(Config::getBlackAndWhite())
    {
        return sf::Color(255, 255, 255, styleData.getMainColor().a);
    }

    return styleData.getMainColor();
}

auto HexagonGame::getColorPlayer() const -> sf::Color
{
    if(Config::getBlackAndWhite())
    {
        return sf::Color(255, 255, 255, styleData.getPlayerColor().a);
    }

    return styleData.getPlayerColor();
}

auto HexagonGame::getColorPlayerAdjustedForSwap() const -> sf::Color
{
    if(Config::getBlackAndWhite())
    {
        return sf::Color(255, 255, 255, styleData.getPlayerColor().a);
    }

    if(!Config::getShowSwapBlinkingEffect())
    {
        return getColorPlayer();
    }

    return player.getColorAdjustedForSwap(getColorPlayer());
}

auto HexagonGame::getColorPlayerTrail() const -> sf::Color
{
    return Config::getPlayerTrailHasSwapColor()
               ? getColorPlayerAdjustedForSwap()
               : getColorPlayer();
}

auto HexagonGame::getColorText() const -> sf::Color
{
    if(Config::getBlackAndWhite())
    {
        return sf::Color(255, 255, 255, styleData.getTextColor().a);
    }

    return styleData.getTextColor();
}

auto HexagonGame::getColorCap() const -> sf::Color
{
    if(Config::getBlackAndWhite())
    {
        return sf::Color::Black;
    }

    return styleData.getCapColorResult();
}

auto HexagonGame::getColorWall() const -> sf::Color
{
    if(Config::getBlackAndWhite())
    {
        return sf::Color(255, 255, 255, styleData.getWallColor().a);
    }

    return styleData.getWallColor();
}

[[nodiscard]] const sf::Color& HexagonGame::getColor(int mIdx) const noexcept
{
    return styleData.getColor(mIdx);
}
```

(`player.getColorAdjustedForSwap` is in CPlayer — gameplay.md §CPlayer: blends
the player color toward white based on swap cooldown; `getSwapCooldown()` =
`std::max(36.f * levelStatus.swapCooldownMult, 8.f)`.)

### 4.6 `death_shakeCamera` / `death_flashEffect` (camera/flash triggers)

```cpp
void HexagonGame::death_shakeCamera()
{
    if(!backgroundCamera.has_value() || !overlayCamera.has_value())
    {
        return;
    }

    if(!Config::getCameraShake())
    {
        return;
    }

    preShakeCenters = PreShakeCenters{
        backgroundCamera->getCenter(), overlayCamera->getCenter()};

    levelStatus.cameraShake = Config::getCameraShakeMagnitude();
    ...
}

void HexagonGame::death_flashEffect()
{
    if(!Config::getFlash())
    {
        return;
    }

    status.flashEffect = 255.f;
}
```

(Exact body, verified against `HexagonGame.cpp:822-853` during issue 11 — the
earlier note here was stale: `death_shakeCamera` early-returns when
`window == nullptr`, asserts both cameras, sets the overlay view to
`{w/2, h/2} / {w, h}`, `backgroundCamera->setCenter(0,0)`, and
`status.cameraShake = 45.f * Config::getCameraShakeMultiplier()` (the
**status** shake, not `levelStatus`; multiplier, not magnitude). There is no
`preShakeCenters` here — that is the `update_cameraShake` bookkeeping. The
`death_flashEffect` body is unconditional: `initFlashEffect(255,255,255)` +
`status.flashEffect = 255` (no `Config::getFlash()` gate). Both called from
`death()`.)

## 5. `HGUpdate.cpp` — render-driving updates

### 5.1 `update` — camera/particle/style call sites (structure)

`update(mFT, timescale)` (full text in gameplay.md; the render-relevant part,
inside `if(!debugPause)` after input/replay handling):

```cpp
        // --------------------------------------------------------------------
        // Update key icons.
        if(Config::getShowKeyIcons() || mustShowReplayUI())
        {
            updateKeyIcons();
        }

        // --------------------------------------------------------------------
        // Update level info.
        if(Config::getShowLevelInfo() || mustShowReplayUI())
        {
            updateLevelInfo();
        }

        // --------------------------------------------------------------------
        // Update input leniency time after death to avoid accidental restart.
        if(deathInputIgnore > 0.f)
        {
            deathInputIgnore -= mFT;
        }

        // --------------------------------------------------------------------
        if(status.started)
        {
            styleData.computeColors();

            player.update(getInputFocused(), getLevelStatus().swapEnabled, mFT);

            if(!status.hasDied)
            {
                ...(onInput Lua hook, player.updateInputMovement, swap logic —
                gameplay.md)...;

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
```

Notes:
- `styleData.update(mFT, pow(difficultyMult, 0.8f))` is skipped in B&W mode.
- `styleData.computeColors()` runs every frame while started (drives all
  current colors + background-color rotation, see §7).
- Camera `update()` calls (pan/zoom/turn easing) happen **after** the
  particle updates, in overlay→background order; they only do work when
  `invalid` (a pan/zoom/turn was queued).
- `updateText(mFT)` runs unconditionally at the end (also during debugPause).
- `levelStatus.rotationSpeed *= 0.99f` while dead (spin-down).

### 5.2 `updatePulse` / `refreshPulse` (camera zoom driver)

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
```

`refreshPulse` **replaces the whole background view** (center 0,0; size
`width*zoomFactor*p` × `height*zoomFactor*p`; rotation preserved) each frame
— this is the breathing zoom. `p = pulse/pulseMin` (1 when NoPulse).
`getMusicDMSyncFactor() = pow(difficultyMult, 0.12f)`.

### 5.3 `updateBeatPulse` / `refreshBeatPulse` (radius driver)

```cpp
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
```

### 5.4 `updateRotation` (camera turn driver)

```cpp
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

(`turn` queues `nextRotation += deg`; applied in `Camera::update` as
`view.rotate(degrees(nextRotation * mFT))`.)

### 5.5 `updateCameraShake`

```cpp
void HexagonGame::updateCameraShake(ssvu::FT mFT)
{
    if(!backgroundCamera.has_value() || !overlayCamera.has_value())
    {
        return;
    }

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

    SSVOH_ASSERT(backgroundCamera.has_value());
    SSVOH_ASSERT(overlayCamera.has_value());
    SSVOH_ASSERT(preShakeCenters.has_value());

    const auto makeShakeVec = [this]
    {
        const float i = status.cameraShake;
        return sf::Vector2f(rng.get_real(-i, i), rng.get_real(-i, i));
    };

    backgroundCamera->setCenter(preShakeCenters->background + makeShakeVec());
    overlayCamera->setCenter(preShakeCenters->overlay + makeShakeVec());
}
```

(Note: `status.cameraShake` here is a duration in frames·FT — distinct from
`levelStatus.cameraShake` magnitude used in `draw()`; shake vectors come from
the **game RNG** `rng.get_real` — replay-relevant. `draw()` uses independent
`ssvu::getRndR` jitter while `levelStatus.cameraShake > 0`.)

### 5.6 `updateFlash`

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
```

(Decay 3·FT per frame; alpha written into all 6 vertices each frame. Note it
is called even when `Config::getFlash()` is false — decay + vertex write
still happen; rendering is gated by `getFlash()` in draw().)

### 5.7 `updatePulse3D`

```cpp
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

### 5.8 `updateParticles` (PB star particles, overlay)

```cpp
void HexagonGame::updateParticles(ssvu::FT mFT)
{
    SSVOH_ASSERT(window != nullptr);

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

        SSVOH_ASSERT(txStarParticle != nullptr);
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

(Spawn: `mustSpawnPBParticles` set + `nextPBParticleSpawn` by
`death_saveScoreIfNeededAndShowPBEffects` — see gameplay.md. Uses
`ssvu::getRndR`/`getRndI` — the *non-game* RNG, so PB particles are
non-deterministic w.r.t. replays.)

### 5.9 `updateTrailParticles`

```cpp
void HexagonGame::updateTrailParticles(ssvu::FT mFT)
{
    SSVOH_ASSERT(window != nullptr);

    const auto isDead = [&](const TrailParticle& p)
    { return p.sprite.getColor().a <= 3; };

    const auto makeTrailParticle = [this]
    {
        TrailParticle p;

        SSVOH_ASSERT(txSmallCircle != nullptr);
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

Notes: trail particles are positioned in **background/world coordinates**
(`getVecFromRad(angle, radius + 2.4f)` — just outside the cap) even though
they're drawn in the backgroundCamera view before the 2D walls. Alpha decays
by `PlayerTrailDecay * mFT` toward 0; dead at `a <= 3`. Scale shrinks ×0.98
per frame. Spawn only when the player's side angle changed.

### 5.10 `updateSwapParticles`

```cpp
void HexagonGame::updateSwapParticles(ssvu::FT mFT)
{
    SSVOH_ASSERT(window != nullptr);

    const auto isDead = [&](const SwapParticle& p)
    { return p.sprite.getColor().a <= 3; };

    const auto makeSwapParticle = [this](const SwapParticleSpawnInfo& si,
                                      const float expand, const float speedMult,
                                      const float scaleMult, const float alpha)
    {
        SwapParticle p;

        SSVOH_ASSERT(txSmallCircle != nullptr);
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

Bursts: "just ready" (ready==true, blip) → 14 particles, expand 3.14 (rad),
speed 0.1..10 × 1.3, scale rnd 0.65..1.35 × 0.4, alpha 140. "after swap"
(ready==false) → 20 particles (expand 0.45, speed ×1.0, scale ×1.0, alpha
45) + 10 particles (expand 3.14, speed ×0.45, scale ×0.75, alpha 35).
Decay: alpha −3.5·mFT → 0, dead at ≤3; scale ×0.98/frame; move along
velocity·mFT.

### 5.11 `ilcTextEditCallback` (ImGui text-callback: tab completion + history)

File-static helpers (used by the callback):

```cpp
static int ilcTextEditCallbackStub(ImGuiInputTextCallbackData* data)
{
    auto hg = (HexagonGame*)data->UserData;
    return hg->ilcTextEditCallback(data);
}

static int Stricmp(const char* s1, const char* s2)
{
    int d;
    while((d = toupper(*s2) - toupper(*s1)) == 0 && *s1)
    {
        s1++;
        s2++;
    }

    return d;
}

static int Strnicmp(const char* s1, const char* s2, int n)
{
    int d = 0;
    while(n > 0 && (d = toupper(*s2) - toupper(*s1)) == 0 && *s1)
    {
        s1++;
        s2++;
        n--;
    }

    return d;
}
```

(`Stricmp/Strnicmp` return a signed char-difference — 0 means the prefix
matched case-insensitively.)

```cpp
int HexagonGame::ilcTextEditCallback(
    [[maybe_unused]] ImGuiInputTextCallbackData* data)
{
    switch(data->EventFlag)
    {
        case ImGuiInputTextFlags_CallbackCompletion:
        {
            // Locate beginning of current word
            const char* word_end = data->Buf + data->CursorPos;
            const char* word_start = word_end;
            while(word_start > data->Buf)
            {
                const char c = word_start[-1];
                if(c == ' ' || c == '\t' || c == ',' || c == ';')
                {
                    break;
                }

                word_start--;
            }

            // Skip starting `?` for Lua docs
            if(*word_start == '?')
            {
                ++word_start;
            }

            // Build a list of candidates
            ImVector<const char*> candidates;
            for(const std::string& fnName : LuaScripting::getAllFunctionNames())
            {
                if(Strnicmp(fnName.c_str(), word_start,
                       (int)(word_end - word_start)) == 0)
                {
                    candidates.push_back(fnName.c_str());
                }
            }

            if(candidates.Size == 0)
            {
                char buf[255];
                std::snprintf(buf, sizeof(buf), "No match for \"%.*s\"!\n",
                    (int)(word_end - word_start), word_start);

                // No match
                ilcCmdLog.emplace_back(buf);
            }
            else if(candidates.Size == 1)
            {
                // Single match. Delete the beginning of the word and
                // replace it entirely so we've got nice casing.
                data->DeleteChars((int)(word_start - data->Buf),
                    (int)(word_end - word_start));
                data->InsertChars(data->CursorPos, candidates[0]);
                data->InsertChars(data->CursorPos, " ");
            }
            else
            {
                // Multiple matches. Complete as much as we can..
                // So inputing "C"+Tab will complete to "CL" then display
                // "CLEAR" and "CLASSIFY" as matches.
                int match_len = (int)(word_end - word_start);
                for(;;)
                {
                    int c = 0;
                    bool all_candidates_matches = true;
                    for(int i = 0;
                        i < candidates.Size && all_candidates_matches; i++)
                        if(i == 0)
                            c = std::toupper(candidates[i][match_len]);
                        else if(c == 0 ||
                                c != std::toupper(candidates[i][match_len]))
                            all_candidates_matches = false;
                    if(!all_candidates_matches) break;
                    match_len++;
                }

                if(match_len > 0)
                {
                    data->DeleteChars((int)(word_start - data->Buf),
                        (int)(word_end - word_start));

                    data->InsertChars(data->CursorPos, candidates[0],
                        candidates[0] + match_len);
                }

                // List matches
                ilcCmdLog.emplace_back("Possible matches:\n");
                for(int i = 0; i < candidates.Size; i++)
                {
                    ilcCmdLog.emplace_back(
                        Utils::concat("- ", candidates[i], '\n'));
                }
            }

            break;
        }
        case ImGuiInputTextFlags_CallbackHistory:
        {
            const int prev_history_pos = ilcHistoryPos;
            if(data->EventKey == ImGuiKey_UpArrow)
            {
                if(ilcHistoryPos == -1)
                {
                    ilcHistoryPos = ilcHistory.size() - 1;
                }
                else if(ilcHistoryPos > 0)
                {
                    ilcHistoryPos--;
                }
            }
            else if(data->EventKey == ImGuiKey_DownArrow)
            {
                if(ilcHistoryPos != -1)
                {
                    if(++ilcHistoryPos >= static_cast<int>(ilcHistory.size()))
                    {
                        ilcHistoryPos = -1;
                    }
                }
            }

            if(prev_history_pos != ilcHistoryPos)
            {
                const char* history_str =
                    (ilcHistoryPos >= 0) ? ilcHistory[ilcHistoryPos].c_str()
                                         : "";

                data->DeleteChars(0, data->BufTextLen);
                data->InsertChars(0, history_str);
            }
        }
    }

    return 0;
}
```

Semantics: word = chars before cursor up to space/tab/`,`/`;` (leading `?`
ignored for matching but left in the buffer). Candidates = all Lua function
names whose case-insensitive prefix equals the word. 0 candidates → log
`No match for "<word>"!`; 1 → replace word with the correctly-cased name +
trailing space; >1 → replace with the common case-insensitive prefix longer
than the input (if any) and log `Possible matches:` + one `- <name>\n` per
candidate. Up/Down arrows walk `ilcHistory` (−1 = empty buffer; Up from −1
jumps to the last entry). All of this only in non-Android builds
(`#ifndef SSVOH_ANDROID`).

### 5.12 `postUpdate_ImguiLuaConsole` (what the console draws)

```cpp
void HexagonGame::postUpdate_ImguiLuaConsole()
{
    if(window == nullptr)
    {
        return;
    }

    if(!Config::getDebug() || Config::getOfficial())
    {
        ilcShowConsole = ilcShowConsoleNext = false;
        return;
    }

    if(ilcShowConsoleNext)
    {
        ilcShowConsole = !ilcShowConsole;
        ilcShowConsoleNext = false;

        ImGui::SFML::ProcessEvent(sf::Event{sf::Event::GainedFocus});
    }

    if(!ilcShowConsole)
    {
        return;
    }

    ImGui::SFML::Update(*window, ilcDeltaClock.restart());

    ImGui::SetNextWindowSize(ImVec2(600, 700), ImGuiCond_FirstUseEver);
    ImGui::Begin("Lua Console");

    ImGui::Text("Enter `!help` to show help.");
    ImGui::Separator();

    const float footer_height_to_reserve = ImGui::GetStyle().ItemSpacing.y +
                                           ImGui::GetFrameHeightWithSpacing() +
                                           150;

    ImGui::PushStyleVar(
        ImGuiStyleVar_ItemSpacing, ImVec2(4, 1)); // Tighten spacing

    ImGui::BeginChild("ScrollingRegion", ImVec2(0, -footer_height_to_reserve),
        false, ImGuiWindowFlags_HorizontalScrollbar);

    for(const std::string& sItem : ilcCmdLog)
    {
        const char* item = sItem.c_str();

        const auto color = [&]() -> std::optional<ImVec4>
        {
            if(std::strstr(item, "[error]"))
            {
                return ImVec4(1.0f, 0.4f, 0.4f, 1.0f);
            }

            if(std::strstr(item, "[warning]"))
            {
                return ImVec4(1.0f, 0.4f, 1.0f, 1.0f);
            }

            if(std::strstr(item, "[lua]"))
            {
                return ImVec4(0.4f, 1.0f, 0.4f, 1.0f);
            }

            if(std::strncmp(item, "# ", 2) == 0)
            {
                return ImVec4(1.0f, 0.8f, 0.6f, 1.0f);
            }

            if(std::strstr(item, "[?]"))
            {
                return ImVec4(0.4f, 0.4f, 1.0f, 1.0f);
            }

            return std::nullopt;
        }();

        if(color.has_value())
        {
            ImGui::PushStyleColor(ImGuiCol_Text, *color);
        }

        // Split the item on '\n'; each line longer than 80 chars is emitted
        // as ceil(size/80) 80-char chunks:
        std::vector<std::string> split;
        std::size_t last = 0;

        for(std::size_t j = 0; j < sItem.size(); ++j)
        {
            if(sItem[j] == '\n')
            {
                split.emplace_back(sItem.substr(last, j - last));
                last = j + 1;
            }
        }

        const std::string lastPiece = sItem.substr(last);
        if(!lastPiece.empty())
        {
            split.emplace_back(lastPiece);
        }

        for(const std::string& s : split)
        {
            constexpr std::size_t lineLimit = 80;
            if(s.size() <= lineLimit)
            {
                ImGui::TextUnformatted(s.c_str());
            }
            else
            {
                constexpr std::size_t charsPerSubstr = lineLimit;
                const std::size_t nSubstrs = s.size() / charsPerSubstr;

                for(std::size_t j = 0; j < nSubstrs + 1; ++j)
                {
                    ImGui::TextUnformatted(
                        s.substr(j * charsPerSubstr, charsPerSubstr).c_str());
                }
            }
        }

        if(color.has_value())
        {
            ImGui::PopStyleColor();
        }
    }

    ImGui::SetScrollHereY(1.0f);
    ImGui::PopStyleVar();
    ImGui::EndChild();
    ImGui::Separator();

    ImGuiInputTextFlags input_text_flags =
        ImGuiInputTextFlags_EnterReturnsTrue |
        ImGuiInputTextFlags_CallbackCompletion |
        ImGuiInputTextFlags_CallbackHistory;

    if(ImGui::InputText("Command", &ilcCmdBuffer, input_text_flags,
           &ilcTextEditCallbackStub, (void*)this))
    {
        const std::string cmdString = ilcCmdBuffer;
        ilcCmdBuffer.clear();

        ilcCmdLog.emplace_back(Utils::concat("# ", cmdString, '\n'));

        ilcHistoryPos = -1;
        for(int i = ilcHistory.size() - 1; i >= 0; --i)
        {
            if(ilcHistory[i] == cmdString)
            {
                ilcHistory.erase(ilcHistory.begin() + i);
                break;
            }
        }

        ilcHistory.emplace_back(cmdString);

        const std::vector<std::string> cmdSplit =
            Utils::split<std::string>(cmdString);

        if(Stricmp(cmdString.c_str(), "!CLEAR") == 0)
        {
            ilcCmdLog.clear();
        }
        else if(Stricmp(cmdString.c_str(), "!HELP") == 0)
        {
            ilcCmdLog.emplace_back(R"(Built-in commands:
!clear          Clears the console
!help           Display this help
!ff <seconds>   Fast-forward simulation to specified time
!advt <ticks>   Advance simulation by specified number of ticks
?fn             Display Lua docs for function `fn`
)");
        }
        else if(cmdSplit.size() > 1 && cmdSplit.at(0) == "!ff")
        {
            try
            {
                const std::string& secondsStr = cmdSplit.at(1);
                const double seconds = std::stod(secondsStr);

                ilcCmdLog.emplace_back(
                    Utils::concat("[ff]: fast forwarding to ", seconds, '\n'));

                fastForwardTarget = seconds;
            }
            catch(const std::invalid_argument&)
            {
                ilcCmdLog.emplace_back(
                    "[error]: invalid argument for <seconds>\n");
            }
            catch(const std::out_of_range&)
            {
                ilcCmdLog.emplace_back("[error]: out of range for <seconds>\n");
            }
        }
        else if(cmdSplit.size() > 1 && cmdSplit.at(0) == "!advt")
        {
            try
            {
                const std::string& ticksStr = cmdSplit.at(1);
                const int ticks = std::stoi(ticksStr);

                ilcCmdLog.emplace_back(Utils::concat(
                    "[advt]: advancing simulation by ", ticks, " ticks\n"));

                advanceTickCount = ticks >= 0 ? ticks : 0;
            }
            catch(const std::invalid_argument&)
            {
                ilcCmdLog.emplace_back(
                    "[error]: invalid argument for <seconds>\n");
            }
            catch(const std::out_of_range&)
            {
                ilcCmdLog.emplace_back("[error]: out of range for <seconds>\n");
            }
        }
        else if(cmdString[0] == '?')
        {
            const std::string rest = Utils::getRTrim(cmdString.substr(1));
            const std::string docs = LuaScripting::getDocsForFunction(rest);
            ilcCmdLog.emplace_back(Utils::concat("[?]: ", docs));
        }
        else
        {
            try
            {
                try
                {
                    lua.executeCode(Utils::concat("u_log(", cmdString, ")\n"));
                }
                catch(std::runtime_error& mError)
                {
                    lua.executeCode(cmdString + "\n");
                }
            }
            catch(std::runtime_error& mError)
            {
                std::string temp = "[error]: ";
                temp += mError.what();
                temp += '\n';

                ilcCmdLog.emplace_back(temp);
            }
            catch(...)
            {
                ilcCmdLog.emplace_back("[error]: unknown\n");
            }
        }

        ImGui::SetItemDefaultFocus();
        ImGui::SetKeyboardFocusHere(-1);
    }

    ImGui::Separator();

    ImGui::Text("update ms: %.2f", window->getMsUpdate());
    ImGui::SameLine();
    ImGui::Text("draw ms: %.2f", window->getMsDraw());

    static float simSpeed = Config::getTimescale();
    ImGui::DragFloat("Timescale", &simSpeed, 0.005f);
    Config::setTimescale(simSpeed);

    ImGui::SameLine();

    static bool invincible = Config::getInvincible();
    ImGui::Checkbox("Invincible", &invincible);
    Config::setInvincible(invincible);

    ImGui::Separator();

    {
        if(ImGui::InputText(
               "Track", &ilcTrackBuffer, ImGuiInputTextFlags_EnterReturnsTrue))
        {
            const std::string codeToTrack = Utils::getLRTrim(ilcTrackBuffer);
            ilcLuaTracked.emplace_back(
                Utils::concat("u_impl_addTrackedResult(", codeToTrack, ")\n"));
            ilcLuaTrackedNames.emplace_back(codeToTrack);

            ilcTrackBuffer.clear();
            ImGui::SetItemDefaultFocus();
            ImGui::SetKeyboardFocusHere(-1);
        }

        ImGui::SameLine();

        if(ImGui::Button("Untrack All"))
        {
            ilcLuaTracked.clear();
            ilcLuaTrackedNames.clear();
        }
    }

    if(ilcLuaTracked.size() > 0)
    {
        ilcLuaTrackedResults.clear();
        bool problem = false;

        for(std::size_t i = 0; i < ilcLuaTracked.size(); ++i)
        {
            const std::string& code = ilcLuaTracked[i];

            try
            {
                lua.executeCode(code);
            }
            catch(std::runtime_error& e)
            {
                ilcCmdLog.emplace_back(Utils::concat("[error]: error '",
                    e.what(), "' while tracking ", code, '\n'));

                ilcLuaTracked.erase(ilcLuaTracked.begin() + i);
                ilcLuaTrackedNames.erase(ilcLuaTrackedNames.begin() + i);
                problem = true;
                break;
            }
            catch(...)
            {
                ilcCmdLog.emplace_back(Utils::concat(
                    "[error]: unknown error while tracking ", code, '\n'));

                ilcLuaTracked.erase(ilcLuaTracked.begin() + i);
                ilcLuaTrackedNames.erase(ilcLuaTrackedNames.begin() + i);
                problem = true;
                break;
            }
        }

        if(!problem)
        {
            ImGui::Separator();
            SSVOH_ASSERT(ilcLuaTracked.size() == ilcLuaTrackedNames.size());
            SSVOH_ASSERT(ilcLuaTracked.size() == ilcLuaTrackedResults.size());

            if(ImGui::BeginTable("TrackedResults", 2))
            {
                for(std::size_t i = 0; i < ilcLuaTracked.size(); ++i)
                {
                    ImGui::TableNextRow();

                    ImGui::TableSetColumnIndex(0);
                    ImGui::TextUnformatted(ilcLuaTrackedNames[i].c_str());

                    ImGui::TableSetColumnIndex(1);
                    ImGui::TextUnformatted(ilcLuaTrackedResults[i].c_str());
                }

                ImGui::EndTable();
            }
        }
    }

    ImGui::End();
}

void HexagonGame::postUpdate()
{
    postUpdate_ImguiLuaConsole();
}
```

Console state (all `HexagonGame` members):

```cpp
    sf::Clock ilcDeltaClock;
    std::vector<std::string> ilcCmdLog;       // log lines
    std::vector<std::string> ilcHistory;      // command history
    int ilcHistoryPos{-1};                    // -1 = not browsing
    std::string ilcCmdBuffer;                 // command input
    std::string ilcTrackBuffer;               // tracked-var input
    bool ilcShowConsole{false};
    bool ilcShowConsoleNext{false};
    std::vector<std::string> ilcLuaTracked;         // "u_impl_addTrackedResult(<expr>)\n"
    std::vector<std::string> ilcLuaTrackedNames;    // raw expression
    std::vector<std::string> ilcLuaTrackedResults;  // result strings
    bool debugPause{false};
```

Key handling / behavior summary:

- Toggle: `Tid::LuaConsole` (F1 by default — see binds in 04-config.md) sets
  `ilcShowConsoleNext` when `Config::getDebug()`; next postUpdate flips
  `ilcShowConsole` and injects a synthetic `GainedFocus` ImGui event. Console
  is force-closed when `!Config::getDebug() || Config::getOfficial()`.
- Window: `"Lua Console"`, initial size `600×700` (`ImGuiCond_FirstUseEver`).
  Header text: ``Enter `!help` to show help.``.
- Log colors: `[error]` → (1,0.4,0.4,1); `[warning]` → (1,0.4,1,1); `[lua]`
  → (0.4,1,0.4,1); lines starting `"# "` → (1,0.8,0.6,1); `[?]` →
  (0.4,0.4,1,1). Item spacing tightened to (4,1) in the scroll region;
  region height = window height − footer, where footer =
  `ItemSpacing.y + GetFrameHeightWithSpacing() + 150`. Long lines wrapped in
  80-char chunks. Auto-scroll to bottom after every update.
- Commands (case-insensitive where noted):
  - `!clear` → clears `ilcCmdLog`.
  - `!help` → prints the built-in help (verbatim above).
  - `!ff <seconds>` → `fastForwardTarget = stod(seconds)`; consumed by
    `update()` next frame (see `fastForwardTo`, gameplay.md):
    `update(Config::TIME_STEP, 1.0f)` + `postUpdate()` until
    `status.hasDied || time >= target || 3 s wall-clock exceeded`.
  - `!advt <ticks>` → `advanceTickCount = max(stoi(ticks), 0)`; consumed by
    `update()`: run exactly N `(update, postUpdate)` pairs (debugPause
    temporarily cleared).
  - `?<fn>` → log `[?]: ` + Lua docs for function.
  - anything else → evaluate `u_log(<expr>)\n` in Lua; on runtime_error fall
    back to executing `<expr>\n` directly (so statements work too); errors
    logged as `[error]: <msg>\n` (`[error]: unknown\n` for non-runtime).
- Tracked variables: entering an expression into the `Track` input appends
  `u_impl_addTrackedResult(<expr>)\n` to `ilcLuaTracked` (that Lua fn,
  registered in `initLua_Utils`, appends the result string to
  `ilcLuaTrackedResults` — see HGScripting.cpp line ~57). Each postUpdate the
  tracked expressions are re-executed (results cleared first); a failing
  expression is logged + removed + aborts the rest. `"Untrack All"` button
  clears both lists. Results shown in a 2-column table `TrackedResults`
  (name | result).
- Footer: `update ms: %.2f` / `draw ms: %.2f` (from GameWindow),
  `DragFloat("Timescale", step 0.005f)` writing `Config::setTimescale`,
  `Checkbox("Invincible")` writing `Config::setInvincible`.

## 6. Utils headers

### 6.1 `FastVertexVector.hpp` — `hg::Utils::FastVertexVector<TPrimitive>`

A zero-construct raw `sf::Vertex` array (union trick to skip
`sf::Vertex`'s default ctor), implementing `sf::Drawable`. Verbatim API:

```cpp
template <sf::PrimitiveType TPrimitive>
struct FastVertexVector : public sf::Drawable
{
private:
    union VertexUnion
    {
        // To avoid invoking the default constructor of `sf::Vertex` when
        // resizing the dynamic array.
        sf::Vertex _v;

        VertexUnion() {}
    };

    static_assert(sizeof(VertexUnion) == sizeof(sf::Vertex));
    static_assert(alignof(VertexUnion) == alignof(sf::Vertex));

    Utils::UniquePtrArray<VertexUnion> _data{nullptr};
    std::size_t _size{};
    std::size_t _capacity{};

public:
    void reserve_more(const std::size_t n) { reserve(_size * 2 + n); }

    void reserve(const std::size_t n)
    {
        if(_capacity >= n) [[likely]] { return; }

        auto new_data = Utils::makeUniqueArray<VertexUnion>(n);

        if(_data != nullptr) [[unlikely]]
        {
            std::memcpy(new_data.get(), _data.get(), sizeof(sf::Vertex) * _size);
        }
        else
        {
            SSVOH_ASSERT(_size == 0);
            SSVOH_ASSERT(_capacity == 0);
        }

        _data = SSVOH_MOVE(new_data);
        _capacity = n;
    }

    void unsafe_emplace_other(const FastVertexVector& rhs) noexcept
    {
        SSVOH_ASSERT(_size + rhs._size <= _capacity);

        if(rhs.size() == 0) [[unlikely]] { return; }

        SSVOH_ASSERT(_data != nullptr);

        std::memcpy(_data.get() + _size, rhs._data.get(),
            sizeof(sf::Vertex) * rhs._size);

        _size += rhs._size;
    }

    void clear() noexcept { _size = 0; }

    std::size_t size() const noexcept { return _size; }

    template <typename... Ts>
    void unsafe_emplace_back(Ts&&... xs)
    {
        SSVOH_ASSERT(_size <= _capacity);
        SSVOH_ASSERT(_data != nullptr);

        new(&_data[_size++]._v) sf::Vertex{SSVOH_FWD(xs)...};
    }

    template <typename... Ts>
    void batch_unsafe_emplace_back(const sf::Color& color, Ts&&... positions)
    {
        SSVOH_ASSERT(_size + sizeof...(positions) <= _capacity);
        SSVOH_ASSERT(_data != nullptr);

        ((new(&_data[_size++]._v) sf::Vertex{positions, color}), ...);
    }

    void draw(sf::RenderTarget& mRenderTarget,
        const sf::RenderStates& mRenderStates) const override
    {
        if(_data == nullptr) [[unlikely]]
        {
            SSVOH_ASSERT(_size == 0);
            SSVOH_ASSERT(_capacity == 0);
            return;
        }

        // UB:
        mRenderTarget.draw(reinterpret_cast<const sf::Vertex*>(_data.get()),
            _size, TPrimitive, mRenderStates);
    }

    // operator[], begin()/end() access _v elements (bounds-checked by assert only)
};
```

`sf::Vertex` is `{sf::Vector2f position; sf::Color color;}` (position first,
color second — `batch_unsafe_emplace_back(color, pos...)` builds
`sf::Vertex{pos, color}`).

```cpp
class FastVertexVectorTris
    : public FastVertexVector<sf::PrimitiveType::Triangles>
{
public:
    // quad as 2 triangles: (nw,sw,se) + (nw,se,ne)
    void batch_unsafe_emplace_back_quad(
        const sf::Color& color, const sf::Vector2f& nw, const sf::Vector2f& sw,
        const sf::Vector2f& se, const sf::Vector2f& ne)
    {
        batch_unsafe_emplace_back(color,
            nw, sw, se,
            nw, se, ne);
    }

    // per-vertex colored quad (same triangle split)
    void unsafe_emplace_back_quad(
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
};
```

**Vertex emission order matters for parity:** every quad is 6 vertices
`(nw,sw,se,nw,se,ne)`; `CWall::draw` / `CPlayer::draw` / `CWall3D` etc. emit
into these buffers in the orders transcribed in gameplay.md (§CWall, §CPlayer).

### 6.2 `Easing.hpp` (verbatim, complete)

```cpp
namespace hg::Utils {

[[nodiscard]] inline float getSaturated(const float mValue)
{
    if(mValue < 0.f) { return 0.f; }
    if(mValue > 1.f) { return 1.f; }
    return mValue;
}

[[nodiscard]] inline float getSmoothStep(const float edge0, const float edge1, float x)
{
    x = getSaturated((x - edge0) / (edge1 - edge0));
    return x * x * (3.f - 2.f * x);
}

[[nodiscard]] inline float getSmootherStep(const float edge0, const float edge1, float x)
{
    x = getSaturated((x - edge0) / (edge1 - edge0));
    return x * x * x * (x * (x * 6.f - 15.f) + 10.f);
}

} // namespace hg::Utils
```

### 6.3 `Color.hpp` (verbatim, complete — `hg::Utils`)

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
```

Note: **divides** each of r,b,g by the multiplier (uint8 arithmetic:
cast-to-float divide, truncated back to uint8). Multiplier 0 → unchanged.

```cpp
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
        static_cast<std::uint8_t>(255)
    };
}
```

```cpp
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

    return ret(1.f, 0.f, q);  // i == 5
}
```

```cpp
inline constexpr std::uint8_t componentClamp(const float value) noexcept
{
    if(value > 255.f) { return std::uint8_t(255); }
    if(value < 0.f)   { return std::uint8_t(0); }
    return static_cast<std::uint8_t>(value);
}

inline sf::Glsl::Vec3 toGLSLVec3(const sf::Color& color) noexcept
{
    return {static_cast<float>(color.r) / 255.f,
            static_cast<float>(color.g) / 255.f,
            static_cast<float>(color.b) / 255.f};
}

inline sf::Glsl::Vec4 toGLSLVec4(const sf::Color& color) noexcept
{
    return {static_cast<float>(color.r) / 255.f,
            static_cast<float>(color.g) / 255.f,
            static_cast<float>(color.b) / 255.f,
            static_cast<float>(color.a) / 255.f};
}
```

(`sf::Glsl::Vec3/Vec4` = float3/float4. `sf::Color(float,float,float[,float])`
ctor clamps each channel to 0..255 via static_cast-to-uint8 semantics —
SFML impl uses `componentClamp`.)

### 6.4 `FontHeight.hpp` + `FontHeight.cpp`

```cpp
namespace hg::Utils {
[[nodiscard]] float getFontHeight(sf::Text& font);
[[nodiscard]] float getFontHeight(sf::Text& font, const unsigned int charSize);
}
```

```cpp
float getFontHeight(sf::Text& font)
{
    font.setString("ABCDEFGHILMNOPQRSTUVZ:");
    return font.getGlobalBounds().height;
}

float getFontHeight(sf::Text& font, const unsigned int charSize)
{
    font.setCharacterSize(charSize);
    return getFontHeight(font);
}
```

(Probes the font's cap+baseline height with the fixed string
`"ABCDEFGHILMNOPQRSTUVZ:"` — note it **mutates** the text's string/size.)

## 7. StyleData / ColorData / CapColor

### 7.1 `ColorData.hpp` (complete)

```cpp
struct PulseColor
{
    int r;
    int g;
    int b;
    int a;
};

[[nodiscard]] PulseColor pulse_from_json(const ssvuj::Obj& root) noexcept;

struct ColorData
{
    bool main{};         // "main": true means use the dynamic hue color wholesale
    bool dynamic{};      // animate from the current hue
    bool dynamicOffset{}; // add dynamic color (divided by offset) to base color
    float dynamicDarkness{};
    float hueShift{};
    float offset{};

    sf::Color color{};   // base color
    PulseColor pulse{};  // per-frame additive pulse (int, defaults 0,0,0,255)
};
```

JSON parsing (`ColorData(const ssvuj::Obj&)`, keys → defaults):

| field | key | default |
|---|---|---|
| main | `main` | `false` |
| dynamic | `dynamic` | `false` |
| dynamicOffset | `dynamic_offset` | `false` |
| dynamicDarkness | `dynamic_darkness` | `1.f` |
| hueShift | `hue_shift` | `0.f` |
| offset | `offset` | `0.f` |
| color | `value` | `sf::Color::Black` |
| pulse | `pulse` (array [r,g,b,a]) | `{0,0,0,255}` |

```cpp
[[nodiscard]] PulseColor pulse_from_json(const ssvuj::Obj& root) noexcept
{
    if(!ssvuj::hasObj(root, "pulse"))
    {
        return {0, 0, 0, 255};
    }

    const auto& pulseObj = ssvuj::getObj(root, "pulse");

    return {
        ssvuj::getExtr<int>(pulseObj, 0),
        ssvuj::getExtr<int>(pulseObj, 1),
        ssvuj::getExtr<int>(pulseObj, 2),
        ssvuj::getExtr<int>(pulseObj, 3)
    };
}
```

### 7.2 `CapColor` (`Data/CapColor.hpp/.cpp`)

```cpp
namespace CapColorMode {
struct Main         { };
struct MainDarkened { };
struct ByIndex      { int _index; };
} // namespace CapColorMode

using CapColor = vittorioromeo::tinyvariant<
    CapColorMode::Main,
    CapColorMode::MainDarkened,
    CapColorMode::ByIndex,
    ColorData
    >;
```

```cpp
[[nodiscard]] CapColor parseCapColor(const ssvuj::Obj& obj) noexcept
{
    if(ssvuj::isObjType<std::string>(obj))
    {
        const auto str = ssvuj::getExtr<std::string>(obj);

        if(str == "main")
        {
            return CapColor{CapColorMode::Main{}};
        }

        if(str == "main_darkened")
        {
            return CapColor{CapColorMode::MainDarkened{}};
        }
    }

    if(ssvuj::isObj(obj))
    {
        const bool legacy = ssvuj::getExtr<bool>(obj, "legacy", true);
        if(legacy)
        {
            const int index = ssvuj::getExtr<int>(obj, "index", 0);
            return CapColor{CapColorMode::ByIndex{index}};
        }
        else
        {
            return CapColor{
                ColorData{false, ssvuj::getExtr<bool>(obj, "dynamic", false),
                    ssvuj::getExtr<bool>(obj, "dynamic_offset", false),
                    ssvuj::getExtr<float>(obj, "dynamic_darkness", 1.f),
                    ssvuj::getExtr<float>(obj, "hue_shift", 0.f),
                    ssvuj::getExtr<float>(obj, "offset", 0.f),
                    ssvuj::getExtr<sf::Color>(obj, "value", sf::Color::White),
                    hg::pulse_from_json(obj)}};
        }
    }

    // Fallback case:
    return CapColor{CapColorMode::ByIndex{0}};
}
```

(Non-legacy object cap color defaults `value` to **White**, unlike
ColorData's Black default.)

### 7.3 `StyleData.hpp` — fields

```cpp
class StyleData
{
private:
    float currentHue{0};
    float currentSwapTime{0};
    float pulseFactor{0};
    sf::Color currentMainColor{sf::Color::Black};
    sf::Color currentPlayerColor{sf::Color::Black};
    sf::Color currentTextColor{sf::Color::Black};
    sf::Color currentWallColor{sf::Color::White};
    sf::Color current3DOverrideColor{sf::Color::Black};
    std::vector<sf::Color> currentColors;

public:
    std::string id{};
    float hueMin{};
    float hueMax{};
    float hueIncrement{};
    bool huePingPong{};

    float pulseMin{};
    float pulseMax{};
    float pulseIncrement{};
    float maxSwapTime{};

    float _3dDepth{};
    float _3dSkew{};
    float _3dSpacing{};
    float _3dDarkenMult{};
    float _3dAlphaMult{};
    float _3dAlphaFalloff{};
    float _3dPulseMax{};
    float _3dPulseMin{};
    float _3dPulseSpeed{};
    float _3dPerspectiveMult{};

    float bgTileRadius{10000.f};
    unsigned int BGColorOffset{0};
    float BGRotOff{0}; // In degrees

private:
    sf::Color _3dOverrideColor;
    ColorData mainColorData;
    ColorData playerColor;
    ColorData textColor;
    ColorData wallColor;
    CapColor capColor;
    std::vector<ColorData> colorDatas;
```

JSON constructor (keys → defaults):

| field | key | default |
|---|---|---|
| id | `id` | `"nullId"` |
| hueMin | `hue_min` | `0.f` |
| hueMax | `hue_max` | `360.f` |
| hueIncrement | `hue_increment` | `0.f` |
| huePingPong | `hue_ping_pong` | `false` |
| pulseMin | `pulse_min` | `0.f` |
| pulseMax | `pulse_max` | `0.f` |
| pulseIncrement | `pulse_increment` | `0.f` |
| maxSwapTime | `max_swap_time` | `100.f` |
| _3dDepth | `3D_depth` | `15.f` |
| _3dSkew | `3D_skew` | `0.18f` |
| _3dSpacing | `3D_spacing` | `1.f` |
| _3dDarkenMult | `3D_darken_multiplier` | `1.5f` |
| _3dAlphaMult | `3D_alpha_multiplier` | `0.5f` |
| _3dAlphaFalloff | `3D_alpha_falloff` | `3.f` |
| _3dPulseMax | `3D_pulse_max` | `3.2f` |
| _3dPulseMin | `3D_pulse_min` | `0.f` |
| _3dPulseSpeed | `3D_pulse_speed` | `0.01f` |
| _3dPerspectiveMult | `3D_perspective_multiplier` | `1.f` |
| _3dOverrideColor | `3D_override_color` | `sf::Color::Transparent` |
| mainColorData | `main` | (required object) |
| playerColor | `player_color` | falls back to main |
| textColor | `text_color` | falls back to main |
| wallColor | `wall_color` | falls back to main |
| capColor | `cap_color` | via `parseCapColor` |

Plus: `colorDatas` = the `colors` array (each entry a ColorData JSON object),
in order; `currentHue = hueMin` after construction.

### 7.4 `calculateColor` (static, exact)

```cpp
[[nodiscard]] static sf::Color StyleData::calculateColor(
    const float mCurrentHue, const float mPulseFactor, const ColorData& mColorData)
{
    sf::Color color{mColorData.color};

    if(mColorData.dynamic)
    {
        const float hue =
            std::fmod(mCurrentHue + mColorData.hueShift, 360.f) / 360.f;

        const sf::Color dynamicColor = Utils::getColorFromHue(hue);

        if(!mColorData.main)
        {
            if(mColorData.dynamicOffset)
            {
                if(mColorData.offset != 0)
                {
                    color.r += dynamicColor.r / mColorData.offset;
                    color.g += dynamicColor.g / mColorData.offset;
                    color.b += dynamicColor.b / mColorData.offset;
                }

                color.a += dynamicColor.a;
            }
            else
            {
                color = Utils::getColorDarkened(
                    dynamicColor, mColorData.dynamicDarkness);
            }
        }
        else
        {
            color = dynamicColor;
        }
    }

    return sf::Color( //
        Utils::componentClamp(color.r + mColorData.pulse.r * mPulseFactor),
        Utils::componentClamp(color.g + mColorData.pulse.g * mPulseFactor),
        Utils::componentClamp(color.b + mColorData.pulse.b * mPulseFactor),
        Utils::componentClamp(color.a + mColorData.pulse.a * mPulseFactor));
}
```

Note the branch quirks: `dynamic && !main && !dynamicOffset` **ignores the
base color** and returns darkened dynamic color (base only used in the
`dynamicOffset` branch); `main` → dynamic color replaces base entirely. Pulse
is added on top for all modes (pulse default a=255 keeps alpha saturated for
non-pulsing colors: `a + 255*0`… note pulseFactor scales it).

### 7.5 `update` (hue/pulseFactor animation; called per frame, see §5.1)

```cpp
void StyleData::update(ssvu::FT mFT, float mMult)  // mMult = pow(difficultyMult, 0.8f)
{
    currentSwapTime += mFT * mMult;
    if(currentSwapTime > maxSwapTime)
    {
        currentSwapTime = 0;
    }

    currentHue += hueIncrement * mFT * mMult;

    if(currentHue < hueMin)
    {
        if(huePingPong)
        {
            currentHue = hueMin;
            hueIncrement *= -1.f;
        }
        else
        {
            currentHue = hueMax;
        }
    }
    else if(currentHue > hueMax)
    {
        if(huePingPong)
        {
            currentHue = hueMax;
            hueIncrement *= -1.f;
        }
        else
        {
            currentHue = hueMin;
        }
    }

    pulseFactor += pulseIncrement * mFT;   // NOTE: mFT only, no mMult

    if(pulseFactor < pulseMin)
    {
        pulseIncrement *= -1.f;
        pulseFactor = pulseMin;
    }
    else if(pulseFactor > pulseMax)
    {
        pulseIncrement *= -1.f;
        pulseFactor = pulseMax;
    }
}
```

### 7.6 `computeColors` (called every frame while started)

```cpp
void StyleData::computeColors()
{
    currentMainColor = calculateColor(currentHue, pulseFactor, mainColorData);
    currentPlayerColor = calculateColor(currentHue, pulseFactor, playerColor);
    currentTextColor = calculateColor(currentHue, pulseFactor, textColor);
    currentWallColor = calculateColor(currentHue, pulseFactor, wallColor);

    current3DOverrideColor =
        _3dOverrideColor.a != 0 ? _3dOverrideColor : getMainColor();

    currentColors.clear();

    for(const ColorData& cd : colorDatas)
    {
        currentColors.emplace_back(calculateColor(currentHue, pulseFactor, cd));
    }

    if(currentColors.size() > 1)
    {
        const unsigned int rotation = currentSwapTime / (maxSwapTime / 2.f);

        ssvu::rotate(currentColors,
            std::begin(currentColors) +
                ssvu::getMod(rotation + BGColorOffset, currentColors.size()));
    }
}
```

Background palette rotation: the `colors` array is rotated by
`mod(rotation + BGColorOffset, n)` every time `currentSwapTime` crosses
`maxSwapTime/2` (two swaps per full cycle) — i.e. the "swap blinking" palette
cycle. `get3DOverrideColor()` returns `current3DOverrideColor`, which is the
explicit `3D_override_color` when its alpha != 0, **else the current main
color** (this is the sentinel used by draw()'s 3D override-color comparison).

### 7.7 `drawBackground` / `drawBackgroundMenu` (exact vertex math)

```cpp
void StyleData::drawBackgroundImpl(Utils::FastVertexVectorTris& vertices,
    const sf::Vector2f& mCenterPos, const unsigned int sides,
    const bool darkenUnevenBackgroundChunk, const bool blackAndWhite) const
{
    const float div{ssvu::tau / sides * 1.0001f};
    const float halfDiv{div / 2.f};
    const float distance{bgTileRadius};

    const std::vector<sf::Color>& colors(getColors());
    if(colors.empty())
    {
        return;
    }

    for(auto i(0u); i < sides; ++i)
    {
        const float angle{ssvu::toRad(BGRotOff) + div * i};
        sf::Color currentColor{ssvu::getByModIdx(colors, i)};

        const bool mustDarkenUnevenBackgroundChunk =
            (i % 2 == 0 && i == sides - 1) && darkenUnevenBackgroundChunk;

        if(blackAndWhite)
        {
            currentColor = sf::Color::Black;
        }
        else if(mustDarkenUnevenBackgroundChunk)
        {
            currentColor = Utils::getColorDarkened(currentColor, 1.4f);
        }

        vertices.batch_unsafe_emplace_back(currentColor, mCenterPos,
            ssvs::getOrbitRad(mCenterPos, angle + halfDiv, distance),
            ssvs::getOrbitRad(mCenterPos, angle - halfDiv, distance));
    }
}
```

- One triangle fan sector per side: center + two orbit points at radius
  `bgTileRadius` (default 10000). `div = tau/sides * 1.0001` (tiny overlap so
  no hairline gaps between sectors). Sector `i` centered at angle
  `toRad(BGRotOff) + div*i`, spanning `±halfDiv`. Color = `colors[i mod n]`
  (Black in B&W; sector `sides-1` darkened ×1.4 when `sides` is even and the
  flag is set).
- `drawBackground` = `reserve_more(sides*3)` + `drawBackgroundImpl` only.
- `drawBackgroundMenu` = `reserve_more(sides*3 + sides*6)` + `drawBackgroundImpl`
  + the hexagon overlay below (used by MenuGame):

```cpp
void StyleData::drawBackgroundMenuHexagonImpl(
    Utils::FastVertexVectorTris& vertices, const sf::Vector2f& mCenterPos,
    const unsigned int sides, const bool fourByThree,
    const bool blackAndWhite) const
{
    const float div{ssvu::tau / sides * 1.0001f};
    const float halfDiv{div / 2.f};
    const float hexagonRadius{fourByThree ? 75.f : 100.f};

    const sf::Color& colorMain{
        blackAndWhite ? sf::Color::White : getMainColor()};
    const sf::Color colorCap{
        blackAndWhite ? sf::Color::Black : getCapColorResult()};

    for(auto i(0u); i < sides; ++i)
    {
        const float angle{ssvu::toRad(BGRotOff) + div * i};

        vertices.batch_unsafe_emplace_back(colorMain, mCenterPos,
            ssvs::getOrbitRad(
                mCenterPos, angle + halfDiv, hexagonRadius + 10.f),
            ssvs::getOrbitRad(
                mCenterPos, angle - halfDiv, hexagonRadius + 10.f));

        vertices.batch_unsafe_emplace_back(colorCap, mCenterPos,
            ssvs::getOrbitRad(mCenterPos, angle + halfDiv, hexagonRadius),
            ssvs::getOrbitRad(mCenterPos, angle - halfDiv, hexagonRadius));
    }
}
```

Menu hexagon: 60°-sector wedges (one per `sides`, same angle scheme), two
concentric rings: outer at `hexagonRadius + 10` in main color, inner at
`hexagonRadius` in cap color; radius 75 for 4:3 windows, 100 otherwise.

Getters: `getMainColor/getPlayerColor/getTextColor/getWallColor/getColors` →
the `current*` fields; `getColor(idx)` = `getByModIdx(currentColors, idx)`;
`getCurrentHue`, `getCurrentSwapTime`, `get3DOverrideColor` →
`current3DOverrideColor`.

```cpp
sf::Color StyleData::getCapColorResult() const noexcept
{
    return capColor.linear_match(
        [this](CapColorMode::Main) { return getMainColor(); },
        [this](CapColorMode::MainDarkened)
        { return Utils::getColorDarkened(getMainColor(), 1.4f); },
        [this](CapColorMode::ByIndex x) { return getColor(x._index); },
        [this](const ColorData& data)
        { return calculateColor(currentHue, pulseFactor, data); });
}
```

## 8. `Global/Imgui` wrapper (verbatim, complete)

```cpp
// include/SSVOpenHexagon/Global/Imgui.hpp
namespace hg::Imgui {

[[nodiscard]] bool initialize(sf::RenderWindow&);
void shutdown();
[[nodiscard]] bool wantCaptureKeyboard();
[[nodiscard]] bool wantCaptureMouse();
void processEvent(const sf::Event&);
void render(sf::RenderTarget& renderTarget);

} // namespace hg::Imgui
```

```cpp
// src/SSVOpenHexagon/Global/Imgui.cpp
namespace hg::Imgui {

bool initialize(sf::RenderWindow& window)      { return ImGui::SFML::Init(window); }
void shutdown()                               { ImGui::SFML::Shutdown(); }
bool wantCaptureKeyboard()                    { return ImGui::GetIO().WantCaptureKeyboard; }
bool wantCaptureMouse()                       { return ImGui::GetIO().WantCaptureMouse; }
void processEvent(const sf::Event& event)     { ImGui::SFML::ProcessEvent(event); }
void render(sf::RenderTarget& renderTarget)   { ImGui::SFML::Render(renderTarget); }

} // namespace hg::Imgui
```

All bodies are `#ifndef SSVOH_ANDROID` (Android build: `initialize` returns
nothing → false, others no-op). Uses the `imgui-SFML` binding
(`ImGui::SFML::Init/Shutdown/ProcessEvent/Update/Render`) +
`misc/cpp/imgui_stdlib.h` (`ImGui::InputText` with `std::string`).
`HexagonGame` registers `game.onAnyEvent += Imgui::processEvent;` so **every
window event** is forwarded to ImGui (see §4.1).

## 9. Shader system

### 9.1 Loading (`Global/Assets.cpp`, `loadPackAssets_loadShaders`)

- Scans each pack's `Shaders/` directory (skipped entirely in headless mode)
  for `.vert` → Vertex, `.geom` → Geometry, `.frag` → Fragment shaders
  (three separate scans, in that order; files inside are scanned by
  `scanSingleByExt` — sorted-by-name enumeration).
- Each successfully loaded shader gets a sequential `shaderId` (index into
  `shadersById`, a `std::vector<sf::Shader*>` shared across all packs in
  load order); the name key is `<packId>_<fileName>` (e.g.
  `experimental_text.frag`). Failed loads log
  `"Failed to load shader '...'\n"` and are skipped (id not consumed).
- `shdr_getShaderId(name)` (Lua) resolves by that name key (returns -1 if
  missing — see gameplay.md §LuaScripting for the exact function).
- `getShaderByShaderId(id)` returns `shadersById[id]` or nullptr.
- `reloadAllShaders()` re-loads every shader from its path (used by asset
  reload).

### 9.2 Lua API (`Core/LuaScripting.cpp`, `initShaders`)

Functions (docs verbatim):

- `shdr_getShaderId(name)` → id (see gameplay.md).
- `shdr_setUniform*` family (F/FVec2/FVec3/FVec4/I/IVec2/…/`setUniformUnsafe`):
  set a uniform by name on a shader id; invalid id → log
  `"Invalid shader ID ..."` and no-op (via `withValidShaderId`).
- `shdr_resetAllActiveFragmentShaders()` → sets every
  `status.fragmentShaderIds[i]` to `nullopt`.
- `shdr_resetActiveFragmentShader(renderStage)` → `nullopt` for that stage
  (validates stage < RenderStage::Count, logs otherwise).
- `shdr_setActiveFragmentShader(renderStage, shaderId)` → stores the id for
  that stage.

### 9.3 Per-frame binding (`HGGraphics.cpp::draw::getRenderStates`)

Per render stage: no shaders enabled (`Config::getShaders()` false) →
`sf::RenderStates::Default`; no bound id → Default; otherwise run Lua
`onRenderStage(int stage, float frametime = 60/FPS)` (if defined) **before**
returning the shader's render states.

### 9.4 Shaders shipped in `_RELEASE` (verbatim)

**`Packs/experimental/Shaders/pixelate.frag`**

```glsl
uniform vec2 u_resolution;
uniform vec3 color1;
uniform vec3 color2;

void main() {

  vec2 st = gl_FragCoord.xy/u_resolution.xy;

  float mixValue = distance(st,vec2(0,1));
  vec3 color = mix(color1,color2,mixValue);

  gl_FragColor = vec4(color,mixValue);
}
```

**`Packs/experimental/Shaders/text.frag`**

```glsl
#version 130
uniform vec4 color0;
uniform sampler2D font;
uniform float time;

vec4 getColorFromHue(const float hue) {
    int i = int(hue * 6.f);

    float f = (hue * 6.f) - i;
    float q = 1.f - f;
    float t = f;

    switch(i)
    {
        case 0: return vec4(1.f, t, 0.f, 1.f);
        case 1: return vec4(q, 1.f, 0.f, 1.f);
        case 2: return vec4(0.f, 1.f, t, 1.f);
        case 3: return vec4(0.f, q, 1.f, 1.f);
        case 4: return vec4(t, 0.f, 1.f, 1.f);
    }

    return vec4(1.f, 0.f, q, 1.f);
}

void main() {
	vec4 fill_color = getColorFromHue(mod(time * 360.f + gl_FragCoord.x + gl_FragCoord.y, 360.f) / 360.f);
	vec4 color = gl_Color.rgb == color0.rgb / 255.f ? vec4(vec3(1,1,1) - fill_color.rgb, 1.f) : fill_color;
	gl_FragColor = color * texture(font, gl_TexCoord[0].xy);
}
```

(Colors the TEXT stage: hue ramps with `time` (level time) + screen coords;
where `gl_Color` equals `color0/255` it inverts the fill — outline vs glyph.
Usage in `experimental/Scripts/Levels/text_shader.lua`:
onInit → `shdr_getShaderId("text.frag")`,
`shdr_setActiveFragmentShader(RenderStage.TEXT, ...)`, `s_setMaxSwapTime(0)`;
onUpdate → `shdr_setUniformFVec4(id, "color0", s_getColor(0))` +
`shdr_setUniformF(id, "time", l_getLevelTime())`.)

**`Packs/artwork/Shaders/gradient.frag`**

```glsl
uniform vec2 u_resolution;
uniform vec3 u_color0;
uniform vec3 u_color1;
uniform float u_blend;

void main()
{
    vec2 st = gl_FragCoord.xy / u_resolution.xy;

    gl_FragColor = mix(gl_Color, vec4(mix(vec4(u_color1, 1.0), vec4(u_color0, 1.0), st.y)), u_blend);
}
```

**`Packs/artwork/Shaders/steamart.frag`**

```glsl
uniform vec2 u_resolution;
uniform float u_time;
uniform float u_rotation;

// These functions were written by patriciogv (https://patriciogonzalezvivo.com).
// He's an absolute beast for writing this. I can't comprehend this.

vec3 mod289(vec3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec2 mod289(vec2 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec3 permute(vec3 x) { return mod289(((x*34.0)+1.0)*x); }

float snoise(vec2 v) {
    const vec4 C = vec4(0.211324865405187,  // (3.0-sqrt(3.0))/6.0
                        0.366025403784439,  // 0.5*(sqrt(3.0)-1.0)
                        -0.577350269189626, // -1.0 + 2.0 * C.x
                        0.024390243902439); // 1.0 / 41.0
    vec2 i  = floor(v + dot(v, C.yy) );
    vec2 x0 = v -   i + dot(i, C.xx);
    vec2 i1;
    i1 = (x0.x > x0.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
    vec4 x12 = x0.xyxy + C.xxzz;
    x12.xy -= i1;
    i = mod289(i); // Avoid truncation effects in permutation
    vec3 p = permute( permute( i.y + vec3(0.0, i1.y, 1.0 ))
        + i.x + vec3(0.0, i1.x, 1.0 ));

    vec3 m = max(0.5 - vec3(dot(x0,x0), dot(x12.xy,x12.xy), dot(x12.zw,x12.zw)), 0.0);
    m = m*m ;
    m = m*m ;
    vec3 x = 2.0 * fract(p * C.www) - 1.0;
    vec3 h = abs(x) - 0.5;
    vec3 ox = floor(x + 0.5);
    vec3 a0 = x - ox;
    m *= 1.79284291400159 - 0.85373472095314 * ( a0*a0 + h*h );
    vec3 g;
    g.x  = a0.x  * x0.x  + h.x  * x0.y;
    g.yz = a0.yz * x12.xz + h.yz * x12.yw;
    return 130.0 * dot(m, g);
}


void main()
{
    vec2 st = gl_FragCoord.xy / u_resolution.xy;
    st = st * 2.0 - 1.0;
    st.x *= u_resolution.x / u_resolution.y;

    // Rotate the coordinates with the level
    float s = sin(u_rotation);
    float c = cos(u_rotation);
    mat2 rot = mat2(c, s, -s, c);
    st = st * rot;
    
    // Set up the noise and animate it
    vec2 noise_vec = st;
    vec2 pos = vec2(noise_vec / 2.0);

    float DF = 0.0;

    // Add a random position
    float a = 0.0;
    vec2 vel = vec2(u_time*.1);
    DF += snoise(pos+vel)*.25+.25;

    // Add a random position
    a = snoise(pos*vec2(cos(u_time*0.15),sin(u_time*0.1))*0.1)*3.1415;
    vel = vec2(cos(a),sin(a));
    DF += snoise(pos+vel)*.25+.25;

    // Generate the noise
    vec3 n = vec3(  smoothstep(1.0, 0.0, fract(DF)) * .8 + .2);

    vec3 color1 = vec3(1.0, 0.0, 113.0/255.0);
    vec3 color2 = vec3(1.0, 45.0 / 255.0, 30.0 / 255.0);
    vec3 color3 = vec3(1.0, 198.0 / 255.0, 47.0 / 255.0);

    // Convert the black and white noise into the gradient colors we want.
    vec3 color_map = mix(mix(color1, color2, n.x/0.5), mix(color2, color3, (n.x - 0.5)/(1.0 - 0.5)), step(0.5, n.x));

    // Apply our shader to the final colors
    // vec4(mix(vec4(u_color1, 1.0), vec4(u_color0, 1.0), st.y))
    // Considering that our style is B&W, blend the shader in a Multiplying style
    gl_FragColor = gl_Color * vec4(color_map, 1.0);
}
```

Usage in `artwork/Scripts/Levels/pointless.lua`: onInit binds
`steamart.frag` to stages BACKGROUNDTRIS, WALLQUADS3D, PIVOTQUADS3D,
PLAYERTRIS3D, WALLQUADS, CAPTRIS, PIVOTQUADS, PLAYERTRIS; onRenderStage sets
`u_resolution = (width, height)`, `u_time = l_getLevelTime()`,
`u_rotation = math.rad(l_getRotation())`.

## 10. SSVStart geometry helpers used by draw code

### 10.1 `Utils/SFML.hpp` (text/sprite bound accessors)

All `getGlobal*`/`getLocal*` accessors are computed from
`getGlobalBounds()` / `getLocalBounds()` respectively:

- `getGlobalLeft/Top/Width/Height` → bounds.left/top/width/height
- `getGlobalRight = left + width`; `getGlobalBottom = top + height`
- `getGlobalNW = (left, top)`; `getGlobalNE = (left+width, top)`;
  `getGlobalSW = (left, top+height)`; `getGlobalSE = (left+width, top+height)`
- `getGlobalCenter = (left + width/2, top + height/2)`
- `getGlobalCenterN = (cx, top)`; `getGlobalCenterE = (left+width, cy)`;
  `getGlobalCenterS = (cx, top+height)`; `getGlobalCenterW = (left, cy)`
- Same set for local bounds (`getLocalNW` etc.) — **without** transform.

For `sf::Text`: `getLocalBounds()` returns the glyph bounds in the text's
local coordinate system (for the current string/char size, **ignoring**
position/origin/scale — its left/top are usually small negative values from
font metrics); `getGlobalBounds()` = local bounds transformed by
position + origin + scale (and rotation). All the `drawText_*` positioning in
§3.10/§4.3 relies on this distinction.

(Also in this header: `getColorFromHSV`, `getHSVFromColor`,
`getColorFromHex` — used by other systems; `setOrigin(mX, &fn)` helper.)

### 10.2 `Utils/Vector2.hpp` (subset used by render code)

```cpp
static Vec2f zeroVec2f{0.f, 0.f};

template <typename T> inline void nullify(Vec2<T>& mVec) noexcept { mVec.x = mVec.y = 0; }

template <typename T> inline auto getMag(const Vec2<T>& mVec) { return std::sqrt(mVec.x * mVec.x + mVec.y * mVec.y); }

template <typename T1, typename T2 = float>
inline auto getVecFromRad(const T1& mRad, const T2& mMag = 1.f)
{
    return Vec2<CT<T1, T2>>(mMag * std::cos(mRad), mMag * std::sin(mRad));
}

template <typename T1, typename T2 = float>
inline auto getVecFromDeg(const T1& mDeg, const T2& mMag = 1.f)
{
    return getVecFromRad(ssvu::toRad(mDeg), mMag);
}

template <typename T1, typename T2, typename T3>
inline auto getOrbitRad(const Vec2<T1>& mVec, const T2& mRad, const T3& mRadius)
{
    return Vec2<CT<T1, T2, T3>>(mVec) + getVecFromRad(mRad, mRadius);
}

template <typename T1, typename T2>
inline auto getOrbitDeg(const Vec2<T1>& mVec, const T2& mDeg, const T3& mRadius)
{
    return getOrbitRad(mVec, ssvu::toRad(mDeg), mRadius);
}
```

(`getDeg(vec)` = atan2(y, x) in degrees; used by Camera::apply for the
offset. `ssvu::toRad(deg) = deg * pi / 180`.)

## 11. MenuGame (`Core/MenuGame.hpp` + `Core/MenuGame.cpp`)

### 11.1 States enum + class skeleton (verbatim from hpp)

```cpp
enum class States
{
    ETLPNewBoot,
    LoadingScreen,
    EpilepsyWarning,
    SLPSelectBoot,
    SMain,
    LevelSelection,
    MOpts,
    MOnline,
    SLPSelect,
    ETLPNew
};
```

MenuGame holds:

- `ssvs::Camera backgroundCamera; ssvs::Camera overlayCamera;` (default
  camera c'tors — see §11.5 for the exact init views)
- `HexagonDialogBox dialogBox` (built with `openSquare` font + window; §12)
- Menus: `ssvms::Menu welcomeMenu, mainMenu, optionsMenu, onlineMenu,
  profileSelectionMenu` (SSVMenuSystem — see issue 12 mapping)
- `MenuFont` struct: `{ sf::Text font; float height; void updateHeight(); }`
  with members: `txtVersion, txtProf, txtLoadBig, txtLoadSmall,
  txtRandomTip, txtMenuBig, txtMenuSmall, txtMenuTiny, txtProfile,
  txtInstructionsBig, txtInstructionsMedium, txtInstructionsSmall,
  txtEnteringText, txtSelectionBig, txtSelectionMedium, txtSelectionSmall,
  txtSelectionScore, txtSelectionRanked`. Each `updateHeight()` calls
  `hg::Utils::getFontHeight(font)` (§6.4) — "the height of the font ... only
  changes when the resolution does. So we only calculate it on boot and when
  the res changes and store it." (comment in hpp)
- Colors: `menuTextColor, menuQuadColor, menuSelectionColor,
  dialogBoxTextColor` (computed in `refreshCamera`, §11.9)
- Vertex buffers: `menuBackgroundTris, menuQuads` (FastVertexVectorTris)
- `LoadInfo& loadInfo` (assets' load results), `randomTip[2]` (string_view
  pair), `hexagonRotation` (load screen)
- Text entry: `std::vector<char> enteredChars`; `isEnteringText()` =
  `state <= States::ETLPNewBoot || state == States::ETLPNew`
- Online status bar: `sf::Sprite sOnline` (onlineIconFail.png initially),
  `sf::RectangleShape rsOnlineStatus{128.f, 32.f}`, `sf::Text
  txtOnlineStatus{"", openSquare, 24}`
- Title/credit sprites: `titleBar` (titleBar.png), `creditsBar1`
  (creditsBar1.png), `creditsBar2` (creditsBar2.png), `epilepsyWarning`
  (epilepsyWarning.png); `static constexpr std::array<const char*, 3>
  creditsIds{"creditsBar2.png", "creditsBar2b.png", "creditsBar2c.png"}`
- Misc: `float w, h` (window size in px), `int scrollbarOffset`,
  `bool fourByThree`, `mustRefresh`, `wasFocusHeld/focusHeld`,
  `wheelProgress`, `touchDelay`, `States state` (starts at
  `LoadingScreen`), `packChangeDirection`, `LevelStatus levelStatus` (ctor:
  `Config::getMusicSpeedDMSync(), Config::getSpawnDistance()`),
  `int ignoreInputs`, `menuHalfHeight`, `enteringTextOffset`,
  `scoresMessage`, `float exitTimer{0}, currentCreditsId{0}`,
  `mustTakeScreenshot`, `currentLeaderboard, enteredStr, leaderboardString`,
  `mustShowLoginAtStartup{true}`, `showFirstTimeTips`,
  `mustShowFTTMainMenu/LevelSelect/DeathTips{true}`, `float
  dialogBoxDelay{0.f}`.
- Level-selection state (see §11.8): `LevelDrawer lvlSlct, favSlct` +
  `LevelDrawer* lvlDrawer{&lvlSlct}`, `favoriteLevelDataIds`,
  `isLevelFavorite`, `diffMultIdx{0}`, `firstLevelSelection{true}`,
  `PackChange packChangeState`, `namesScroll[Label::ScrollsSize]`,
  `levelDescription`, `textToQuadBorder`, `slctFrameSize`,
  `packLabelHeight`, `levelLabelHeight`, `packChangeOffset`,
  `levelDetailsOffset`, `baseScrollSpeed{30.f}` (static constexpr),
  `scrollSpeed`, `difficultyBumpEffect{0.f}`,
  `difficultyBumpEffectMax{24.f}` (static constexpr).
- Dialog input: `DialogInputState` enum
  `{Nothing, Registration_EnteringUsername, Registration_EnteringPassword,
  Registration_EnteringPasswordConfirm, Login_EnteringUsername,
  Login_EnteringPassword, DeleteAccount_EnteringPassword}` + strings
  `registrationUsername/registrationPassword/registrationPasswordConfirm,
  loginUsername/loginPassword, deleteAccountPassword`.
- Hexagon-game callback seams (public): `fnHGTriggerRefresh(Trigger, int)`,
  `fnHGNewGame(pack, level, firstPlay, difficultyMult, executeLastReplay)`,
  `fnHGUpdateRichPresenceCallbacks()`.

File-static helpers (top of MenuGame.cpp):

```cpp
[[nodiscard]] static bool anyItemEnabled(const ssvms::Menu& menu)
{
    for(const auto& i : menu.getItems())
    {
        if(i->isEnabled()) { return true; }
    }
    return false;
}

[[nodiscard]] static bool scrollToEnabledMenuItem(ssvms::Menu* menu)
{
    if(menu == nullptr) { return false; }

    menu->update();

    if(!anyItemEnabled(*menu)) { return false; }

    while(!menu->getItem().isEnabled())
    {
        menu->next();
    }

    return true;
}

inline constexpr float maxOffset{100.f};
```

State→menu mapping (`getCurrentMenu()`):

| state | menu |
|---|---|
| SMain | `mainMenu` |
| MOpts | `optionsMenu` |
| MOnline | `onlineMenu` |
| SLPSelectBoot, SLPSelect | `profileSelectionMenu` |
| all others | nullptr |

`initOnlineIcons()`: sets `setSmooth(true)` on textures `onlineIcon.png` and
`onlineIconFail.png`.

### 11.2 Constructor (init-order-relevant parts)

Fonts: `openSquare` = `OpenSquare-Regular.ttf`, `openSquareBold` =
`OpenSquare-Bold.ttf`. Text initial char sizes:
`txtVersion` 40, `txtProf` 18, `txtProfile` 32, `txtInstructionsBig` 46,
`txtInstructionsSmall` 20, `txtEnteringText` 54, `txtSelectionMedium` 19
(bold), `txtSelectionScore` 28. (txtLoadBig/Small, txtRandomTip, txtMenuBig/
Small/Tiny, txtInstructionsMedium, txtSelectionBig/Small/Ranked are default
size — sized per-state in the draw functions.)

**Camera init (exact — note the quirk in the overlay center Y):**

```cpp
backgroundCamera{
    {ssvs::zeroVec2f, {Config::getSizeX() * Config::getZoomFactor(),
                          Config::getSizeY() * Config::getZoomFactor()}}},
overlayCamera{{{Config::getWidth() / 2.f,
                     Config::getHeight() * Config::getZoomFactor() / 2.f},
    {Config::getWidth() * Config::getZoomFactor(),
        Config::getHeight() * Config::getZoomFactor()}}},
```

(background = `sf::View{center(0,0), size(w*zoom, h*zoom)}`;
overlay = `sf::View{center(w/2, h*zoom/2), size(w*zoom, h*zoom)}` — the
overlay center Y uses `getHeight() * zoomFactor / 2`, i.e. NOT `h/2`; with
zoom 1 both coincide. `Config::getSizeX/getSizeY` = current resolution
width/height (window pixels).)

Constructor body (order):

1. `setMouseCursorVisible(true)`.
2. If `Config::getFirstTimePlaying()`: `showFirstTimeTips = true`;
   `Config::setFirstTimePlaying(false)`.
3. `initAssets()` (smooth on: titleBar.png, creditsBar1.png, creditsBar2.png,
   creditsBar2b.png, creditsBar2c.png, creditsBar2d.png, bottomBar.png,
   epilepsyWarning.png), `initOnlineIcons()`, `refreshCamera()`.
4. `game.onUpdate += update(mFT); game.onDraw += draw();`
5. Events:
   - **Resized** → `changeResolutionTo(w, h)` (no-op if same; else
     `Config::setCurrentResolution`, set window size, `refreshCamera()`,
     `adjustLevelsOffset()`, `adjustMenuOffset(true)`, `resetNamesScrolls()`).
   - **TextEntered** (first handler): unicode < 128 → `enteredChars.push_back`.
   - **KeyPressed**: if window has focus → `setMouseCursorVisible(false)`.
   - **MouseMoved**: track `lastMouseMovedPosition`; if focus and actual move
     → `setMouseCursorVisible(true)`.
   - **MouseWheelScrolled**:
     - if focus → cursor visible.
     - In MOpts: if current item is a `BindControlBase` waiting for bind →
       ignore.
     - In LevelSelection:
       - focusHeld → `changePackQuick(delta > 0 ? -1 : 1)` (note sign flip);
       - else `lvlDrawer->YScrollTo += delta * 48.f`; clamp 0; if < −4000 →
         `steamManager.unlock_achievement("a35_eagerformore")` +
         `playSoundOverride("error.ogg")` + reset to 0. return.
     - Otherwise: `wheelProgress += delta`; > 1 → upAction (reset 0); < −1 →
       downAction (reset 0).
   - **TextEntered** (second handler, dialog input box): if dialog box not
     empty and is input box: unicode in [32, 127) → if input size < 32,
     beep ("beep.ogg") + append char; unicode == 8 (backspace) → if non-empty,
     beep + pop_back.
   - **KeyReleased / MouseButtonReleased / JoystickButtonReleased** (all with
     the same `ignoreInputs`-gated logic, summarized):
     - If `ignoreInputs == 0` → return (normal processing).
     - If state is EpilepsyWarning/LoadingScreen → `checkCloseBootScreens()`.
     - If `dialogBoxDelay > 0` → return.
     - Dialog box open:
       - KeyReleased: close if key == `dialogBox.getKeyToClose()` or Unknown;
         Esc on an input box → cancel (clear dialog, `dialogInputState =
         Nothing`, select.ogg); then `checkCloseDialogBox()`.
       - Mouse/Joy: only if `getKeyToClose() == Unknown`; decrement and
         `checkCloseDialogBox()`.
     - Else (bind capture): KeyReleased Esc while in a menu → `exec()` the
       bind item (turn off bind inputting) + beep + clear ignore; else when
       `--ignoreInputs == 0`: if not MOpts or no menu → clear; for
       KeyReleased/MouseButtonReleased cast item to `KeyboardBindControl*`,
       for JoystickButtonReleased cast to `JoystickBindControl*`; mismatched
       kind → error.ogg + `ignoreInputs = 1` (retry); hardcoded-key collision
       (KeyReleased only) → error + dialog:
       `"THE KEY YOU ARE TRYING TO ASSIGN TO THIS ACTION\nIS ALREADY BOUND TO IT BY DEFAULT,\nYOUR LAST INPUT HAS BEEN IGNORED\n\nPRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n"`;
       success → select.ogg, clear ignore, `touchDelay = 10.f`.
   - `checkCloseBootScreens` (lambda captured by the three release handlers):
     when `--ignoreInputs == 0`: from LoadingScreen →
     `changeStateTo(EpilepsyWarning)`, `setIgnoreAllInputs(1)`,
     `scrollbarOffset = 0`; from EpilepsyWarning → set
     `mainMenu.getItems()[0]->getOffset() = maxOffset` (100) and
     `mainMenu.getCategory().getOffset() = fourByThree ? 280.f : 400.f`,
     `playLocally()` (state → SMain), `setIgnoreAllInputs(0)`; then
     `playSoundOverride("select.ogg")`.
   - `checkCloseDialogBox` (dialog-input state machine — verbatim flow):
     - ignoreInputs != 0 → return.
     - `Nothing` → close box (select.ogg, clear, ignore 0).
     - `Registration_EnteringUsername` → username := input;
       transition to `Registration_EnteringPassword`; show
       `showInputDialogBoxNice("REGISTRATION", "PASSWORD")` +
       `dialogBox.setInputBoxPassword(true)`; ignore 1.
     - `Registration_EnteringPassword` → password := input; transition to
       `Registration_EnteringPasswordConfirm`; show
       `showInputDialogBoxNice("REGISTRATION", "CONFIRM PASSWORD")` + password
       mode; ignore 1.
     - `Registration_EnteringPasswordConfirm` → confirm := input; if
       passwords equal → `hexagonClient.tryRegister(username, password)`;
       else error.ogg + dialog
       `"PASSWORD MISMATCH\n\nPRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n"`
       + ignore 1.
     - `Login_EnteringUsername` → username := input; transition to
       `Login_EnteringPassword`; show `showInputDialogBoxNice("LOGIN",
       "PASSWORD")` + password mode; ignore 1.
     - `Login_EnteringPassword` → password := input;
       `hexagonClient.tryLogin(username, password)`.
     - `DeleteAccount_EnteringPassword` → password := input;
       `hexagonClient.tryDeleteAccount(password)`.
6. `window.onRecreation += { refreshCamera(); adjustLevelsOffset();
   adjustMenuOffset(true); resetNamesScrolls(); mustRefresh = true; }`.
7. `initMenus(); initInput(); initLua();`
8. Main-menu background pick: `pickRandomMainMenuBackgroundStyle()` →
   `(randomPack, randomLevel)`; `lvlSlct.levelDataIds =
   &assets.getLevelIdsByPack(getNthSelectablePackInfo(randomPack).id)`;
   `setIndex(randomLevel)` (sets styleData + colors from that level — see
   §11.7).
9. Loading-screen tips (static, verbatim):

```cpp
    static constexpr std::array<std::array<std::string_view, 2>, 4> tips{
        {{"HOLDING FOCUS WHILE CHANGING PACK", "SKIPS THE SWITCH ANIMATION"},
            {"REMEMBER TO TAKE BREAKS", "OPEN HEXAGON IS AN INTENSE GAME"},
            {"EXPERIMENT USING SWAP", "IT MAY SAVE YOUR LIFE"},
            {"IF A LEVEL IS TOO CHALLENGING",
                "PRACTICE IT AT A LOWER DIFFICULTY"}}};
    randomTip = tips[ssvu::getRndI(0, tips.size())];
```

10. Resize `lvlSlct.lvlOffsets` to the max level count across all selectable
    packs (packs with levels only).

`init(bool error)`: Steam rich presence `set_rich_presence_in_menu()` +
`update_hardcoded_achievements()`; Discord `set_rich_presence_in_menu()`;
`audio.stopMusic(); audio.stopSounds();` then `error.ogg` if error else
`select.ogg`. Overload `init(error, pack, level)` also calls
`loadCommandLineLevel(pack, level)`.

`loadCommandLineLevel` (§11.10 below) resolves a `pack/level` id via
`LevelValidator`, sets `currentPack`/`levelData`, `changeStateTo(ETLPNewBoot)`
or straight to LevelSelection with the level pre-selected.

### 11.3 `changeStateTo` (verbatim core)

```cpp
void MenuGame::changeStateTo(const States mState)
{
    const States prevState = state;
    state = mState;

    if(prevState == state)
    {
        // Not a state transition.
        return;
    }

    if(state == States::SMain)
    {
        if(std::exchange(mustShowLoginAtStartup, false) &&
            Config::getShowLoginAtStartup())
        {
            openLoginDialogBoxAndStartLoginProcess();
            setIgnoreAllInputs(2);
        }
    }

    if(state == States::LevelSelection)
    {
        firstLevelSelection = false;
    }

    if(!showFirstTimeTips)
    {
        // Not the first time playing.
        return;
    }

    const auto mustShowTip = [&](const States s, bool& flag)
    { return state == s && std::exchange(flag, false); };

    const auto showTip = [&](const char* str)
    {
        playSoundOverride("select.ogg");

        showDialogBox(str);
        setIgnoreAllInputs(1);

        // Prevent dialog box from being closed immediately:
        dialogBoxDelay = 64.f;
    };

    if(mustShowTip(States::SMain, mustShowFTTMainMenu))
    {
        showTip(
            "WELCOME TO OPEN HEXAGON!\n\n"
            "YOU CAN NAVIGATE THE MAIN MENU WITH THE UP/DOWN ARROW KEYS\n"
            "OR THE DPAD/THUMBSTICK ON YOUR CONTROLLER\n\n"
            "REMEMBER TO CHECK OUT THE OPTIONS MENU\n\n"
            "PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n");
    }
    else if(mustShowTip(States::LevelSelection, mustShowFTTLevelSelect))
    {
        showTip(
            "THIS IS WHERE YOU CAN PICK A LEVEL TO PLAY\n\n"
            "LEVELS ARE ORGANIZED IN 'LEVEL PACKS'\n"
            "TO BROWSE LEVELS AND LEVEL PACKS, GO UP/DOWN\n"
            "HOLD SHIFT (FOCUS) TO QUICKLY JUMP BETWEEN PACKS\n"
            "TO CHANGE THE DIFFICULTY OF A LEVEL, GO LEFT/RIGHT\n\n"
            "AS A FIRST TIMER, PLAY THE 'CUBE' LEVELS IN ORDER\n"
            "THEN YOU CAN GET NEW LEVELS ON THE STEAM WORKSHOP\n\n"
            "HAVE FUN!\n\n"
            "PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n");
    }
    else if(mustShowTip(States::LevelSelection, mustShowFTTDeathTips))
    {
        showTip(
            "IF YOU FEEL LIKE THE GAME IS TOO HARD, TRY THESE TIPS\n\n"
            "- DECREMENT THE DIFFICULTY BY PRESSING LEFT\n"
            "- PLAY AN EASIER LEVEL TO BUILD UP MUSCLE MEMORY\n"
            "- TURN ON 'INVINCIBILITY' IN 'GAMEPLAY' OPTIONS TO PRACTICE\n\n"
            "REMEMBER THAT PRACTICE IS THE SECRET TO SUCCESS!\n\n"
            "PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n");
    }
}
```

(First-time tips fire once each: SMain welcome, then the first
LevelSelection entry (level-picking tip), then the *next* LevelSelection
entry (death tips). `dialogBoxDelay = 64.f` frames blocks closing.)

### 11.4 `initInput` (complete — all menu binds)

```cpp
    using k = ssvs::KKey;
    using t = ssvs::Input::Type;
    using Tid = Config::Tid;

    addTidInput(Tid::RotateCCW, t::Once, [this](ssvu::FT) { if(!mouseHovering) { leftAction(); } });
    addTidInput(Tid::RotateCW,  t::Once, [this](ssvu::FT) { if(!mouseHovering) { rightAction(); } });
    game.addInput({{k::Up}},    upAction,   t::Once);            // hardcoded
    addTidInput(Tid::Up,        t::Once, upAction);
    game.addInput({{k::Down}},  downAction, t::Once);            // hardcoded
    addTidInput(Tid::Down,      t::Once, downAction);
    addTidInput(Tid::NextPack,      t::Once, [this]{ changePackAction(1); });
    addTidInput(Tid::PreviousPack,  t::Once, [this]{ changePackAction(-1); });
    add2StateInput(game, Config::getTrigger(Tid::Focus), focusHeld, (int)Tid::Focus);
    game.addInput({{k::Enter}}, [this]{ okAction(); }, t::Once); // hardcoded
    // Escape Always: if(state != States::MOpts) exitTimer += mFT;
    // Escape Once:    exitAction();                                       // hardcoded
    addTidInput(Tid::Exit, t::Once, [this] { if(isEnteringText()) { return; } exitAction(); });
    addTidInput(Tid::Screenshot, t::Once, [this] { mustTakeScreenshot = true; });
    game.addInput({{k::LAlt, k::Enter}},
        [this] { Config::setFullscreen(window, !window.getFullscreen()); game.ignoreNextInputs(); },
        t::Once).setPriorityUser(-1000);
    game.addInput({{k::Backspace}}, eraseAction, t::Once);   // hardcoded
    game.addInput({{k::F1}}, addRemoveFavoriteLevel, t::Once);   // hardcoded
    game.addInput({{k::F2}}, switchToFromFavoriteLevels, t::Once); // hardcoded
    game.addInput({{k::F3}}, [this]{ reloadAssets(false); }, t::Once); // hardcoded (debug only)
    game.addInput({{k::F4}}, [this]{ reloadAssets(true); },  t::Once); // hardcoded (debug only)
```

(Rotate CCW/CW drive left/right; Up/Down are also Tid-binds on top of the
hardcoded arrows; `add2StateInput` maintains `focusHeld` from the Focus
trigger. F3/F4 are debug-only in behavior — `reloadAssets` checks
`Config::getDebug()`.)

### 11.5 `initLua` (menu Lua environment)

Static `CCustomWallManager cwManager`, `random_number_generator rng{0}` (fixed
seed 0 in menu), static `HexagonGameStatus hexagonGameStatus`.
`LuaScripting::init(lua, rng, true /* inMenu */, cwManager, levelStatus,
hexagonGameStatus, styleData, assets, runLuaFile-callback,
execScriptPackPathContext, packPath-getter (`levelData->packPath`),
PackData-getter (`*currentPack`), false /* headless */)` — full Lua API
available in menus (see gameplay.md §LuaScripting). Then:

- `u_log` → logs to `lua-menu`.
- `u_getDifficultyMult` → 1; `u_getSpeedMultDM` → 1; `u_getDelayMultDM` → 1;
  `u_getPlayerAngle` → 0.
- A long list of "unused functions" written as no-op `[] {}` closures:
  `u_isKeyPressed, u_isMouseButtonPressed, u_isFastSpinning,
  u_setPlayerAngle, u_forceIncrement, u_haltTime, u_timelineWait,
  u_clearWalls, u_setFlashEffect, a_setMusic, a_setMusicSegment,
  a_setMusicSeconds, a_playSound, a_playPackSound, a_syncMusicToDM,
  a_setMusicPitch, a_overrideBeepSound, a_overrideIncrementSound,
  a_overrideSwapSound, a_overrideDeathSound, t_eval, t_kill, t_clear, t_wait,
  t_waitS, t_waitUntilS, e_eval, e_kill, e_stopTime, e_stopTimeS, e_wait,
  e_waitS, e_waitUntilS, e_messageAdd, e_messageAddImportant,
  e_messageAddImportantSilent, e_clearMessages, ct_create, ct_eval, ct_kill,
  ct_stopTime, ct_stopTimeS, ct_wait, ct_waitS, ct_waitUntilS, l_overrideScore,
  l_setRotation, l_getRotation, l_getOfficial, s_setStyle, w_wall, w_wallAdj,
  w_wallAcc, w_wallHModSpeedData, w_wallHModCurveData,
  steam_unlockAchievement, u_kill, u_eventKill, u_playSound,
  u_playPackSound, u_setFlashEffect (again in list), u_setFlashColor,
  e_eventStopTime, e_eventStopTimeS, e_eventWait, e_eventWaitS,
  e_eventWaitUntilS, m_messageAdd, m_messageAddImportant,
  m_messageAddImportantSilent, m_clearMessages`.

### 11.6 All menus and items (verbatim names + exact callbacks)

**Welcome menu** (category `"welcome"`; state SLPSelectBoot? No — welcome is
used at boot, see §11.9 draw):

| item | type | action |
|---|---|---|
| `"play locally"` | Single | `playLocally()` (→ SMain) |
| `"exit game"` | Single | `window.stop()` |

**Options menu** — top category `"options"` + subcategories `"gameplay"`,
`"controls"`, `"keyboard"`, `"joystick"`, `"resolution"`, `"graphics"`,
`"audio"`, `"advanced"`:

options:

| item | type | action |
|---|---|---|
| `"GAMEPLAY"` | Goto | → gameplay |
| `"CONTROLS"` | Goto | → controls |
| `"RESOLUTION"` | Goto | → resolution |
| `"GRAPHICS"` | Goto | → gfx |
| `"AUDIO"` | Goto | → sfx |
| `"ADVANCED"` | Goto | → advanced |
| `"RESET CONFIG"` | Single | `Config::resetConfigToDefaults(); refreshBinds();` |

gameplay:

| item | type | params / visibility |
|---|---|---|
| `"autorestart"` | Toggle | getAutoRestart/setAutoRestart |
| `"rotate to start"` | Toggle | getRotateToStart |
| `"OFFICIAL MODE"` | Toggle | getOfficial |
| `"debug mode"` | Toggle | getDebug — visible when NOT official |
| `"invincible"` | Toggle | getInvincible — not official |
| `"timescale"` | Slider | getTimescale, range 0.1..2, step 0.05 — not official |
| `"save last username"` | Toggle | getSaveLastLoginUsername |
| `"show login at startup"` | Toggle | getShowLoginAtStartup |
| `"back"` | GoBack | |

controls:

| item | type | action |
|---|---|---|
| `"keyboard"` | Goto | |
| `"joystick"` | Goto | |
| `"joystick deadzone"` | Slider | getJoystickDeadzone, 0..100, step 1 |
| `"reset binds"` | Single | `Config::resetBindsToDefaults(); refreshBinds();` |
| `"hardcoded keys reference"` | Single | dialog (verbatim below) + `ignoreInputsAfterMenuExec()` |
| `"back"` | GoBack | |

hardcoded keys reference dialog (verbatim):

```
UP ARROW - UP
DOWN ARROW - DOWN
RETURN - ENTER
BACKSPACE - REMOVE BIND
F1 - ADD LEVEL TO FAVORITES
F2 - SWITCH TO/FROM FAVORITE LEVELS
F3 - RELOAD LEVEL ASSETS (DEBUG MODE ONLY)
F4 - RELOAD PACK ASSETS (DEBUG MODE ONLY)

PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE
```

keyboard (each a `KeyboardBindControl`; name, Tid, hardcoded-key default):

| item | Tid | hardcoded |
|---|---|---|
| `"rotate ccw"` | RotateCCW | — |
| `"rotate cw"` | RotateCW | — |
| `"focus"` | Focus | — |
| `"exit"` | Exit | Escape |
| `"force restart"` | ForceRestart | — |
| `"restart"` | Restart | — |
| `"replay"` | Replay | — |
| `"screenshot"` | Screenshot | — |
| `"swap"` | Swap | — |
| `"up"` | Up | Up |
| `"down"` | Down | Down |
| `"next pack"` | NextPack | — |
| `"previous pack"` | PreviousPack | — |
| `"lua console (debug only)"` | LuaConsole | — |
| `"pause game (debug only)"` | Pause | — |
| `"back"` | GoBack | |

Bind-control callback: `game.refreshTrigger(trig, bindID)` +
`fnHGTriggerRefresh(trig, bindID)` (the HexagonGame seam).

joystick (each a `JoystickBindControl`; Jid):

| item | Jid |
|---|---|
| `"select"` | Select |
| `"exit"` | Exit |
| `"focus"` | Focus |
| `"swap"` | Swap |
| `"force restart"` | ForceRestart |
| `"restart"` | Restart |
| `"replay"` | Replay |
| `"screenshot"` | Screenshot |
| `"next pack"` | NextPack |
| `"previous pack"` | PreviousPack |
| `"(un)favorite level"` | AddToFavorites |
| `"favorites menu"` | FavoritesMenu |
| `"back"` | GoBack |

joystick callback: `Joystick::setJoystickBind(button, buttonID)`.

resolution (categories `"16x9 resolutions"`, `"4x3 resolutions"`,
`"16x10 resolutions"` created on the optionsMenu; the ratio grouping uses
`sf::VideoMode::getFullscreenModes()`, only `bitsPerPixel == 32`, with
`ratio = 10.f * w / h` → `case 17:` = 16:9, `case 13:` = 4:3, `default:` →
the 16x10/uncommon list; each item name is `"<w>x<h>"`):

| item | type | action |
|---|---|---|
| `"automatically set resolution"` | Single | `Config::setCurrentResolutionAuto(window)` |
| `"16x9 resolutions"` | Goto | |
| `"4x3 resolutions"` | Goto | |
| `"16x10 - uncommon resolutions"` | Goto | |
| `<w>x<h>` items | Single | `changeResolutionTo(w, h)` |
| `"go windowed"` | Single | `Config::setFullscreen(window, false)` |
| `"go fullscreen"` | Single | `Config::setFullscreen(window, true)` |
| `"back"` | GoBack | |

gfx (graphics) top-level items:

| item | type | action |
|---|---|---|
| `"visual fxs"` | Goto | → visfx |
| `"player visual fxs"` | Goto | → playervisfx |
| `"fps settings"` | Goto | → fps |
| `"text outlines"` | Toggle | getDrawTextOutlines |
| `"text padding"` | Slider | 0..64, step 1 |
| `"text scaling"` | Slider | 0.1..4, step 0.05 |
| `"antialiasing"` | Slider (custom get/set) | get: `"<n>x"` string; right: 0→1, else `clamp(level << 1, 0, 16)`; left: `clamp(level >> 1, 0, 16)` |
| `"darken background chunk"` | Toggle | getDarkenUnevenBackgroundChunk |
| `"show key icons"` | Toggle | getShowKeyIcons |
| `"key icons scaling"` | Slider | 0.1..4, step 0.05 |
| `"show level info"` | Toggle | getShowLevelInfo |
| `"show timer"` | Toggle | getShowTimer — not official |
| `"show status text"` | Toggle | getShowStatusText — not official |
| `"back"` | GoBack | |

visual fxs:

| item | type |
|---|---|
| `"3D effects"` | Toggle get3D |
| `"shader effects"` | Toggle getShaders |
| `"no pulse"` | Toggle getNoPulse (not official) |
| `"no rotation"` | Toggle getNoRotation (not official) |
| `"no background"` | Toggle getNoBackground (not official) |
| `"b&w colors"` | Toggle getBlackAndWhite (not official) |
| `"flash"` | Toggle getFlash |
| `"shake mult."` | Slider getCameraShakeMultiplier 0..5 step 0.1 |

player visual fxs:

| item | type |
|---|---|
| `"angle tilt mult."` | Slider getAngleTiltIntensity 0..5 step 0.1 |
| `"show trail"` | Toggle getShowPlayerTrail |
| `"trail alpha"` | Slider getPlayerTrailAlpha 0..255 step 5 (uint) |
| `"trail scale"` | Slider getPlayerTrailScale 0.05..1 step 0.05 |
| `"trail decay"` | Slider getPlayerTrailDecay 0.5..50 step 2.5 |
| `"trail has swap color"` | Toggle getPlayerTrailHasSwapColor |
| `"show swap particles"` | Toggle getShowSwapParticles |
| `"swap blinking effect"` | Toggle getShowSwapBlinkingEffect |
| `"back"` | GoBack |

fps settings:

| item | type |
|---|---|
| `"vsync"` | Toggle getVsync (setVsync(window, …)) |
| `"limit fps"` | Toggle getLimitFPS (setLimitFPS(window, …)) |
| `"max fps"` | Slider getMaxFPS 30..1000 step 5 (uint, setMaxFPS(window, …)) |
| `"show fps"` | Toggle getShowFPS |
| `"back"` | GoBack |

audio:

| item | type |
|---|---|
| `"no sound"` | Toggle getNoSound |
| `"no music"` | Toggle getNoMusic |
| `"sound volume"` | Slider 0..100 step 5 (also `audio.setSoundVolume`) |
| `"music volume"` | Slider 0..100 step 5 (also `audio.setMusicVolume`) |
| `"sync music with difficulty"` | Toggle getMusicSpeedDMSync |
| `"music speed multiplier"` | Slider 0.7..1.3 step 0.05 |
| `"play swap ready blip sound"` | Toggle getPlaySwapReadySound |
| `"back"` | GoBack |

advanced:

| item | type |
|---|---|
| `"cache lua files"` | Toggle getUseLuaFileCache |
| `"clear lua file cache"` | Single `assets.getLuaFileCache().clear()` |
| `"disable game rendering"` | Toggle getDisableGameRendering |

**Main menu** — categories `"main"` + `"local profiles"` (on mainMenu):

| item | type | action |
|---|---|---|
| `"LEVEL SELECT"` | Single | if `firstLevelSelection`: `lvlSlct.packIdx = diffMultIdx = 0`, `lvlSlct.levelDataIds = &assets.getLevelIdsByPack(getNthSelectablePackInfo(0).id)`, `setIndex(0)`; `changeStateTo(LevelSelection)`; select.ogg |
| `"LOCAL PROFILES"` | Goto | → localProfiles |
| `"ONLINE"` | Single | `changeStateTo(MOnline)` |
| `"OPTIONS"` | Single | `changeStateTo(MOpts)` |
| `"EXIT"` | Single | `window.stop()` |

local profiles:

| item | type | action |
|---|---|---|
| `"CHOOSE PROFILE"` | Single | `changeStateTo(SLPSelect)` |
| `"NEW PROFILE"` | Single | `changeStateTo(ETLPNew)`; `enteredStr = ""`; select.ogg |
| `"BACK"` | GoBack | |

**Online menu** (category named `"options"` on onlineMenu; visibility
predicates on `hexagonClient.getState()`):

| item | type | visibility | action |
|---|---|---|---|
| `"CONNECT"` | Single | Disconnected or ConnectionError | `hexagonClient.connect()` |
| `"LOGIN"` | Single | Connected && hasRTKeys | open login dialog flow (§11.6.1) |
| `"REGISTER"` | Single | Connected && hasRTKeys | `dialogInputState = Registration_EnteringUsername`; `showInputDialogBoxNice("REGISTRATION", "USERNAME")`; ignore |
| `"LOGOUT"` | Single | LoggedIn/LoggedIn_Ready && hasRTKeys | `hexagonClient.tryLogoutFromServer()` |
| `"DISCONNECT"` | Single | Connected | `hexagonClient.disconnect()` |
| `"DELETE ACCOUNT"` | Single | Connected && hasRTKeys | `dialogInputState = DeleteAccount_EnteringPassword`; `showInputDialogBoxNice("DELETE ACCOUNT", "PASSWORD", "WARNING: THIS WILL DELETE ALL YOUR SCORES")`; password mode; ignore |

**Profile selection menu** (category `"profile selection"`):

- One `i::Single` per local profile name (from `assets.getLocalProfileNames()`),
  action: `assets.pSetCurrent(p); changeFavoriteLevelsToProfile();`
- `profileSelection.sortByName()` at the end.

(`ignoreInputsAfterMenuExec()` = `setIgnoreAllInputs(mustUseMenuItem.has_value() ? 1 : 2)` —
mouse-driven menu exec ignores 1 press, keyboard 2.)

### 11.7 State machine (state → what is drawn → inputs → transitions)

| state | drawn (see §11.9+) | inputs accepted | transitions |
|---|---|---|---|
| `ETLPNewBoot` | entering-text screen over main-menu bg (drawEnteringTextBoot, §11.15) | TextEntered→enteredChars; Backspace (eraseAction); any key/mouse/joypad release → `checkCloseBootScreens`-style: on okAction with non-empty `enteredStr` create profile → SMain (openHexagon.ogg). Exit (exitAction) → SMain | → SMain (ok with name) |
| `LoadingScreen` | load results list + scrolling hexagon (drawLoadResults, §11.16) | Up/Down scroll `scrollbarOffset` (0 .. errors.size()−`maxErrorsOnScreen`); any release → `checkCloseBootScreens` → EpilepsyWarning | → EpilepsyWarning |
| `EpilepsyWarning` | epilepsyWarning sprite (drawGraphics, §11.14) | any release → `checkCloseBootScreens` → SMain (via playLocally; main category offset 400/280; item[0] offset 100) | → SMain |
| `SLPSelectBoot` | profile selection screen (drawProfileSelectionBoot, §11.13) | Up/Down navigate profileSelectionMenu (with scrollbarOffset, `maxProfilesOnScreen` 6); Enter/ok → exec selected + SMain (openHexagon.ogg); Left/Right → okAction (leftRightActionImpl short-circuit); Esc → ETLPNewBoot | → SMain, → ETLPNewBoot |
| `SMain` | main menu (drawMainMenu, §11.11) + title bar + version + credits + online status | Up/Down navigate mainMenu; Left/Right = slider adjustment (modifier ×2); Enter okAction; Esc: exitTimer accumulates (state != MOpts) → exitAction (no-op in "main" category); mouse click (mustUseMenuItem); wheel up/down | → LevelSelection ("LEVEL SELECT"), → MOpts, → MOnline, → SLPSelect (local profiles/CHOOSE PROFILE), → ETLPNew (NEW PROFILE), exit (window.stop) |
| `LevelSelection` | level selection L+R sides (drawLevelSelectionLeftSide/RightSide, §11.17) + bg | Up/Down move levels/packs; Left/Right change `diffMultIdx` (difficultyBumpEffect=24, difficultyMultDown/Up.ogg); wheel: focusHeld→changePackQuick else YScrollTo += delta*48 (clamp 0; < −4000 → achievement `a35_eagerformore` + error.ogg + reset); F1 favorite, F2 favorites list, F3/F4 reload (debug); Enter okAction → playSelectedLevel; Esc → SMain; joystick edges (NextPack/PreviousPack/Favorites/Select/Exit/Screenshot, dirs) | → SMain (Esc), → play level (fnHGNewGame) |
| `MOpts` | options menu (drawOptionsSubmenus, §11.12) | Up/Down nav; Left/Right slider adjust; Enter exec (bind items start bind-capture, ignoreInputs=2); Backspace erases bind; Esc exitAction (goBack); wheel nav (blocked while binding) | Goto subcategories, GoBack, "RESET CONFIG" etc.; exit → SMain |
| `MOnline` | online menu (drawMainSubmenus over the online category, §11.11) | same as MOpts; HexagonClient events → dialog boxes (update) | exec items (CONNECT/LOGIN/REGISTER/LOGOUT/DISCONNECT/DELETE ACCOUNT); exit → SMain |
| `SLPSelect` | profile selection (drawProfileSelection, §11.13) | Up/Down nav (scrollbarOffset); Enter exec profile; Backspace deletes profile (with guards, §11.8 eraseAction); Esc exit | → exec profile (stay, pSetCurrent), delete item |
| `ETLPNew` | entering-text screen (drawEnteringText, §11.15) | TextEntered; Backspace; Enter okAction (create profile → SMain); Esc → SMain | → SMain |

Boot flow: `LoadingScreen` (any key) → `EpilepsyWarning` (any key) →
`playLocally()`: `assets.pSaveCurrent()`, `enteredStr = ""`, state =
`getLocalProfilesSize() == 0 ? ETLPNewBoot : SLPSelectBoot`; ok there → SMain.

`exitAction()` (full semantics):

```cpp
void MenuGame::exitAction()
{
    if(isInMenu() && getCurrentMenu()->getCategory().getName() == "main")
    {
        return;
    }

    playSoundOverride("beep.ogg");

    if(state == States::SLPSelectBoot)
    {
        changeStateTo(States::ETLPNewBoot);
        return;
    }

    if(state == States::LevelSelection)
    {
        changeStateTo(States::SMain);
        resetNamesScrolls();
        adjustMenuOffset(false);
        return;
    }

    if(assets.pIsValidLocalProfile())
    {
        if(isInMenu())
        {
            if(getCurrentMenu()->canGoBack())
            {
                getCurrentMenu()->goBack();
            }
            else
            {
                changeStateTo(States::SMain);
            }
            adjustMenuOffset(false);
        }
        else if(isEnteringText())
        {
            changeStateTo(States::SMain);
        }
    }
}
```

(Esc in SMain's "main" category does nothing — the main menu is a root.
Also note the hardcoded Escape `t::Always` handler increments `exitTimer`
when `state != MOpts`; `window.stop()` when `exitTimer > 20` — a 20-frame
(scaled FT) hold-to-exit, only effective while on the main menu category... in
practice it accumulates in every non-MOpts state.)

### 11.8 Navigation actions (leftRight/up/down/ok/erase + pack change)

Constants: `inline constexpr int maxProfilesOnScreen{6};` (in upAction),
`inline constexpr int maxErrorsOnScreen{7};` (in downAction),
`maxOffset{100.f}` (file-level).

`leftRightActionImpl(bool left)` (full, verbatim):

```cpp
void MenuGame::leftRightActionImpl(bool left)
{
    if(state == States::SLPSelectBoot)
    {
        okAction();
        return;
    }

    // Change difficulty in the level selection menu.
    if(state == States::LevelSelection)
    {
        if(left)
        {
            --diffMultIdx;
            playSoundOverride("difficultyMultDown.ogg");
        }
        else
        {
            ++diffMultIdx;
            playSoundOverride("difficultyMultUp.ogg");
        }

        difficultyBumpEffect = difficultyBumpEffectMax;
        touchDelay = 50.f;
        return;
    }

    // If there is no valid action abort.
    if(!isInMenu() || !getCurrentMenu()->getItem().canIncrease())
    {
        return;
    }

    const bool modifier = (sf::Keyboard::isKeyPressed(sf::Keyboard::LShift) ||
                           sf::Keyboard::isKeyPressed(sf::Keyboard::RShift) ||
                           focusHeld || wasFocusHeld);

    for(int i{0}; i < (modifier ? 2 : 1); ++i)
    {
        if(left)
        {
            getCurrentMenu()->decrease();
        }
        else
        {
            getCurrentMenu()->increase();
        }
    }

    playSoundOverride("beep.ogg");
    touchDelay = 50.f;
}
```

(Note: difficulty is NOT clamped here — clamping happens in the drawing /
play path via `levelData->getNthDiffMult(diffMultIdx)`; `diffMultIdx` is reset
to the index of the 1.0x mult by `setIndex`.)

`upAction` / `downAction` (verbatim, with the LevelSelection, LoadingScreen
and menu branches):

```cpp
void MenuGame::upAction()
{
    if(state == States::LevelSelection)
    {
        if(packChangeState != PackChange::Rest) { return; }

        if(focusHeld)
        {
            changePackQuick(-1);
            return;
        }

        const int prevIdx{lvlDrawer->currentIndex - 1};

        if(getSelectablePackInfosSize() == 1)
        {
            if(prevIdx < 0)
            {
                setIndex(lvlDrawer->levelDataIds->size() - 1);
                calcScrollSpeed();

                const float scroll{
                    packLabelHeight +
                    levelLabelHeight * (lvlDrawer->currentIndex + 1) +
                    2.f * slctFrameSize};
                if(scroll > h - lvlDrawer->YOffset)
                {
                    lvlDrawer->YScrollTo = h - scroll;
                }
            }
            else
            {
                setIndex(prevIdx);
                calcLevelChangeScroll(-2);
            }
        }
        else if(prevIdx < 0)
        {
            // -2 means "go to previous pack and
            // skip to the last level of the list"
            changePackAction(-2);
        }
        else
        {
            setIndex(prevIdx);
            calcLevelChangeScroll(-2);
        }

        resetLevelNamesScrolls();
        playSoundOverride("beep.ogg");
        touchDelay = 50.f;
        return;
    }

    if(state == States::LoadingScreen)
    {
        if(scrollbarOffset != 0)
        {
            --scrollbarOffset;
            playSoundOverride("beep.ogg");
            touchDelay = 50.f;
        }
        return;
    }

    if(!isInMenu()) { return; }

    if((state == States::SLPSelect || state == States::SLPSelectBoot) &&
        getCurrentMenu()->getIdx() - 1 < scrollbarOffset)
    {
        const int index = ssvu::getMod(getCurrentMenu()->getIdx() - 1, 0,
            static_cast<int>(getCurrentMenu()->getItems().size()));
        scrollbarOffset = std::max(index - (maxProfilesOnScreen - 1), 0);
    }

    if(getCurrentMenu() != nullptr && !anyItemEnabled(*getCurrentMenu()))
    {
        return;
    }

    do { getCurrentMenu()->previous(); }
    while(!getCurrentMenu()->getItem().isEnabled());

    playSoundOverride("beep.ogg");
    touchDelay = 50.f;
}

void MenuGame::downAction()
{
    if(state == States::LevelSelection)
    {
        if(packChangeState != PackChange::Rest) { return; }

        if(focusHeld)
        {
            changePackQuick(1);
            return;
        }

        const int nextIdx{lvlDrawer->currentIndex + 1};
        if(getSelectablePackInfosSize() == 1)
        {
            if(nextIdx > ssvu::toInt(lvlDrawer->levelDataIds->size() - 1))
            {
                // Skip to the first level label.
                setIndex(0);
                calcScrollSpeed();
                lvlDrawer->YScrollTo = 0.f;
            }
            else
            {
                setIndex(nextIdx);
                calcLevelChangeScroll(2);
            }
        }
        else if(nextIdx > ssvu::toInt(lvlDrawer->levelDataIds->size() - 1))
        {
            // Go to the next pack.
            changePackAction(1);
        }
        else
        {
            setIndex(nextIdx);
            calcLevelChangeScroll(2);
        }

        resetLevelNamesScrolls();
        playSoundOverride("beep.ogg");
        touchDelay = 50.f;
        return;
    }

    if(state == States::LoadingScreen)
    {
        if(scrollbarOffset <
            static_cast<int>(loadInfo.errorMessages.size()) - maxErrorsOnScreen)
        {
            ++scrollbarOffset;
            playSoundOverride("beep.ogg");
            touchDelay = 50.f;
        }
        return;
    }

    if(!isInMenu()) { return; }

    if((state == States::SLPSelect || state == States::SLPSelectBoot) &&
        getCurrentMenu()->getIdx() + 1 >
            maxProfilesOnScreen - 1 + scrollbarOffset)
    {
        const int index = ssvu::getMod(getCurrentMenu()->getIdx() + 1, 0,
            static_cast<int>(getCurrentMenu()->getItems().size()));
        scrollbarOffset = std::max(index - (maxProfilesOnScreen - 1), 0);
    }

    if(getCurrentMenu() != nullptr && !anyItemEnabled(*getCurrentMenu()))
    {
        return;
    }

    do { getCurrentMenu()->next(); }
    while(!getCurrentMenu()->getItem().isEnabled());

    playSoundOverride("beep.ogg");
    touchDelay = 50.f;
}
```

Pack-change functions (verbatim):

```cpp
void MenuGame::changePackTo(const int idx)
{
    const auto& p{assets.getSelectablePackInfos()};

    lvlSlct.packIdx = ssvu::getMod(idx, 0, static_cast<int>(p.size()));

    lvlSlct.levelDataIds =
        &assets.getLevelIdsByPack(p.at(lvlDrawer->packIdx).id);

    setIndex(0);

    resetNamesScrolls();
}

void MenuGame::changePack()
{
    const auto& p{assets.getSelectablePackInfos()};

    lvlSlct.packIdx =
        ssvu::getMod(lvlDrawer->packIdx + (packChangeDirection > 0 ? 1 : -1), 0,
            static_cast<int>(p.size()));

    lvlSlct.levelDataIds =
        &assets.getLevelIdsByPack(p.at(lvlDrawer->packIdx).id);

    setIndex(packChangeDirection == -2 ? lvlSlct.levelDataIds->size() - 1 : 0);

    resetNamesScrolls();
}

void MenuGame::changePackQuick(const int direction)
{
    if(isFavoriteLevels()) { return; }

    packChangeDirection = direction;
    playSoundOverride("beep.ogg");
    changePack();
    adjustLevelsOffset();

    // YOffset is 0 when the first pack is shown and gets lower
    // the further down we have to scroll.

    // Height of the top of the pack label that is one index before the current
    // one.
    float scroll{packLabelHeight * (lvlDrawer->packIdx - 1)};

    std::function<void(const float)> action{[this](const float target)
        { lvlDrawer->YScrollTo = lvlDrawer->YOffset = target; }};

    if(!checkWindowTopScrollWithResult(scroll, action))
    {
        // Height of the bottom of the pack label that is one index after the
        // current one.
        scroll = packLabelHeight *
                     std::min(lvlDrawer->packIdx + 2,
                         static_cast<int>(getSelectablePackInfosSize())) +
                 levelLabelHeight + 3.f * slctFrameSize;

        checkWindowBottomScroll(scroll, action);
    }
}

void MenuGame::changePackAction(const int direction)
{
    if(state != States::LevelSelection || getSelectablePackInfosSize() == 1 ||
        packChangeState != PackChange::Rest)
    {
        return;
    }

    lvlDrawer->YScrollTo = lvlDrawer->YOffset; // stop scrolling for safety
    packChangeState = PackChange::Folding;
    packChangeDirection = direction;
    calcScrollSpeed();

    touchDelay = 50.f;
    playSoundOverride("beep.ogg");
}
```

`okAction` (verbatim, full switch — see §11.7 for the per-state summary):

```cpp
void MenuGame::okAction()
{
    touchDelay = 50.f;

    switch(state)
    {
        case States::ETLPNewBoot: [[fallthrough]];
        case States::ETLPNew:
        {
            if(!enteredStr.empty())
            {
                ssvms::Category& profiles(
                    profileSelectionMenu.getCategoryByName(
                        "profile selection"));

                // Abort if user is trying to create a profile
                // with a name already in use
                for(auto& i : profiles.getItems())
                {
                    if(enteredStr == i->getName())
                    {
                        playSoundOverride("error.ogg");
                        showDialogBox(
                            "A PROFILE WITH THE SAME NAME ALREADY EXISTS\n"
                            "PLEASE ENTER ANOTHER NAME\n\n"
                            "PRESS ANY KEY OR BUTTON TO CLOSE THIS "
                            "MESSAGE\n");
                        setIgnoreAllInputs(2);
                        return;
                    }
                }

                // All good
                assets.pCreate(enteredStr);
                assets.pSetCurrent(enteredStr);
                changeFavoriteLevelsToProfile();

                // Create new menu item
                profiles.create<ssvms::Items::Single>(enteredStr,
                    [this, name = enteredStr]
                    {
                        assets.pSetCurrent(name);
                        changeFavoriteLevelsToProfile();
                    });

                profiles.sortByName();

                enteredStr = "";
                if(state == States::ETLPNewBoot)
                {
                    playSoundOverride("openHexagon.ogg");
                    changeStateTo(States::SMain);
                    return;
                }
                changeStateTo(States::SMain);
            }

            break;
        }

        case States::SLPSelectBoot:
        {
            playSoundOverride("openHexagon.ogg");
            getCurrentMenu()->exec();
            changeStateTo(States::SMain);
            return;
        }

        case States::SMain:
        {
            const std::string& category{
                getCurrentMenu()->getCategory().getName()};
            getCurrentMenu()->exec();

            if(state == States::LevelSelection)
            {
                setIndex(lvlDrawer->currentIndex);
                adjustLevelsOffset();
                return;
            }

            if(getCurrentMenu() == nullptr) { return; }

            if(scrollToEnabledMenuItem(getCurrentMenu()) == false)
            {
                return;
            }

            if(getCurrentMenu()->getCategory().getName() != category)
            {
                adjustMenuOffset(true);
            }

            break;
        }

        case States::MOpts:
        {
            getCurrentMenu()->exec();

            const auto* const bc{
                dynamic_cast<BindControlBase*>(&getCurrentMenu()->getItem())};
            if(bc != nullptr && bc->isWaitingForBind())
            {
                setIgnoreAllInputs(2);
                touchDelay = 10.f;
                playSoundOverride("beep.ogg");
                return;
            }

            if(scrollToEnabledMenuItem(getCurrentMenu()) == false)
            {
                return;
            }

            break;
        }

        case States::MOnline:
        {
            getCurrentMenu()->exec();

            if(scrollToEnabledMenuItem(getCurrentMenu()) == false)
            {
                return;
            }

            break;
        }

        case States::LevelSelection:
        {
            resetNamesScrolls();
            playSelectedLevel();

            break;
        }

        default:
        {
            if(isInMenu()) { getCurrentMenu()->exec(); }

            break;
        }
    }

    playSoundOverride("select.ogg");
}
```

`playSelectedLevel`:

```cpp
void MenuGame::playSelectedLevel()
{
    if(fnHGNewGame)
    {
        setMouseCursorVisible(false);

        fnHGNewGame(                                              //
            getNthSelectablePackInfo(lvlDrawer->packIdx).id,      //
            lvlDrawer->levelDataIds->at(lvlDrawer->currentIndex), //
            true /* firstPlay */,                                 //
            levelData->getNthDiffMult(diffMultIdx),               //
            false /* executeLastReplay */                         //
        );
    }
}
```

`eraseAction` (Backspace) — verbatim branches:

- `isEnteringText() && !enteredStr.empty()` → pop last char + beep.
- `SLPSelect`: name = selected item; guards: `items.size() <= 1` → error.ogg +
  dialog `"YOU CANNOT ERASE THE ONLY REMAINING PROFILE\n\nPRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n"` + ignore 2;
  `assets.pGetName() == name` → error.ogg + dialog
  `"YOU CANNOT ERASE THE CURRENTLY IN USE PROFILE\n\nPRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n"` +
  ignore 2; else remove file `"Profiles/" + name + ".json"` (log error if
  missing), `assets.pRemove(name)`, remove menu item, beep.
- `MOpts && isInMenu()`: if item is a `BindControlBase` and `bc->erase()` →
  beep; `touchDelay = 10.f`.

### 11.9 `update` (menu game loop; verbatim structure)

```cpp
void MenuGame::update(ssvu::FT mFT)
{
    hexagonClient.update();

    const auto showHCEventDialogBox = [this](const bool error,
                                          const std::string& msg,
                                          const std::string& err = "")
    {
        if(!dialogBox.empty()) { return; }

        if(state != States::SMain && state != States::MOnline &&
            state != States::MOpts && state != States::SLPSelect &&
            state != States::LevelSelection)
        {
            return;
        }

        if(error) { playSoundOverride("error.ogg"); }
        else      { playSoundOverride("select.ogg"); }

        strBuf.clear();
        strBuf += msg;
        if(!err.empty()) { strBuf += "\n\n"; strBuf += err; }
        strBuf += "\n";

        dialogBoxDelay = 16.f;   // prevent immediate close

        showDialogBox(strBuf);
        setIgnoreAllInputs(1);
    };

    std::optional<HexagonClient::Event> hcEvent;
    while((hcEvent = hexagonClient.pollEvent()).has_value())
    {
        Utils::match(*hcEvent,
            EConnectionSuccess  → showHCEventDialogBox(false, "CONNECTION SUCCESS"),
            EConnectionFailure  → showHCEventDialogBox(true, "CONNECTION FAILURE", e.error),
            EKicked             → showHCEventDialogBox(true, "DISCONNECTED FROM SERVER"),
            ERegistrationSuccess→ showHCEventDialogBox(false, "REGISTRATION SUCCESS"),
            ERegistrationFailure→ showHCEventDialogBox(true, "REGISTRATION FAILURE", e.error),
            ELoginSuccess       → showHCEventDialogBox(false, "LOGIN SUCCESS") + steamManager.unlock_achievement("a23_login"),
            ELoginFailure       → showHCEventDialogBox(true, "LOGIN FAILURE", e.error),
            ELogoutSuccess      → showHCEventDialogBox(false, "LOGOUT SUCCESS"),
            ELogoutFailure      → showHCEventDialogBox(true, "LOGOUT FAILURE"),
            EDeleteAccountSuccess→ showHCEventDialogBox(false, "DELETE ACCOUNT SUCCESS"),
            EDeleteAccountFailure→ showHCEventDialogBox(true, "DELETE ACCOUNT FAILURE", e.error),
            EReceivedTopScores  → leaderboardCache->receivedScores(e.levelValidator, e.scores),
            EReceivedOwnScore   → leaderboardCache->receivedOwnScore(e.levelValidator, e.score),
            EGameVersionMismatch→ log "Client/server game version mismatch, likely not a problem\n",
            EProtocolVersionMismatch → showHCEventDialogBox(true, "CLIENT/SERVER PROTOCOL VERSION MISMATCH"));
    }

    if(fnHGUpdateRichPresenceCallbacks) { fnHGUpdateRichPresenceCallbacks(); }

    Joystick::update(Config::getJoystickDeadzone());

    // Focus handling for quick pack switching (LevelSelection, non-favorites,
    // Rest state): focusHeld mirrored from Joystick::pressed(Focus); on
    // rising/falling edge (vs wasFocusHeld): if currentIndex != 0 setIndex(0)
    // (rising only), quickPackFoldStretch(); track wasFocusHeld.
    // Else wasFocusHeld = focusHeld = false.

    // Mouse-driven menu exec (SMain/MOpts/MOnline/SLPSelect): if
    // mustUseMenuItem.has_value() and item index valid+enabled → beep + exec;
    // reset mustUseMenuItem.

    // LevelSelection mouse actions (Rest state): mustFavorite →
    // addRemoveFavoriteLevel; mustPlay → playSelectedLevel;
    // mustChangeIndexTo → beep + setIndex; mustChangePackIndexTo → beep +
    // changePackTo.

    // Joystick rising edges:
    //   NextPack → changePackAction(1); PreviousPack → changePackAction(-1);
    //   AddToFavorites → addRemoveFavoriteLevel();
    //   FavoritesMenu → switchToFromFavoriteLevels();
    //   Jdir Left/Right/Up/Down → left/right/up/downAction();
    //   Select → okAction(); Exit → exitAction();
    //   Screenshot → mustTakeScreenshot = true;

    if(touchDelay > 0.f) { touchDelay -= mFT; }
    if(dialogBoxDelay > 0.f) { dialogBoxDelay -= mFT; }
    if(difficultyBumpEffect > 0.f) { difficultyBumpEffect -= mFT * 2.f; }

    // Touch controls: single finger:
    //   y in (h/3, 2h/3): x in (0, w/3) → leftAction; x in (2w/3, w) →
    //   rightAction; x in (w/3, 2w/3) → okAction;
    //   y < h/3 → upAction; y > 2h/3 → downAction.

    overlayCamera.update(mFT);
    backgroundCamera.update(mFT);

    if(getCurrentMenu() != nullptr)
    {
        getCurrentMenu()->update();
    }

    currentCreditsId += mFT;
    creditsBar2.setTexture(assets.getTexture(
        ssvu::getByModIdx(creditsIds, ssvu::toInt(currentCreditsId / 100))));

    if(exitTimer > 20)
    {
        window.stop();
    }

    styleData.update(mFT);
    backgroundCamera.turn(levelStatus.rotationSpeed * 10.f);

    if(isEnteringText())
    {
        constexpr unsigned int limit{18u};
        for(auto& c : enteredChars)
        {
            if(enteredStr.size() < limit &&
                (ssvu::isAlphanumeric(c) || ssvu::isPunctuation(c)))
            {
                playSoundOverride("beep.ogg");
                enteredStr.append(ssvu::toStr(c));
            }
        }
    }
    enteredChars.clear();

    switch(state)
    {
        case States::LoadingScreen: hexagonRotation += mFT / 100.f; break;

        case States::LevelSelection:
        {
            switch(packChangeState)
            {
                case PackChange::Rest: scrollLevelListToTargetY(mFT); break;

                case PackChange::Folding:
                {
                    const float listHeight{getLevelListHeight()};
                    packChangeOffset = std::min(
                        packChangeOffset + mFT * scrollSpeed, listHeight);
                    calcPackChangeScrollFold(listHeight);
                    if(packChangeOffset < listHeight) { break; }

                    changePack();

                    packChangeOffset = getLevelListHeight();
                    calcScrollSpeed();
                    packChangeState = PackChange::Stretching;
                }
                break;

                case PackChange::Stretching:
                    packChangeOffset =
                        std::max(packChangeOffset - mFT * scrollSpeed, 0.f);
                    calcPackChangeScrollStretch(getLevelListHeight());
                    if(packChangeOffset > 0.f) { break; }

                    packChangeOffset = 0.f;
                    adjustLevelsOffset();
                    packChangeState = PackChange::Rest;
                    break;
            }

            lvlDrawer->YOffset = std::min(lvlDrawer->YOffset, 0.f);
        }
        break;

        default: break;
    }
}
```

Menu camera notes:
- Both menu cameras get `update(mFT)` per frame (easing of queued pans/zooms).
- `backgroundCamera.turn(levelStatus.rotationSpeed * 10.f)` every frame —
  the menu background spins with the level's rotation speed (levelStatus
  defaults: see 04-config/LevelStatus mapping — `rotationSpeed` defaults 0;
  menus set it via Lua `l_setRotation` in the level's onInit when run by
  `setIndex`).
- `styleData.update(mFT)` with default mult 1 (no DM scaling in menus).
- credits bar texture cycles every 100·FT through `creditsIds`
  (creditsBar2 → 2b → 2c).
- `exitTimer > 20` → `window.stop()` (FT units; escape-hold timer from the
  Escape Always handler).

### 11.10 `setIndex` (level selection selection; verbatim)

```cpp
void MenuGame::setIndex(const int mIdx)
{
    lvlDrawer->currentIndex = mIdx;

    const std::string levelID{
        lvlDrawer->levelDataIds->at(lvlDrawer->currentIndex)};

    levelData = &assets.getLevelData(levelID);
    currentPack = &assets.getPackData(levelData->packId);

    formatLevelDescription();

    styleData = assets.getStyleData(levelData->packId, levelData->styleId);
    styleData.computeColors();

    // If we are in the favorite menu we must find the packId relative
    // to the selected level.
    if(isFavoriteLevels())
    {
        isLevelFavorite = true;

        const auto& p{assets.getSelectablePackInfos()};

        for(int i{0}; i < static_cast<int>(p.size()); ++i)
        {
            if(levelData->packId == p.at(i).id)
            {
                lvlDrawer->packIdx = i;
                break;
            }
        }
    }
    else if(!firstLevelSelection)
    {
        isLevelFavorite =
            assets.getCurrentLocalProfile().isLevelFavorite(levelID);
    }

    // Set the colors of the menus
    auto& colors{styleData.getColors()};
    menuQuadColor = Config::getBlackAndWhite() ? sf::Color(20, 20, 20, 255)
                                               : styleData.getTextColor();
    if(ssvu::toInt(menuQuadColor.a) == 0 && !Config::getBlackAndWhite())
    {
        for(auto& c : colors)
        {
            if(ssvu::toInt(c.a) != 0)
            {
                menuQuadColor = c;
                break;
            }
        }
    }
    menuTextColor = Config::getBlackAndWhite() ? sf::Color::White : colors[0];

    dialogBoxTextColor = menuQuadColor;
    dialogBoxTextColor.a = 255;

    // If there is only one color set the remaining colors
    // to be alpha variants of the ones we have already set.
    if(colors.size() == 1)
    {
        menuTextColor.a = 255;
        menuSelectionColor = menuQuadColor;
        menuSelectionColor.a = 75;
    }
    else
    {
        if(ssvu::toInt(menuTextColor.a) == 0 || menuTextColor == menuQuadColor)
        {
            for(auto& c : colors)
            {
                if(ssvu::toInt(c.a) != 0 && c != menuQuadColor &&
                    !Config::getBlackAndWhite())
                {
                    menuTextColor = c;
                    break;
                }
            }
        }

        menuSelectionColor =
            Config::getBlackAndWhite() ? sf::Color::White : colors[1];
        if(ssvu::toInt(menuSelectionColor.a) == 0 ||
            menuSelectionColor == menuQuadColor ||
            menuSelectionColor == menuTextColor)
        {
            for(auto& c : colors)
            {
                if(ssvu::toInt(c.a) != 0 && c != menuQuadColor &&
                    c != menuTextColor && !Config::getBlackAndWhite())
                {
                    menuSelectionColor = c;
                    break;
                }
            }
        }
        menuSelectionColor.a = 175;
    }

    // Set color of the fonts.
    txtSelectionBig.font.setFillColor(menuQuadColor);
    txtSelectionSmall.font.setFillColor(menuTextColor);
    txtSelectionRanked.font.setFillColor(menuTextColor);
    txtInstructionsSmall.font.setFillColor(menuTextColor);
    txtSelectionScore.font.setFillColor(menuTextColor);

    // Set gameplay values
    diffMultIdx = 0;
    for(; levelData->difficultyMults.at(diffMultIdx) != 1.f; ++diffMultIdx)
    {
    }

    try
    {
        runLuaFile(levelData->luaScriptPath);
        Utils::runLuaFunctionIfExists<void>(lua, "onInit");
        Utils::runLuaFunctionIfExists<void>(lua, "onLoad");
    }
    catch(std::runtime_error& mError)
    {
        std::cout << "[MenuGame::init] Runtime Lua error on menu "
                     "(loadFile/onInit/onLoad) with level \"" << levelData->name
                  << "\": \n" << mError.what() << '\n' << std::endl;

        if(!Config::getDebug()) { playSoundOverride("error.ogg"); }
    }
    catch(...)
    {
        std::cout << "[MenuGame::init] Unknown runtime Lua error on menu "
                     "(loadFile/onInit/onLoad) with level \"" << levelData->name
                  << "\"\n" << std::endl;

        if(!Config::getDebug()) { playSoundOverride("error.ogg"); }
    }
}
```

**Menu color derivation (exact):** from the selected level's style colors
`colors = styleData.getColors()`: `menuQuadColor` = B&W ? (20,20,20,255) :
textColor (first non-alpha color if alpha==0); `menuTextColor` = B&W ? White
: colors[0] (swap to first usable different color if alpha 0 or == quad);
`menuSelectionColor` = B&W ? White : colors[1] (or first usable different
color), final `a = 175` (with one color: quad with `a = 75`; textColor forced
`a = 255`); `dialogBoxTextColor` = menuQuadColor with `a = 255`.
`diffMultIdx` = index of the first `1.f` in `difficultyMults` (search loop —
assumes 1x exists). The level's Lua script is executed (`runLuaFile` +
onInit + onLoad) in the menu Lua env — that's how the menu background style
(pulse/hue) is set.

### 11.11 `reloadAssets` + `refreshCamera` (menu camera, layout anchors)

`reloadAssets(reloadEntirePack)`: only in LevelSelection, no dialog box, and
`Config::getDebug()`; `assets.reloadAllShaders()`; reload pack or level (log
string); `setIndex(currentIndex)`; dialog shows uppercased log +
`"\nPRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n"` with
`setIgnoreAllInputs(2)` (comment: dialog reacts to key *releases*; the first
release is the key that popped it up).

`refreshCamera()` — **the master layout function** (verbatim):

```cpp
void MenuGame::refreshCamera()
{
    const float fw{1024.f / getWindowWidth()};
    const float fh{768.f / getWindowHeight()};
    const float fmax{std::max(fw, fh)};

    w = getWindowWidth() * fmax;
    h = getWindowHeight() * fmax;

    backgroundCamera.setView(
        {ssvs::zeroVec2f, {Config::getSizeX() * Config::getZoomFactor(),
                              Config::getSizeY() * Config::getZoomFactor()}});

    overlayCamera.setView(sf::View{sf::FloatRect({0, 0}, {w, h})});

    titleBar.setOrigin(ssvs::zeroVec2f);
    titleBar.setScale({0.5f, 0.5f});
    titleBar.setPosition({20.f, 20.f});

    txtVersion.font.setString(GAME_VERSION_STR);
    txtVersion.font.setOrigin({ssvs::getLocalRight(txtVersion.font), 0.f});
    txtVersion.font.setPosition({ssvs::getGlobalRight(titleBar) - 15.f,
        ssvs::getGlobalTop(titleBar) + 15.f});

    creditsBar1.setOrigin({ssvs::getLocalWidth(creditsBar1), 0.f});
    creditsBar1.setScale({0.373f, 0.373f});
    creditsBar1.setPosition({w - 20.f, 20.f});

    creditsBar2.setOrigin({ssvs::getLocalWidth(creditsBar2), 0});
    creditsBar2.setScale({0.373f, 0.373f});
    creditsBar2.setPosition(
        {w - 20.f, 17.f + ssvs::getGlobalBottom(creditsBar1)});

    const float scaleFactor{w / 1024.f};
    epilepsyWarning.setOrigin(ssvs::getLocalCenter(epilepsyWarning));
    epilepsyWarning.setPosition({1024 / (2.f / scaleFactor), 768 / 2.f - 50});
    epilepsyWarning.setScale({0.36f, 0.36f});

    // Readjust the menu background skew and the indents
    fourByThree = 10.f * getWindowWidth() / getWindowHeight() < 16;

    if(fourByThree)
    {
        backgroundCamera.setSkew({1.f, 0.8f});
    }
    else
    {
        backgroundCamera.setSkew({1.f, 0.6f});
    }

    for(auto& c : mainMenu.getCategories())        { c->getOffset() = 0.f; }
    for(auto& c : welcomeMenu.getCategories())     { c->getOffset() = 0.f; }
    for(auto& c : optionsMenu.getCategories())     { c->getOffset() = 0.f; }
    for(auto& c : onlineMenu.getCategories())      { c->getOffset() = 0.f; }
    for(auto& c : profileSelectionMenu.getCategories()) { c->getOffset() = 0.f; }

    // Update the height infos of the fonts.
    if(fourByThree)
    {
        txtMenuBig.font.setCharacterSize(26);
        txtMenuTiny.font.setCharacterSize(9);
        txtMenuSmall.font.setCharacterSize(16);

        txtSelectionBig.font.setCharacterSize(24);
        txtSelectionSmall.font.setCharacterSize(14);
        txtSelectionRanked.font.setCharacterSize(10);

        txtLoadBig.font.setCharacterSize(56);
        txtLoadSmall.font.setCharacterSize(16);

        txtRandomTip.font.setCharacterSize(24);
    }
    else
    {
        txtMenuBig.font.setCharacterSize(36);
        txtMenuTiny.font.setCharacterSize(14);
        txtMenuSmall.font.setCharacterSize(24);

        txtSelectionBig.font.setCharacterSize(28);
        txtSelectionSmall.font.setCharacterSize(14);
        txtSelectionRanked.font.setCharacterSize(10);

        txtLoadBig.font.setCharacterSize(70);
        txtLoadSmall.font.setCharacterSize(24);

        txtRandomTip.font.setCharacterSize(32);
    }

    // txtVersion and txtProfile are not in here cause they do not need it.
    for(auto f : {&txtProf, &txtLoadBig, &txtLoadSmall, &txtMenuBig,
            &txtMenuTiny, &txtMenuSmall, &txtInstructionsBig, &txtRandomTip,
            &txtInstructionsMedium, &txtInstructionsSmall, &txtEnteringText,
            &txtSelectionBig, &txtSelectionMedium, &txtSelectionSmall,
            &txtSelectionScore, &txtSelectionRanked})
    {
        f->updateHeight();
    }

    updateLevelSelectionDrawingParameters();

    // Reformat the level description, but not on boot.
    if(!firstLevelSelection)
    {
        formatLevelDescription();
    }

    overlayCamera.update(0.5f);
    backgroundCamera.update(0.5f);
}
```

**Menu coordinate system (the key layout formulas):**

- The menu lays out in a virtual **1024×768 space**: `fmax = max(1024/wpx,
  768/hpx)`; `w = wpx * fmax`, `h = hpx * fmax` (the *larger* of the two
  scales — i.e. the menu always sees ≥1024×768 virtual pixels and crops the
  overflow; 4:3 windows get w > 1024… precisely: for a 4:3 window 1024/w >
  768/h so w' = 1024 and h' > 768; for 16:9 w' > 1024 and h' = 768).
- `overlayCamera` view = `{origin (0,0), size (w, h)}` → 1:1 mapping of
  virtual pixels to window pixels (SFML view centered: the view is centered
  on (w/2, h/2) implicitly by FloatRect? No — `sf::View{sf::FloatRect}` sets
  center = rect center, size = rect size, so it maps exactly).
- `backgroundCamera` size = window px × zoomFactor (world units), then
  `setSkew({1, 0.8})` for 4:3 or `{1, 0.6}` otherwise — the background is
  drawn in world units, vertically compressed by the skew via
  `Camera::apply()` (view size y × skew.y → more world visible vertically…
  note: size × skew, so a 0.6 skew shows 1.667× more vertically — the
  background sectors get squashed). Baked per `apply()`.
- Title bar: scale 0.5 at (20, 20); version text (40px OpenSquare regular)
  right-aligned origin at (titleBar global right − 15, titleBar global top +
  15); credits bar1: origin local-W-E, scale 0.373, at (w − 20, 20);
  credits bar2: same scale, at (w − 20, 17 + creditsBar1 global bottom).
- Epilepsy warning: origin local center, scale 0.36, position
  `(1024 / (2/scaleFactor), 768/2 − 50)` = `(w/2, 334)`.
- `fourByThree = 10.f * wpx / hpx < 16` (i.e. aspect < 16:10… strictly < 1.6).
- Font sizes per aspect (above). `txtVersion` stays 40, `txtProfile` stays 32
  (set in the ctor initializer list).
- All category offsets reset to 0; font heights recomputed via
  `updateHeight` (the `MenuFont::height` values used by every draw below);
  level-selection params recomputed; `formatLevelDescription()` (skipped on
  boot); both cameras `update(0.5f)` to flush queued turns.

`loadCommandLineLevel(pack, level)` — resolution flow (verbatim in §11.2
summary; detailed here):

- Find pack id by *name* in `assets.getPackDatas()`; log + return false if
  missing or pack has no levels.
- `levelID = packID + "_" + level`; find pack index i in
  `getSelectablePackInfos()` and the level index in
  `getLevelIdsByPack(packID)`; set `lvlSlct.packIdx = i`,
  `lvlSlct.levelDataIds`, `setIndex(idx)`.
- Then: `playLocally()` (→ SLPSelectBoot or ETLPNewBoot); if
  `state == ETLPNew` (no profiles) → log + false; else
  `enteredStr = getLocalProfileNames()[0]`; `assets.pSetCurrent(enteredStr)`;
  `changeStateTo(SMain)`; `resetNamesScrolls()`; `changeStateTo(LevelSelection)`;
  `playSelectedLevel()`.

`returnToLevelSelection()`: `adjustLevelsOffset(); lvlDrawer->XOffset = 0.f;
setIgnoreAllInputs(1);`

`refreshBinds()`: refreshes every `Config::triggerGetters[i]()` trigger on the
menu game + `fnHGTriggerRefresh` seam, then `Config::loadAllJoystickBinds()`.

`setIgnoreAllInputs(presses)`: sets `ignoreInputs`; 0 →
`game.ignoreAllInputs(false); Joystick::ignoreAllPresses(false)`; else true.

`getWindowWidth/Height()` = `window.getRenderWindow().getSize().x/y` (window
pixels, not virtual).

### 11.12 renderText helpers (verbatim semantics)

- `renderText(str, text, pos)` → setString + setPosition + `render(text)`.
- `renderText(str, text, pos, color)` → save fill, set, draw, restore.
- `renderText(str, text, size, pos)` → setCharacterSize(size) then the above
  (size change persists — no restore!).
- `renderText(str, text, size, pos, color)` → save size+color, set both, draw,
  restore both.
- `renderTextCentered(str, text, pos)` → setString +
  `setPosition({pos.x − getGlobalHalfWidth(text), pos.y})` + render. (Color/
  size overloads same pattern; note the size-only overload of centered does
  NOT restore the size.)
- `renderTextCenteredOffset(str, text, pos, xOffset)` → setString +
  `setPosition({xOffset + pos.x − getGlobalHalfWidth(text), pos.y})`.
- `render(sf::Drawable&)` = `window->draw(d)` (with default render states —
  see §11.14).

### 11.13 Menu offset/animation math (verbatim)

```cpp
void MenuGame::adjustMenuOffset(const bool resetMenuOffset)
{
    if(getCurrentMenu() == nullptr) { return; }

    if(resetMenuOffset)
    {
        getCurrentMenu()->getCategory().getOffset() = 0.f;
    }

    const auto& items{getCurrentMenu()->getItems()};
    for(auto& i : items) { i->getOffset() = 0.f; }

    items[getCurrentMenu()->getIdx()]->getOffset() = maxOffset;
}

void MenuGame::adjustLevelsOffset()
{
    for(auto& offset : lvlSlct.lvlOffsets) { offset = 0.f; }
    lvlSlct.lvlOffsets[lvlSlct.currentIndex] = maxOffset;

    if(!favSlct.lvlOffsets.size()) { return; }
    for(auto& offset : favSlct.lvlOffsets) { offset = 0.f; }
    favSlct.lvlOffsets[favSlct.currentIndex] = maxOffset;
}

inline constexpr float offsetSpeed{4.f};
inline constexpr float offsetSnap{0.25f};

[[nodiscard]] float MenuGame::calcMenuOffset(float& offset,
    const float maxOffset, const bool revertOffset, const bool speedUp)
{
    // Adjust the offset of the menu depending on whether it
    // is being opened or closed.

    float speed;
    if(revertOffset)
    {
        // FPS consistent offset speed
        speed = (speedUp ? 12.f : 8.f) * offsetSpeed * getFPSMult();

        offset = std::max(offset - speed, 0.f);
        if(offset <= offsetSnap) { offset = 0.f; }
    }
    else if(offset < maxOffset)
    {
        speed = (speedUp ? 12.f : 8.f) * offsetSpeed * getFPSMult();
        offset += speed * (1.f - offset / maxOffset);
        if(offset >= maxOffset - offsetSnap) { offset = maxOffset; }
    }

    return maxOffset - offset;
}

void MenuGame::calcMenuItemOffset(float& offset, bool selected)
{
    float speed;
    if(selected)
    {
        if(offset < maxOffset)
        {
            speed = offsetSpeed * getFPSMult();
            offset += speed * (1.f - offset / maxOffset);
            if(offset >= maxOffset - offsetSnap) { offset = maxOffset; }
        }
    }
    else if(offset > 0.f)
    {
        speed = offsetSpeed * getFPSMult();
        offset -= 1.5f * speed * offset / maxOffset;
        if(offset <= offsetSnap) { offset = 0.f; }
    }
}
```

`getFPSMult()` (verbatim, §11.14):

```cpp
[[nodiscard]] float MenuGame::getFPSMult() const
{
    return 60.f / window.getFPS();
}
```

(`calcMenuOffset` returns the *panel* offset `maxOffset − offset` — the drawn
panel indents by that amount; easing is an ease-out `+speed*(1 − offset/max)`
approach; item offsets use the same approach and a 1.5× proportional decay when
deselected.)

### 11.14 Quads, scrollbar, mouse, `draw`, `render`, `drawGraphics`

Constants: `inline constexpr float fontHeightOffset{0.55f};` (comment: "The
font used has some empty space on top that causes text to be drawn below what
you would expect... if we assume the text has to be drawn at 'Y' height the
actual value to be sent to the renderer is 'Y − fontHeight * fontHeightOffset'")
and `inline constexpr float frameSizeMulti{0.6f};`.

```cpp
void MenuGame::createQuad(const sf::Color& color, const float x1,
    const float x2, const float y1, const float y2)
{
    sf::Vector2f nw{x1, y1}, ne{x2, y1}, se{x2, y2}, sw{x1, y2};
    menuQuads.batch_unsafe_emplace_back_quad(color, nw, sw, se, ne);
}

void MenuGame::createQuad(const sf::Color& color, const sf::Vector2f& mins,
    const sf::Vector2f& maxs)
{
    createQuad(color, mins.x, maxs.x, mins.y, maxs.y);
}

void MenuGame::createQuad(const sf::Color& color, const sf::FloatRect& rect)
{
    createQuad(color, rect.left, rect.left + rect.width, rect.top, rect.top + rect.height);
}

void MenuGame::createQuadTrapezoid(const sf::Color& color, const float x1,
    const float x2, const float x3, const float y1, const float y2,
    const bool left)
{
    sf::Vector2f nw, ne, se, sw;

    if(left)
    {
        nw = {x1, y1}; ne = {x2, y1}; se = {x3, y2}; sw = {x1, y2};
    }
    else
    {
        nw = {x1, y1}; ne = {x2, y1}; se = {x2, y2}; sw = {x3, y2};
    }

    menuQuads.batch_unsafe_emplace_back_quad(color, nw, sw, se, ne);
}
```

(the `createQuad(color, FloatRect)` overload — check exact body; it forwards
`rect.left, rect.left + rect.width, rect.top, rect.top + rect.height`.)

```cpp
std::pair<int, int> MenuGame::getScrollbarNotches(const int size, const int maxSize) const
{
    if(size > maxSize) { return {size - maxSize, maxSize}; }
    return {0, size};
}

void MenuGame::drawScrollbar(const float totalHeight, const int size,
    const int notches, const float x, const float y, const sf::Color& color)
{
    const float notchHeight{totalHeight / size},
        barHeight{totalHeight - notches * notchHeight},
        startHeight{y + notchHeight * scrollbarOffset};

    menuQuads.clear();
    menuQuads.reserve_quad(1);
    createQuad(color, x, x + textToQuadBorder, startHeight, startHeight + barHeight);
    render(menuQuads);
}
```

Mouse helpers:

```cpp
bool MenuGame::overlayMouseOverlap(const sf::Vector2f& mins, const sf::Vector2f& maxs) const
{
    constexpr float tolerance = 1.f;

    if(!isMouseCursorVisible() || !window.hasFocus()) { return false; }

    const sf::Vector2f mp = overlayCamera.getMousePosition(window);

    return mp.x > mins.x - tolerance && mp.x < maxs.x + tolerance &&
           mp.y > mins.y - tolerance && mp.y < maxs.y + tolerance;
}

bool MenuGame::overlayMouseOverlapAndUpdateHover(const sf::Vector2f& mins, const sf::Vector2f& maxs)
{
    if(overlayMouseOverlap(mins, maxs))
    {
        mouseHovering = true;
        return true;
    }
    return false;
}

sf::Color MenuGame::mouseOverlapColor(const bool mouseOverlap, const sf::Color& c) const
{
    if(!mouseOverlap) { return c; }

    return sf::Color{static_cast<std::uint8_t>(255 - c.r),
                     static_cast<std::uint8_t>(255 - c.g),
                     static_cast<std::uint8_t>(255 - c.b)};
}

bool MenuGame::mouseLeftRisingEdge() const
{
    return !mouseWasPressed && mousePressed;
}
```

(`overlayCamera.getMousePosition` maps the mouse pixel through the overlay
view — i.e. coordinates in the virtual 1024×768 space. `mousePressed` /
`mouseWasPressed` are maintained by the `mouseLeftRisingEdge` bookkeeping in
`draw()` — see below. Note: overlap colors are the 255-complement (RGB only,
alpha unchanged).)

`drawMainMenu` (verbatim — main-menu + online-menu rendering):

```cpp
void MenuGame::drawMainMenu(
    ssvms::Category& mSubMenu, float baseIndent, const bool revertOffset)
{
    const auto& items(mSubMenu.getItems());
    const int size = items.size();

    const float panelOffset{
        calcMenuOffset(mSubMenu.getOffset(), -(baseIndent - w), revertOffset)};
    baseIndent += panelOffset;

    // Calculate quads coordinates.
    // We use font height as our reference parameter.
    const float interline{3.f * txtMenuBig.height},
        quadBorder{txtMenuBig.height * frameSizeMulti},
        doubleBorder{2.f * quadBorder},
        totalHeight{interline * (size - 1) + doubleBorder + txtMenuBig.height};
    float quadHeight{(h - totalHeight) / 2.f + interline - quadBorder},
        txtHeight{
            quadHeight - txtMenuBig.height * fontHeightOffset + quadBorder},
        indent;

    // Store info needed to draw the submenus
    menuHalfHeight = quadHeight + totalHeight / 2.f;

    // Draw the quads that surround the text
    menuQuads.clear();
    menuQuads.reserve_quad(size);

    static std::vector<bool> mouseOverlaps;
    mouseOverlaps.resize(size);

    for(int i{0}; i < size; ++i)
    {
        calcMenuItemOffset(items[i]->getOffset(), i == mSubMenu.getIdx());
        indent = baseIndent - items[i]->getOffset();

        const sf::Vector2f bodyMins{
            indent - txtMenuBig.height * 2.5f, quadHeight};

        const sf::Vector2f bodyMaxs{
            w, quadHeight + doubleBorder + txtMenuBig.height};

        const bool mouseOverlap =
            overlayMouseOverlapAndUpdateHover(bodyMins, bodyMaxs);

        mouseOverlaps[i] = mouseOverlap;

        sf::Color c = !items[i]->isEnabled() ? sf::Color{110, 110, 110, 255}
                                             : menuQuadColor;

        createQuadTrapezoid(mouseOverlapColor(mouseOverlap, c),
            indent - txtMenuBig.height * 2.5f, w,
            indent - txtMenuBig.height / 2.f, quadHeight,
            quadHeight + doubleBorder + txtMenuBig.height, false);

        // TODO (P2): cleanup mouse control
        if(mouseOverlap && !mustUseMenuItem.has_value() &&
            mouseLeftRisingEdge() && items[i]->isEnabled())
        {
            mustUseMenuItem = i;
        }

        quadHeight += interline;
    }

    render(menuQuads);

    // Draw the text on top of the quads
    for(int i{0}; i < size; ++i)
    {
        indent = baseIndent - items[i]->getOffset();

        const sf::Color c = mouseOverlapColor(mouseOverlaps.at(i),
            !items[i]->isEnabled() ? sf::Color{150, 150, 150, 255}
                                   : menuTextColor);

        renderText(
            items[i]->getName(), txtMenuBig.font, {indent, txtHeight}, c);

        txtHeight += interline;
    }
}
```

Layout summary: right-anchored trapezoid panels (from `indent` to `w`),
interline `3·Hbig`, border `0.6·Hbig`, first row top at
`(h − totalHeight)/2 + interline − quadBorder`; disabled items grey
(110,110,110,255 quads / 150,150,150,255 text); selected item slides in to
`maxOffset` (100) via `calcMenuItemOffset`; mouse hover → complement colors +
click sets `mustUseMenuItem` (executed in update).

`drawMainSubmenus` (drawn by `draw()` for the main menu's extra categories):

```cpp
void MenuGame::drawMainSubmenus(
    const std::vector<ssvms::UniquePtr<ssvms::Category>>& subMenus,
    const float indent)
{
    bool currentlySelected, hasOffset;
    for(auto& c : subMenus)
    {
        currentlySelected = mainMenu.getCategory().getName() == c->getName();
        hasOffset = c->getOffset() != 0.f;

        if(!currentlySelected && !hasOffset) { continue; }

        drawMainMenu(*c, w - indent, !currentlySelected && hasOffset);
    }
}
```

`drawOptionsSubmenus` (verbatim):

```cpp
void MenuGame::drawOptionsSubmenus(
    ssvms::Category& mSubMenu, const float baseIndent, const bool revertOffset)
{
    const auto& items{mSubMenu.getItems()};
    const int size(items.size());

    // Calculate quads coordinates
    float quadBorder{txtMenuSmall.height * frameSizeMulti};
    const float doubleBorder{quadBorder * 2.f};
    const float interline{2.1f * txtMenuSmall.height};
    const float totalHeight{
        interline * (size - 1) + 2.f * doubleBorder + txtMenuSmall.height};
    const float quadHeight{std::max(menuHalfHeight - totalHeight / 2.f,
        ssvs::getGlobalBottom(creditsBar2) + 10.f)};

    // Offset
    const float panelOffset{
        calcMenuOffset(mSubMenu.getOffset(), baseIndent, revertOffset, true)};
    const float indent{baseIndent - quadBorder - panelOffset};

    // Draw the quads that surround the text
    menuQuads.clear();
    menuQuads.reserve_quad(2);

    createQuad(menuTextColor, 0, indent + doubleBorder, quadHeight,
        quadHeight + totalHeight);
    createQuad(menuQuadColor, 0, indent + quadBorder, quadHeight + quadBorder,
        quadHeight + totalHeight - quadBorder);
    render(menuQuads);

    // Draw the text on top of the quads
    quadBorder = quadBorder * 1.5f - panelOffset;
    std::string itemName;
    float txtHeight{
        quadHeight - txtMenuSmall.height * fontHeightOffset + doubleBorder};
    for(int i{0}; i < size; ++i)
    {
        itemName = items[i]->getName();

        Utils::uppercasify(itemName);
        if(i == mSubMenu.getIdx())
        {
            itemName = "> " + itemName;
        }

        renderText(itemName, txtMenuSmall.font, {quadBorder, txtHeight},
            !items[i]->isEnabled() ? sf::Color{150, 150, 150, 255}
                                   : menuTextColor);

        if(!items[i]->isEnabled())
        {
            renderText("[OFFICIAL MODE ENABLED]", txtMenuTiny.font,
                {ssvs::getGlobalRight(txtMenuSmall.font) + 6.f,
                    ssvs::getGlobalTop(txtMenuSmall.font) - 2.f},
                sf::Color{150, 150, 150, 255});
        }

        txtHeight += interline;
    }
}
```

(Options submenus: left-anchored two-layer quads (textColor outer, quadColor
inner), uppercased item names, selected prefixed `"> "`, disabled items get a
`"[OFFICIAL MODE ENABLED]"` annotation in tiny font at
(itemRight + 6, itemTop − 2). `drawSubmenusSmall` iterates the options menu's
categories skipping `"options"`, calling `drawOptionsSubmenus(c, indent,
!currentlySelected && hasOffset)`.)

`draw()` / `render()` / `drawGraphics()` (exact, §11.18 — full text after the
level-selection sections; key points here):

- `render(d)` = `window->draw(d)` with default render states.
- `drawGraphics()`: sets views (background then overlay), draws
  `menuBackgroundTris` (rebuilt every frame in `draw()` via
  `styleData.drawBackgroundMenu(...)`), then the title bar, credits bars,
  version text, then state-specific content, then the online status bar,
  then `dialogBox` if not empty, then screenshot (`"screenshot.png"`), and
  resets `mouseWasPressed`.

`drawOnlineStatus` — see §11.18.

### 11.15 Profile selection + entering-text drawing

Constants: `inline constexpr float profFrameSize{10.f};`,
`inline constexpr unsigned int profCharSize{30};`,
`inline constexpr unsigned int profSelectedCharSize{30 + 12};` (= 42).

`formatSurvivalTime` (verbatim):

```cpp
std::string MenuGame::formatSurvivalTime(ProfileData* data)
{
    int time{0};
    for(auto& s : data->getScores()) { time += s.second; }

    std::stringstream stream;
    stream << "Total survival time ";
    if(time < 60)
    {
        stream << std::setfill('0') << std::setw(2) << time;
    }
    else if(time < 3600)
    {
        stream << std::setfill('0') << std::setw(2) << time / 60 << ":"
               << std::setfill('0') << std::setw(2) << time % 60;
    }
    else
    {
        stream << time / 3600 << ":";
        time %= 3600;
        stream << std::setfill('0') << std::setw(2) << time / 60 << ":"
               << std::setfill('0') << std::setw(2) << time % 60;
    }

    return stream.str();
}
```

(`getScores()` returns a container of (levelValidator → seconds) pairs;
formats as "Total survival time SS" / "Total survival time MM:SS" /
"Total survival time H:MM:SS".)

`drawProfileSelection(xOffset, revertOffset)` — verbatim in the transcript
below; layout constants: `profMinWidth` 400, `profMinHeight` 360,
`interline = 4·fontHeight`, `scrollbarInterspace` 3, font heights probed at
char sizes 30/42 via `Utils::getFontHeight(txtProfile.font, size)` (note:
these probe calls mutate the font's char size temporarily), text width = max
uppercased item width (≥400) + doubleBorder + 2·scrollbarInterspace,
`indent = max(((w + xOffset)·0.5 − textWidth)/2 + profFrameSize, doubleBorder)`
clamped further so the instructions string fits (instructions text verbatim:
`"Press backspace to delete the selected profile\nYou cannot delete the
profile currently in use"`), `quadHeight = max((h − totalHeight)/2,
globalBottom(titleBar) + 60)`, `totalHeight = max(interline·(drawnSize−1) +
2·doubleBorder + 3·fontHeight, 360)`; outer quad textColor / inner quad
quadColor (profFrameSize inset); scrollbar (if notches) at
`(indent + textWidth − scrollbarInterspace, quadHeight + profFrameSize +
scrollbarInterspace)` with height
`totalHeight − 2(profFrameSize + scrollbarInterspace)` in menuTextColor;
rows drawn for `i ∈ [scrollbarOffset, drawnSize + scrollbarOffset)`: name
centered (uppercased) at `(indent + textWidth/2, txtHeight − (selected ?
fontHeight·0.75 : 0))` in menuTextColor at char size 30 (42 when selected);
survival time below at `yPos + (selected ? selectedFontHeight : fontHeight)·1.75`
with char size `current − 15`; `txtHeight += interline`; panel slides via
`calcMenuOffset(mSubmenu.getOffset(), h − quadHeight, revertOffset)`.

`drawProfileSelectionBoot()` (no profile menu offset — boot screen):

- Same scrollbar-notches math (`maxProfilesOnScreen` 6), font heights at
  30/42.
- `interline = 4·fontHeight`, `totalHeight = interline·drawnSize`,
  `height = (h − totalHeight)/2 − selectedFontHeight·1.5`, then
  `height = max(height − 2·instructionsHeight, globalBottom(titleBar) + 40)`
  where `instructionsHeight = 1.5·txtInstructionsBig.height`.
- Instructions (centered, txtInstructionsBig): `"SELECT LOCAL PROFILE"`,
  `"PRESS ESC TO CREATE A NEW PROFILE"`; then `height +=
  selectedFontHeight`.
- Scrollbar width probe: max uppercased name width, then also probes the
  string `"Total survival time 0000:00"` width, `width += 10`; scrollbar at
  `((w + width)/2, height)` in White.
- Rows same as drawProfileSelection but centered at `w/2`, color default
  (white, since txtProfile fill not set on boot).

`drawEnteringText(xOffset, revertOffset)`:

- `enteredStr` uppercased into the font; `textWidth = max(200,
globalWidth)` (min 200);
- `doubleFrame = 2·profFrameSize`, `indent = ((w + xOffset)·0.5 −
  textWidth)/2 + profFrameSize`, `txtBottom = txtEnteringText.height·0.45`,
  `totalHeight = height + txtBottom + 2·doubleFrame`;
- `quadHeight = menuHalfHeight − totalHeight/2`, `txtHeight = quadHeight −
  height·fontHeightOffset + doubleFrame`;
- panel slide: `calcMenuOffset(enteringTextOffset, h − quadHeight,
  revertOffset)` added to both;
- quads: outer textColor `(indent − doubleFrame → indent + doubleFrame +
  textWidth, quadHeight → quadHeight + totalHeight)`; inner quadColor
  (profFrameSize inset);
- entered text via `renderTextCenteredOffset(enteredStr, font,
  {textWidth/2, txtHeight + profFrameSize/2}, indent, menuTextColor)`;
- instructions (txtInstructionsMedium, fill = menuQuadColor, 1.5× height
  spacing, drawn *above* the box starting at
  `txtHeight − profFrameSize − 3·instructionsHeight`): verbatim lines
  `"INSERT TEXT"`, `"PRESS ENTER WHEN DONE"`, `"PRESS ESC TO ABORT"`.

`drawEnteringTextBoot()`:

- `height = (h − txtEnteringText.height)/2`; `instructionsHeight =
  1.5·txtInstructionsBig.height`; `height −= 2·instructionsHeight`;
  centered instructions `"PROFILE CREATION"`, `"PLEASE TYPE A NAME AND PRESS
  ENTER"`;
- `height += instructionsHeight/2`; entered string uppercased, centered at
  `(w/2, height)` in White.

### 11.16 Loading screen (`drawLoadResults`) — verbatim

```cpp
void MenuGame::drawLoadResults()
{
    // Hexagon
    const float div{ssvu::tau / 6 * 0.5f}, hexagonRadius{100.f};
    const sf::Vector2f centerPos = {w / 2.f, h / 5.f};

    menuQuads.clear();
    menuQuads.reserve_quad(6);
    for(int i{0}; i < 6; ++i)
    {
        const float sAngle{div * 2.f * (i + hexagonRotation)};

        const sf::Vector2f nw{ssvs::getOrbitRad(centerPos, sAngle - div, hexagonRadius)};
        const sf::Vector2f ne{ssvs::getOrbitRad(centerPos, sAngle + div, hexagonRadius)};
        const sf::Vector2f se{ssvs::getOrbitRad(centerPos, sAngle + div, hexagonRadius + 10.f)};
        const sf::Vector2f sw{ssvs::getOrbitRad(centerPos, sAngle - div, hexagonRadius + 10.f)};

        menuQuads.batch_unsafe_emplace_back_quad(
            sf::Color::White, nw, sw, se, ne);
    }

    // Vertical separators
    menuQuads.reserve_more_quad(3);
    const float xOffset{w / 4.f};
    float topHeight{h / 2.f - h / 15.f}, bottomHeight{h / 2.f + h / 15.f};

    for(int i{-1}; i < 2; ++i)
    {
        createQuad(sf::Color::White, w / 2.f + i * xOffset - 5.f,
            w / 2.f + i * xOffset + 5.f, topHeight, bottomHeight);
    }

    render(menuQuads);

    // Counters
    topHeight += 5.f - txtLoadSmall.height;
    const float numbersHeight = bottomHeight - txtLoadBig.height * 2.f;

    txtLoadSmall.font.setFillColor(sf::Color::White);
    txtLoadBig.font.setFillColor(sf::Color::White);

    // 1
    float textOffset{w - 3.f * xOffset};
    renderTextCentered("PACKS LOADED", txtLoadSmall.font, {textOffset / 2.f, topHeight});
    renderTextCentered(ssvu::toStr(loadInfo.packs), txtLoadBig.font, {textOffset / 2.f, numbersHeight});

    // 2
    textOffset = w - xOffset;
    renderTextCentered("LEVELS LOADED", txtLoadSmall.font, {textOffset / 2.f, topHeight});
    renderTextCentered(ssvu::toStr(loadInfo.levels), txtLoadBig.font, {textOffset / 2.f, numbersHeight});

    // 3
    textOffset = w + xOffset;
    renderTextCentered("ASSETS LOADED", txtLoadSmall.font, {textOffset / 2.f, topHeight});
    renderTextCentered(ssvu::toStr(loadInfo.assets), txtLoadBig.font, {textOffset / 2.f, numbersHeight});

    // Random tip
    const float tipInterline{txtRandomTip.height * 1.5f};
    float height{h - tipInterline * 2.f};
    for(int i{1}; i >= 0; --i) // all tips are on two lines
    {
        renderTextCentered(std::string(randomTip[i]), txtRandomTip.font, {w / 2.f, height - tipInterline});
        height -= tipInterline;
    }

    // Errors (if any)
    int size = loadInfo.errorMessages.size();

    textOffset = w + 3.f * xOffset;
    renderTextCentered("ERRORS", txtLoadSmall.font, {textOffset / 2.f, topHeight}, sf::Color::Red);
    renderTextCentered(ssvu::toStr(size), txtLoadBig.font, {textOffset / 2.f, numbersHeight}, sf::Color::Red);

    if(!size)
    {
        bottomHeight += txtLoadSmall.height * 1.75f;
        renderTextCentered("NO LOAD ERRORS", txtLoadSmall.font, {w / 2.f, bottomHeight}, sf::Color::White);
        return;
    }

    // Print errors from last to first.
    bottomHeight += txtLoadSmall.height / 2.f;
    const float txtSpacing{txtLoadSmall.height * 2.f};

    auto [scrollbarNotches, drawnSize] =
        getScrollbarNotches(size, maxErrorsOnScreen);
    if(scrollbarNotches != 0)
    {
        drawScrollbar(txtSpacing * drawnSize, size, scrollbarNotches,
            w - 4.f - textToQuadBorder, bottomHeight + txtSpacing / 4.f,
            sf::Color::Red);
    }

    for(int i{drawnSize - 1 + scrollbarOffset}; i > -1 + scrollbarOffset; --i)
    {
        renderTextCentered(loadInfo.errorMessages[i], txtLoadSmall.font, {w / 2.f, bottomHeight});
        bottomHeight += txtSpacing;
    }
}
```

Layout notes: rotating hexagon outline (6 trapezoid quads, white, radius 100
/ 110, center `(w/2, h/5)`, `sAngle = (tau/6·0.5)·2·(i + hexagonRotation)`
where `hexagonRotation += mFT/100` per frame); three 10-px-wide white vertical
separators at `w/2 ± w/4` (from `h/2 − h/15` to `h/2 + h/15`); counters at
`x = (w − 3·w/4)/2, (w − w/4)/2, (w + w/4)/2` → i.e. `w/8, w/2, 3w/8`... note:
`textOffset/2` with textOffset `w − 3xOffset` → `(w − 3w/4)/2 = w/8`;
`w − xOffset` → `3w/8`; `w + xOffset` → `5w/8`; and ERRORS at
`(w + 3w/4)/2 = 7w/8`. Top labels at `topHeight + 5 − Hsmall`... precisely:
topHeight becomes `h/2 − h/15 + 5 − Hsmall`; numbers at `bottomHeight −
2·Hbig` (`h/2 + h/15 − 2·Hbig`). Errors listed bottom-up from
`bottomHeight + Hsmall/2`, spacing `2·Hsmall`, red, with red scrollbar at
`w − 4 − textToQuadBorder`.

// MAPPING-DOC-CONTINUES
