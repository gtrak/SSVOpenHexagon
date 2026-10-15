//! Camera management for the game frame — the `ssvs::Camera` *usage* logic
//! that lives in `HexagonGame.cpp` / `HGUpdate.cpp` / `HGGraphics.cpp`
//! (the `Camera` type itself is `crate::view::Camera`, mapping §2).
//!
//! Sources (mapping/rendering-menu.md):
//! - §4.1 camera initial state (background: centre (0,0), size
//!   `width*zoomFactor × height*zoomFactor`; overlay: centre
//!   `(width/2, height/2)`, size `width × height`);
//! - §5.2 `refreshPulse` (the breathing zoom — replaces the whole
//!   background view, rotation preserved);
//! - §5.5 `updateCameraShake` (+ the `preShakeCenters` capture/restore);
//! - step 2 of §3.3 `draw()` (per-frame camera-centre setting from
//!   `levelStatus.cameraShake` jitter, or the fixed (0,0)/(w/2,h/2) centres);
//! - `death_shakeCamera` / `death_flashEffect` triggers.
//!
//! NOTE: the actual C++ `death_shakeCamera` (HexagonGame.cpp) resets the
//! overlay view, zeroes the background centre, and sets
//! `status.cameraShake = 45.f * Config::getCameraShakeMultiplier()` — it
//! does **not** touch `preShakeCenters` (those are captured lazily by
//! `update_camera_shake`). Mapping §4.6 describes an older variant; the
//! source file is authoritative (D-000).

use oh_math::ssvs::vector2::Vec2;

use crate::view::{Camera, View};

/// C++ `HexagonGame::PreShakeCenters` (HexagonGame.hpp) — the camera centres
/// captured before a shake starts, restored when the shake ends.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreShakeCenters {
    /// `backgroundCamera->getCenter()` at capture time.
    pub background: Vec2,
    /// `overlayCamera->getCenter()` at capture time.
    pub overlay: Vec2,
}

/// C++ constructor camera setup (§4.1): `(backgroundCamera, overlayCamera)`.
#[must_use]
pub fn init_cameras(width: f32, height: f32, zoom_factor: f32) -> (Camera, Camera) {
    let background = Camera::at_center_size(
        Vec2::new(0.0, 0.0),
        Vec2::new(width * zoom_factor, height * zoom_factor),
    );
    let overlay = Camera::at_center_size(
        Vec2::new(width / 2.0, height / 2.0),
        Vec2::new(width, height),
    );
    (background, overlay)
}

/// C++ `refreshPulse` (§5.2) — replaces the whole background view
/// (`p = pulse / pulseMin`, `1.f` when `NoPulse`), rotation preserved.
///
/// `pulse_min` is `levelStatus.pulseMin` (may be 0 when the level does not
/// define pulse — the C++ divides unconditionally; callers pass the live
/// status values).
pub fn refresh_pulse(
    background_camera: Option<&mut Camera>,
    width: u32,
    height: u32,
    zoom_factor: f32,
    no_pulse: bool,
    pulse: f32,
    pulse_min: f32,
) {
    // C++ asserts window + backgroundCamera exist; `None` mirrors
    // "window == nullptr" (the update loop only calls this with a window).
    let Some(camera) = background_camera else {
        return;
    };

    let p = if no_pulse { 1.0 } else { pulse / pulse_min };
    let rotation = camera.get_rotation();

    camera.set_view(View::new(
        Vec2::new(0.0, 0.0),
        Vec2::new(
            (width as f32 * zoom_factor) * p,
            (height as f32 * zoom_factor) * p,
        ),
    ));
    camera.set_rotation(rotation);
}

/// C++ `updateCameraShake` (§5.5). `camera_shake` is `status.cameraShake` —
/// a duration in frames·FT (NOT `levelStatus.cameraShake`, the draw-time
/// magnitude). The shake vectors come from the **game RNG**
/// (`rng.get_real`) — pass it as `get_real` (replay-relevant).
pub fn update_camera_shake(
    background_camera: Option<&mut Camera>,
    overlay_camera: Option<&mut Camera>,
    pre_shake_centers: &mut Option<PreShakeCenters>,
    camera_shake: &mut f32,
    ft: f32,
    get_real: &dyn Fn(f32, f32) -> f32,
) {
    // C++ returns when either camera is absent.
    let (Some(background), Some(overlay)) = (background_camera, overlay_camera) else {
        return;
    };

    if *camera_shake <= 0.0 {
        if let Some(pre) = pre_shake_centers.take() {
            background.set_center(pre.background);
            overlay.set_center(pre.overlay);
        }

        return;
    }

    *camera_shake -= ft;

    if pre_shake_centers.is_none() {
        *pre_shake_centers = Some(PreShakeCenters {
            background: background.get_center(),
            overlay: overlay.get_center(),
        });
    }

    let pre = pre_shake_centers.as_ref().expect("set above");

    // C++ `makeShakeVec`: two `rng.get_real(-i, i)` draws per camera,
    // background first (RNG order matters for replay parity).
    let i = *camera_shake;
    let bg_shake = Vec2::new(get_real(-i, i), get_real(-i, i));
    let ov_shake = Vec2::new(get_real(-i, i), get_real(-i, i));

    background.set_center(pre.background + bg_shake);
    overlay.set_center(pre.overlay + ov_shake);
}

/// C++ `death_shakeCamera` (HexagonGame.cpp) — verbatim: resets the overlay
/// view to the pixel-space default, zeroes the background centre, arms
/// `status.cameraShake` for `update_camera_shake`.
pub fn death_shake_camera(
    background_camera: Option<&mut Camera>,
    overlay_camera: Option<&mut Camera>,
    camera_shake: &mut f32,
    width: u32,
    height: u32,
    camera_shake_multiplier: f32,
) {
    // C++ asserts both cameras exist (window != nullptr).
    let (Some(background), Some(overlay)) = (background_camera, overlay_camera) else {
        panic!("death_shake_camera: both cameras must be set (window != nullptr)");
    };

    overlay.set_view(View::new(
        Vec2::new(width as f32 / 2.0, height as f32 / 2.0),
        Vec2::new(width as f32, height as f32),
    ));

    background.set_center(Vec2::new(0.0, 0.0));

    *camera_shake = 45.0 * camera_shake_multiplier;
}

/// Step 2 of C++ `draw()` (§3.3): set both camera centres — jitter-based
/// while `levelStatus.cameraShake > 0` (via the non-game RNG `ssvu::getRndR`,
/// pass as `jitter`), otherwise the fixed `(0, 0)` / `(w/2, h/2)` centres.
/// No-op while dead (`status.hasDied`).
pub fn draw_set_camera_centers(
    background_camera: Option<&mut Camera>,
    overlay_camera: Option<&mut Camera>,
    has_died: bool,
    level_status_camera_shake: f32,
    width: u32,
    height: u32,
    jitter: &dyn Fn(f32, f32) -> f32,
) {
    if has_died {
        return;
    }

    // C++ asserts both cameras exist at this point (a DrawState only
    // renders with a live window, which sets both cameras).
    let (Some(background), Some(overlay)) = (background_camera, overlay_camera) else {
        return;
    };

    if level_status_camera_shake > 0.0 {
        let shake = Vec2::new(
            jitter(-level_status_camera_shake, level_status_camera_shake),
            jitter(-level_status_camera_shake, level_status_camera_shake),
        );

        background.set_center(shake);
        overlay.set_center(Vec2::new(
            shake.x + width as f32 / 2.0,
            shake.y + height as f32 / 2.0,
        ));
    } else {
        background.set_center(Vec2::new(0.0, 0.0));
        overlay.set_center(Vec2::new(width as f32 / 2.0, height as f32 / 2.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::View;

    #[test]
    fn init_cameras_match_constructor_views() {
        // 800x450 window, zoomFactor 1.5.
        let (bg, ov) = init_cameras(800.0, 450.0, 1.5);
        assert_eq!(
            bg.get_view(),
            View::new(Vec2::new(0.0, 0.0), Vec2::new(1200.0, 675.0))
        );
        assert_eq!(
            ov.get_view(),
            View::new(Vec2::new(400.0, 225.0), Vec2::new(800.0, 450.0))
        );
    }

    #[test]
    fn refresh_pulse_replaces_view_and_preserves_rotation() {
        let (mut bg, _) = init_cameras(800.0, 450.0, 1.0);
        bg.set_rotation(90.0);
        // pulse 1.5 * pulseMin 0.5 -> p = 3.
        refresh_pulse(Some(&mut bg), 800, 450, 1.0, false, 1.5, 0.5);
        assert_eq!(bg.get_rotation(), 90.0);
        assert_eq!(
            bg.get_view().get_size(),
            Vec2::new(800.0 * 3.0, 450.0 * 3.0)
        );
        assert_eq!(bg.get_center(), Vec2::new(0.0, 0.0));

        // NoPulse -> p = 1.
        refresh_pulse(Some(&mut bg), 800, 450, 2.0, true, 0.0, 0.0);
        assert_eq!(bg.get_view().get_size(), Vec2::new(1600.0, 900.0));
    }

    #[test]
    fn update_camera_shake_captures_and_restores_pre_shake_centers() {
        let (mut bg, mut ov) = init_cameras(800.0, 450.0, 1.0);
        let mut pre: Option<PreShakeCenters> = None;
        let mut shake = 10.0;
        // deterministic jitter: always returns the range min.
        let jitter = |a: f32, b: f32| a.min(b);

        update_camera_shake(
            Some(&mut bg),
            Some(&mut ov),
            &mut pre,
            &mut shake,
            1.0,
            &jitter,
        );
        // captured at (0,0) and (400,225)
        assert_eq!(
            pre,
            Some(PreShakeCenters {
                background: Vec2::new(0.0, 0.0),
                overlay: Vec2::new(400.0, 225.0)
            })
        );
        // shake 10 -> 9 after the frame; jitter(-9,9) = -9
        assert_eq!(shake, 9.0);
        assert_eq!(bg.get_center(), Vec2::new(-9.0, -9.0));
        assert_eq!(ov.get_center(), Vec2::new(400.0 - 9.0, 225.0 - 9.0));

        // run until done — the pre-shake centres stay captured until the
        // NEXT frame (C++ `cameraShake <= 0` branch), which restores them.
        while shake > 0.0 {
            update_camera_shake(
                Some(&mut bg),
                Some(&mut ov),
                &mut pre,
                &mut shake,
                1.0,
                &jitter,
            );
        }
        assert!(pre.is_some());
        update_camera_shake(
            Some(&mut bg),
            Some(&mut ov),
            &mut pre,
            &mut shake,
            1.0,
            &jitter,
        );
        assert!(pre.is_none());
        assert_eq!(bg.get_center(), Vec2::new(0.0, 0.0));
        assert_eq!(ov.get_center(), Vec2::new(400.0, 225.0));
        assert!(pre.is_none());
    }

    #[test]
    fn draw_set_camera_centers_fixed_and_jittered() {
        let (mut bg, mut ov) = init_cameras(800.0, 450.0, 1.0);
        // jitter returns the midpoint of its range.
        let jitter = |a: f32, b: f32| (a + b) / 2.0;

        draw_set_camera_centers(Some(&mut bg), Some(&mut ov), false, 0.0, 800, 450, &jitter);
        assert_eq!(bg.get_center(), Vec2::new(0.0, 0.0));
        assert_eq!(ov.get_center(), Vec2::new(400.0, 225.0));

        // magnitude 4: jitter(-4, 4) = 0 here (midpoint); use an offset
        // jitter to see the shift.
        let jitter2 = |_a: f32, b: f32| b; // returns +magnitude
        draw_set_camera_centers(Some(&mut bg), Some(&mut ov), false, 4.0, 800, 450, &jitter2);
        assert_eq!(bg.get_center(), Vec2::new(4.0, 4.0));
        assert_eq!(ov.get_center(), Vec2::new(404.0, 229.0));

        // hasDied: untouched.
        let before = ov.get_center();
        draw_set_camera_centers(Some(&mut bg), Some(&mut ov), true, 4.0, 800, 450, &jitter2);
        assert_eq!(ov.get_center(), before);
    }

    #[test]
    fn death_shake_camera_arms_status_shake() {
        let (mut bg, mut ov) = init_cameras(800.0, 450.0, 1.5);
        bg.set_center(Vec2::new(7.0, 8.0));
        ov.set_center(Vec2::new(1.0, 2.0));
        let mut shake = 0.0;

        death_shake_camera(Some(&mut bg), Some(&mut ov), &mut shake, 800, 450, 2.0);
        assert_eq!(shake, 45.0 * 2.0);
        assert_eq!(
            ov.get_view(),
            View::new(Vec2::new(400.0, 225.0), Vec2::new(800.0, 450.0))
        );
        assert_eq!(bg.get_center(), Vec2::new(0.0, 0.0));
    }
}
