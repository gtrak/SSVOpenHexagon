//! Oracle tests for the gameplay geometry — assert the exact golden values in
//! `.agents/plans/001-rust-port/mapping/oracle-vectors.md` (the "Gameplay
//! geometry" section), reproduced by `bash tools/oracle/run.sh`.
//!
//! Each test mirrors a C++ fixture from `tools/oracle/gameplay_probe.cpp`.
//! Coordinates are compared with a small absolute tolerance (f32 libm may
//! differ in the last ulp across C/Rust); booleans/codes/ints are exact.

// The golden literals are the exact oracle decimal strings from
// mapping/oracle-vectors.md and must stay verbatim as acceptance evidence.
#![allow(clippy::excessive_precision)]

use oh_game::{CPlayer, CWall, SpeedData};
use oh_math::hg::geometry::get_line_circle_intersection;
use oh_math::ssvs::Vec2;

const TOL: f32 = 1e-3;

fn close(got: f32, want: f32, tol: f32, msg: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{msg}: got {got}, want {want} (tol {tol})"
    );
}

// --- Fixture A: CWall vertex construction ---------------------------------

#[test]
fn wall_vertices_sides6_side0() {
    let center = Vec2::new(0.0, 0.0);
    let speed = SpeedData::new(5.0, 0.0, 0.0, 0.0, false);
    let curve = SpeedData::new(0.0, 0.0, 0.0, 0.0, false);
    let w = CWall::new(
        6, 0.0, 0.0, 0.0, 0.0, &center, 0, 40.0, 1600.0, &speed, &curve, 0.0,
    );
    let v = w.get_vertex_positions();

    close(v[0].x, 1385.64062, TOL, "wall_v0.x");
    close(v[0].y, -800.0, TOL, "wall_v0.y");
    close(v[1].x, 1385.64062, TOL, "wall_v1.x");
    close(v[1].y, 800.0, TOL, "wall_v1.y");
    close(v[2].x, 1420.28162, TOL, "wall_v2.x");
    close(v[2].y, 820.0, TOL, "wall_v2.y");
    close(v[3].x, 1420.28162, TOL, "wall_v3.x");
    close(v[3].y, -820.0, TOL, "wall_v3.y");
}

#[test]
fn wall_overlap() {
    let center = Vec2::new(0.0, 0.0);
    let speed = SpeedData::new(5.0, 0.0, 0.0, 0.0, false);
    let curve = SpeedData::new(0.0, 0.0, 0.0, 0.0, false);
    let w = CWall::new(
        6, 0.0, 0.0, 0.0, 0.0, &center, 0, 40.0, 1600.0, &speed, &curve, 0.0,
    );

    assert!(
        w.is_overlapping(&Vec2::new(1400.0, 0.0)),
        "wall_overlap_inner_pt expected 1"
    );
    assert!(
        !w.is_overlapping(&Vec2::new(0.0, 0.0)),
        "wall_overlap_centroid expected 0"
    );
}

// --- Fixture B: CWall update (move-towards-center + curve) ----------------

#[test]
fn wall_update_move_and_curve() {
    let center = Vec2::new(0.0, 0.0);
    let speed = SpeedData::new(5.0, 0.0, 0.0, 0.0, false);
    let curve = SpeedData::new(3.0, 0.0, 0.0, 0.0, false);
    let mut w = CWall::new(
        6, 0.0, 0.0, 0.0, 0.0, &center, 0, 40.0, 1600.0, &speed, &curve, 0.0,
    );
    w.update(1600.0, 75.0, &center, 1.0);
    let v = w.get_vertex_positions();

    close(v[0].x, 1401.64404, TOL, "wall_u_v0.x");
    close(v[0].y, -718.344727, TOL, "wall_u_v0.y");
    close(v[1].x, 1322.92676, TOL, "wall_u_v1.x");
    close(v[1].y, 854.68689, TOL, "wall_u_v1.y");
    close(v[2].x, 1356.5249, TOL, "wall_u_v2.x");
    close(v[2].y, 876.39325, TOL, "wall_u_v2.y");
    close(v[3].x, 1437.24121, TOL, "wall_u_v3.x");
    close(v[3].y, -736.58844, TOL, "wall_u_v3.y");
    assert!(!w.is_dead(), "wall_dead expected 0");
}

// --- Fixture C: CPlayer position after setPlayerAngle + updatePosition ----

#[test]
fn player_position_after_set_angle() {
    let origin = Vec2::new(0.0, 0.0);
    let mut p = CPlayer::new(&origin, 8.0, 7.3, 9.45, 4.625);
    p.set_player_angle(0.5);
    p.update_position(75.0);

    let pos = p.get_position();
    close(pos.x, 65.8186951, TOL, "player_pos.x");
    close(pos.y, 35.9569168, TOL, "player_pos.y");
    close(p.get_player_angle(), 0.5, 1e-6, "player_angle");
    assert!(!p.is_ready_to_swap(), "player_ready_to_swap expected 0");
    assert!(!p.get_just_swapped(), "player_just_swapped expected 0");
}

// --- Fixture D: player push against a real wall ---------------------------

#[test]
fn player_push_against_wall() {
    let center = Vec2::new(0.0, 0.0);
    let speed = SpeedData::new(0.0, 0.0, 0.0, 0.0, false);
    let curve = SpeedData::new(0.0, 0.0, 0.0, 0.0, false);
    let w = CWall::new(
        6, 0.0, 0.0, 0.0, 0.0, &center, 0, 40.0, 75.0, &speed, &curve, 0.0,
    );

    let origin = Vec2::new(0.0, 0.0);
    let mut p = CPlayer::new(&origin, 8.0, 7.3, 9.45, 4.625);
    p.set_player_angle(0.0); // facing +x
    p.update_position(75.0);

    let radius_squared = 75.0f32 * 75.0f32 + 8.0f32;
    let pushed = p.push_cwall(1, 75.0, &w, &center, radius_squared, 1.0);

    assert!(pushed, "push_result expected 1 (player killed)");
    let pos = p.get_position();
    close(pos.x, 75.0, 1e-6, "push_pos.x");
    close(pos.y, 0.0, 1e-6, "push_pos.y");
}

// --- Fixture E: line/circle intersection codes ----------------------------

#[test]
fn line_circle_tangent_returns_one() {
    let mut a = Vec2::new(0.0, 0.0);
    let mut b = Vec2::new(0.0, 0.0);
    let code = get_line_circle_intersection(
        &mut a,
        &mut b,
        &Vec2::new(10.0, 0.0),
        &Vec2::new(10.0, 10.0),
        100.0,
    );
    assert_eq!(code, 1, "lci_code2 expected 1");
    close(a.x, 10.0, 1e-5, "lci a.x");
    close(a.y, 0.0, 1e-5, "lci a.y");
    // `b` is only written on a 2-intersection; it stays at its initial (0,0).
    close(b.x, 0.0, 0.0, "lci b.x (unwritten)");
    close(b.y, 0.0, 0.0, "lci b.y (unwritten)");
}

#[test]
fn line_circle_miss_returns_zero() {
    let mut a = Vec2::new(0.0, 0.0);
    let mut b = Vec2::new(0.0, 0.0);
    let code = get_line_circle_intersection(
        &mut a,
        &mut b,
        &Vec2::new(5.0, 0.0),
        &Vec2::new(5.0, 10.0),
        1.0,
    );
    assert_eq!(code, 0, "lci_code0 expected 0");
}
