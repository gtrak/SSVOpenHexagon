# Issue 10 — `oh-render` backend: window, input, vertices, shaders, fonts, audio device

## Objective

Provide the platform backend the rest of the game draws and reads input through:
a window + event loop, an input state (`KKey`/`MBtn` enums with the **exact integer
values** from SSVStart), vertex-array drawing with the same primitive/ordering
semantics as SFML, textures, fonts + text layout, fragment shaders, render textures,
views/cameras, and an audio device. This is the SFML analogue.

## Key decisions (settle D-004 SFML entry here)

- **Primary candidate:** `rsfml` (a pure-Rust SFML reimplementation: `rsfml-graphics`
  / `-window` / `-audio` / `-system`). Assess real gaps before committing.
- **Fallback:** hand-written backend on `winit` + `glow`/`wgpu` + `rodio` +
  `fontdue`/`ab_glyph`. Choose per-module if needed (e.g. rsfml for window/graphics,
  a different crate for audio).
- Whatever is chosen, the **enum values** of keys/buttons and the vertex/colour
  semantics are compatibility surfaces (binds are stored in config by key name, but
  the in-memory enum must map to the same set).
- The backend must build on **Windows and Linux**.

## Files

| File | Change |
|---|---|
| `crates/oh-render/Cargo.toml` | new — backend deps |
| `crates/oh-render/src/{window,input,draw,texture,font,shader,view,audio}.rs` | new |
| `crates/oh-render/tests/backend.rs` | new — headless-capable smoke tests |
| `.agents/skills/lib-sfml.md` | new — SFML API used + semantics, from real source/call sites |
| `.agents/skills/lib-rsfml.md` (or chosen backend) | new — from the crate's real source |

## Steps

1. Decide the backend by reading the candidate crate's real source against the
   SFML usage list in `mapping/thirdparty-cpp.md`; record the decision in
   `DECISIONS.md` with evidence.
2. Port the input enums (`KKey`, `MBtn`, `Jid`) with exact values.
3. Port the window + event loop surface (`poll_event`, `close`, `set_vsync`,
   `set_framerate_limit`, `set_icon`, `set_mouse_cursor_visible`, joystick).
4. Port `VertexArray`/`Vertex`/`PrimitiveType`/`Color`/`RenderStates` semantics and
   the draw call surface (`draw_vertices`, `draw_text`, `draw_sprite`).
5. Port texture loading, `Font`/`Text` (origin, scale, rotation, colour, outline),
   `View`/camera transforms, `RenderTexture` (for screenshots/shaders).
6. Port fragment-shader loading + uniform setting; copy the shader uniform names
   used by the game (`u_resolution`, `u_color0`, ... see `mapping/rendering-menu.md`).
7. Audio device: play/stop/pitch/volume/playing-offset for `Music`/`Sound`.

## Verification

- A headless test that creates an offscreen render target and draws a triangle,
  reading back pixels to assert colour/position.
- Key/button enum value table matches the SSVStart source exactly (test with
  literals from `Enums.hpp`).
- Text layout test: measure a known string and compare to a C++/backend reference.
- A build for both `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-gnu`/`-msvc`
  compiles (the latter if the target is installed).
- `tools/gate.sh` green.

## Notes / risks

- If `rsfml` lacks a needed feature, prefer adding a small local module over
  abandoning the whole backend; record each gap.
- Audio: OGG Vorbis decode + streaming for music; ensure the volume/pitch mapping
  matches SFML's (perceptual volume is applied in the game layer, not here).
