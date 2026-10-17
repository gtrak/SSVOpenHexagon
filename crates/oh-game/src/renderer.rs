//! The `Renderer` seam (plan 001 issue 09 / issues 10–11).
//!
//! `oh-game` must not depend on `oh-render` (the render backend lands in
//! issues 10/11 and is a *consumer* of this crate's vertex batches, not a
//! dependency of the simulation). The update loop is therefore
//! headless-capable: with no renderer attached the engine runs the C++
//! `window == nullptr` path exactly (no particle/camera/text presentation
//! work — the C++ `ReplayExecution` tests do precisely this).
//!
//! A renderer MAY be attached (e.g. a test recorder) to observe the batches
//! the engine keeps current during `update` — today that is the flash
//! polygon (`update_flash` sets every vertex alpha to `status.flashEffect`
//! each frame; the C++ draws it in `draw()`, issue 11). Attaching a
//! renderer does NOT re-enable the windowed particle/camera paths: those
//! require the platform window of issue 10, not a draw sink.

use oh_math::ssvs::Vec2;

use crate::lua_state::ReentrantMutex;
use crate::FastVertexVectorTris;

/// Draw sink for the engine's presentation work. `oh-render` (issues 10/11)
/// will implement this against the real GL stack.
pub trait Renderer: Send + Sync + 'static {
    /// Present the flash-effect batch for the frame just simulated
    /// (C++ `draw()` flash pass; state updated by `HexagonGame::update_flash`).
    fn draw_flash_effect(&self, _flash: &FastVertexVectorTris) {}

    /// C++ `window->getFingerDownPositions()` (touch input resolution in
    /// `updateInput_UpdateTouchControls`).
    fn finger_down_positions(&self) -> Vec<Vec2> {
        Vec::new()
    }

    /// C++ `window->getFPS()` (the FPS text in `updateText`). A `float`
    /// — the C++'s window FPS is the timer's `float fps` field
    /// (GameEngine.hpp:104-107 → TimerBase.hpp:23): the CURRENT window
    /// FPS, refreshed by `runFPS` at the end of each loop iteration
    /// (GameWindow.hpp:117-150 — runUpdate → runDraw → display → runFPS),
    /// so a draw pass reads the PREVIOUS iteration's value and the very
    /// first frame sees the field's initial `0.f`.
    fn fps(&self) -> f32 {
        0.0
    }

    /// C++ `window->getInputState()[ssvs::KKey(mKey)]` (HGScripting.cpp:
    /// 92 — `u_isKeyPressed`): the window's live key state. The default
    /// (`false`) IS the C++'s `window == nullptr` half — a composition
    /// with no window-backed renderer is headless.
    fn is_key_pressed(&self, _key_code: i32) -> bool {
        false
    }

    /// C++ `window->getInputState()[ssvs::MBtn(mKey)]` (HGScripting.cpp:
    /// 138 — `u_isMouseButtonPressed`): the window's live button state
    /// (the default's `false` is the C++'s `window == nullptr` half).
    fn is_mouse_button_pressed(&self, _button_code: i32) -> bool {
        false
    }

    /// C++ `window->getRenderWindow().getSize()` (the touch controls'
    /// half-split, HGUpdate.cpp:598 — `getSize().x / 2.f`: the ACTUAL
    /// render window's size, NOT the config's — the 025g lesson: a
    /// phone's 1530×678 window against its 1024×768 config). The
    /// default's `(0, 0)` is the no-renderer (headless) half — the
    /// touch controls' `finger_down_positions` is empty there too, so
    /// the zero never drives a split.
    fn render_size(&self) -> Vec2 {
        Vec2::new(0.0, 0.0)
    }
}

/// A no-op renderer that records every flash batch it is shown, so tests can
/// assert vertex output without a display.
#[derive(Debug, Default)]
pub struct RecordingRenderer {
    /// The flash batch as of each recorded call (a copy; the engine keeps
    /// mutating its own batch).
    flash_frames: ReentrantMutex<Vec<FlashFrame>>,
}

/// One recorded flash frame (a copy of the batch's vertex/color pairs).
type FlashFrame = Vec<(Vec2, [u8; 4])>;

impl RecordingRenderer {
    fn snapshot(batch: &FastVertexVectorTris) -> Vec<(Vec2, [u8; 4])> {
        batch
            .as_slice()
            .iter()
            .map(|v| (v.position, [v.color.r, v.color.g, v.color.b, v.color.a]))
            .collect()
    }

    /// Every recorded frame, oldest first.
    #[must_use]
    pub fn flash_frames(&self) -> Vec<Vec<(Vec2, [u8; 4])>> {
        self.flash_frames.lock().clone()
    }

    /// The most recent flash batch.
    #[must_use]
    pub fn last_flash(&self) -> Option<Vec<(Vec2, [u8; 4])>> {
        self.flash_frames.lock().last().cloned()
    }
}

impl Renderer for RecordingRenderer {
    fn draw_flash_effect(&self, flash: &FastVertexVectorTris) {
        self.flash_frames.lock().push(Self::snapshot(flash));
    }
}
