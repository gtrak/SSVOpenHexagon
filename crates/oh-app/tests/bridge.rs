//! Issue 15 step (a): the D-020 draw bridge — `HexagonGame::draw()`
//! (`HGGraphics.cpp:52-316`) driven from the engine's state.
//!
//! See the module docs in `src/bridge.rs` for the C++ draw-pass order the
//! bridge implements and the no-lock-guard-across-`draw_frame` discipline.

mod common;

use std::sync::Arc;

use oh_config::Config;

use common::{compose, release_dir, test_fonts, Recording};

fn load_release() -> Arc<oh_assets::Assets> {
    // C++: `hg::HGAssets assets{nullptr, true /* headless */};`
    Arc::new(oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"))
}

/// The exact pack/level ids the C++ test names (re-derived via
/// `buildPackId` against the shipped `pack.json`s — D-018; the same ids
/// as `oh-game`'s `tests/determinism.rs`).
const PACK: &str = "ohvrvanilla_vittorio_romeo_cube_1";
const LEVEL: &str = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";

/// A window=false engine → `draw` is a no-op (no clear, no vertices):
/// the C++ `if(window == nullptr || ...) return;` guard
/// (`HGGraphics.cpp:54`) — the engine's `window` ctor flag models
/// `window != nullptr`, so nothing after the guard is observed.
#[test]
fn draw_is_a_no_op_without_a_window() {
    let assets = load_release();
    let c = compose(assets, release_dir(), false);
    c.game.new_game(PACK, LEVEL, true, 1.0, false);

    let (font, font_bold) = test_fonts();
    let mut app = oh_app::bridge::AppDraw::new(&Config::default(), font, font_bold);
    let mut rec = Recording::default();
    oh_app::bridge::draw(&c.game, &mut app, &mut rec);

    assert!(rec.events.is_empty(), "window == nullptr ⇒ no events");
    // The bridge must not have touched the draw state either.
    assert!(app.state().background_camera.is_none());
    assert!(app.state().overlay_camera.is_none());
}
