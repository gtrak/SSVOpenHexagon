//! The engine-camera → render-camera conversion (D-020, the draw bridge).
//!
//! Both types port the SAME C++ `ssvs::Camera`
//! (`extlibs/SSVStart/include/SSVStart/Camera/Camera.hpp`, verified
//! field-for-field against that source):
//!
//! - `oh_game::camera::Camera` — the UPDATE-loop surface the engine
//!   maintains (issue 08/09): the public view state (`center`, `size`,
//!   `rotation` in degrees) + `skew`, plus the deferred `pan` / `turn`
//!   deltas that `update(ft)` applies (zooms enter through `set_view`'s
//!   full-view replacement — the engine camera exposes no zoom delta).
//! - `oh_render::view::Camera` — the FULL `ssvs::Camera` (issue 10/11):
//!   the same public state wrapped in a `View`, the deferred deltas,
//!   `skew` / `offset`, and the `apply()` `computedView` bake
//!   (`size × skew`, the polar `offset` subtracted from the centre and
//!   rotated by the view rotation).
//!
//! The bridge (`crate::bridge`) rebuilds the render camera EVERY FRAME
//! from the engine camera's public state — the C++ `draw()` observes
//! exactly that (it never reads the deferred deltas: the update loop
//! ran `camera.update(ft)` first, `HGUpdate.cpp` camera block, and
//! `draw()` only uses `apply()` / `getRotation()` / `setCenter()` /
//! `setSkew()`, `HGGraphics.cpp:85-165`). The engine's update loop runs
//! `camera.update(ft)` on both cameras every windowed frame
//! (`oh-game/src/update.rs:415-422`).

use oh_game::camera::Camera as EngineCamera;
use oh_render::view::{Camera as RenderCamera, View};

/// `oh_game::camera::Camera` → `oh_render::view::Camera` (value-preserving
/// over the draw-observable state).
///
/// Field mapping (C++ `Camera` members in parens):
///
/// | render field | source |
/// |---|---|
/// | `view.center` (`view.getCenter()`) | `EngineCamera::get_center()` |
/// | `view.size` (`view.getSize()`) | `EngineCamera::get_size()` |
/// | `view.rotation` (`view.getRotation()`) | `EngineCamera::get_rotation()` — both sides store DEGREES (the C++ round-trips through `sf::Angle` but the value is the degree number) |
/// | `skew` (`skew`) | `EngineCamera::get_skew()` |
/// | `offset` (`offset`) | fixed `(0, 0)` — the C++ member defaults to zero and NO game code calls `setOffset` (verified: zero `setOffset` call sites in `src/`); the engine camera has no offset member |
/// | `computed_view` + `must_recompute` | the fresh-camera ctor state (`computed_view = view`, `must_recompute = true`) — C++ `Camera(const sf::View&)` leaves `mustRecompute == true`, so `apply()` bakes `view + skew + offset` on first use, exactly as here |
///
/// The deferred deltas (`nextPan` / `nextZoomFactor` / `nextRotation` /
/// `invalid`) are deliberately NOT carried: at the draw snapshot the
/// engine's update loop has already applied them to the public state via
/// `update(ft)` (the windowed update block, `update.rs`), and the C++
/// `draw()` never observes them. A fresh render camera's
/// `invalid == true` + zeroed deltas is behaviourally identical to the
/// C++ fresh-camera state for every `apply()` / `update()` sequence the
/// render path performs.
#[must_use]
pub fn to_render_camera(engine: &EngineCamera) -> RenderCamera {
    let mut view = View::new(engine.get_center(), engine.get_size());
    view.set_rotation(engine.get_rotation());

    // `from_view` leaves `offset = (0,0)`, `skew = (1,1)`,
    // `computed_view = view`, `must_recompute = true` (the C++ ctor
    // state).
    let mut camera = RenderCamera::from_view(view);
    camera.set_skew(engine.get_skew());

    camera
}

#[cfg(test)]
mod tests {
    use super::*;
    use oh_math::ssvs::Vec2;

    /// Value preservation over representative centres / sizes /
    /// rotations (spec: "value-preservation test over representative
    /// centers/sizes/rotations").
    #[test]
    fn conversion_preserves_center_size_rotation_and_skew() {
        let cases: [(Vec2, Vec2, f32, Vec2); 6] = [
            // The constructor cameras (800x600 window, zoom 1.27, C++
            // HexagonGame.cpp:318-331): background + overlay.
            (
                Vec2::new(0.0, 0.0),
                Vec2::new(800.0 * 1.27, 600.0 * 1.27),
                0.0,
                Vec2::new(1.0, 1.0),
            ),
            (
                Vec2::new(400.0, 300.0),
                Vec2::new(800.0, 600.0),
                0.0,
                Vec2::new(1.0, 1.0),
            ),
            // Death shake (C++ `death_shakeCamera` resets the overlay to
            // the pixel-space view, zeroes the background centre).
            (
                Vec2::new(0.0, 0.0),
                Vec2::new(1024.0, 768.0),
                0.0,
                Vec2::new(1.0, 1.0),
            ),
            (
                Vec2::new(512.0, 384.0),
                Vec2::new(1024.0, 768.0),
                0.0,
                Vec2::new(1.0, 1.0),
            ),
            // Rotations the update loop produces (turn() accumulates in
            // degrees; refreshPulse preserves the rotation).
            (
                Vec2::new(1.5, -2.25),
                Vec2::new(900.0, 500.0),
                90.0,
                Vec2::new(1.0, 1.0),
            ),
            (
                Vec2::new(-0.5, 0.5),
                Vec2::new(1200.0, 675.0),
                -30.0,
                Vec2::new(1.0, 1.4), // a 3D-style vertical skew
            ),
        ];

        for (center, size, rotation, skew) in cases {
            let engine = {
                let mut c = EngineCamera::new(center, size);
                c.set_rotation(rotation);
                c.set_skew(skew);
                c
            };
            let mut cam = to_render_camera(&engine);

            assert_eq!(cam.get_center(), center, "center ({center:?})");
            assert_eq!(cam.get_view().get_size(), size, "size ({size:?})");
            assert_eq!(cam.get_rotation(), rotation, "rotation ({rotation})");
            assert_eq!(cam.get_skew(), skew, "skew ({skew:?})");
            // The offset stays the C++ default (zero) — no game code
            // calls `setOffset`.
            assert_eq!(cam.get_offset(), Vec2::new(0.0, 0.0));
            // `apply()` must agree with a direct `View` construction for
            // the identity skew (no bake delta).
            if skew == Vec2::new(1.0, 1.0) {
                let mut expected = View::new(center, size);
                expected.set_rotation(rotation);
                assert_eq!(cam.apply(), expected, "identity-skew apply()");
            }
        }
    }

    /// `apply()` bakes the skew into the size (C++ `apply()`:
    /// `computedView.setSize(size * skew)`), rotated centre untouched.
    #[test]
    fn apply_bakes_skew_into_size() {
        let mut engine = EngineCamera::new(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        engine.set_skew(Vec2::new(1.0, 1.0 + 0.5)); // a `1 + effect` 3D skew
        let mut cam = to_render_camera(&engine);
        let view = cam.apply();
        assert_eq!(view.get_center(), Vec2::new(0.0, 0.0));
        assert_eq!(view.get_size(), Vec2::new(800.0, 600.0 * 1.5));
        // Cached: a second apply() without recompute gives the same view.
        assert_eq!(cam.apply(), view);
    }

    /// The engine applies deferred deltas in `update(ft)` BEFORE the draw
    /// snapshot; the conversion copies the POST-update public state.
    /// (Zooms reach the engine camera only through `set_view` — the
    /// `refreshPulse` full-view replacement; the engine camera exposes no
    /// `zoom_in`/`zoom_out` delta, so the zoom delta stays 1.0.)
    #[test]
    fn update_then_convert_matches_manual_view() {
        let mut engine = EngineCamera::new(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        engine.pan(Vec2::new(5.0, -5.0));
        engine.turn(90.0);
        engine.update(1.0);
        // C++ `update`: center += nextPan; rotation += 90.
        assert_eq!(engine.get_center(), Vec2::new(5.0, -5.0));
        assert_eq!(engine.get_size(), Vec2::new(800.0, 600.0));
        assert_eq!(engine.get_rotation(), 90.0);

        let cam = to_render_camera(&engine);
        let mut expected = View::new(Vec2::new(5.0, -5.0), Vec2::new(800.0, 600.0));
        expected.set_rotation(90.0);
        assert_eq!(cam.get_view(), expected);
        assert_eq!(cam.get_rotation(), 90.0);
    }
}
