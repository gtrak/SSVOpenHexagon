# lib-render-stack — `oh-render`'s Rust platform stack (D-011)

The SFML analog per **DECISIONS.md D-011** (`.agents/plans/001-rust-port/
DECISIONS.md:183-208`) and the plan `.agents/plans/001-rust-port/10-
render-backend.md`: real GL, **not** a software rasterizer and **not** `rsfml`
(rejected: not on crates.io). Stack: `winit` (window/events/input) + `glutin`
(GL context) + `glow` (GL bindings) + `rodio` (audio device) + `fontdue`
(TTF metrics/rasterization), with `oh-config`'s `Key`/`Button` reused for the
exact SFML enum integers (see `lib-sfml.md`, `lib-binds.md`).

**Note vs the plan wording:** the stack is `glutin` used **directly**
(`glutin::display::Display` + raw-window-handles). `glutin-winit` is **not in
`Cargo.lock`** and not used anywhere in `crates/oh-render` (grep-verified).

Pinned versions from `Cargo.lock` (registry, checksums quoted):

| crate | version | `Cargo.lock` |
|---|---|---|
| `winit` | 0.30.13 | `:2643-2646` (`a6755fa5…df5978d`) |
| `glutin` | 0.32.3 | `:625-628` (`12124de8…041325`) |
| `glow` | 0.18.0 | `:613-616` (`263218b0…8472d1386`) |
| `rodio` | 0.23.0 | `:1660-1663` (`36e3ee0a…11f39`) |
| `fontdue` | 0.9.4 | `:507-510` (`7894823f…2d7d3d`) |
| `raw-window-handle` | 0.6.2 | `:1627-1630` (`20675572…1c539`) |
| `dpi` (re-exported as `winit::dpi`) | 0.1.2 | `:431-434` |

Declared in `crates/oh-render/Cargo.toml` as `winit = "0.30"`, `glutin =
"0.32"`, `raw-window-handle = "0.6"`, `glow = "0.18"`, `fontdue = "0.9"`,
`rodio = "0.23"`.

Sources read (do NOT fetch anything else):
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/{winit-0.30.13,
glutin-0.32.3,glow-0.18.0,rodio-0.23.0,fontdue-0.9.4}/src/`.

## `winit` 0.30.13 — the surface `oh-render` uses

Event loop / application model:

- `EventLoop::new() -> Result<EventLoop<()>, EventLoopError>`
  (`winit-0.30.13/src/event_loop.rs:197`).
- **`EventLoop::run_app(&mut app)` (`event_loop.rs:264`)** — the only run
  model in winit 0.30. `ApplicationHandler` trait: `application.rs:8`;
  methods `resumed` `:83`, `window_event` `:93`, `about_to_wait` `:121`.
  **`MainEventsCleared` does not exist in 0.30** (grep-verified absent from
  `event.rs`); `about_to_wait` is the frame-pace point. SFML's
  `while(pollEvent)` loop maps onto this — see
  `crates/oh-render/src/window.rs:10-19` (module doc note) and `RunHandler`
  (`window.rs:623-645`).
- Deprecated pre-`run_app` creation path used by the GL bootstrap:
  `EventLoop::create_window(WindowAttributes)` (`event_loop.rs:307`) +
  `WindowAttributes::new()` (`window.rs:182`) +
  `dpi::PhysicalSize::new(w, h)` (`winit::dpi` is a re-export of the
  external `dpi` crate, `lib.rs:200`; `dpi` 0.1.2 in `Cargo.lock`).
  `crates/oh-render/src/gl.rs:68` carries `#[allow(deprecated)]` for this.
- Handles: `Window::window_handle() -> Result<rwh_06::WindowHandle>`
  (`window.rs:1626`); `ActiveEventLoop::display_handle()` for the raw display
  handle (used at `gl.rs:84,102` via `event_loop.display_handle()`).

Events / input types (all in `event.rs` unless noted):

- `WindowEvent` variants consumed: `CloseRequested`, `Focused(bool)`,
  `Resized`, `KeyboardInput { event, .. }`, `MouseInput`, `CursorMoved`,
  `MouseWheel { delta, .. }`, `Touch`.
- `ElementState::{Pressed, Released}`; `MouseButton { Left, Right, Middle,
  Other(u16) }` (`event.rs:941-947`); `MouseScrollDelta { LineDelta(f32,
  f32) (:959), PixelDelta }` (`event.rs:953`); `TouchPhase::{Started, Ended,
  …}`.
- `keyboard` module: `Key::{Named(NamedKey) (keyboard.rs:1474),
  Character(Str) (:1479)}`; `PhysicalKey` (`keyboard.rs:225`,
  `PhysicalKey::Code(KeyCode)`); `KeyLocation { Standard, Left, Right }`
  (`keyboard.rs:1623`).

`oh-render` mapping (headless-pure, unit-tested):
`winit_button_to_sfml` (`window.rs:84-93`), `winit_key_to_sfml`
(`window.rs:98-215` — note the numpad disambiguation via
`PhysicalKey::Code` because numpad digits arrive logically as
`Character`), `map_window_event` (`window.rs:217-296`), `apply_event`
(`window.rs:599-618`). Scroll: `LineDelta(_, y)` passes `y` through;
`PixelDelta(p)` uses `p.y as f32 / 10.0` (`window.rs:250-254`).

## `glutin` 0.32.3 — context bootstrap (only in `src/gl.rs`)

Bootstrap order (`gl.rs:69-138`):

1. `unsafe Display::new(RawDisplayHandle, DisplayApiPreference) ->
   Result<Display, DisplayError>` (`glutin-0.32.3/src/display.rs:210`).
   Preference enum `DisplayApiPreference` (`display.rs:417`): `Egl`
   (Linux), `Cgl` (macOS), `Wgl(RawWindowHandle)` (Windows — the window
   handle is needed **at display-creation time**, `gl.rs:77-87`).
2. `unsafe display.find_configs(ConfigTemplateBuilder::new().build())` →
   iterator of configs (`display.rs:267`; `ConfigTemplateBuilder`
   `src/config.rs:91`, `.build()` `:268`).
3. `SurfaceAttributesBuilder::<WindowSurface>::new().build(window_handle,
   width, height)` (`src/surface.rs:152`) then
   `unsafe display.create_window_surface(&config, &attrs)`
   (`display.rs:317`) → `Surface<WindowSurface>`.
4. `ContextAttributesBuilder::new().build(None)` (`src/context.rs:238`) then
   `unsafe display.create_context(&config, &attrs)` (`display.rs:291`) →
   `NotCurrentGlContext`.
5. `not_current.make_current(&surface)` (`src/context.rs:417-434`, dispatches
   per API) → `PossiblyCurrentContext` (per-API structs, e.g.
   `src/api/egl/context.rs:282`).
6. `display.get_proc_address(&CStr) -> *const c_void` (`display.rs:395`) —
   the loader closure handed to `glow`.

All entry points in steps 2-4 are `unsafe` (that's the 0.32 API shape — the
`GlDisplay` trait, `display.rs:52/74/87/125`).

## `glow` 0.18.0 — GL bindings

- `Context` (`glow-0.18.0/src/native.rs:46`);
  **`unsafe Context::from_loader_function_cstr(|name: &CStr|
  display.get_proc_address(name))`** (`native.rs:55`; used at
  `crates/oh-render/src/gl.rs:119`).
- `HasContext` trait (`lib.rs:142`) — `&gl: &glow::Context` calls go through
  it (`batching.rs:24`, `tests/golden_frames.rs:22`).
- **Every GL entry point is `unsafe` and takes raw `u32` constants** — each
  call is wrapped in `unsafe` in the crate. Constants live in `lib.rs`:
  `COLOR_BUFFER_BIT` `:2297`, `DYNAMIC_DRAW` `:2697`,
  `FRAMEBUFFER_COMPLETE` `:2813`, `RGBA8` `:3945`, …
- Used surfaces: `gl.version()` (safe, `lib.rs:160`; wrapper
  `GlParts::version_string` `gl.rs:140-145`), `gl.clear_color`
  (`lib.rs:596`), `gl.clear`, `gl.read_pixels` (`lib.rs:1882`;
  `PixelPackData::Slice(Some(&mut buf))`), `gl.viewport` — the
  `glClear → glReadPixels` readback is `GlParts::clear_and_read_center`
  (`gl.rs:149-170`), the "this is a real context" proof. The full
  vertex/triangle pipeline (buffers, programs, FBOs: `Buffer`, `Program`,
  `Shader`, `VertexArray`, `UniformLocation`, `RENDERBUFFER`/`FRAMEBUFFER` +
  `FRAMEBUFFER_COMPLETE` check) is in `src/batching.rs` and
  `tests/golden_frames.rs` (issue 011 draws).

## `rodio` 0.23.0 — audio device surface (only in `src/audio.rs`)

- `DeviceSinkBuilder::open_default_sink() -> Result<MixerDeviceSink,
  DeviceSinkError>` (`rodio-0.23.0/src/stream.rs:233`).
- `MixerDeviceSink` (`stream.rs:58`), `.mixer() -> &Mixer` (`stream.rs:67`).
- `Player::connect_new(&Mixer) -> Player` (`src/player.rs:73`);
  `player.append(source)` (`player.rs:104`); `set_volume(Float)`
  (`player.rs:178`); `set_speed(f32)` (`player.rs:204`).
- `buffer::SamplesBuffer::new(channels: ChannelCount /* NonZero<u16> */,
  sample_rate: SampleRate /* NonZero<u32> */, data: Into<Vec<f32>>)`
  (`src/buffer.rs:31-41`).
- `oh-render` wrappers: `AudioDevice { sink: Option<MixerDeviceSink> }`
  (`audio.rs:389-393`); `AudioDevice::new()` = headless no-sink
  (`audio.rs:397`); `open_default()` (`audio.rs:405`); `play_samples(…)`
  (`audio.rs:420-435`): volume is SFML-style `0..=100` →
  `(volume / 100.0).clamp(0.0, 1.0)`, pitch → `set_speed`. **No file
  decoding in this crate** — `rodio::Decoder` appears only in a doc comment
  (`audio.rs:251`); decoding/formatting is the `oh-audio` layer's job.

## `fontdue` 0.9.4 — TTF metrics + rasterization (only in `src/font.rs`)

- `Font::from_bytes(data, FontSettings) -> FontResult<Font>`
  (`fontdue-0.9.4/src/font.rs:242`); `FontSettings::default()`
  (`font.rs:161`).
- `font.metrics(char, px) -> Metrics` (`font.rs:453`);
  `font.rasterize(char, px) -> (Metrics, Vec<u8>)` (`font.rs:529`) — the
  alpha bitmap is `width * height` bytes.
- `font.horizontal_line_metrics(px) -> Option<LineMetrics>`
  (`font.rs:376`).
- `Metrics` fields (`font.rs:61-74`): `xmin`, `ymin` (`i32`, **may be
  negative** — glyph extends left/below baseline/origin), `width`, `height`
  (`usize`), `advance_width`/`advance_height` (`f32` subpixels), `bounds`.
- `LineMetrics` (`font.rs:97-108`): `ascent`, `descent` (negative),
  `line_gap`, `new_line_size = ascent - descent + line_gap`.
- `oh-render` wrappers: `Font::load` (`font.rs:35`), `Font::from_bytes`
  (`font.rs:44`), `line_metrics` (`font.rs:60`), `glyph_metrics`
  (`font.rs:66`), `rasterize` (`font.rs:71`). `Text` layout
  (character size default 30, fill/outline + thickness, local vs global
  bounds) mirrors `sf::Text` — see `lib-sfml.md` and
  `mapping/rendering-menu.md` §3.9-§4.3 (cited by `font.rs:9-11`).

## Headless limitation and the skip-with-reason tests (D-011 policy)

D-011 (`.agents/plans/001-rust-port/DECISIONS.md:200-207`): pure logic is
tested headless with real assertions; the GPU path gets **one** smoke test
that "skips with a printed reason — never a silent pass and never a fake
rasterizer standing in for GL".

Mechanics:

- **No display → no `EventLoop` / no GL context.** `GlBackend::try_create`
  (`crates/oh-render/src/gl.rs:60-67`) returns `Option<Self>`: `None` when
  `EventLoop::new()` fails, or when any `glutin` step errors; the whole
  bootstrap runs inside `std::panic::catch_unwind` so a display-panic
  degrades to `None` instead of aborting (`gl.rs:60-64`).
- `Window::create(w, h, title)` (`window.rs:306-317`) degrades to a headless
  window (`gl() == None`) when `try_create` returns `None`;
  `Window::headless(w, h)` (`window.rs:299-301`) is the explicit
  no-GL constructor. Headless: `run()` executes exactly one frame
  (`window.rs:582-597`), joystick API is clean no-ops
  (`window.rs:545-558`).
- **`tests/backend.rs::gpu_smoke`** (`tests/backend.rs:473-481`):
  `Window::create(64, 64, …)`; if `win.gl().is_none()` it prints
  `"SKIP: no display/GPU in this environment (winit/GL bootstrap failed or
  is unavailable); headless path active"` and returns. Every other test in
  the file (key/button mapping, event folding, headless window surface,
  camera math) runs with real assertions regardless of display.
- **`tests/golden_frames.rs::golden_frame_coarse_guard`**
  (`tests/golden_frames.rs:87-95,182-187`): `GlBackend::try_create(800, 450,
  …)`; `None =>` prints
  `"SKIP golden_frames: no display/GPU available (headless) — …"` and
  returns; `Some(backend)` proceeds to a real offscreen
  renderbuffer/framebuffer draw + `glReadPixels` coarse guard.

## sf:: → Rust surface map (quick orientation)

| SFML (`lib-sfml.md`) | `oh-render` file |
|---|---|
| `RenderWindow` + `Event` + `pollEvent` loop | `src/window.rs` (`Window`, `Event`, `map_window_event`, `run`) |
| `Keyboard::Key` / `Mouse::Button` | `oh-config` `Key`/`Button` (exact ints) + `window.rs` mappers |
| `Drawable::draw` / `RenderTarget` / vertex arrays | `src/batching.rs` (`FastVertexVector` analog) + `src/draw.rs` |
| `Texture` | `src/texture.rs` |
| `Font` / `Text` | `src/font.rs` (+ `src/text_draw.rs`) |
| `View` / camera | `src/view.rs`, `src/camera.rs` |
| `RenderStates` | `src/graphics.rs` |
| `Shader` / `Glsl` | `src/shader.rs` |
| `SoundBuffer`/`Sound`/`Music` + device | `src/audio.rs` |
| `Joystick` (stubs) | `src/input.rs` + `window.rs` |

## Claims NOT verified (stated honestly)

- Whether the `#[deprecated]` attribute on `EventLoop::create_window` /
  `WindowAttributes::new` carries a specific deprecation message: not quoted;
  only the oh-render `#[allow(deprecated)]` (`gl.rs:68`) is asserted.
- `glow`/`winit`/`glutin` internal behavior beyond the cited lines was not
  audited (e.g. platform driver paths); only the API surface above is
  documented.
- D-011 lists `ab_glyph` as an alternative font crate; only `fontdue` is in
  `Cargo.lock`/used — `ab_glyph` is not in the dependency tree
  (grep-verified in `Cargo.lock`).
