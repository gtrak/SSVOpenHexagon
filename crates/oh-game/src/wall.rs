//! Port of `include/SSVOpenHexagon/Components/CWall.hpp` +
//! `src/SSVOpenHexagon/Components/CWall.cpp` — `hg::CWall`.
//!
//! All positions are `f32`; every formula and literal is copied verbatim
//! (D-000). Vertex order is **nw(0), sw(1), se(2), ne(3)**.

use oh_math::hg::color::{transform_hue, Color};
use oh_math::ssvs::vector2::{get_mag, get_orbit_rad};
use oh_math::ssvs::Vec2;
use oh_math::ssvu::math::TAU;
use oh_math::FT;

use crate::polygon::point_in_four_vertex_polygon;
use crate::speed_data::SpeedData;
use crate::vertex_batch::FastVertexVectorTris;

/// Shared surface for the two wall kinds used by
/// `CPlayer::checkWallCollisionEscape` (the C++ template over `Wall`).
pub trait Wall {
    /// `isCustomWall()` — false for `CWall`, true for `CCustomWall`. Drives the
    /// escape-search vertex step (`2` for normal walls, `1` for custom).
    fn is_custom_wall(&self) -> bool;
    /// `getVertexPositions()`.
    fn get_vertex_positions(&self) -> [Vec2; 4];
    /// `getKillingSide()`.
    fn get_killing_side(&self) -> u8;
}

/// `hg::CWall` — a normal (non-custom) wall.
#[derive(Debug, Clone, PartialEq)]
pub struct CWall {
    vertex_positions: [Vec2; 4],
    speed: SpeedData,
    curve: SpeedData,
    hue_mod: f32,
    killed: bool,
}

impl CWall {
    /// C++ ctor (see mapping §2).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sides: u32,
        wall_angle_left: f32,
        wall_angle_right: f32,
        wall_skew_left: f32,
        wall_skew_right: f32,
        center_pos: &Vec2,
        side: i32,
        thickness: f32,
        distance: f32,
        speed: &SpeedData,
        curve: &SpeedData,
        hue_mod: f32,
    ) -> Self {
        // `div = tau / sides * 0.5f` (left-associative: `(tau/sides)*0.5`).
        let div = TAU / (sides as f32) * 0.5f32;
        // `angle = div * 2.f * side`.
        let angle = div * 2.0f32 * (side as f32);

        let angle_n = angle - div;
        let angle_p = angle + div;

        // `getOrbitRad(center, rad, dist)` == `center + (dist*cos, dist*sin)`.
        let vertex_positions = self_vertex_positions(
            center_pos,
            angle_n,
            angle_p,
            wall_angle_left,
            wall_angle_right,
            distance,
            thickness,
            wall_skew_left,
            wall_skew_right,
        );

        Self {
            vertex_positions,
            speed: *speed,
            curve: *curve,
            hue_mod,
            killed: false,
        }
    }

    /// `update(wallSpawnDist, radius, centerPos, ft)`.
    pub fn update(&mut self, wall_spawn_dist: f32, radius: f32, center_pos: &Vec2, ft: FT) {
        self.speed.update(ft);
        self.curve.update(ft);

        self.move_towards_center(wall_spawn_dist, radius, center_pos, ft);
        if self.curve.speed != 0.0f32 {
            self.move_curve(center_pos, ft);
        }
    }

    fn move_towards_center(
        &mut self,
        wall_spawn_dist: f32,
        radius: f32,
        center_pos: &Vec2,
        ft: FT,
    ) {
        let half_radius = radius * 0.5f32;
        let outer_bounds = wall_spawn_dist * 1.1f32;

        let mut points_out_of_bounds = 0;
        let mut points_on_center = 0;

        for vp in self.vertex_positions.iter_mut() {
            let x_distance = (vp.x - center_pos.x).abs();
            let y_distance = (vp.y - center_pos.y).abs();

            if x_distance < half_radius && y_distance < half_radius {
                points_on_center += 1;
                continue;
            }

            if x_distance > outer_bounds || y_distance > outer_bounds {
                points_out_of_bounds += 1;
            }

            // C++: `vp += (centerPos - vp).normalized() * _speed._speed * 5.f * ft;`
            // The scaling is applied in this exact left-to-right order (float
            // mult is not associative, so the order is preserved).
            let mut d = *center_pos - *vp;
            normalize_vec(&mut d);
            let t1 = d * self.speed.speed;
            let t2 = t1 * 5.0f32;
            let t3 = t2 * ft;
            *vp += t3;
        }

        if points_on_center == 4 || points_out_of_bounds == 4 {
            self.killed = true;
        }
    }

    /// `getCurveRadians(ft)` — `_curve._speed * (1.f/60.f) * ft`.
    #[must_use]
    pub fn get_curve_radians(&self, ft: FT) -> f32 {
        const DIV_BY_60: f32 = 1.0f32 / 60.0f32;
        self.curve.speed * DIV_BY_60 * ft
    }

    /// `moveVertexAlongCurve(vertex, centerPos, ft)` — the public single-vertex
    /// version used by `CPlayer::push` to pre-displace the test position.
    pub fn move_vertex_along_curve(&self, vertex: &mut Vec2, center_pos: &Vec2, ft: FT) {
        let rad = self.get_curve_radians(ft);
        Self::move_vertex_along_curve_impl(vertex, center_pos, rad.sin(), rad.cos());
    }

    /// `moveVertexAlongCurveImpl(vertex, centerPos, xSin, xCos)` — a 2D rotation
    /// of `vertex` about `centerPos` by angle `rad` (positive = CCW in math
    /// coords).
    pub fn move_vertex_along_curve_impl(
        vertex: &mut Vec2,
        center_pos: &Vec2,
        x_sin: f32,
        x_cos: f32,
    ) {
        let temp_x = vertex.x - center_pos.x;
        let temp_y = vertex.y - center_pos.y;
        vertex.x = temp_x * x_cos - temp_y * x_sin + center_pos.x;
        vertex.y = temp_x * x_sin + temp_y * x_cos + center_pos.y;
    }

    fn move_curve(&mut self, center_pos: &Vec2, ft: FT) {
        let rad = self.get_curve_radians(ft);
        let rad_sin = rad.sin();
        let rad_cos = rad.cos();

        for vp in self.vertex_positions.iter_mut() {
            Self::move_vertex_along_curve_impl(vp, center_pos, rad_sin, rad_cos);
        }
    }

    /// `draw(color, wallQuads)` — hue-shifts the color if `hueMod != 0`, then
    /// emits the quad.
    pub fn draw(&self, color: Color, wall_quads: &mut FastVertexVectorTris) {
        let color = if self.hue_mod != 0.0f32 {
            transform_hue(&color, self.hue_mod)
        } else {
            color
        };

        wall_quads.batch_unsafe_emplace_back_quad(
            color,
            self.vertex_positions[0],
            self.vertex_positions[1],
            self.vertex_positions[2],
            self.vertex_positions[3],
        );
    }

    pub fn set_hue_mod(&mut self, hue_mod: f32) {
        self.hue_mod = hue_mod;
    }

    #[must_use]
    pub fn get_vertex_positions(&self) -> [Vec2; 4] {
        self.vertex_positions
    }

    #[must_use]
    pub fn get_speed(&self) -> &SpeedData {
        &self.speed
    }

    #[must_use]
    pub fn get_curve(&self) -> &SpeedData {
        &self.curve
    }

    /// `isOverlapping(point)` — the four-vertex cross-product test.
    #[must_use]
    pub fn is_overlapping(&self, point: &Vec2) -> bool {
        point_in_four_vertex_polygon(
            &self.vertex_positions[0],
            &self.vertex_positions[1],
            &self.vertex_positions[2],
            &self.vertex_positions[3],
            point,
        )
    }

    #[must_use]
    pub const fn is_custom_wall(&self) -> bool {
        false
    }

    #[must_use]
    pub fn get_killing_side(&self) -> u8 {
        0
    }

    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.killed
    }
}

impl Wall for CWall {
    fn is_custom_wall(&self) -> bool {
        false
    }
    fn get_vertex_positions(&self) -> [Vec2; 4] {
        self.vertex_positions
    }
    fn get_killing_side(&self) -> u8 {
        0
    }
}

/// Builds the four wall vertices exactly as the C++ ctor (kept out of `new` to
/// keep the constructor body readable).
#[allow(clippy::too_many_arguments)]
#[must_use]
fn self_vertex_positions(
    center_pos: &Vec2,
    angle_n: f32,
    angle_p: f32,
    wall_angle_left: f32,
    wall_angle_right: f32,
    distance: f32,
    thickness: f32,
    wall_skew_left: f32,
    wall_skew_right: f32,
) -> [Vec2; 4] {
    let v0 = get_orbit_rad(center_pos, angle_n, distance);
    let v1 = get_orbit_rad(center_pos, angle_p, distance);
    let v2 = get_orbit_rad(
        center_pos,
        angle_p + wall_angle_left,
        distance + thickness + wall_skew_left,
    );
    let v3 = get_orbit_rad(
        center_pos,
        angle_n + wall_angle_right,
        distance + thickness + wall_skew_right,
    );
    [v0, v1, v2, v3]
}

/// `sf::Vector2f::normalized()` — normalizes in place; a zero vector is left
/// unchanged (SFML guards the zero case).
fn normalize_vec(v: &mut Vec2) {
    let mag = get_mag(v);
    if mag != 0.0f32 {
        v.x /= mag;
        v.y /= mag;
    }
}
