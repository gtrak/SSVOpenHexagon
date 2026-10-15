//! Port of `include/SSVOpenHexagon/Utils/PointInPolygon.hpp` — the two
//! point-in-polygon tests the game uses.
//!
//! (The line/circle `Geometry` helpers, also listed under §12 of the mapping,
//! already live in `oh-math::hg::geometry`.)

use oh_math::ssvs::Vec2;

/// `hg::Utils::pointInPolygon<N>` — the classic even-odd ray-cast
/// (Paul Bourke's algorithm). `N` is the polygon's vertex count in C++; the
/// Rust form takes a slice and iterates the same `j = i++` loop.
///
/// `mVertices` must be in consistent winding order is NOT required here (this
/// is the parity test, used by custom walls whose vertices are Lua-defined).
#[must_use]
pub fn point_in_polygon(vertices: &[Vec2], x: f32, y: f32) -> bool {
    let mut result = false;

    let mut i = 0usize;
    let mut j = vertices.len() - 1;
    while i < vertices.len() {
        let v_i = &vertices[i];
        let v_j = &vertices[j];

        if (v_i.y > y) != (v_j.y > y) && x < (v_j.x - v_i.x) * (y - v_i.y) / (v_j.y - v_i.y) + v_i.x
        {
            result = !result;
        }

        j = i;
        i += 1;
    }

    result
}

/// `sf::Vector2f::cross(o)` — `x * o.y - y * o.x`.
#[must_use]
fn cross(v: Vec2, o: Vec2) -> f32 {
    v.x * o.y - v.y * o.x
}

/// `hg::Utils::pointInFourVertexPolygon` — sign-of-cross-product test (used by
/// `CWall::isOverlapping`). All four cross products must share a sign (the
/// point is on the same side of every directed edge). Requires the four
/// vertices to be in consistent CCW/CW order (the CWall nw/sw/se/ne ordering).
#[must_use]
pub fn point_in_four_vertex_polygon(a: &Vec2, b: &Vec2, c: &Vec2, d: &Vec2, point: &Vec2) -> bool {
    let ab = *b - *a;
    let bc = *c - *b;
    let cd = *d - *c;
    let da = *a - *d;

    let ap_ab = *point - *a;
    let bp_bc = *point - *b;
    let cp_cd = *point - *c;
    let dp_da = *point - *d;

    let ab_x_ap = cross(ab, ap_ab);
    let bc_x_bp = cross(bc, bp_bc);
    let cd_x_cp = cross(cd, cp_cd);
    let da_x_dp = cross(da, dp_da);

    (ab_x_ap <= 0.0f32 && bc_x_bp <= 0.0f32 && cd_x_cp <= 0.0f32 && da_x_dp <= 0.0f32)
        || (ab_x_ap >= 0.0f32 && bc_x_bp >= 0.0f32 && cd_x_cp >= 0.0f32 && da_x_dp >= 0.0f32)
}
