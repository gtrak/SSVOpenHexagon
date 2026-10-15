//! Port of `include/SSVOpenHexagon/Utils/FastVertexVector.hpp` — the
//! `FastVertexVectorTris` vertex batch the game draws walls/pivots/player
//! into (the `Triangles` specialisation).
//!
//! The C++ is a raw growable array of `sf::Vertex` (`{ position, color,
//! texCoords }`) with an explicit `reserve`/capacity bookkeeping and
//! "unsafe" emplace that appends vertices in a fixed order. What the game
//! observes is the **ordered vertex sequence**; capacity is a performance
//! concern with a debug assert. This port backs the batch with a
//! `std::vec::Vec<Vertex>` (which grows as needed and enforces its own bounds),
//! keeping the `reserve*` methods as capacity hints and the exact tri/quad
//! vertex ordering.
//!
//! Corner naming used throughout: **nw = vertex 0, sw = vertex 1, se = vertex
//! 2, ne = vertex 3**. A quad is split on the (nw, se) diagonal into two
//! triangles: `(nw, sw, se)` and `(nw, se, ne)`.

use oh_math::hg::color::Color;
use oh_math::ssvs::Vec2;

/// `sf::Vertex` analogue: position, color, texCoords (the game never sets
/// texCoords; they stay `(0, 0)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub position: Vec2,
    pub color: Color,
    pub tex_coords: Vec2,
}

/// `Utils::FastVertexVectorTris` (`FastVertexVector<sf::PrimitiveType::Triangles>`).
#[derive(Debug, Default, Clone)]
pub struct FastVertexVectorTris {
    vertices: Vec<Vertex>,
    capacity: usize,
}

impl FastVertexVectorTris {
    /// Empty batch.
    #[must_use]
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            capacity: 0,
        }
    }

    /// Ensure room for at least `needed` vertices (a capacity hint; the `Vec`
    /// grows as needed regardless, mirroring the C++ reserve intent).
    fn ensure_capacity(&mut self, needed: usize) {
        if self.capacity < needed {
            self.capacity = needed;
        }
        self.vertices
            .reserve(needed.saturating_sub(self.vertices.len()));
    }

    /// `reserve(n)` — C++ no-ops when `capacity >= n`, else reallocates to `n`.
    pub fn reserve(&mut self, n: usize) {
        self.ensure_capacity(n);
    }

    /// `reserve_more(n)` — `reserve(_size * 2 + n)`.
    pub fn reserve_more(&mut self, n: usize) {
        self.reserve(self.vertices.len() * 2 + n);
    }

    /// `reserve_more_quad(n)` — `reserve_more(n * 6)`.
    pub fn reserve_more_quad(&mut self, n: usize) {
        self.reserve_more(n * 6);
    }

    /// `reserve_quad(n)` — `reserve(n * 6)`.
    pub fn reserve_quad(&mut self, n: usize) {
        self.reserve(n * 6);
    }

    /// `batch_unsafe_emplace_back(color, positions...)` — appends one
    /// `sf::Vertex{position, color}` per position, in order.
    pub fn batch_unsafe_emplace_back(&mut self, color: Color, positions: &[Vec2]) {
        self.ensure_capacity(self.vertices.len() + positions.len());
        for &p in positions {
            self.vertices.push(Vertex {
                position: p,
                color,
                tex_coords: Vec2::new(0.0, 0.0),
            });
        }
    }

    /// `batch_unsafe_emplace_back_quad(color, nw, sw, se, ne)` — a quad as two
    /// triangles `(nw, sw, se)` and `(nw, se, ne)`: emits `nw, sw, se, nw, se,
    /// ne`.
    pub fn batch_unsafe_emplace_back_quad(
        &mut self,
        color: Color,
        nw: Vec2,
        sw: Vec2,
        se: Vec2,
        ne: Vec2,
    ) {
        self.batch_unsafe_emplace_back(color, &[nw, sw, se, nw, se, ne]);
    }

    /// `unsafe_emplace_back_quad(...)` — the per-vertex-color variant used by
    /// custom walls. Emits (in order): `nw, sw, se, nw, se, ne` with their
    /// respective colors (`nw` and `se` colors are reused for the two
    /// triangles).
    #[allow(clippy::too_many_arguments)]
    pub fn unsafe_emplace_back_quad(
        &mut self,
        nw: Vec2,
        color_nw: Color,
        sw: Vec2,
        color_sw: Color,
        se: Vec2,
        color_se: Color,
        ne: Vec2,
        color_ne: Color,
    ) {
        self.ensure_capacity(self.vertices.len() + 6);
        let push = |v: &mut Vec<Vertex>, p: Vec2, c: Color| {
            v.push(Vertex {
                position: p,
                color: c,
                tex_coords: Vec2::new(0.0, 0.0),
            });
        };
        push(&mut self.vertices, nw, color_nw);
        push(&mut self.vertices, sw, color_sw);
        push(&mut self.vertices, se, color_se);
        push(&mut self.vertices, nw, color_nw);
        push(&mut self.vertices, se, color_se);
        push(&mut self.vertices, ne, color_ne);
    }

    /// `clear()` — `_size = 0` (capacity retained).
    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    /// `size()`.
    #[must_use]
    pub fn size(&self) -> usize {
        self.vertices.len()
    }

    /// The ordered vertex sequence (for the render backend / tests).
    #[must_use]
    pub fn as_slice(&self) -> &[Vertex] {
        &self.vertices
    }
}
