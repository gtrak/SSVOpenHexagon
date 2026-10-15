//! Port of `include/SSVOpenHexagon/Components/CCustomWall.hpp` (+
//! `CCustomWallHandle.hpp`) — `hg::CCustomWall`.
//!
//! Custom walls are Lua-defined: their four vertices may be in *any* order, so
//! `isOverlapping` uses the even-odd `pointInPolygon<4>` (NOT the
//! cross-product `pointInFourVertexPolygon`). `_oldVertexPositions` mirrors the
//! previous frame so `push` can compute per-side sweep polygons.
//!
//! The two `CWFlags` are stored as a `u8` bitfield (bit 0 = `NoCollision`,
//! bit 1 = `Deadly`), matching `std::bitset<2>`.

use oh_math::hg::color::Color;
use oh_math::ssvs::Vec2;

use crate::polygon::point_in_polygon;
use crate::vertex_batch::FastVertexVectorTris;
use crate::wall::Wall;

/// `CCustomWallHandle` == `int`.
pub type CCustomWallHandle = i32;

const FLAG_NO_COLLISION: u8 = 1 << 0;
const FLAG_DEADLY: u8 = 1 << 1;

/// `hg::CCustomWall`.
#[derive(Debug, Clone, PartialEq)]
pub struct CCustomWall {
    vertex_positions: [Vec2; 4],
    old_vertex_positions: [Vec2; 4],
    vertex_colors: [Color; 4],
    killing_side: u8,
    flags: u8, // Default: collides, not deadly.
}

impl Default for CCustomWall {
    fn default() -> Self {
        Self {
            vertex_positions: [Vec2::new(0.0, 0.0); 4],
            old_vertex_positions: [Vec2::new(0.0, 0.0); 4],
            vertex_colors: [Color::default(); 4],
            killing_side: 0,
            flags: 0,
        }
    }
}

impl CCustomWall {
    /// Default-constructed custom wall (all vertices at origin, default colors,
    /// no flags).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `reset()` — clears the killing side and all flags (restores defaults).
    pub fn reset(&mut self) {
        self.killing_side = 0;
        self.flags = 0;
    }

    /// `draw(wallQuads)` — the per-vertex-color quad (two triangles).
    pub fn draw(&self, wall_quads: &mut FastVertexVectorTris) {
        wall_quads.unsafe_emplace_back_quad(
            self.vertex_positions[0],
            self.vertex_colors[0],
            self.vertex_positions[1],
            self.vertex_colors[1],
            self.vertex_positions[2],
            self.vertex_colors[2],
            self.vertex_positions[3],
            self.vertex_colors[3],
        );
    }

    /// `isOverlapping(point)` — even-odd test (vertices may be unordered).
    #[must_use]
    pub fn is_overlapping(&self, point: &Vec2) -> bool {
        point_in_polygon(&self.vertex_positions, point.x, point.y)
    }

    /// `setVertexPos(i, pos)` — `std::exchange` (old value captured into
    /// `_oldVertexPositions[i]`).
    pub fn set_vertex_pos(&mut self, vertex_index: usize, pos: Vec2) {
        self.old_vertex_positions[vertex_index] =
            std::mem::replace(&mut self.vertex_positions[vertex_index], pos);
    }

    /// `moveVertexPos(i, offset)`.
    pub fn move_vertex_pos(&mut self, vertex_index: usize, offset: Vec2) {
        self.old_vertex_positions[vertex_index] = self.vertex_positions[vertex_index];
        self.vertex_positions[vertex_index] += offset;
    }

    /// `moveVertexPos4Same(offset)` — all four vertices move together.
    pub fn move_vertex_pos4_same(&mut self, offset: Vec2) {
        self.old_vertex_positions = self.vertex_positions;
        for v in self.vertex_positions.iter_mut() {
            *v += offset;
        }
    }

    /// `setVertexColor(i, color)`.
    pub fn set_vertex_color(&mut self, vertex_index: usize, color: Color) {
        self.vertex_colors[vertex_index] = color;
    }

    /// `setCanCollide(collide)` — `NoCollision = !collide`.
    pub fn set_can_collide(&mut self, collide: bool) {
        self.flags = match collide {
            false => self.flags | FLAG_NO_COLLISION,
            true => self.flags & !FLAG_NO_COLLISION,
        };
    }

    /// `setDeadly(deadly)`.
    pub fn set_deadly(&mut self, deadly: bool) {
        self.flags = match deadly {
            true => self.flags | FLAG_DEADLY,
            false => self.flags & !FLAG_DEADLY,
        };
    }

    #[must_use]
    pub fn get_vertex_pos(&self, vertex_index: usize) -> Vec2 {
        self.vertex_positions[vertex_index]
    }

    #[must_use]
    pub fn get_vertex_positions(&self) -> [Vec2; 4] {
        self.vertex_positions
    }

    #[must_use]
    pub fn get_old_vertex_positions(&self) -> [Vec2; 4] {
        self.old_vertex_positions
    }

    /// `getCanCollide()` — `!NoCollision`.
    #[must_use]
    pub fn get_can_collide(&self) -> bool {
        self.flags & FLAG_NO_COLLISION == 0
    }

    /// `getDeadly()`.
    #[must_use]
    pub fn get_deadly(&self) -> bool {
        self.flags & FLAG_DEADLY != 0
    }

    #[must_use]
    pub fn is_custom_wall(&self) -> bool {
        true
    }

    /// `setKillingSide(side)`.
    pub fn set_killing_side(&mut self, side: u8) {
        self.killing_side = side;
    }

    #[must_use]
    pub fn get_killing_side(&self) -> u8 {
        self.killing_side
    }
}

impl Wall for CCustomWall {
    fn is_custom_wall(&self) -> bool {
        true
    }
    fn get_vertex_positions(&self) -> [Vec2; 4] {
        self.vertex_positions
    }
    fn get_killing_side(&self) -> u8 {
        self.killing_side
    }
}
