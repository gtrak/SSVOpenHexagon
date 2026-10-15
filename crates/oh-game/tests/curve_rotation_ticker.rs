//! Curve-rotation sign-convention and Ticker-overflow tests (task spec
//! verification items 2 and 3).

use oh_game::{CWall, SpeedData, Ticker};
use oh_math::ssvs::Vec2;

// --- Curve radians ---------------------------------------------------------

#[test]
fn curve_radians_is_speed_over_60_times_ft() {
    let center = Vec2::new(0.0, 0.0);
    let speed = SpeedData::new(0.0, 0.0, 0.0, 0.0, false);
    let curve = SpeedData::new(30.0, 0.0, 0.0, 0.0, false);
    let w = CWall::new(
        6, 0.0, 0.0, 0.0, 0.0, &center, 0, 40.0, 1600.0, &speed, &curve, 0.0,
    );

    // getCurveRadians(ft) = _curve._speed * (1.f/60.f) * ft = 30 * (1/60) * 2.
    let rad = w.get_curve_radians(2.0);
    assert!((rad - 1.0).abs() < 1e-5, "rad = {rad}");
}

// --- Rotation sign convention (hand-computed sin/cos) ----------------------

#[test]
fn rotation_impl_sign_ccw_for_positive_sin() {
    // move_vertex_along_curve_impl:
    //   x' = tempX * xCos - tempY * xSin + cx
    //   y' = tempX * xSin + tempY * xCos + cy
    // with vertex (1, 0) about center (0, 0) and (xSin, xCos) = (0.5, 0.8660254):
    //   x' = 1*0.8660254 - 0*0.5 = 0.8660254
    //   y' = 1*0.5       + 0*0.8660254 = 0.5
    // Positive xSin rotates +x toward +y (CCW in math coords) — the same
    // convention as getOrbitRad/getVecFromRad.
    let mut v = Vec2::new(1.0, 0.0);
    let center = Vec2::new(0.0, 0.0);
    CWall::move_vertex_along_curve_impl(&mut v, &center, 0.5f32, 0.8660254f32);
    assert!((v.x - 0.8660254f32).abs() < 1e-6, "v.x = {}", v.x);
    assert!((v.y - 0.5f32).abs() < 1e-6, "v.y = {}", v.y);
}

#[test]
fn move_vertex_along_curve_matches_hand_sin_cos() {
    let center = Vec2::new(0.0, 0.0);
    let speed = SpeedData::new(0.0, 0.0, 0.0, 0.0, false);
    let curve = SpeedData::new(30.0, 0.0, 0.0, 0.0, false);
    let w = CWall::new(
        6, 0.0, 0.0, 0.0, 0.0, &center, 0, 40.0, 1600.0, &speed, &curve, 0.0,
    );

    let rad = 30.0f32 * (1.0f32 / 60.0f32) * 2.0f32; // == getCurveRadians(2.0)
    let mut v = Vec2::new(1.0, 0.0);
    w.move_vertex_along_curve(&mut v, &center, 2.0);

    let want_x = rad.cos(); // 1*cos(rad) - 0*sin(rad)
    let want_y = rad.sin(); // 1*sin(rad) + 0*cos(rad)
    assert!(
        (v.x - want_x).abs() < 1e-5,
        "v.x = {}, want {}",
        v.x,
        want_x
    );
    assert!(
        (v.y - want_y).abs() < 1e-5,
        "v.y = {}, want {}",
        v.y,
        want_y
    );
}

// --- Ticker overflow / pause / loop ----------------------------------------

#[test]
fn ticker_drops_overflow_on_fire() {
    let mut t = Ticker::new(10.0); // running, loop = true
    assert!(!t.update(8.0)); // current = 8 < 10
    assert!((t.get_current() - 8.0).abs() < 1e-6);

    // current -> 13 (>= 10): fire, ticks += 1, current RESET to 0 (the 3.0
    // overflow beyond the target is dropped, not carried).
    assert!(t.update(5.0));
    assert_eq!(t.get_ticks(), 1);
    assert!(
        (t.get_current() - 0.0).abs() < 1e-6,
        "current = {}",
        t.get_current()
    );
    assert!((t.get_total() - 13.0).abs() < 1e-6);
}

#[test]
fn ticker_paused_never_advances() {
    let mut t = Ticker::new_running(5.0, false);
    assert!(!t.update(100.0));
    assert!((t.get_current() - 0.0).abs() < 1e-6);
    assert!((t.get_total() - 0.0).abs() < 1e-6);
    assert_eq!(t.get_ticks(), 0);
}

#[test]
fn ticker_non_loop_stops_after_firing() {
    let mut t = Ticker::new(5.0);
    t.set_loop(false);
    assert!(t.update(5.0)); // fires
    assert!(!t.is_running()); // running := loop (false)
    assert!(!t.update(5.0)); // no longer running -> never fires
    assert_eq!(t.get_ticks(), 1);
}
