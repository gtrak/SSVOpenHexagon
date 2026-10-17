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

#[cfg(target_os = "android")]
pub mod android;
pub mod args;
pub mod bridge;
pub mod camera;
// The `mainClient` entry (issue 15 step (b)) loads content through
// `oh_vfs::RealFs`, which is `#[cfg(not(target_arch = "wasm32"))]` (the web
// content path is `MemoryVfs` / in-memory bytes — sub-goal 4B). So the
// client + its dispatch + the `-printLuaDocs` entry do not COMPILE for
// emscripten yet; the web client lands in the `oh-web` crate (sub-goal 4B).
#[cfg(not(target_os = "emscripten"))]
pub mod client;
pub mod compose;
#[cfg(not(target_os = "emscripten"))]
pub mod dispatch;
pub mod game_state;
pub mod lifecycle;
// The egui console overlay (issue 014b) is the windowed-only half of the
// composition (D-022): it needs `egui` / `egui-winit` / `egui_glow` /
// `glow 0.17`, which are gated off `wasm32-unknown-emscripten` (sub-goal
// 4A — the web vertical slice ships without the console, 26c §5).
#[cfg(not(target_os = "emscripten"))]
pub mod lua_console_ui;
pub mod menu_draw;
#[cfg(not(target_os = "emscripten"))]
pub mod print_lua_docs;
pub mod steam;
// The windowed loop (issue 15 step (c)) drives `oh_render::window::Window`
// through the winit event loop + the egui console hook; both are
// desktop-only (sub-goal 4A). The web client's windowed composition lands
// in the `oh-web` crate (sub-goal 4B).
#[cfg(not(target_os = "emscripten"))]
pub mod windowed;
