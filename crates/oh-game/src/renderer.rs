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

    /// C++ `window->getFPS()` (the FPS text in `updateText`).
    fn fps(&self) -> u32 {
        0
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
