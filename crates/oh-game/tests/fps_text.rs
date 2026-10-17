//! 029e3 (the graphics audit's M3): the FPS text must be the LIVE window
//! FPS — the C++'s `window->getFPS()` (the timer's `float fps`,
//! TimerBase.hpp:23: refreshed by `runFPS` at the end of each loop
//! iteration, GameWindow.hpp:148 — so a tick sees the PREVIOUS frame's
//! value, 0 on the very first frame) fed through the `Renderer` seam's
//! `fps()` (an `f32` now, not the pre-fix constant-`0` `u32`), formatted
//! as the C++'s `ssvu::toStr(float)` (HGGraphics.cpp:562 — the default
//! `std::ostream << float`: 6 significant digits, no trailing zeros;
//! `format_ostream_f32`'s oracle-pinned port). The pre-fix port's
//! `fps.to_string()` of the seam's `u32` default pinned the text to the
//! constant "0" forever (the live `timer.get_fps` was never read).
//!
//! Windowed engine only: the C++'s `updateText` runs per tick
//! (HGUpdate.cpp:384) and the port's engine half guards on `window`
//! (`update_text`'s first line).

mod common;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use oh_config::TIME_STEP;
use oh_game::renderer::Renderer;
use oh_game::seams::{NoJoystick, NullReplayStore, PcgCosmeticRng};

/// The exact pack/level ids the C++ test names (the same ids as
/// `oh-game`'s `tests/determinism.rs`).
const PACK: &str = "ohvrvanilla_vittorio_romeo_cube_1";
const LEVEL: &str = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";

fn load_release() -> Arc<oh_assets::Assets> {
    // C++: `hg::HGAssets assets{nullptr, true /* headless */};`
    Arc::new(
        oh_assets::Assets::new(common::release_dir(), true, false)
            .expect("_RELEASE loads headless"),
    )
}

/// A `Renderer` seam whose `fps()` reports a test-set value — the C++
/// `window->getFPS()` (the timer's `float fps`) the engine's FPS text
/// reads; the windowed client publishes `timer.get_fps()`'s bits into
/// the shared slot each frame (`WindowTouchRenderer::fps`, 029e3's
/// `fps_bits` slot), so the seam's value is the LIVE window rate.
struct FakeWindow {
    fps: Arc<AtomicU32>, // `f32::to_bits` storage — the C++'s `float fps`.
}

impl Renderer for FakeWindow {
    fn fps(&self) -> f32 {
        f32::from_bits(self.fps.load(Ordering::SeqCst))
    }
}

fn windowed_with_fps(fps: f32) -> (common::Composed, Arc<AtomicU32>) {
    let slot = Arc::new(AtomicU32::new(fps.to_bits()));
    let renderer: Arc<dyn Renderer> = Arc::new(FakeWindow { fps: slot.clone() });
    // The C++ test's seam defaults (the [`common::compose`] set).
    let c = common::compose_with_renderer(
        load_release(),
        common::release_dir(),
        true,
        Some(renderer),
        Arc::new(common::TestProfiles),
        Arc::new(NullReplayStore),
        Arc::new(NoJoystick),
        Arc::new(PcgCosmeticRng::new()),
    );
    (c, slot)
}

/// `ssvu::toStr(window->getFPS())` at a live (non-round) rate: 6
/// significant digits, no trailing zeros — 59.9342 → "59.9342" (the
/// pre-fix port's constant "0" is gone), and a round rate drops its
/// zeros — 120.0 → "120" (NOT `std::to_string`'s "120.000000", the
/// format the audit's M3 flagged as wrong).
#[test]
fn fps_text_is_the_live_window_fps_in_ostream_float_format() {
    let (c, slot) = windowed_with_fps(59.9342);
    // The C++'s `Config::getShowFPS()` gate (HGGraphics.cpp:560-564) —
    // the option must be on for the FPS text to update at all.
    c.config
        .write()
        .expect("config poisoned")
        .set_show_fps(true);

    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    // The C++'s per-tick `updateText` (HGUpdate.cpp:384).
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(
        c.game.fps_text(),
        "59.9342",
        "6 significant digits — `ssvu::toStr(float)`"
    );

    // The seam is live: the next tick sees the next value.
    slot.store(120.0f32.to_bits(), Ordering::SeqCst);
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(
        c.game.fps_text(),
        "120",
        "the default ostream format drops trailing zeros"
    );
}

/// The C++'s `TimerBase::fps{0}` initial value (TimerBase.hpp:23):
/// before the first `runFPS`, `window->getFPS()` is 0 — the FPS text is
/// "0" (the C++'s `ssvu::toStr(0.f)` = "0"; the windowed client's
/// `fps_bits` slot starts at `0.0f32.to_bits()`, the same value).
#[test]
fn fps_text_before_the_first_run_fps_is_zero() {
    let (c, _slot) = windowed_with_fps(0.0);
    c.config
        .write()
        .expect("config poisoned")
        .set_show_fps(true);

    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(c.game.fps_text(), "0");
}

/// The C++'s `Config::getShowFPS()` gate (HGGraphics.cpp:560-564): with
/// the option off (the `Config` default — the C++'s default), the FPS
/// text is never written: the ctor's empty string stays (the C++'s
/// `fpsText("", font)`).
#[test]
fn fps_text_gate_off_keeps_the_ctor_empty_string() {
    let (c, _slot) = windowed_with_fps(59.9342);
    // `Config::default` — showFPS off.

    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.game.update(TIME_STEP, 1.0);
    assert_eq!(c.game.fps_text(), "");
}
