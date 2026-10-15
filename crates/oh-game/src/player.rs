//! Port of `include/SSVOpenHexagon/Components/CPlayer.hpp` +
//! `src/SSVOpenHexagon/Components/CPlayer.cpp` — `hg::CPlayer`.
//!
//! All positions/angles/speeds are `f32`. The collision-escape walk, the
//! push displacement, and the triangle-width transition are copied verbatim
//! (D-000). World space is origin-centered: the player's `_pos` lives in world
//! coordinates and `_angle` is `atan2(y, x)` from the world origin.

use oh_math::hg::color::{get_color_from_hue, Color};
use oh_math::hg::easing::get_smooth_step;
use oh_math::hg::geometry::{get_line_circle_closest_intersection, get_line_circle_intersection};
use oh_math::hg::move_towards::{
    get_move_towards, get_move_towards_zero, move_towards, move_towards_zero,
};
use oh_math::ssvs::vector2::{get_mag_squared, get_normalized, get_orbit_rad, get_rad};
use oh_math::ssvs::Vec2;
use oh_math::ssvu::math::{get_sign, to_rad, PI};
use oh_math::FT;

use crate::custom_wall::CCustomWall;
use crate::polygon::point_in_polygon;
use crate::ticker::Ticker;
use crate::vertex_batch::FastVertexVectorTris;
use crate::wall::{CWall, Wall};

// File-local constants (C++ `inline constexpr`, CPlayer.cpp top).
const BASE_THICKNESS: f32 = 5.0f32;
const UNFOCUSED_TRIANGLE_WIDTH: f32 = 3.0f32;
const FOCUSED_TRIANGLE_WIDTH: f32 = -1.5f32;
const TRIANGLE_WIDTH_RANGE: f32 = UNFOCUSED_TRIANGLE_WIDTH - FOCUSED_TRIANGLE_WIDTH; // 4.5
                                                                                     // At file scope just before the template `push` methods.
const COLLISION_PADDING: f32 = 0.5f32;

/// `ssvs::getDotProduct(a, b)` — `a.x*b.x + a.y*b.y`.
#[must_use]
fn get_dot_product(a: &Vec2, b: &Vec2) -> f32 {
    a.x * b.x + a.y * b.y
}

/// `hg::CPlayer`.
#[derive(Debug, Clone, PartialEq)]
pub struct CPlayer {
    start_pos: Vec2,
    pos: Vec2,
    // Position before push; NOT updated after a successful wall push.
    pre_push_pos: Vec2,
    // Previous-frame position, re-projected onto the current frame's radius.
    last_pos: Vec2,
    hue: f32,
    angle: f32,
    last_angle: f32,
    size: f32,
    speed: f32,
    focus_speed: f32,
    dead: bool,
    just_swapped: bool,
    // Set when the player was moved via `setPlayerAngle()`.
    forced_move: bool,
    radius: f32,
    // Max distance (squared) to the closest safe (non-colliding) position.
    max_safe_distance: f32,
    // Cached player speed this frame (degrees per FT-unit).
    current_speed: f32,
    triangle_width: f32,
    triangle_width_transition_time: f32,
    swap_timer: Ticker,
    swap_blink_timer: Ticker,
    dead_effect_timer: Ticker,
    curr_tilted_angle: f32,
}

impl CPlayer {
    /// C++ ctor.
    #[must_use]
    pub fn new(pos: &Vec2, swap_cooldown: f32, size: f32, speed: f32, focus_speed: f32) -> Self {
        Self {
            start_pos: *pos,
            pos: *pos,
            pre_push_pos: *pos,
            last_pos: *pos,
            hue: 0.0,
            angle: 0.0,
            last_angle: 0.0,
            size,
            speed,
            focus_speed,
            dead: false,
            just_swapped: false,
            forced_move: false,
            radius: 0.0,
            max_safe_distance: 0.0,
            current_speed: 0.0,
            triangle_width: UNFOCUSED_TRIANGLE_WIDTH,
            triangle_width_transition_time: 0.0,
            swap_timer: Ticker::new(swap_cooldown),
            swap_blink_timer: Ticker::new(6.0f32),
            dead_effect_timer: Ticker::new_running(80.0f32, false),
            curr_tilted_angle: 0.0,
        }
    }

    /// `getPosition()` — `_pos`.
    #[must_use]
    pub fn get_position(&self) -> Vec2 {
        self.pos
    }

    /// `getPlayerAngle()` — `_angle`.
    #[must_use]
    pub fn get_player_angle(&self) -> f32 {
        self.angle
    }

    /// `setPlayerAngle(newAng)` — sets the angle and marks a forced move.
    pub fn set_player_angle(&mut self, new_ang: f32) {
        self.angle = new_ang;
        self.forced_move = true;
    }

    /// `playerSwap()` — half a turn (`_angle += pi`).
    pub fn player_swap(&mut self) {
        self.angle += PI;
    }

    /// `kill(fatal)` — restarts the death-effect timer; a fatal kill also
    /// optionally rolls `_pos` back to the triangle tip.
    pub fn kill(&mut self, fatal: bool) {
        self.dead_effect_timer.restart();

        if fatal {
            self.dead = true;

            // Avoid moving back position if the player had just swapped or was
            // forcibly moved by a lot via Lua scripting.
            let dist = get_dist_euclidean(&self.pos, &self.last_pos);
            if !self.just_swapped && dist < 24.0f32 {
                // Show the triangle tip (not its center) hitting the wall.
                self.pos = get_orbit_rad(&self.last_pos, self.angle, -self.size);
            }
        }
    }

    /// `getColor(colorPlayer)`.
    #[must_use]
    pub fn get_color(&self, color_player: Color) -> Color {
        if !self.dead_effect_timer.is_running() {
            color_player
        } else {
            get_color_from_hue(self.hue / 360.0f32)
        }
    }

    /// `getColorAdjustedForSwap(colorPlayer)`.
    #[must_use]
    pub fn get_color_adjusted_for_swap(&self, color_player: Color) -> Color {
        if !self.swap_timer.is_running() && !self.dead {
            return get_color_from_hue((self.swap_blink_timer.get_current() / 12.0f32) % 0.2f32);
        }

        self.get_color(color_player)
    }

    /// `draw(...)` — pivot + (optional) death effect + the player triangle.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        sides: u32,
        color_main: Color,
        color_player: Color,
        wall_quads: &mut FastVertexVectorTris,
        cap_tris: &mut FastVertexVectorTris,
        player_tris: &mut FastVertexVectorTris,
        cap_color: Color,
        angle_tilt_intensity: f32,
        swap_blinking_effect: bool,
    ) {
        self.draw_pivot(sides, color_main, wall_quads, cap_tris, cap_color);

        if self.dead_effect_timer.is_running() {
            self.draw_death_effect(wall_quads);
        }

        let tilted_angle =
            self.angle + (self.curr_tilted_angle * to_rad(24.0f32) * angle_tilt_intensity);

        let p_left = get_orbit_rad(
            &self.pos,
            tilted_angle - to_rad(100.0f32),
            self.size + self.triangle_width,
        );
        let p_right = get_orbit_rad(
            &self.pos,
            tilted_angle + to_rad(100.0f32),
            self.size + self.triangle_width,
        );

        player_tris.reserve_more(3);
        let apex = get_orbit_rad(&self.pos, tilted_angle, self.size);
        let color = if swap_blinking_effect {
            self.get_color_adjusted_for_swap(color_player)
        } else {
            self.get_color(color_player)
        };
        player_tris.batch_unsafe_emplace_back(color, &[apex, p_left, p_right]);
    }

    fn draw_pivot(
        &self,
        sides: u32,
        color_main: Color,
        wall_quads: &mut FastVertexVectorTris,
        cap_tris: &mut FastVertexVectorTris,
        cap_color: Color,
    ) {
        // `div = tau / sides * 0.5f` (sides: unsigned int -> float).
        let div = oh_math::ssvu::math::TAU / (sides as f32) * 0.5f32;
        let p_radius = self.radius * 0.75f32;

        for i in 0..sides {
            let s_angle = div * 2.0f32 * (i as f32);

            let p1 = get_orbit_rad(&self.start_pos, s_angle - div, p_radius);
            let p2 = get_orbit_rad(&self.start_pos, s_angle + div, p_radius);
            let p3 = get_orbit_rad(&self.start_pos, s_angle + div, p_radius + BASE_THICKNESS);
            let p4 = get_orbit_rad(&self.start_pos, s_angle - div, p_radius + BASE_THICKNESS);

            wall_quads.reserve_more_quad(1);
            wall_quads.batch_unsafe_emplace_back_quad(color_main, p1, p2, p3, p4);

            cap_tris.reserve_more(3);
            cap_tris.batch_unsafe_emplace_back(cap_color, &[p1, p2, self.start_pos]);
        }
    }

    fn draw_death_effect(&self, wall_quads: &mut FastVertexVectorTris) {
        let div = oh_math::ssvu::math::TAU / 6.0f32 * 0.5f32;
        let d_radius = self.hue / 8.0f32;
        let thickness = self.hue / 20.0f32;

        let color_main = get_color_from_hue((360.0f32 - self.hue) / 360.0f32);

        for i in 0..6 {
            let s_angle = div * 2.0f32 * (i as f32);

            let p1 = get_orbit_rad(&self.pos, s_angle - div, d_radius);
            let p2 = get_orbit_rad(&self.pos, s_angle + div, d_radius);
            let p3 = get_orbit_rad(&self.pos, s_angle + div, d_radius + thickness);
            let p4 = get_orbit_rad(&self.pos, s_angle - div, d_radius + thickness);

            wall_quads.reserve_more_quad(1);
            wall_quads.batch_unsafe_emplace_back_quad(color_main, p1, p2, p3, p4);
        }
    }

    /// `checkWallCollisionEscape` — the C++ template method, generic over the
    /// wall kind (driven by `Wall::is_custom_wall`).
    fn check_wall_collision_escape<W: Wall>(
        &self,
        wall: &W,
        pos: &mut Vec2,
        radius_squared: f32,
    ) -> bool {
        let mut saved = false;
        let mut vec1 = Vec2::default();
        let mut vec2 = Vec2::default();
        let mut safe_distance = self.max_safe_distance;
        let vx_increment: u32 = if wall.is_custom_wall() { 1 } else { 2 };
        let w_vertexes = wall.get_vertex_positions();
        let killing_side = wall.get_killing_side() as u32;

        // C++ `for(unsigned int i{0u}, j{3u}; i < 4u; i += vxIncrement, j = i - 1)`
        // (`j` is unsigned, so `i - 1` underflow only ever yields the initial 3).
        let mut i: u32 = 0;
        let mut j: u32 = 3;
        while i < 4 {
            if j != killing_side {
                match get_line_circle_intersection(
                    &mut vec1,
                    &mut vec2,
                    &w_vertexes[i as usize],
                    &w_vertexes[j as usize],
                    radius_squared,
                ) {
                    1 => {
                        let temp_distance = get_mag_squared(&(vec1 - *pos));
                        if temp_distance < safe_distance {
                            *pos = vec1;
                            saved = true;
                            safe_distance = temp_distance;
                        }
                    }
                    2 => {
                        if get_mag_squared(&(vec1 - *pos)) > get_mag_squared(&(vec2 - *pos)) {
                            vec1 = vec2;
                        }
                        let temp_distance = get_mag_squared(&(vec1 - *pos));
                        if temp_distance < safe_distance {
                            *pos = vec1;
                            saved = true;
                            safe_distance = temp_distance;
                        }
                    }
                    _ => {}
                }
            }
            i = i.wrapping_add(vx_increment);
            j = i.wrapping_sub(1);
        }

        saved
    }

    /// `push` for a normal `CWall` — returns `true` when the player is killed.
    pub fn push_cwall(
        &mut self,
        movement_dir: i32,
        radius: f32,
        wall: &CWall,
        center_pos: &Vec2,
        radius_squared: f32,
        ft: FT,
    ) -> bool {
        if self.dead {
            return false;
        }

        let mut test_pos = self.pos;
        let mut push_vel = Vec2::new(0.0, 0.0);

        // Rotating-wall push: only when the rotation opposes the movement.
        let curve_data = wall.get_curve();
        if curve_data.speed != 0.0f32 && get_sign(curve_data.speed) != movement_dir {
            wall.move_vertex_along_curve(&mut test_pos, center_pos, ft);
            push_vel = test_pos - self.pos;
        }

        // Player not moving: displace by `2 * collisionPadding` and test.
        if movement_dir == 0 && !self.forced_move {
            let n = get_normalized(test_pos - self.pre_push_pos);
            self.pos = test_pos + n * (2.0f32 * COLLISION_PADDING);
            self.angle = get_rad(&self.pos);
            self.update_position(radius);
            return wall.is_overlapping(&self.pos);
        }

        // Moving: escape from the previous frame's position + push velocity.
        test_pos = self.last_pos + push_vel;
        if wall.is_overlapping(&test_pos)
            || !self.check_wall_collision_escape(wall, &mut test_pos, radius_squared)
        {
            return true;
        }

        // Survived: sit just outside the wall border.
        let n = get_normalized(test_pos - self.pre_push_pos);
        self.pos = test_pos + n * COLLISION_PADDING;
        self.angle = get_rad(&self.pos);
        self.update_position(radius);
        false
    }

    /// `push` for a `CCustomWall` — returns `true` when the player is killed.
    ///
    /// `ft` is unused in the C++ (documented with `(void)ft;`).
    pub fn push_c_custom_wall(
        &mut self,
        movement_dir: i32,
        radius: f32,
        wall: &CCustomWall,
        radius_squared: f32,
        _ft: FT,
    ) -> bool {
        if self.dead {
            return false;
        }

        let w_vertexes = wall.get_vertex_positions();
        let w_old_vertexes = wall.get_old_vertex_positions();
        let mut push_vel = Vec2::new(0.0, 0.0);
        let mut i1 = Vec2::default();
        let mut i2 = Vec2::default();
        let killing_side = wall.get_killing_side() as u32;
        // 0.15f — 0.1 would be enough in most scenarios, raised for fast walls.
        const PUSH_DOT_THRESHOLD: f32 = 0.15f32;

        // C++ `for(unsigned int i{0u}, j{3u}; i < 4; j = i++)` — `j` takes the
        // old `i`, then `i` increments: (0,3),(1,0),(2,1),(3,2).
        let mut i: u32 = 0;
        let mut j: u32 = 3;
        while i < 4 {
            if j != killing_side {
                let collision_polygon = [
                    w_vertexes[i as usize],
                    w_old_vertexes[i as usize],
                    w_old_vertexes[j as usize],
                    w_vertexes[j as usize],
                ];

                if point_in_polygon(&collision_polygon, self.last_pos.x, self.last_pos.y) {
                    // A side is an effective push source only if it intersected
                    // the player's circle both now and last frame.
                    if get_line_circle_closest_intersection(
                        &mut i1,
                        &self.last_pos,
                        &w_old_vertexes[i as usize],
                        &w_old_vertexes[j as usize],
                        radius_squared,
                    ) && get_line_circle_closest_intersection(
                        &mut i2,
                        &self.last_pos,
                        &w_vertexes[i as usize],
                        &w_vertexes[j as usize],
                        radius_squared,
                    ) {
                        push_vel = i2 - i1;
                        let dot = get_dot_product(
                            &get_normalized(push_vel),
                            &get_normalized(self.last_pos),
                        );
                        if dot.abs() > PUSH_DOT_THRESHOLD {
                            push_vel = Vec2::new(0.0, 0.0);
                        }
                    }
                    break;
                }
            }
            j = i;
            i = i.wrapping_add(1);
        }

        // Player not moving: exit now.
        if movement_dir == 0 && !self.forced_move {
            self.pos += push_vel;
            let n = get_normalized(self.pos - self.pre_push_pos);
            self.pos += n * (2.0f32 * COLLISION_PADDING);
            self.angle = get_rad(&self.pos);
            self.update_position(radius);
            return wall.is_overlapping(&self.pos);
        }

        // Alive: find a close enough safe position.
        let mut test_pos = self.last_pos + push_vel;
        if wall.is_overlapping(&test_pos)
            || !self.check_wall_collision_escape(wall, &mut test_pos, radius_squared)
        {
            return true;
        }

        let n = get_normalized(test_pos - self.pre_push_pos);
        self.pos = test_pos + n * COLLISION_PADDING;
        self.angle = get_rad(&self.pos);
        self.update_position(radius);

        false
    }

    /// `updateTriangleWidthTransition(focused, ft)` — C++ formulas verbatim.
    /// (The named constants only define the *range*; the formula maps 0->4.5
    /// and 1->0. Ported exactly as written, not "fixed".)
    fn update_triangle_width_transition(&mut self, focused: bool, ft: FT) {
        if focused && self.triangle_width_transition_time < 1.0f32 {
            move_towards(
                &mut self.triangle_width_transition_time,
                1.0f32,
                ft * 0.1f32,
            );
        } else if !focused && self.triangle_width_transition_time > 0.0f32 {
            move_towards_zero(&mut self.triangle_width_transition_time, ft * 0.1f32);
        }

        self.triangle_width = TRIANGLE_WIDTH_RANGE
            * (1.0f32 - get_smooth_step(0.0f32, 1.0f32, self.triangle_width_transition_time));
    }

    /// `update(focused, swapEnabled, ft)`.
    pub fn update(&mut self, focused: bool, swap_enabled: bool, ft: FT) {
        self.update_triangle_width_transition(focused, ft);

        if self.dead_effect_timer.is_running() {
            self.dead_effect_timer.update(ft);

            self.hue += 18.0f32 * ft;

            if self.hue > 360.0f32 {
                self.hue = 0.0f32;
            }

            if self.dead {
                return;
            }

            if self.dead_effect_timer.get_total() >= 100.0f32 {
                self.dead_effect_timer.stop();
                self.dead_effect_timer.reset_all();
            }
        }

        self.swap_blink_timer.update(ft / 3.0f32);

        if swap_enabled && self.swap_timer.update(ft) {
            self.swap_timer.stop();
        }

        self.last_angle = self.angle;
        self.forced_move = false;
    }

    /// `updateInputMovement(movementDir, playerSpeedMult, focused, ft)`.
    pub fn update_input_movement(
        &mut self,
        movement_dir: f32,
        player_speed_mult: f32,
        focused: bool,
        ft: FT,
    ) {
        self.current_speed = player_speed_mult
            * if focused {
                self.focus_speed
            } else {
                self.speed
            }
            * ft;
        self.angle += to_rad(self.current_speed * movement_dir);

        let inc = ft / 10.0f32;

        self.curr_tilted_angle = if movement_dir == 0.0f32 {
            get_move_towards_zero(self.curr_tilted_angle, inc)
        } else {
            get_move_towards(self.curr_tilted_angle, movement_dir, inc * 2.0f32)
        };
    }

    /// `resetSwap(swapCooldown)`.
    pub fn reset_swap(&mut self, swap_cooldown: f32) {
        self.swap_timer.restart_target(swap_cooldown);
        self.swap_blink_timer.restart_target(6.0f32);
    }

    /// `setJustSwapped(value)`.
    pub fn set_just_swapped(&mut self, value: bool) {
        self.just_swapped = value;
    }

    /// `updatePosition(radius)`.
    pub fn update_position(&mut self, radius: f32) {
        self.radius = radius;

        // C++: `_prePushPos = _pos = getOrbitRad(_startPos, _angle, _radius)`.
        let p = get_orbit_rad(&self.start_pos, self.angle, self.radius);
        self.pre_push_pos = p;
        self.pos = p;
        self.last_pos = get_orbit_rad(&self.start_pos, self.last_angle, self.radius);

        self.max_safe_distance = get_mag_squared(
            &(self.last_pos
                - get_orbit_rad(
                    &self.start_pos,
                    self.last_angle + to_rad(self.current_speed),
                    self.radius,
                )),
        ) + 32.0f32;
    }

    /// `getJustSwapped()`.
    #[must_use]
    pub fn get_just_swapped(&self) -> bool {
        self.just_swapped
    }

    /// `isReadyToSwap()` — `!_swapTimer.isRunning()`.
    #[must_use]
    pub fn is_ready_to_swap(&self) -> bool {
        !self.swap_timer.is_running()
    }

    /// `hasChangedAngle()` — `_angle != _lastAngle`.
    #[must_use]
    pub fn has_changed_angle(&self) -> bool {
        self.angle != self.last_angle
    }
}

/// `ssvs::getDistEuclidean(a, b)` — `sqrt(distSquared)`.
#[must_use]
fn get_dist_euclidean(a: &Vec2, b: &Vec2) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}
