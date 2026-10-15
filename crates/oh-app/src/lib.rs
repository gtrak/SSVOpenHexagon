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
//! `main.cpp:80-154` CLI surface); the composition / lifecycle / mode
//! modules land with the later step-(b) milestones.
//!
//! Out of scope for step (b) (step (c)): the winit/glow window + event
//! pump, MenuGame↔HexagonGame switching, signals, the FPS text timing loop,
//! and the egui console overlay (issue 14 step (b)).

pub mod args;
pub mod bridge;
pub mod camera;
