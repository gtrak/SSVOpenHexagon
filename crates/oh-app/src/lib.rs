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
//! Issue 31: [`touch_overlay`] — the gameplay touch overlay (the
//! operator's blocker: the tutorial's step-2 focus gate is unpassable
//! on a touch-only device) — the platform-neutral pointer-feed state
//! machine + the key-icon buttons, the D-034-class sanctioned
//! deviation (the C++'s touch controls are rotation-only, HGUpdate.
//! cpp:589).

#[cfg(target_os = "android")]
pub mod android;
pub mod args;
pub mod bridge;
pub mod camera;
pub mod client;
pub mod compose;
pub mod dispatch;
pub mod game_state;
#[cfg(not(any(target_os = "android", target_family = "wasm")))]
pub mod joystick;
pub mod lifecycle;
pub mod lua_console_ui;
pub mod menu_draw;
pub mod print_lua_docs;
pub mod replay_store;
pub mod sigint;
pub mod steam;
pub mod touch_overlay;
pub mod windowed;
