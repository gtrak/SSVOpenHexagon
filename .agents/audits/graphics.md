# 1:1 Port Audit — `HGGraphics.cpp` → `crates/oh-render/src/graphics.rs` + `crates/oh-app/src/windowed.rs`

Repo: `/home/gary/dev/SSVOpenHexagon`
C++ module (audited completely): `src/SSVOpenHexagon/Core/HGGraphics.cpp` (739 lines)
Rust counterparts (audited completely): `crates/oh-render/src/graphics.rs` (2823 lines),
`crates/oh-app/src/windowed.rs` (3266 lines).

Audit focus: every draw stage, camera setup, the screenshot path, the
`drawImguiLuaConsole` tail.

> Cross-file note: the C++ `draw()` body is split across several Rust files by
> layering (`oh-game` may not call `oh-render`): the frame pipeline lives in
> `oh-render::graphics::draw_frame` (graphics.rs:397), the per-frame
> composition (window guard, camera/flash/particle/text copying, `FrameInput`
> assembly, the `onRenderStage` hook, the screenshot write-back) in
> `oh-app::bridge::draw` (bridge.rs:247), the camera-centre step in
> `oh-render::camera::draw_set_camera_centers` (camera.rs:162), the
> `drawText*` family + the size/origin half of `updateText` in
> `oh-render::text_draw` (text_draw.rs), the string-content half of
> `updateText` in `oh-game::update::update_text` (update.rs:1333), and the
> `drawImguiLuaConsole` tail in `oh-app::lua_console_ui` (frame_draw
> lua_console_ui.rs:484, wired in windowed.rs:2724). The audit direction is:
> for each C++ behavior in `HGGraphics.cpp`, find its Rust counterpart
> anywhere in the port.

## BEHAVIOR INVENTORY (C++ `HGGraphics.cpp`)

File-local:
- `formatTime(double)` :33-36 — `ssvu::toStr(std::floor(x * 1000) / 1000.f)`.

Member functions:
- `render(mDrawable, mStates)` :38-49 — `window == nullptr` → log
  "Attempted to render without a game window" + return; else
  `window->draw(mDrawable, mStates)`.
- `draw()` :52-316 —
  - :54-58 early return: `window == nullptr || Config::getDisableGameRendering()`.
  - :59-78 `getRenderStates(rs)` lambda: `!getShaders()` → default;
    `status.fragmentShaderIds[rs]` empty → default; else
    `runLuaFunctionIfExists<int, float>("onRenderStage", rs, 60.f /
    window->getFPS())` (:73-74) and return the bound pack fragment shader.
  - :80-81 asserts both cameras exist.
  - :83 `window->clear(sf::Color::Black)`.
  - :85-106 camera centres while `!status.hasDied`: `levelStatus.cameraShake > 0`
    → jittered centres via two `ssvu::getRndR(-shake, shake)` draws each
    (background = shake, overlay = shake + {W/2, H/2}); else fixed
    (0,0) / (W/2, H/2).
  - :108-120 while `!getNoBackground()`: `setView(backgroundCamera)`,
    `backgroundTris.clear()`, `styleData.drawBackground(backgroundTris,
    zeroVec2f, levelStatus.sides, getDarkenUnevenBackgroundChunk() &&
    levelStatus.darkenUnevenBackgroundChunk, getBlackAndWhite())`,
    `render(backgroundTris, states(BackgroundTris))`.
  - :122 `setView(backgroundCamera)` (unconditional).
  - :124-133 clear the 7 member vertex batches;
    `wallQuads.reserve_more_quad(walls.size() + cwManager.count())` (capacity hint).
  - :135-139 `w.draw(getColorWall(), wallQuads)` per wall; `cwManager.draw(wallQuads)`.
  - :141-145 `status.started` → `player.draw(getSides(), getColorMain(),
    getColorPlayer(), pivotQuads, capTris, playerTris, getColorCap(),
    getAngleTiltIntensity(), getShowSwapBlinkingEffect())`.
  - :149-259 3D block while `get3D()`: depth = `styleData._3dDepth`; reserve
    `num*depth`; `pulse3D = getNoPulse() ? 1 : status.pulse3D`;
    `effect = _3dSkew * get3DMultiplier() * pulse3D`;
    `backgroundCamera->setSkew({1, 1+effect})` (:165); radRot =
    `toRad(bgRotation) + pi/2`, sin/cos; copy loop `i < depth` (float compare,
    :173-177) appending whole batches to the 3D vectors; layer loop
    `j < int(depth)` (:200-259): `i = depth-j-1`, offset =
    `_3dSpacing * (i+1) * _3dPerspectiveMult * (effect*3.6) * 1.4` along
    (cosRot, sinRot); `adjustAlpha` (skip when `_3dAlphaMult == 0`, else
    `(a/_3dAlphaMult) - i*_3dAlphaFalloff` via `Utils::componentClamp`,
    :183-196); override colour = `getColorDarkened(get3DOverrideColor(),
    _3dDarkenMult)` (B&W: white with `getMainColor().a`); pivot layers get it;
    when `get3DOverrideColor() == getMainColor()` wall layers get darkened
    `getColorWall()` and player layers darkened `getColorPlayer()` (each with
    its own `adjustAlpha`); positions += newPos, colours assigned per layer
    slice.
  - :261-263 render `wallQuads3D` / `pivotQuads3D` / `playerTris3D`
    (WallQuads3D / PivotQuads3D / PlayerTris3D states).
  - :265-268 `getShowPlayerTrail() && status.showPlayerTrail` →
    `drawTrailParticles()`; :270-272 `getShowSwapParticles()` →
    `drawSwapParticles()`.
  - :275-278 render `wallQuads` / `capTris` / `pivotQuads` / `playerTris`
    (WallQuads / CapTris / PivotQuads / PlayerTris states).
  - :280 `setView(overlayCamera)`.
  - :282 `drawParticles()`; :283 `drawText(states(Text))`.
  - :285-289 `getShowKeyIcons() || mustShowReplayUI()` → `drawKeyIcons()`.
  - :290-296 `getShowLevelInfo() || mustShowReplayUI()` →
    `drawLevelInfo(states(Text))` (fresh evaluation).
  - :299-302 `getFlash()` → `render(flashPolygon)` (default states).
  - :305-313 `mustTakeScreenshot` → (`window != nullptr` →)
    `window->saveScreenshot("screenshot.png")`; `mustTakeScreenshot = false`.
  - :315 `drawImguiLuaConsole()`.
- `drawImguiLuaConsole()` :318-336 — `window == nullptr` → return (:319-321);
  `!ilcShowConsole` → return (:324-326); assert `overlayCamera` (:330);
  `window->setView(renderWindow.getDefaultView())` (:333);
  `Imgui::render(*window)` (:335).
- `initFlashEffect(r, g, b)` :338-356 — `flashPolygon.clear(); reserve(6)`;
  colour `{r,g,b,0}`; quad nw(-100,-100) sw(-100,H+100) se(W+100,H+100)
  ne(W+100,-100) (`W/H = Config::getWidth()/getHeight()`, 100 px margin).
- `drawKeyIcons()` :358-385 — off-opacity 90 / on-opacity 255 versions of
  `getColorText()`; left lit when `getInputMovement() == -1`, right when `== 1`,
  focus/swap when focused/swapped; render the four icons; `mustShowReplayUI()`
  → replay icon in on-colour + render.
- `drawLevelInfo(mStates)` :387-395 — render `levelInfoRectangle` + the five
  `levelInfoText*` (Level/Pack/Author/By/DM), all with the given states.
- `drawParticles()` :397-402 / `drawTrailParticles()` :405-409 /
  `drawSwapParticles()` :413-417 — render each `p.sprite` (default states).
- `updateText(mFT)` :421-600 — `window == nullptr` → return (:422-424).
  `pbTextGrowth += 0.08*mFT`, wrap at `2*PI` (:426-429). Status string:
  `debugPause` → "(!) PAUSED (!)\n"; `tutorialMode` → "TUTORIAL MODE\n" else
  `getOfficial()` → "OFFICIAL MODE\n"; `getDebug()` → "DEBUG MODE\n" +
  "CUSTOM WALLS: n / max"; while `started`: swapEnabled, invincibility,
  `timescale != 1`, scoreInvalid (+reason), `hasDied` →
  `restartInput + replayInput`, `calledDeprecatedFunctions` size 1/many
  warning lines, tracked variables (`getShowTrackedVariables()` + non-empty →
  "\n" + per-var `doesVariableExist` skip (:505-507) /
  `readVariable<string>` (:511), uppercased display + value); else
  `getRotateToStart()` → "ROTATE TO START\n" +
  `messageText.setString("ROTATE TO START")` (:520-521). Timer text:
  `!scoreOverridden` → `formatTime(status.getTimeSeconds())` (:532) or "0";
  else `lua.readVariable<string>(levelStatus.scoreOverride)` (:541-543).
  Per-frame render properties: `timeText.setCharacterSize(scaled(70))` (:552);
  `text.setString(os.str()); setCharacterSize(scaled(20)); setOrigin(0,0)`
  (:555-557); `getShowFPS()` → `fpsText.setString(ssvu::toStr(window->getFPS()))`
  + `setCharacterSize(scaled(20))` (:560-564);
  `messageText.setCharacterSize(scaled(32)); setOrigin(globalWidth/2, 0)`
  (:566-567); `pbText.setCharacterSize(scaled(64) + sin(pbTextGrowth)*10);
  setOrigin(globalWidth/2, 0)` (:569-571); `mustShowReplayUI()` →
  `replayText.setCharacterSize(scaled(16))` +
  `formatTime(rf.played_seconds())+"s" BY <name>` (or `formatTime(_played_score)`
  when score-overridden, :574-595), else `replayText.setString("")` (:598).
  `getScaledCharacterSize(size)` = `unsigned(size / zoomFactor * textScaling)`
  (:545-547).
- `drawText_TimeAndStatus(offsetColor, mStates)` :602-680 — outlines on
  (`getDrawTextOutlines()`) → outline colour = offset, thickness 2/1/1/1
  (time/text/fps/replay), else thickness 0 on all four (:605-620);
  `padding = getTextPadding()*getTextScaling()/getZoomFactor()` (:622-624);
  while `getShowTimer()` → timeText fill = `getColorText()`, origin local-NW,
  position (padding, padding), render (:628-635); while
  `getShowStatusText()` → text fill, origin local-NW, position
  (padding, `getGlobalBottom(timeText)` + padding), render (:637-645); while
  `getShowFPS()` → fpsText fill, origin local-SW, position (padding,
  `getGlobalTop(levelInfoRectangle) - padding` when level-info/replay UI else
  `height - padding`), render (:647-662); while `mustShowReplayUI()` →
  `scaling = getKeyIconsScale()/zoomFactor`, `replayPadding = 8*scaling`,
  replayText fill, origin local-center-E, position
  `getGlobalCenterW(replayIcon) - {replayPadding, 0}`, render (:664-679).
- `drawTextMessagePBImpl` :682-706 — skip when the string is empty; outline
  on/off with the given thickness; position; fill colour; render.
- `drawText_Message(offsetColor, mStates)` :708-715 — at
  `{W/2, H/5.5}`, outline thickness 1.
- `drawText_PersonalBest(offsetColor, mStates)` :717-725 — at
  `{W/2, H - H/4}`, outline thickness 4.
- `drawText(mStates)` :727-738 — `offsetColor = (getBlackAndWhite() ||
  styleData.getColors().empty()) ? Black : getColor(0)`; then
  TimeAndStatus → Message → PersonalBest.

Globals / statics read: `Config::*` (getDisableGameRendering, getShaders,
getNoBackground, getDarkenUnevenBackgroundChunk, getBlackAndWhite, get3D,
get3DMultiplier, getNoPulse, getShowPlayerTrail, getShowSwapParticles,
getShowKeyIcons, getShowLevelInfo, getFlash, getDrawTextOutlines,
getShowTimer, getShowStatusText, getShowFPS, getTextPadding, getTextScaling,
getZoomFactor, getWidth, getHeight, getKeyIconsScale, getAngleTiltIntensity,
getShowSwapBlinkingEffect, getInvincible, getTimescale, getDebug, getOfficial,
getShowTrackedVariables, getRotateToStart), `assets` (textures,
`getShaderByShaderId`), `ssvu::getRndR` (non-game RNG), Lua (`onRenderStage`,
`doesVariableExist`, `readVariable`), `window` (draw/clear/setView/getFPS/
saveScreenshot/getRenderWindow).

Members read/written: `backgroundCamera`/`overlayCamera` (centres, skew),
`status` (hasDied, fragmentShaderIds, pulse3D, showPlayerTrail, started,
scoreInvalid, invalidReason, restartInput, replayInput, getTimeSeconds),
`levelStatus` (cameraShake, sides, darkenUnevenBackgroundChunk, tutorialMode,
swapEnabled, trackedVariables, scoreOverridden, scoreOverride), `styleData`
(colours, 3D fields, `drawBackground`, `getMainColor`, `get3DOverrideColor`,
`getCapColorResult`), the frame vertex batches (`backgroundTris`,
`wallQuads*`, `pivotQuads*`, `playerTris*`, `capTris`), `walls`,
`cwManager`, `player`, `flashPolygon`, `particles`/`trailParticles`/
`swapParticles`, `keyIcon*`/`replayIcon`, `levelInfo*`, `fpsText`/`timeText`/
`text`/`replayText`/`messageText`/`pbText`, `pbTextGrowth`,
`mustTakeScreenshot`, `ilcShowConsole`, `debugPause`,
`calledDeprecatedFunctions`, `activeReplay`.

Side effects: window clear/draw/setView; **file write `screenshot.png`**
(SSVStart `GameWindow::saveScreenshot`, GameWindow.hpp:167-174); Lua
`onRenderStage` dispatch (can `goToMenu` via the Lippincott error handler);
console log on null-window `render`; ImGui console paint (draw tail).

---

## IMPLEMENTED (Rust side)

- `oh-render/src/graphics.rs` — `RenderStage` enum + `FragmentShaderIds`
  (:44-97, discriminants 0-8 + `COUNT = 9`); `render_states_for_stage`
  (:102-129, the `getRenderStates` lambda incl. the `onRenderStage` hook call);
  `DrawTarget` trait (:131-176: clear_black, set_view (view copied),
  draw_vertices, draw_sprite, draw_text, draw_rectangle, save_screenshot,
  present); `Particle`/`TrailParticle`/`SwapParticle`/`SwapParticleSpawnInfo`
  (:173-204); `DrawState` (:206-254, the C++ graphics members); `init_text`
  (:256-258); `FrameInput` (:276-395); **`draw_frame` (:397-711 — the full
  `draw()` pipeline: clear_black :416, camera centres :421, background +
  (inert) menu-background branch :431-476, second background set_view :487,
  3D copies/offsets/colours :497-613, 3D renders :618-630, trail/swap
  particles :634-640, flat renders :645-654, overlay view + particles +
  `text_draw::draw_text` :657-674, key icons :677-688, level info :691-693,
  flash :697-699, screenshot :704-707)**; `adjust_alpha` (:717-733);
  `init_flash_effect` (:735-757); `update_flash` (:759-770);
  `death_flash_effect` (:779-786); `draw_particles`/`draw_trail_particles`/
  `draw_swap_particles` (:789-809); `update_particles` + `make_pb_particle`
  (:845-900); `update_trail_particles` (:927-968); `update_swap_particles` +
  `make_swap_particle` (:994-1034); `init_key_icons` (:1077-1113, smooth +
  texture assignment + `update_key_icons`); `update_key_icons` (:1119-1170);
  `draw_key_icons` (:1180-1225); `trim_26` (:1233-1244); `update_level_info`
  (:1258-1368); `draw_level_info` (:1370-1380); `draw_background`
  (:1398-1443); `draw_background_menu` (:1459-1520); tests (:1522-2823,
  incl. the exact stage-sequence assertion `draw_frame_emits_exact_stage_
  sequence`).
- `oh-render/src/text_draw.rs` — `format_time` (:29),
  `get_scaled_character_size` (:36), `diff_format` (:47), `time_format` (:109),
  `TextUpdateInput` (:115), **`update_text` (:170-327 — the C++
  `updateText` string + character-size + origin half)**,
  `draw_text_time_and_status` (:329-451), `draw_text_message` (:453-475),
  `draw_text_personal_best` (:477-500), `draw_text` (:502-526).
- `oh-render/src/camera.rs` — `init_cameras` (:39), `refresh_pulse` (:57),
  `update_camera_shake` (:89), `death_shake_camera` (:135),
  `draw_set_camera_centers` (:162-199 — the `draw()` step-2 camera centres,
  jitter via the injected non-game RNG, no-op while dead).
- `oh-app/src/bridge.rs` — `AppDraw` (:106-194: render-side `Text`/`Sprite`/
  camera/flash/particle state, `fps: 60.0` :122/:209) and **`draw`
  (:247-447 — the `window_present` guard :254-257, per-frame
  `update_key_icons`/`update_level_info` with the C++ gates :262-292, camera +
  pre-shake-centre copy :297-305, flash copy :310-318, particle copy from the
  engine's sprite states :320-344, text string + `pb_text_growth` copy
  :346-353, `draw_scene_batches` :356-359, `FrameInput` assembly :362-437
  (incl. the `onRenderStage` closure + Lippincott error path :401-412 and
  `menu_background_enabled: false` :431-437), `draw_frame` :440,
  `mustTakeScreenshot` write-back :444-446)**.
- `oh-app/src/windowed.rs` — `GlDrawTarget` (:1058-1176: `clear_black` :1067,
  `set_view` :1073, `draw_vertices` :1077-1105, `draw_sprite` :1107,
  `draw_text` :1120, `draw_rectangle` :1128, `save_screenshot` :1157-1166,
  `present` :1168-1176); `NullDrawTarget` (:1181-1192, all no-ops);
  `WindowedClient::build` (:1474-1818, incl. the two `ssvs::Input::Manager`
  bind registrations), `boot` (:1838-1917), `install_sigint`/`sigint_stop`
  (:1922-1967), `run`/`tick`/`tick_fields` (:1977-2761 — the frame body:
  pre-update transparent clear :2690, refresh/update :2696-2700, **console
  post-update tail :2702-2705**, draw :2717, **`lua_console_ui::frame_draw`
  :2722-2726 (the `drawImguiLuaConsole` tail)**, `target.present()` :2734,
  fps bookkeeping :2738-2739), `create_window` (:2779-2822), free `frame`
  (:2843-2871), `register_engine_binds`/`register_menu_binds` (:1495-1762).
- `oh-app/src/lua_console_ui.rs` — the egui Lua console: `on_event` (:301,
  the C++ `Imgui::processEvent` feed), `ui` (:327), `paint` (:387-417, the
  C++ `Imgui::render` paint), `frame_post_update` (:429-480), `frame_draw`
  (:484-500, the C++ `drawImguiLuaConsole`), console gate
  `console_gate` (:95-97).
- `oh-game` (supporting, verified present): `update_text` (update.rs:1333-1491,
  string-content half of C++ `updateText`, window-gated), `format_time`
  (hexagon_game.rs:2525-2529, `format_ostream_f64` = std::to_string semantics),
  `update_flash` (update.rs:1092-1109, called update.rs:158 under
  `!debug_pause` as in C++ HGUpdate.cpp:166-168), `init_flash_effect`
  (hexagon_game.rs:2393-2409), `death_flash_effect` (hexagon_game.rs:1259-1261),
  `update_particles`/`update_trail_particles`/`update_swap_particles`
  (update.rs:1133/1190/1238, called :407-415 with the C++ gates),
  `draw_scene_batches` (hexagon_game.rs:2324), `get_rnd_r` →
  `cosmetic_rng` (hexagon_game.rs:2287-2288, the non-game-RNG equivalent),
  `Renderer::fps()` seam (renderer.rs:44-47, **default `0`**).
- `oh-render/src/gl.rs` — `GlParts::save_screenshot` (:434-441:
  `read_framebuffer` glReadPixels bottom-up → row flip → png encode →
  `fs::write`), `GlParts::clear` (:456-462).

Rust-only (no C++ HGGraphics.cpp counterpart): the `menu_background_enabled`
branch inside `draw_frame` (graphics.rs:461-475) + `draw_background_menu`
(:1459-1520) — the C++ menu-hexagon background is drawn by `MenuGame::draw`
(MenuGame.cpp:5780), a different module pair; the game bridge always passes
`menu_background_enabled: false` (bridge.rs:431-437). Also
`death_flash_effect`/`update_flash`/the three `update_*_particles` free
functions in graphics.rs duplicate what `oh-game` runs per-tick (the engine
state is what the bridge copies per frame; the render-side copies are the
issue-009 wiring surface + test fixtures).

---

## MISSING (highest risk first)

| # | item | C++ evidence (file:line) | absence proof (command + result) | risk |
|---|------|--------------------------|----------------------------------|------|
| M1 | **Key-icon / replay-icon / particle sprite textures are never loaded** — the windowed build draws all five key/replay icons and every PB-star / trail / swap particle from `Texture::new()` placeholders (a 1×1 white pixel, `oh-render/src/texture.rs:206-212`), while C++ assigns the real asset textures (`keyArrow.png` ×2, `keyFocus.png`, `keySwap.png`, `replayIcon.png`, `starParticle.png`, `smallCircle.png`) with `setSmooth(true)`. Result: white squares instead of the arrow/focus/swap/replay icons and the particle FX art. | `HexagonGame.cpp:94-105` (`initKeyIcons` texture loop + `setTexture`; ctor call `:453`, `onRecreation` re-apply `:339`); `HexagonGame.cpp:324-325` (`txStarParticle`/`txSmallCircle` from assets in the ctor's window branch); consumed by this pair's draws: `HGGraphics.cpp:373-376, 383` (icons), `:397-417` (particle `p.sprite` renders). | Rust placeholders: `crates/oh-app/src/bridge.rs:186-198` (`key_arrow/key_focus/key_swap/replay = Texture::new()` then `init_key_icons`) and `bridge.rs:125-127` (`tx_star_particle: Texture::new(), tx_small_circle: Texture::new()`), used in `sprite_from_state` `bridge.rs:79-89`. `grep -rn 'keyArrow\|keyFocus\|keySwap\|replayIcon\|starParticle\|smallCircle' crates/oh-app/src/ crates/oh-render/src/ crates/oh-game/src/` → **only doc comments** (bridge.rs:187, graphics.rs:223-224,830,903,973,1069,1074-1075), **no asset load / `get_texture` call anywhere**. The PNGs exist: `ls _RELEASE/Assets/ | grep -iE 'key|star|smallcircle|replay'` → `keyArrow.png keyFocus.png keySwap.png replayIcon.png smallCircle.png starParticle.png`. | uservisible-cosmetic |
| M2 | **The per-frame character-size / origin half of `updateText` is never applied** — the Rust port of it exists (`oh_render::text_draw::update_text`) but has **zero call sites**, so the rendered `Text` objects keep their constructor sizes forever: status text renders at 25/zoom (C++: 20), FPS text at 25/zoom (C++: 20), message text at 38/zoom (C++: 32) **and left-anchored instead of horizontally centred** (C++ sets origin `globalWidth/2` every frame), PB text static 65/zoom **with the `sin(pbTextGrowth)·10` size pulse and centre-origin lost** (C++: `64/zoom + growth·10`, origin `globalWidth/2`), replay text at 20/zoom (C++: 16). `pb_text_growth` is copied into `DrawState` (bridge.rs:353) but nothing on the render side consumes it. | `HGGraphics.cpp:552` (`timeText` 70), `:555-557` (`text` 20 + origin), `:563` (`fpsText` 20), `:566-567` (`messageText` 32 + origin `getGlobalWidth/2`), `:569-571` (`pbText` `64 + sin·10` + origin), `:593` (`replayText` 16). | `grep -rn 'text_draw::update_text\|TextUpdateInput' crates/ --include=*.rs` → **only** the re-export `oh-render/src/lib.rs:93` and a doc reference `graphics.rs:249`; no call site in any crate. The bridge copies strings only: `bridge.rs:346-353` (`set_string` ×6 + `pb_text_growth`), never `set_character_size`/`set_origin`; frozen sizes set once in `AppDraw::new` at `bridge.rs:172-179` (`fps 25, time 70, text 25, replay 20, message 38, pb 65` — matching the C++ ctor values `HexagonGame.cpp:297-310`, which C++ then overwrites every frame). | uservisible-cosmetic |
| M3 | **The FPS text shows a constant `"0"` instead of the live window FPS** — C++ streams `window->getFPS()` (the engine timer's FPS, `GameWindow.hpp:335-339`) with 6-decimal `std::to_string` formatting; Rust reads the engine's `Renderer` seam `fps()` (trait default `0`) and formats a `u32`. The windowed build's renderer never overrides it, and the timer's live FPS is never read anywhere in `oh-app`. | `HGGraphics.cpp:560-564` (`getShowFPS()` → `fpsText.setString(ssvu::toStr(window->getFPS()))` + size 20). | `crates/oh-game/src/update.rs:1460-1464`: `let fps = self.0.renderer.as_ref().map(\|r\| r.fps()).unwrap_or(0); self.est().fps_text = fps.to_string();` with `crates/oh-game/src/renderer.rs:44-47` `fn fps(&self) -> u32 { 0 }` (default; `WindowTouchRenderer` in `windowed.rs:172-185` does not override it). `grep -rn 'get_fps' crates/oh-app/src/` → **no matches** (the live value exists at `oh-render/src/timer.rs:168` `TimerStatic::get_fps` but is unread). | uservisible-cosmetic |
| M4 | **The `onRenderStage` Lua hook always receives a constant frametime (1.0)** instead of C++'s live `60.f / window->getFPS()` — `app.fps` is initialised to 60.0 and never updated in the windowed loop, so `60.0 / input.fps` in `draw_frame` is always 1.0. Levels whose `onRenderStage(stage, frameTime)` scales per-stage shader effects by the measured frame time behave like a locked-60 machine even at other refresh rates. | `HGGraphics.cpp:73-74` (`runLuaFunctionIfExists<int, float>("onRenderStage", static_cast<int>(rs), 60.f / window->getFPS())`). | `crates/oh-app/src/bridge.rs:122` (`pub fps: f32`), `:209` (`fps: 60.0`), `:414` (`fps: app.fps` → `FrameInput`); `grep -n 'fps' crates/oh-app/src/windowed.rs` → **no write to `app.fps` and no `timer.get_fps()` read** (only the config `limit_fps`/`max_fps`/`show_fps` seam methods :873-896). `draw_frame` computes `60.0 / input.fps` at `graphics.rs:405-411`. | uservisible-cosmetic (conditional: only for levels using the hook's `frameTime` parameter at non-60 FPS) |

## SEAMS (accepted / documented deviations)

| seam | C++ behavior (file:line) | Rust marker | note |
|------|--------------------------|-------------|------|
| Per-stage pack fragment-shader binding not executed in GL | `getRenderStates` returns `sf::RenderStates{assets.getShaderByShaderId(id)}` (`HGGraphics.cpp:75-78`) and every stage render uses it | `windowed.rs` module docs: "The per-stage pack-shader binding (`RenderStates::shader`) stays the documented issue-10 gap: the internal passes run, the pack's fragment shaders do not execute."; also `GlDrawTarget::draw_vertices` (`windowed.rs:1085-1087`): "The per-stage `RenderStates` (shaders) are the issue-10 GL surface; the batch uploads with the default program." | Documented **issue-10** gap. The `onRenderStage` Lua dispatch itself still fires (`graphics.rs:102-129` + `bridge.rs:401-412`); only the .frag execution is absent. |
| Console default-view reset | `window->setView(renderWindow.getDefaultView())` (`HGGraphics.cpp:333`) | `lua_console_ui.rs:382-386` (`paint` doc): "The C++'s `window->setView(renderWindow.getDefaultView())` (:334) is implicit: egui paints the full window rect in its own ortho (top-left origin) — the same the `ImGui::SFML::Render` uses" | egui owns its ortho; the GL view state is irrelevant to it. (Citation off by one — actual line is 333; see DIFFERENT D6.) |
| ImGui console replaced by egui | `drawImguiLuaConsole` gates + `Imgui::render(*window)` (`HGGraphics.cpp:318-336`); per-event `Imgui::processEvent` (HexagonGame.cpp:335); post-update tail (HGUpdate.cpp:1287-1646) | `lua_console_ui.rs:489-491` (`!ilc_show_console` → return, the C++ `:324-326` gate), `:492-494` (GL gate, the C++ `window == nullptr`), `:495-498` (paint after the game draw, wired at `windowed.rs:2722-2726` after `engine.draw`); event feed `on_event` (`lua_console_ui.rs:301`) wired via `window.run_with_event_hook` (`windowed.rs:2658-2670`); post-update tail `frame_post_update` (`windowed.rs:2702-2705`) | Platform substitution, behaviour (gates, one-frame `wantCapture*` lag, paint-after-draw) preserved; the Android/headless build never shows it (C++ headless: `window == nullptr`). |
| Headless draw target | every `window != nullptr` branch (e.g. `HGGraphics.cpp:54-58, 305-310`) | `NullDrawTarget` all no-ops incl. `save_screenshot` — `windowed.rs:1181-1192`, doc: "the C++'s `window == nullptr` equivalent — the game bridge's `window_present()` guard is what actually no-ops the game draw" | Headless no-op seam. |
| Screenshot capture mechanism | `window->saveScreenshot("screenshot.png")` (`HGGraphics.cpp:305-313`; SSVStart `GameWindow::saveScreenshot` GameWindow.hpp:167-174: `Texture::update(renderWindow)` → `copyToImage` → `saveToFile`, result ignored) | `GlDrawTarget::save_screenshot` (`windowed.rs:1157-1166`) → `GlParts::save_screenshot` (`gl.rs:434-441`: `glReadPixels` bottom-up → row flip → png crate → `fs::write`, failure swallowed — "The C++ ignores the `saveToFile` result; mirror that") | Present; readback-based equivalent taken inside `draw_frame` step 12 before the swap, as in C++. |
| Camera asserts | `SSVOH_ASSERT(backgroundCamera/overlayCamera)` (`HGGraphics.cpp:80-81`), `SSVOH_ASSERT(overlayCamera)` in the console tail (`:330`) | `Option<Camera>` + `.expect("checked above")` / `draw_set_camera_centers` no-returns on `None` (`camera.rs:174-177`, `graphics.rs:433-437`) | Assert-only; `None` occurs only for the headless engine (C++ `window == nullptr` case), which is guarded earlier. |

## DIFFERENT (present but renamed / relocated / behaviorally different)

- **`render()` null-window log** — C++ logs "Attempted to render without a game
  window" on every render attempt without a window (`HGGraphics.cpp:41-46`);
  the Rust bridge instead early-returns before any draw (`bridge.rs:254-257`)
  and has no equivalent log. Log-only side effect; risk none.
- **`wallQuads.reserve_more_quad(walls.size() + cwManager.count())`**
  (`HGGraphics.cpp:133`) — capacity hint, no Rust counterpart; documented at
  `graphics.rs:484-486` ("a capacity hint on the caller-owned buffers;
  skipped"). Unobservable.
- **Lua `readVariable` error semantics** — C++ throws when the variable is
  missing (`scoreOverride`: `HGGraphics.cpp:541-543`; tracked variables:
  `:505-513`); Rust logs via `host_error` and continues with an empty string /
  skips the entry (`oh-game/src/update.rs:1453-1457` and `:1419-1430`).
  Error-path divergence on invalid level data only.
- **`updateKeyIcons` / `updateLevelInfo` call placement** — C++ runs them
  per-tick inside `HexagonGame::update` (`HGUpdate.cpp:213-224`, with the
  `showKeyIcons/replayUI` and `showLevelInfo/replayUI` gates); the port runs
  them once per frame in `bridge::draw` with the same gates
  (`bridge.rs:262-292`; the `oh-game` copies are no-ops —
  `update.rs:1315-1326`, cross-referenced in `.agents/audits/game.md` SEAMS).
  Idempotent layout, so behaviourally equivalent.
- **Level-info 26-char trim** — C++ `s.substr(0, 26)` is byte-based and can
  yield invalid UTF-8 when a multibyte char straddles byte 26
  (`HexagonGame.cpp:195-200`); Rust `trim_26` falls back to 26 *characters*
  to stay valid UTF-8 (`graphics.rs:1233-1244`, documented). Edge-case
  divergence.
- **Drifting / wrong citations in the Rust comments** (flagged per audit
  policy):
  - `graphics.rs:352-355` (the `FrameInput` menu-background block) cites
    "C++ `state >= States::SMain` … (HGGraphics.cpp lines 122–141)" —
    `HGGraphics.cpp:122-141` is the scene-batch fill
    (`setView`/`clear`/`reserve_more_quad`/wall loop); the C++ menu-hexagon
    background is `MenuGame.cpp:5780` + `StyleData.cpp:276-289`. The bridge's
    comment (`bridge.rs:431-437`) cites it correctly.
  - `lua_console_ui.rs:384-385` cites the default-view reset as "(:334)" —
    actual line is `HGGraphics.cpp:333`.
  - `graphics.rs:392-393` and `:709-710` still mark the `drawImguiLuaConsole`
    tail "out of scope for the draw layer; `TODO(issue-009-wiring)`" — the
    wiring has since landed in `windowed.rs:2722-2726`
    (`lua_console_ui::frame_draw`); the marker is stale (as are the
    `issue-009-wiring` markers at `graphics.rs:29,267,813,1072,1117,1254` for
    the calls that `bridge.rs`/`windowed.rs` now perform).
- **`timeText` character size 70/zoom** — C++ re-sets it every frame
  (`HGGraphics.cpp:552`); Rust sets it once at construction
  (`bridge.rs:174` via `init_text`). Equivalent because
  `Config::getZoomFactor()` is runtime-invariant (the same reasoning makes the
  once-set `levelInfoText*` sizes match C++'s per-frame `updateLevelInfo`).
- **`drawImguiLuaConsole` overlay-camera assert** (`HGGraphics.cpp:330`) has no
  Rust counterpart (the egui paint needs no camera) — assert-only.
- **Rust superset**: `menu_background_enabled` branch + `draw_background_menu`
  in the game `draw_frame` (`graphics.rs:461-475, 1459-1520`) have no C++
  HGGraphics.cpp counterpart (the C++ call site is `MenuGame.cpp:5780`, a
  different pair); inert in the game bridge (`menu_background_enabled: false`,
  `bridge.rs:431-437`). `death_flash_effect` / `update_flash` /
  `update_*_particles` free functions in `graphics.rs` duplicate the
  `oh-game` per-tick implementations (engine state is what the bridge copies;
  the render-side copies are the issue-009 surface + test fixtures).

## VERDICT

FAILS 1:1 — the frame pipeline itself (all nine render stages in exact order,
camera setup/shake, the 3D layer math, flash, level info/key icons, the
screenshot path, and the ImGui-console tail via egui) is a faithful port, but
the windowed build never loads the real key-icon/replay-icon/particle
textures (M1: C++ `HexagonGame.cpp:94-105,324-325` vs placeholder
`Texture::new()` in `bridge.rs:186-205`), so the HUD's five icons and every
PB/trail/swap particle render as 1×1-white-pixel squares — the single most
important gap; the per-frame text size/origin updates are likewise never
applied (M2) and the FPS counter reads a dead `0` (M3).
