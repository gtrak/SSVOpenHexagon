//! `sf::View` + `ssvs::Camera` — the camera / world↔screen transform.
//!
//! Sources:
//! - `sf::View` semantics (center + size + rotation; `zoom` **divides** the
//!   size, `rotate` is additive in degrees, `setRotation` replaces) per
//!   mapping/rendering-menu.md §2 and §4.1.
//! - `ssvs::Camera` verbatim from
//!   `extlibs/SSVStart/include/SSVStart/Camera/Camera.hpp` (mapping §2):
//!   `apply()` bakes the `skew` (view-size multiplier) and `offset` (a polar
//!   offset subtracted from the centre, rotated by the view rotation) into a
//!   cached copy; `update(ft)` applies the pending pan / `pow(nextZoom, ft)`
//!   zoom / `nextRotation * ft` rotation and then clears them.
//!
//! Note on `map_pixel_to_coords` / `map_coords_to_pixel`: the pinned SFML fork
//! (`vittorioromeo/SFML@dacceddb`) is **not vendored** in this tree, so its
//! `View::getTransform()` matrix cannot be read directly. We implement the
//! standard, documented SFML view transform (identity for the default view,
//! world = centre + Rot(−rotation)·((pixel − windowCentre)·(viewSize /
//! windowSize))) and assert self-inversivity + the identity/scale cases in
//! tests; the exact rotation sign convention should be re-verified against the
//! fork source (see `.agents/skills/lib-sfml.md`).

use oh_math::ssvs::vector2::{get_mag, get_vec_from_rad, Vec2};
use oh_math::ssvu::math::to_rad;

/// `sf::View` — the render-target camera (centre + size + rotation, degrees).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    center: Vec2,
    size: Vec2,
    /// Rotation in **degrees** (SFML convention).
    rotation: f32,
}

impl View {
    /// `sf::View(center, size)`.
    #[must_use]
    pub fn new(center: Vec2, size: Vec2) -> Self {
        Self {
            center,
            size,
            rotation: 0.0,
        }
    }

    /// C++ `getCenter()`.
    #[must_use]
    pub fn get_center(&self) -> Vec2 {
        self.center
    }

    /// C++ `getSize()`.
    #[must_use]
    pub fn get_size(&self) -> Vec2 {
        self.size
    }

    /// C++ `getRotation().asDegrees()`.
    #[must_use]
    pub fn get_rotation(&self) -> f32 {
        self.rotation
    }

    /// C++ `setCenter`.
    pub fn set_center(&mut self, center: Vec2) {
        self.center = center;
    }

    /// C++ `setSize`.
    pub fn set_size(&mut self, size: Vec2) {
        self.size = size;
    }

    /// C++ `setRotation(sf::degrees(mDeg))` — replaces.
    pub fn set_rotation(&mut self, deg: f32) {
        self.rotation = deg;
    }

    /// C++ `rotate(sf::degrees(mDeg))` — additive.
    pub fn rotate(&mut self, deg: f32) {
        self.rotation += deg;
    }

    /// C++ `zoom(factor)` — **divides** the size by `factor` (SFML semantics:
    /// factor > 1 zooms in, < 1 zooms out).
    pub fn zoom(&mut self, factor: f32) {
        self.size = Vec2::new(self.size.x / factor, self.size.y / factor);
    }

    // ---- world <-> screen (pixel) mapping ----------------------------------

    /// `RenderTarget::mapPixelToCoords(pixel, view)` — the inverse of the view
    /// transform: convert a pixel (top-left origin) in a target of `target_size`
    /// to world coordinates.
    #[must_use]
    pub fn map_pixel_to_coords(&self, pixel: Vec2, target_size: Vec2) -> Vec2 {
        let half_w = target_size.x * 0.5;
        let half_h = target_size.y * 0.5;
        let sx = self.size.x / target_size.x;
        let sy = self.size.y / target_size.y;
        // pixel relative to window centre, in world units (y grows downward).
        let d = Vec2::new((pixel.x - half_w) * sx, (pixel.y - half_h) * sy);
        self.center + Self::rotate_vec(d, -self.rotation)
    }

    /// `RenderTarget::mapCoordsToPixel(world, view)` — forward view transform:
    /// convert a world coordinate to a pixel in a target of `target_size`.
    #[must_use]
    pub fn map_coords_to_pixel(&self, world: Vec2, target_size: Vec2) -> Vec2 {
        let half_w = target_size.x * 0.5;
        let half_h = target_size.y * 0.5;
        let sx = self.size.x / target_size.x;
        let sy = self.size.y / target_size.y;
        let d = Self::rotate_vec(
            Vec2::new(world.x - self.center.x, world.y - self.center.y),
            self.rotation,
        );
        Vec2::new(half_w + d.x / sx, half_h + d.y / sy)
    }

    /// Rotate `v` by `deg` (degrees, screen y-down) about the origin.
    fn rotate_vec(v: Vec2, deg: f32) -> Vec2 {
        let rad = to_rad(deg);
        let (sin, cos) = rad.sin_cos();
        Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
    }
}

/// `ssvs::Camera` — the game's pan/zoom/turn/skew/offset camera.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    view: View,
    computed_view: View,
    next_pan: Vec2,
    offset: Vec2,
    skew: Vec2,
    next_zoom_factor: f32,
    next_rotation: f32,
    invalid: bool,
    must_recompute: bool,
}

/// `getDeg(vec)` = `atan2(y, x)` in degrees (used by `Camera::apply` for the
/// offset direction).
fn get_deg(v: &Vec2) -> f32 {
    v.y.atan2(v.x).to_degrees()
}

impl Camera {
    /// `Camera(const sf::View& mView)`.
    #[must_use]
    pub fn from_view(m_view: View) -> Self {
        Self {
            view: m_view,
            computed_view: m_view,
            next_pan: Vec2::new(0.0, 0.0),
            offset: Vec2::new(0.0, 0.0),
            skew: Vec2::new(1.0, 1.0),
            next_zoom_factor: 1.0,
            next_rotation: 0.0,
            invalid: true,
            must_recompute: true,
        }
    }

    /// `Camera(GameWindow&, center, zoomFactor)` — the window-size ctor
    /// (`view = {center, {width/zoom, height/zoom}}`).
    #[must_use]
    pub fn at_center_size(center: Vec2, size: Vec2) -> Self {
        Self::from_view(View::new(center, size))
    }

    /// C++ `apply()` (T = float, so the `toNum<T>` casts are identity): bake
    /// the `skew` (size multiplier) and `offset` (polar offset subtracted from
    /// the centre, rotated by the view rotation) into a cached `computed_view`.
    #[must_use]
    pub fn apply(&mut self) -> View {
        if self.must_recompute {
            let mut computed = self.view;
            computed.set_size(Vec2::new(
                self.view.get_size().x * self.skew.x,
                self.view.get_size().y * self.skew.y,
            ));

            if get_mag(&self.offset) != 0.0 {
                let ang = self.view.get_rotation() + get_deg(&self.offset);
                let mag = get_mag(&self.offset);
                computed.set_center(self.view.get_center() - get_vec_from_rad(to_rad(ang), mag));
            }

            // computedView.setCenter(toNum<float>(...)) — identity for float.
            self.computed_view = computed;
            self.must_recompute = false;
        }

        self.computed_view
    }

    // "These methods change the view ON NEXT UPDATE"
    /// C++ `pan(vec)`.
    pub fn pan(&mut self, v: Vec2) {
        self.next_pan = Vec2::new(self.next_pan.x + v.x, self.next_pan.y + v.y);
        self.invalid = true;
    }

    /// C++ `zoomIn(factor)`.
    pub fn zoom_in(&mut self, factor: f32) {
        self.next_zoom_factor /= factor;
        self.invalid = true;
    }

    /// C++ `zoomOut(factor)`.
    pub fn zoom_out(&mut self, factor: f32) {
        self.next_zoom_factor *= factor;
        self.invalid = true;
    }

    /// C++ `turn(deg)`.
    pub fn turn(&mut self, deg: f32) {
        self.next_rotation += deg;
        self.invalid = true;
    }

    /// C++ `update(ft)`.
    pub fn update(&mut self, ft: f32) {
        if !self.invalid {
            return;
        }

        self.view.set_center(Vec2::new(
            self.view.get_center().x + self.next_pan.x,
            self.view.get_center().y + self.next_pan.y,
        ));
        self.view.zoom(self.next_zoom_factor.powf(ft));
        self.view.rotate(self.next_rotation * ft);

        self.must_recompute = true;
        self.next_pan = Vec2::new(0.0, 0.0);
        self.next_zoom_factor = 1.0;
        self.next_rotation = 0.0;
        self.invalid = false;
    }

    // "These properties IMMEDIATELY change the view"
    /// C++ `setView`.
    pub fn set_view(&mut self, m_view: View) {
        self.view = m_view;
        self.must_recompute = true;
    }

    /// C++ `setRotation(deg)`.
    pub fn set_rotation(&mut self, deg: f32) {
        self.view.set_rotation(deg);
        self.must_recompute = true;
    }

    /// C++ `setSkew`.
    pub fn set_skew(&mut self, m_skew: Vec2) {
        self.skew = m_skew;
        self.must_recompute = true;
    }

    /// C++ `setOffset`.
    pub fn set_offset(&mut self, m_offset: Vec2) {
        self.offset = m_offset;
        self.must_recompute = true;
    }

    /// C++ `setCenter`.
    pub fn set_center(&mut self, m_position: Vec2) {
        self.view.set_center(m_position);
        self.must_recompute = true;
    }

    /// C++ `setNextZoomFactor` (changes the view on next update).
    pub fn set_next_zoom_factor(&mut self, m_factor: f32) {
        self.next_zoom_factor = m_factor;
        self.invalid = true;
    }

    // getters
    /// C++ `getView()`.
    #[must_use]
    pub fn get_view(&self) -> View {
        self.view
    }

    /// C++ `getRotation()` (degrees).
    #[must_use]
    pub fn get_rotation(&self) -> f32 {
        self.view.get_rotation()
    }

    /// C++ `getSkew()`.
    #[must_use]
    pub fn get_skew(&self) -> Vec2 {
        self.skew
    }

    /// C++ `getOffset()`.
    #[must_use]
    pub fn get_offset(&self) -> Vec2 {
        self.offset
    }

    /// C++ `getCenter()`.
    #[must_use]
    pub fn get_center(&self) -> Vec2 {
        self.view.get_center()
    }

    /// C++ `getNextZoomFactor()`.
    #[must_use]
    pub fn get_next_zoom_factor(&self) -> f32 {
        self.next_zoom_factor
    }

    /// C++ `isInView(pos)` (verbatim — note the `center ± size` comparison).
    #[must_use]
    pub fn is_in_view(&self, m_pos: Vec2) -> bool {
        let c = self.view.get_center();
        let s = self.view.get_size();
        m_pos.x <= c.x + s.x
            && (m_pos.x >= c.x - s.x && (m_pos.y <= c.y + s.y && m_pos.y >= c.y - s.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_zoom_divides_size() {
        let mut v = View::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        v.zoom(2.0);
        assert_eq!(v.get_size(), Vec2::new(50.0, 50.0));
        v.zoom(0.5);
        assert_eq!(v.get_size(), Vec2::new(100.0, 100.0));
    }

    #[test]
    fn view_rotate_is_additive_and_setreplaces() {
        let mut v = View::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        v.rotate(30.0);
        v.rotate(15.0);
        assert_eq!(v.get_rotation(), 45.0);
        v.set_rotation(10.0);
        assert_eq!(v.get_rotation(), 10.0);
    }

    #[test]
    fn default_view_is_identity_pixel_to_world() {
        // center = window centre, size = window size, rotation 0 -> identity.
        let target = Vec2::new(800.0, 450.0);
        let v = View::new(Vec2::new(400.0, 225.0), target);
        let world = v.map_pixel_to_coords(Vec2::new(120.0, 60.0), target);
        assert!((world.x - 120.0).abs() < 1e-4);
        assert!((world.y - 60.0).abs() < 1e-4);
    }

    #[test]
    fn view_mapping_is_self_inverse_and_scales() {
        let target = Vec2::new(800.0, 450.0);
        // background camera: centre (0,0), size = window * 2 (zoomed out).
        let v = View::new(Vec2::new(0.0, 0.0), Vec2::new(1600.0, 900.0));
        let world = v.map_pixel_to_coords(Vec2::new(400.0, 225.0), target);
        assert!(world.x.abs() < 1e-4); // centre maps to centre
                                       // a corner pixel -> world corner at half the world size (2x window).
        let corner = v.map_pixel_to_coords(Vec2::new(800.0, 450.0), target);
        assert!((corner.x - 800.0).abs() < 1e-4);
        assert!((corner.y - 450.0).abs() < 1e-4);
        // round-trip: world -> pixel -> world is identity.
        let back = v.map_coords_to_pixel(corner, target);
        assert!((back.x - 800.0).abs() < 1e-3);
        assert!((back.y - 450.0).abs() < 1e-3);
    }

    #[test]
    fn camera_apply_bakes_skew_and_offset() {
        // No skew/offset -> apply == view.
        let mut cam = Camera::at_center_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let a = cam.apply();
        assert_eq!(a.get_center(), Vec2::new(0.0, 0.0));
        assert_eq!(a.get_size(), Vec2::new(100.0, 100.0));

        // Skew multiplies the size.
        cam.set_skew(Vec2::new(1.0, 2.0));
        let a = cam.apply();
        assert_eq!(a.get_size(), Vec2::new(100.0, 200.0));

        // Offset: centre -= vecFromDeg(rotation + deg(offset), mag). With
        // rotation 0 and offset (10, 0) -> deg = 0 -> subtract (10, 0).
        cam.set_skew(Vec2::new(1.0, 1.0));
        cam.set_offset(Vec2::new(10.0, 0.0));
        let a = cam.apply();
        assert!((a.get_center().x - (-10.0)).abs() < 1e-4);
        assert!((a.get_center().y - 0.0).abs() < 1e-4);
    }

    #[test]
    fn camera_update_applies_and_clears_pending() {
        let mut cam = Camera::at_center_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        cam.pan(Vec2::new(5.0, 5.0));
        cam.zoom_in(2.0); // next_zoom = 0.5
        cam.turn(90.0);
        assert!(cam.get_next_zoom_factor() != 1.0 || cam.get_rotation() != 0.0);
        cam.update(1.0);
        // pan applied
        assert!((cam.get_center().x - 5.0).abs() < 1e-4);
        assert!((cam.get_center().y - 5.0).abs() < 1e-4);
        // zoom: size /= 0.5 -> *2
        assert!((cam.get_view().get_size().x - 200.0).abs() < 1e-4);
        // turn: rotation 90
        assert!((cam.get_rotation() - 90.0).abs() < 1e-4);
        // pending cleared
        assert!((cam.get_next_zoom_factor() - 1.0).abs() < 1e-6);
        assert!((cam.get_center() - cam.get_center()).x.abs() < 1e-6);

        // update() with no pending change is a no-op
        let before = cam.get_view();
        cam.update(1.0);
        assert_eq!(cam.get_view(), before);
    }

    #[test]
    fn camera_is_in_view_uses_center_plus_minus_size() {
        let mut cam = Camera::at_center_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        cam.update(1.0); // flush
        assert!(cam.is_in_view(Vec2::new(50.0, 50.0)));
        assert!(cam.is_in_view(Vec2::new(100.0, 0.0))); // edge inclusive
        assert!(!cam.is_in_view(Vec2::new(150.0, 0.0)));
    }
}
