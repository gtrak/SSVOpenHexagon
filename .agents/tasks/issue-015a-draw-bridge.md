# Task spec — issue 015 (step a): the draw bridge (new `oh-app` crate, D-020)

You are a worker. Implement exactly this. Do not redesign or widen scope.
**No window/GL/winit code, no `main.rs` binary, no MenuGame switching** — those
are steps (b)/(c).

## Why this task exists

D-020 (`DECISIONS.md`): issue 11 ported the C++ draw layer as free functions in
`oh-render`, but nothing drives `oh_render::graphics::draw_frame` from the
engine — `oh-game` and `oh-render` **deliberately do not depend on each other**
(oh-lua depends on oh-game, so oh-game cannot gain deps; oh-render is a
consumer of vertex data, not of the simulation). The composition therefore
belongs in a NEW crate `crates/oh-app` — which issue 15 creates anyway.

**Key realisation (verified — do not re-derive):** the C++
`HexagonGame::draw()` (`HGGraphics.cpp:52-316`) is ALREADY ported — it is
`oh_render::graphics::draw_frame` (`crates/oh-render/src/graphics.rs:388`),
including the `getRenderStates`/`onRenderStage` callback, camera shake,
screenshot flag consumption, and the `disableGameRendering` guard. This task is
ONLY the bridge: build `FrameInput`/`DrawState` from the engine's state each
frame and call `draw_frame`.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`, branch `master`, HEAD `44c20405`, tree
clean, `bash tools/gate.sh` 4/4 (363 tests default AND feature-on).

Read first:
1. `crates/oh-render/src/graphics.rs:131-155` (`DrawTarget`), `:197-266`
   (`DrawState`), `:267-386` (`FrameInput` — the complete field list), `:388+`
   (`draw_frame`), `:1565` (the `Recording` test target — reuse it).
2. `crates/oh-render/src/view.rs` — oh-render's `Camera`/`View`.
3. `crates/oh-game/src/camera.rs:29` — the ENGINE's `Camera` (a different
   Rust type, same C++ `ssvs::Camera` origin; you must convert).
4. `crates/oh-game/src/hexagon_game.rs:234-235` (the engine's
   `background_camera`/`overlay_camera`, `pub(crate)` — see accessors below),
   `:471` (the `window` ctor flag).
5. C++ oracle: `src/SSVOpenHexagon/Core/HGGraphics.cpp:52-316` (draw),
   `HexagonGame.hpp` (member visibility — vendored at
   `include/SSVOpenHexagon/Core/HexagonGame.hpp`; the colour getters etc. are
   public members, e.g. `:530-536` in the `public:` section from `:444`), and the
   `onRenderStage` Lua dispatch at `HGGraphics.cpp:74-77` (inside `draw()`'s
   `getRenderStates` lambda — NOT `HGUpdate.cpp`; that file's
   `runLuaFunctionIfExists` sites are `onInput` `:244`, `onLoad` `:549`,
   `onUpdate` `:738`, `onStep` `:745`).

## Scope

### 1. `crates/oh-app`: new lib crate

`crates/oh-app/Cargo.toml` — `[lib]`, deps: `oh-game`, `oh-render`,
`oh-config`, `oh-assets`, `oh-menu` (only if the bridge needs it), `oh-math`.
No windowing dependency. Dev-deps: `oh-lua` (for the real composition in
tests, mirroring `crates/oh-game/tests/common/mod.rs` — you WILL need to
replicate a minimal compose helper there; keep it small and note that 15b
dedupes).

### 2. Engine accessors (minimal, faithful)

`FrameInput` needs engine state that is currently `pub(crate)`. Add the
smallest set of `pub` readers to `oh-game` (a `draw_inputs()` struct or
individual getters — your call, document it), mirroring the C++ member
visibility (the C++ members are public). This includes: `status.fragment_shader_ids`,
`status.has_died`, `levelStatus.cameraShake`, `levelStatus.sides`,
`levelStatus.darkenUnevenBackgroundChunk`, `status.showPlayerTrail`,
`status.pulse3D`, `mustShowReplayUI()`, `mustTakeScreenshot`,
`styleData` (background colors, bgTileRadius, BGRotOff, the 3D seven, 3D
override), the wall/pivot/player/cap vertex batches, the key-icon/input state,
`background_camera`/`overlay_camera`, the `window` ctor flag (a
`has_window()`), and the `onRenderStage` Lua dispatch (a callback the bridge
invokes like C++ `runLuaFunctionIfExists<int, float>("onRenderStage", rs,
60.f/fps)`). Do NOT change any simulation code.

### 3. Camera conversion

`oh_game::camera::Camera` → `oh_render::view::Camera`. Both port the same C++
`ssvs::Camera` (verify against `extlibs`/ssvs source or the mapping docs —
cite what you compared). Write the conversion as an explicit, tested function
(value-preservation test over representative centers/sizes/rotations); if the
two types carry different representations, convert exactly and document the
mapping per field.

### 4. The bridge function

`crates/oh-app/src/bridge.rs` (or `draw.rs`):
`pub fn draw(game: &HexagonGame<GameApiState>, app: &mut AppDraw, target: &mut impl DrawTarget)`
where `AppDraw` owns the `DrawState` (cameras + pre-shake) and the frame's
mutable scratch. Faithful to the C++ order:

- C++ `if(window == nullptr || Config::getDisableGameRendering()) return;` —
  the port: `if !game.has_window() { return; }` (the engine's window flag IS
  the C++ null-window concept; document this), then `draw_frame`'s own
  `disable_game_rendering` guard handles the rest.
- Build `FrameInput` field-for-field (use the struct's own doc comments as the
  field-to-C++-member map), call `oh_render::graphics::draw_frame`.
- The batches: the C++ `draw()` fills `wallQuads`/`pivotQuads`/`playerTris`/
  `capOverlayQuads`... via `w.draw(...)`, `cwManager.draw(...)`,
  `player.draw(...)`, `createCapOverlayQuads` etc. IF `draw_frame` already
  takes them as filled slices (`FrameInput` docs say "filled by the caller"),
  the bridge must run those fills first — check whether the engine already
  exposes draw-into-batch methods (issue 09 ported `update_flash`; walls/
  player/custom walls have `draw(&self, quads)` methods per
  `wall.rs:180`, `player.rs:165`, `custom_wall.rs:62`). Port whatever fill
  orchestration C++ `draw()` does BEFORE `draw_frame`'s equivalent section,
  in the C++ order (cite `HGGraphics.cpp` line numbers for the order you
  implement). If `draw_frame` already contains the fill orchestration
  internally, do NOT duplicate it — verify by reading it and say so.

### 5. Tests (headless, `Recording` target)

- window=false engine → `draw` is a no-op (no clear, no vertices).
- Real composition (`_RELEASE`, a real pack/level via the 15-style compose;
  run a few ticks like `tests/determinism.rs` does) with window=true →
  `draw` records: clear_black first, then background pass before wall pass
  before player/cap passes (order per C++), the wall batch is non-empty and
  its vertex count is deterministic across two identical runs (same seed →
  same recording), and `must_take_screenshot` is consumed.
- Camera conversion round-trip tests.
- `onRenderStage`: with a level whose style/level Lua defines it, assert the
  callback fires per stage with the C++ frametime argument (60/fps).

## Process requirements (both previous runs timed out — follow this)

1. **Commit after every green milestone**: (a) crate scaffold + camera
   conversion + its tests; (b) engine accessors; (c) bridge + no-op test;
   (d) full draw test. Never leave uncommitted work past a green
   `cargo test -p oh-app`.
2. Run `cargo test -p oh-app` after each file. Run `bash tools/gate.sh` before
   your final report; fix clippy (`-D warnings`) immediately — the last two
   lanes died mid-clippy-cleanup.
3. Verify every C++ citation against the real source before relying on it
   (D-014); fix the spec in your commit if a citation is wrong and say so.
4. Update `.agents/plans/STATUS.md` row 15 in your final commit.

## Out of scope (do NOT do)

- `main.rs`/bin target, args parsing, `-printLuaDocs` CLI (step b).
- winit/glow window, event pump, MenuGame↔HexagonGame switching, signals,
  FPS text timing loop (step c).
- The egui console overlay (14b, step c).
- Any change to simulation order, the D-019 fix, or the determinism harness.
- Adding `oh-render` as a dependency of `oh-game` (forbidden — dependency
  cycle, D-013/D-020).
