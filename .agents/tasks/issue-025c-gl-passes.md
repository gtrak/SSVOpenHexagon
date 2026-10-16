# Task 025c — the GL draw passes (issue-10 remainder): sprite, text, rectangle

Lane 25c of issue 25. The on-device smoke (commit c3e39c15) proved the whole
android pipeline works — extraction 222/222, GLES 3.0, hot Poll loop,
`swap_buffers` live, framebuffer ground-truth `fb pixel [0,0,0,255]` — and the
screen is black because the **menu's visible content is background-texture +
text**, and the GL target's `draw_sprite` / `draw_text` / `draw_rectangle`
are documented `TODO(issue-10)` no-ops (`crates/oh-app/src/windowed.rs`,
`impl DrawTarget for GlDrawTarget`). This is the port's issue-10 remainder;
the emulator finally made it visible (literally).

## Goal

The game is VISIBLE on the GL path: the menu shows its background texture,
title, and menu entries; in-game, walls (already vertex-drawn) plus the HUD
text render. Verified with an actual on-screen screenshot.

## The pieces (all already in the tree)

- **Fonts**: `fontdue` 0.9 (`crates/oh-render/src/font.rs` — TTF rasterizer,
  `Font` shared via `Rc`), `get_scaled_character_size` (`text_draw.rs:36`).
- **Textures**: `texture.rs` loads PNGs (RGBA8) into a texture pool;
  `Sprite`/`RenderStates` carry pool indices (`draw.rs:76-104`).
- **Shaders**: `GlBatchUploader` (`batching.rs`) is the template — the text
  pass wants a textured-quad shader (font atlas texture + per-vertex UV);
  the sprite pass a textured shader too (SFML sprite semantics: texture
  rect, position/rotation/scale, alpha blend).
- **The C++ mapping**: `mapping/rendering-menu.md` §3.9-§4.3 covers the
  font sizing; `GameWindow.hpp` the draw order. Match SFML's
  `sf::Text`/`sf::Sprite`/`sf::RectangleShape` rendering semantics 1:1
  (literal port discipline).

## Sub-goals (commit after EACH green one)

1. **Texture upload + sampler**: `GlParts` texture objects from the pool
   (PNG RGBA8 → `gl.tex_image_2d`, LINEAR + clamp, premultiplied-alpha
   decision documented — SFML uses straight alpha; match SFML). LRU or
   on-demand upload at first draw; pool eviction per SFML's texture
   lifetime (the pool is the port's; keep its semantics).
2. **Sprite pass** (`draw_sprite`): textured quad shader + the sprite's
   position/rotation/scale/texture-rect + SFML `Alpha` blending
   (`gl.enable(BLEND)`, `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`).
3. **Text pass** (`draw_text`): font-atlas approach (cache glyphs per
   font+size; fontdue rasterizes on miss) OR per-glyph quads — your call,
   justify; the C++ `sf::Text` semantics (letter spacing, outline?, bold
   via the bold font face — check what `Text` carries).
4. **Rectangle pass** (`draw_rectangle`): solid-color quad (SFML
   RectangleShape: position/size/fill color/outline).
5. **State hygiene**: blending on/off, texture binds, program switches
   per pass — measure nothing, but keep the frame order the C++ uses.

## Verification — the emulator is the acceptance surface

- Host: `tools/gate.sh` 4/4, 690+ tests green (add unit tests for the
  atlas/UV math — host-testable, no GL).
- `cargo check --workspace --target aarch64-linux-android` green.
- **On-device** (the emulator is RUNNING: `~/Android/Sdk`, AVD `oh25`,
  adb serial `emulator-5556`, KVM via `sg kvm`; agent-device installed):
  - `OH_APK_TARGET=x86_64-linux-android bash tools/package-android.sh`
    (exit 11 is EXPECTED — its own verification asserts arm64; the x86_64
    APK at `target/release/apk/open-hexagon.apk` is the artifact), then
    `adb install -r` + `am start -n
    info.vittorioromeo.openhexagon/android.app.NativeActivity`.
  - `adb exec-out screencap -p > shot.png` — the MENU must show non-black
    content (the background texture, title text, menu entries). Paste the
    screenshot path + a pixel-color histogram in the report (the PIL
    distinct-colors check is the discriminator: the black screen had 11
    colors; a real menu has thousands).
  - Start a level (`adb shell input keyevent 66` = Enter — the scripted
    desktop test proves Enter starts a level from the menu; if key events
    do not arrive through NativeActivity, SAY SO with evidence — that
    would be a 25b-SG5 input gap to document, not to paper over) and
    screencap the in-game view (polygon + HUD text visible).
  - If the emulator is wedged: `adb -s emulator-5556 reboot` (~1 min) or
    relaunch `sg kvm -c ".../emulator -avd oh25 -no-window -no-audio
    -no-boot-anim -gpu swiftshader_indirect -memory 3072 -no-snapshot ..."`.

## Constraints

- Commit after every green sub-goal. Desktop gate sacred (the GL passes
  change the DESKTOP windowed path too — that is the point: it was always
  broken there, just unverified).
- Do not modify `_RELEASE/`. Do not bump dependency versions (fontdue/glutin
  stay pinned). `Cargo.lock` churn = explain.
- f32/f64, literal-port discipline; match SFML's blend/texture semantics,
  not "modern GL style".
- Revert any probe leftovers before the final commit.

## Report

Per-sub-goal status, commits, gate tail, the on-device screenshot
evidence (paths + histograms + what is visibly rendered), the Enter/input
finding, deviations, and the residual MANUAL list.
