//! `oh-app` — the application composition crate (DECISIONS.md D-020).
//!
//! The C++ `HexagonGame::draw()` (`HGGraphics.cpp:52-316`) is already
//! ported as `oh_render::graphics::draw_frame`; nothing in `oh-game` may
//! call it (D-013/D-020: `oh-game` and `oh-render` deliberately do not
//! depend on each other — `oh-lua` depends on `oh-game`, so `oh-game`
//! cannot gain a render dependency, and `oh-render` is a consumer of
//! vertex data, not of the simulation). The composition — building
//! `FrameInput`/`DrawState` from the engine's state each frame and
//! calling `draw_frame` — therefore lives here, in the one crate that
//! depends on BOTH.
//!
//! Modules (issue 15 step (a)):
//! - [`camera`] — the `oh_game::camera::Camera` → `oh_render::view::Camera`
//!   conversion (both port the same C++ `ssvs::Camera`);
//! - [`bridge`] — `AppDraw` (the per-frame draw state + scratch) and
//!   `draw()` (the `HexagonGame::draw` bridge).
//!
//! Issue 15 step (b): the `open-hexagon` binary pieces — [`args`] (the
//! `main.cpp:80-154` CLI surface), [`lifecycle`] (the `mainClient`
//! folder/config scope-guard pieces, `main.cpp:275-291`), [`compose`] (the
//! headless engine composition, promoted from the step-(a) test tree),
//! [`print_lua_docs`] (the `-printLuaDocs` entrypoint, `main.cpp:167-189`),
//! [`client`] (the `mainClient` headless replay run, `main.cpp:540-581`)
//! and [`dispatch`] (the `main()` dispatch, `main.cpp:604-686`, + the
//! not-yet-ported `mainServer` stub).
//!
//! Issue 15 step (c): [`game_state`] (the C++ `GameEngine` state-pointer /
//! `updateFromTimer` / `drawFromTimer` switching surface, spec §1) and the
//! windowed `mainClient` path (window + `TimerStatic` + MenuGame + the four
//! callbacks + boot dispatch + `window->run()`, in the C++
//! `GameWindow::runEvents` order — see `client::main_client` / `windowed`).
//!
//! Issue 14 step (b): [`lua_console_ui`] — the windowed egui Lua console
//! overlay (the C++ `postUpdate_ImguiLuaConsole`'s UI tail +
//! `drawImguiLuaConsole` on the D-022 egui stack — the composition's
//! windowed-only half; `oh-game` stays egui-free).
//!
//! 29j1: [`joystick`] (desktop-only) — the `RealJoystick` controller
//! poller (the gilrs backend for the C++ `sf::Joystick` controller-0
//! read) shared between the engine's `Joystick` seam and the menu's
//! `MenuJoystick` seam; Android/wasm keep the no-op seams.
//!
//! Issue 31: the gameplay touch overlay (the operator's blocker: the
//! tutorial's step-2 focus gate is unpassable on a touch-only device) —
//! the platform-neutral pointer-feed state machine + the key-icon
//! buttons, the D-034-class sanctioned deviation (the C++'s touch
//! controls are rotation-only, HGUpdate.cpp:589). Lane 057: the state
//! machine's HOME is the `oh-additions` crate (the home for everything
//! the C++ does not have) — this module re-exports it (the public
//! surface is unchanged) and still owns the wiring: the per-frame feed
//! and gate (`windowed`'s `tick_fields`), the consumed-pointer
//! exclusion, and the held-inputs apply
//! (`GameSlotAdapter::update_input`).

#[cfg(target_os = "android")]
pub mod android;
pub mod args;
pub mod bridge;
pub mod camera;
pub mod client;
pub mod compose;
// The `-server` / `-printLuaDocs` entries (the `oh-net` / `std::net` path)
// are desktop-only (W2: the web vertical slice has no online client).
#[cfg(not(target_os = "emscripten"))]
pub mod dispatch;
pub mod game_state;
#[cfg(not(any(target_os = "android", target_family = "wasm")))]
pub mod joystick;
pub mod lifecycle;
// The egui console overlay (issue 014b) is the windowed-only half of the
// composition (D-022): it needs `egui` / `egui-winit` / `egui_glow` /
// `glow 0.17`, which are gated off `wasm32-unknown-emscripten` (W2 — the
// web vertical slice ships without the console).
#[cfg(not(target_os = "emscripten"))]
pub mod lua_console_ui;
pub mod menu_draw;
// The `-printLuaDocs` entrypoint (the `oh-lua` docs path) — the `dispatch`
// module's only consumer; gated with it (W2).
#[cfg(not(target_os = "emscripten"))]
pub mod print_lua_docs;
pub mod replay_store;
// 29o1 — the headless / server SIGINT handlers (`signal-hook`, which is
// gated to `not(emscripten)` in this crate's Cargo.toml, W2: the web loop
// is a plain `request_animation_frame` — no signal handler).
#[cfg(not(target_os = "emscripten"))]
pub mod sigint;
// W2: `oh-steam` is desktop-only (depends on `oh-net`); the emscripten
// path uses `NoneSteamSink` directly (see `client.rs`).
#[cfg(not(target_os = "emscripten"))]
pub mod steam;
// Lane 057: the touch overlay's home is the `oh-additions` crate (the
// home for everything the C++ does not have); this re-export keeps the
// `oh_app::touch_overlay` path (the public surface unchanged).
pub use oh_additions::touch_overlay;
// The windowed loop (issue 15 step (c)) drives `oh_render::window::Window`
// (which has an emscripten backend — W1), so it COMPILES for emscripten
// (W2). Only the egui console overlay (the `lua_console_ui` hook) and the
// SIGINT handler are desktop-only (W2).
pub mod windowed;
