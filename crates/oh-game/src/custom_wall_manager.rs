//! Port of `include/SSVOpenHexagon/Components/CCustomWallManager.hpp` +
//! `src/SSVOpenHexagon/Core/CCustomWallManager.cpp` — `hg::CCustomWallManager`.
//!
//! Handles are `int` indices with a free-list: `create` reserves a growing block
//! of handles on demand (first block: 32 + 2*nextFree), reuses freed handles,
//! and never shrinks. `destroyAllOutOfBounds` is declared in the C++ header but
//! has no definition (dead/unused) and is therefore NOT ported.
//!
//! `create(fAfterCreate)` takes a closure in the Rust port (the C++ takes a raw
//! `void(*)(CCustomWall&)` function pointer); the observable effect is the
//! same — the callback configures the freshly-reset wall before it is handed
//! out.

use oh_math::ssvs::Vec2;
use oh_math::FT;

use crate::custom_wall::{CCustomWall, CCustomWallHandle};
use crate::player::CPlayer;
use crate::vertex_batch::FastVertexVectorTris;
use oh_math::hg::color::Color;

/// A block of all-zero vertices returned as the `getVertexPos4` sentinel.
const fn zero_vertex_array() -> [Vec2; 4] {
    [Vec2::new(0.0, 0.0); 4]
}

/// `hg::CCustomWallManager`.
#[derive(Debug, Default, Clone)]
pub struct CCustomWallManager {
    custom_walls: Vec<CCustomWall>,
    free_handles: Vec<CCustomWallHandle>,
    handle_available: Vec<bool>,
    next_free_handle: CCustomWallHandle,
    count: usize,
    temp_alive_handles: Vec<CCustomWallHandle>,
}

impl CCustomWallManager {
    /// `CCustomWallManager()` (default).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn is_valid_handle(&self, h: CCustomWallHandle) -> bool {
        h >= 0
            && h < self.custom_walls.len() as CCustomWallHandle
            && h < self.handle_available.len() as CCustomWallHandle
    }

    fn check_valid_handle(&self, h: CCustomWallHandle, msg: &str) -> bool {
        if self.handle_available[h as usize] {
            eprintln!(
                "CustomWallManager: Attempted to {} of invalid custom wall {}\n",
                msg, h
            );
            debug_assert!(self.free_handles.contains(&h));
            return false;
        }
        debug_assert!(self.is_valid_handle(h));
        true
    }

    fn check_valid_vertex_idx(&self, vertex_idx: i32, h: CCustomWallHandle, msg: &str) -> bool {
        if (0..=3).contains(&vertex_idx) {
            return true;
        }
        eprintln!(
            "CustomWallManager: Invalid vertex index {} for custom wall {} while attempting to {}\n",
            vertex_idx, h, msg
        );
        false
    }

    fn check_valid_vertex_idx_and_handle(
        &self,
        h: CCustomWallHandle,
        vertex_idx: i32,
        msg: &str,
    ) -> bool {
        self.check_valid_vertex_idx(vertex_idx, h, msg) && self.check_valid_handle(h, msg)
    }

    /// `create(fAfterCreate)` — returns a freshly-reset custom wall configured
    /// by the callback.
    pub fn create(&mut self, f_after_create: impl Fn(&mut CCustomWall)) -> CCustomWallHandle {
        if self.free_handles.is_empty() {
            let reserve_size = 32 + (self.next_free_handle as usize) * 2;
            let max_handle_index = (self.next_free_handle as usize) + reserve_size;

            self.free_handles.reserve(max_handle_index);
            self.custom_walls
                .resize(max_handle_index, CCustomWall::new());
            self.handle_available.resize(max_handle_index, false);

            for i in 0..reserve_size {
                let h = self.next_free_handle + i as CCustomWallHandle;
                self.free_handles.push(h);
                self.handle_available[h as usize] = true;
            }

            self.next_free_handle = max_handle_index as CCustomWallHandle;
        }

        let res = *self.free_handles.last().expect("free handle");
        self.free_handles.pop();
        self.handle_available[res as usize] = false;
        self.count += 1;

        // Restore default state.
        let cw = &mut self.custom_walls[res as usize];
        cw.reset();
        f_after_create(cw);
        res
    }

    fn destroy_unchecked(&mut self, cw_handle: CCustomWallHandle) {
        debug_assert!(!self.handle_available[cw_handle as usize]);
        debug_assert!(self.is_valid_handle(cw_handle));
        self.handle_available[cw_handle as usize] = true;
        self.count -= 1;
        debug_assert!(!self.free_handles.contains(&cw_handle));
        self.free_handles.push(cw_handle);
    }

    /// `destroy(handle)` — no-ops (with a log) if the handle is not live.
    pub fn destroy(&mut self, cw_handle: CCustomWallHandle) {
        if self.handle_available[cw_handle as usize] {
            eprintln!(
                "CustomWallManager: Attempted to destroy invalid wall {}\n",
                cw_handle
            );
            return;
        }
        self.destroy_unchecked(cw_handle);
    }

    // --- Setters ---------------------------------------------------------

    pub fn set_vertex_pos(&mut self, h: CCustomWallHandle, vertex_idx: i32, pos: Vec2) {
        if !self.check_valid_vertex_idx_and_handle(h, vertex_idx, "set vertex pos") {
            return;
        }
        self.custom_walls[h as usize].set_vertex_pos(vertex_idx as usize, pos);
    }

    pub fn move_vertex_pos(&mut self, h: CCustomWallHandle, vertex_idx: i32, offset: Vec2) {
        if !self.check_valid_vertex_idx_and_handle(h, vertex_idx, "add vertex pos") {
            return;
        }
        self.custom_walls[h as usize].move_vertex_pos(vertex_idx as usize, offset);
    }

    pub fn move_vertex_pos4_same(&mut self, h: CCustomWallHandle, offset: Vec2) {
        if !self.check_valid_handle(h, "add four vertex pos same") {
            return;
        }
        self.custom_walls[h as usize].move_vertex_pos4_same(offset);
    }

    pub fn set_can_collide(&mut self, h: CCustomWallHandle, collide: bool) {
        if !self.check_valid_handle(h, "set collision") {
            return;
        }
        self.custom_walls[h as usize].set_can_collide(collide);
    }

    pub fn set_deadly(&mut self, h: CCustomWallHandle, deadly: bool) {
        if !self.check_valid_handle(h, "set deadly status") {
            return;
        }
        self.custom_walls[h as usize].set_deadly(deadly);
    }

    pub fn set_killing_side(&mut self, h: CCustomWallHandle, side: u8) {
        if side > 3 {
            eprintln!(
                "CustomWallManager: Attempted to set killing side with invalid value {}, acceptable values are 0 to 3\n",
                side
            );
            return;
        }
        if !self.check_valid_handle(h, "set killing side") {
            return;
        }
        self.custom_walls[h as usize].set_killing_side(side);
    }

    pub fn set_vertex_color(&mut self, h: CCustomWallHandle, vertex_idx: i32, color: Color) {
        if !self.check_valid_vertex_idx_and_handle(h, vertex_idx, "set vertex color") {
            return;
        }
        self.custom_walls[h as usize].set_vertex_color(vertex_idx as usize, color);
    }

    pub fn set_vertex_pos4(
        &mut self,
        h: CCustomWallHandle,
        p0: Vec2,
        p1: Vec2,
        p2: Vec2,
        p3: Vec2,
    ) {
        if !self.check_valid_handle(h, "set four vertex pos") {
            return;
        }
        let cw = &mut self.custom_walls[h as usize];
        cw.set_vertex_pos(0, p0);
        cw.set_vertex_pos(1, p1);
        cw.set_vertex_pos(2, p2);
        cw.set_vertex_pos(3, p3);
    }

    pub fn set_vertex_color4(
        &mut self,
        h: CCustomWallHandle,
        c0: Color,
        c1: Color,
        c2: Color,
        c3: Color,
    ) {
        if !self.check_valid_handle(h, "set four vertex color") {
            return;
        }
        let cw = &mut self.custom_walls[h as usize];
        cw.set_vertex_color(0, c0);
        cw.set_vertex_color(1, c1);
        cw.set_vertex_color(2, c2);
        cw.set_vertex_color(3, c3);
    }

    pub fn set_vertex_color4_same(&mut self, h: CCustomWallHandle, color: Color) {
        if !self.check_valid_handle(h, "set four vertex color same") {
            return;
        }
        let cw = &mut self.custom_walls[h as usize];
        cw.set_vertex_color(0, color);
        cw.set_vertex_color(1, color);
        cw.set_vertex_color(2, color);
        cw.set_vertex_color(3, color);
    }

    // --- Getters ---------------------------------------------------------

    /// `getVertexPos` — sentinel `(0, 0)` on an invalid handle/vertex.
    #[must_use]
    pub fn get_vertex_pos(&self, h: CCustomWallHandle, vertex_idx: i32) -> Vec2 {
        if !self.check_valid_vertex_idx_and_handle(h, vertex_idx, "get vertex pos") {
            return Vec2::new(0.0, 0.0);
        }
        self.custom_walls[h as usize].get_vertex_pos(vertex_idx as usize)
    }

    /// `getVertexPos4` — sentinel zero array on an invalid handle.
    #[must_use]
    pub fn get_vertex_pos4(&self, h: CCustomWallHandle) -> [Vec2; 4] {
        if !self.check_valid_handle(h, "get four vertex pos") {
            return zero_vertex_array();
        }
        self.custom_walls[h as usize].get_vertex_positions()
    }

    #[must_use]
    pub fn get_can_collide(&self, h: CCustomWallHandle) -> bool {
        if !self.check_valid_handle(h, "get collision") {
            return false;
        }
        self.custom_walls[h as usize].get_can_collide()
    }

    #[must_use]
    pub fn get_deadly(&self, h: CCustomWallHandle) -> bool {
        if !self.check_valid_handle(h, "get deadly status") {
            return false;
        }
        self.custom_walls[h as usize].get_deadly()
    }

    /// `getKillingSide` — returns `-1` (i.e. `255` as `u8`) on an invalid
    /// handle.
    #[must_use]
    pub fn get_killing_side(&self, h: CCustomWallHandle) -> u8 {
        if !self.check_valid_handle(h, "get killing side") {
            return (-1i32) as u8; // == 255
        }
        self.custom_walls[h as usize].get_killing_side()
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.free_handles.clear();
        self.custom_walls.clear();
        self.handle_available.clear();
        self.next_free_handle = 0;
        self.count = 0;
    }

    /// `draw(wallQuads)` — draws every live wall.
    pub fn draw(&self, wall_quads: &mut FastVertexVectorTris) {
        for h in 0..self.custom_walls.len() as CCustomWallHandle {
            if !self.handle_available[h as usize] {
                self.custom_walls[h as usize].draw(wall_quads);
            }
        }
    }

    /// `count()`.
    #[must_use]
    pub fn count(&self) -> usize {
        self.count
    }

    /// `maxHandles()`.
    #[must_use]
    pub fn max_handles(&self) -> usize {
        self.custom_walls.len()
    }

    /// `handleCollision(movement, radius, player, ft)` — the 3-phase algorithm.
    /// Returns `true` when the player must die.
    pub fn handle_collision(
        &mut self,
        movement: i32,
        radius: f32,
        m_player: &mut CPlayer,
        m_ft: FT,
    ) -> bool {
        // Get all alive (collidable) walls.
        {
            self.temp_alive_handles.clear();
            for h in 0..self.custom_walls.len() as CCustomWallHandle {
                if !self.handle_available[h as usize]
                    && self.custom_walls[h as usize].get_can_collide()
                {
                    self.temp_alive_handles.push(h);
                }
            }
        }

        let radius_squared = radius * radius;

        // Phase 1: kill on justSwapped OR deadly OR failed push.
        {
            let mut collided = false;
            for &h in &self.temp_alive_handles {
                let p_pos = m_player.get_position();
                if !self.custom_walls[h as usize].is_overlapping(&p_pos) {
                    continue;
                }

                if m_player.get_just_swapped()
                    || self.custom_walls[h as usize].get_deadly()
                    || m_player.push_c_custom_wall(
                        movement,
                        radius,
                        &self.custom_walls[h as usize],
                        radius_squared,
                        m_ft,
                    )
                {
                    return true;
                }

                collided = true;
            }

            if !collided {
                return false;
            }
        }

        // Phase 2: recheck on all walls (push, no swap/deadly short-circuit).
        {
            let mut collided = false;
            for &h in &self.temp_alive_handles {
                let p_pos = m_player.get_position();
                if !self.custom_walls[h as usize].is_overlapping(&p_pos) {
                    continue;
                }

                if m_player.push_c_custom_wall(
                    movement,
                    radius,
                    &self.custom_walls[h as usize],
                    radius_squared,
                    m_ft,
                ) {
                    return true;
                }

                collided = true;
            }

            if !collided {
                return false;
            }
        }

        // Phase 3: last round, no push — any overlap kills.
        for &h in &self.temp_alive_handles {
            let p_pos = m_player.get_position();
            if !self.custom_walls[h as usize].is_overlapping(&p_pos) {
                continue;
            }
            return true;
        }

        false
    }
}
