//! Pins the game-Utils ports against the C++ build:
//!
//! - `buildPackId` / `getLevelValidator` / `concat` values from
//!   `.agents/plans/001-rust-port/mapping/oracle-vectors.md`
//!   ("String formatting" section, `tools/oracle/concat_probe.cpp`);
//! - `Color`, `Geometry`, `Easing`, `MoveTowards`, `Split`, `String`,
//!   `ssvu::Math`, `ssvu::Containers`, `ssvs::Vector2` values from a
//!   `g++ -std=c++20 -O0` probe of the real C++ sources (same commands
//!   as `tools/oracle/run.sh`; probe compiled 2026-10-15 on this machine).

use oh_math::hg::build_pack_id::build_pack_id;
use oh_math::hg::color::{
    component_clamp, get_color_darkened, get_color_from_hue, transform_hue, Color,
};
use oh_math::hg::concat::{concat, ConcatPart};
use oh_math::hg::easing::{get_saturated, get_smooth_step, get_smoother_step};
use oh_math::hg::geometry::{
    get_line_circle_closest_intersection, get_line_circle_intersection,
};
use oh_math::hg::level_validator::get_level_validator;
use oh_math::hg::move_towards::{get_move_towards, get_move_towards_zero};
use oh_math::hg::split::split;
use oh_math::hg::string::{
    get_lr_trim, get_l_trim, get_r_trim, lowercasify, to_lowercase, to_uppercase,
    uppercasify,
};
use oh_math::ssvs::vector2::{
    get_dist_euclidean, get_dist_squared_euclidean, get_mag, get_mag_squared,
    get_normalized, get_orbit_rad, get_rad, get_vec_from_rad, ZERO_VEC,
};
use oh_math::ssvu::containers::{contains, get_by_mod_idx};
use oh_math::ssvu::math::{
    clamp, get_clamped, get_dist_euclidean as ssvu_get_dist_euclidean,
    get_dist_squared_euclidean as ssvu_get_dist_squared_euclidean, get_mod,
    get_mod_range, get_sign, to_rad, PI, RAD_DEG_RATIO, TAU,
};
use oh_math::Vec2;

// ---------------------------------------------------------------------------
// String formatting oracle (mapping/oracle-vectors.md)
// ---------------------------------------------------------------------------

#[test]
fn concat_matches_cpp_oracle() {
    assert_eq!(
        concat(&[ConcatPart::F32(1.6f32)]),
        "1.6", // concat_float_1.6
    );
    assert_eq!(
        concat(&[ConcatPart::F32(9.45f32)]),
        "9.45", // concat_float_9.45
    );
    assert_eq!(
        concat(&[ConcatPart::F64(3600.0f64)]),
        "3600", // concat_double_3600
    );
    assert_eq!(
        concat(&[ConcatPart::I32(5)]),
        "5", // concat_int_5
    );
    assert_eq!(
        concat(&[
            ConcatPart::Str("v"),
            ConcatPart::Str("_n_"),
            ConcatPart::F32(1.35f32),
            ConcatPart::Str("x"),
            ConcatPart::I32(60),
        ]),
        "v_n_1.35x60", // concat_mixed
    );
    // C++ `oss << bool` → 1 / 0.
    assert_eq!(
        concat(&[ConcatPart::Bool(true), ConcatPart::Bool(false)]),
        "10"
    );
}

#[test]
fn build_pack_id_matches_cpp_oracle() {
    // packid=ohvrvanilla_vittorio_romeo_base_1
    assert_eq!(
        build_pack_id("ohvrvanilla", "vittorio romeo", "base", 1),
        "ohvrvanilla_vittorio_romeo_base_1"
    );
}

#[test]
fn get_level_validator_matches_cpp_oracle() {
    // validator=ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_1.6
    assert_eq!(
        get_level_validator(
            "ohvrvanilla_vittorio_romeo_cube_1_apeirogon",
            1.6f32
        ),
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon_m_1.6"
    );
}

// ---------------------------------------------------------------------------
// Color hue math (C++ probe)
// ---------------------------------------------------------------------------

#[test]
fn get_color_from_hue_matches_cpp() {
    // (hue, r, g, b, a) from the C++ probe.
    let cases: [(f32, u8, u8, u8, u8); 14] = [
        (0.0, 255, 0, 0, 255),
        (59.999, 255, 0, 1, 255),
        (60.0, 255, 0, 255, 255), // i = 360 -> falls out of the switch
        (119.999, 255, 0, 1, 255),
        (120.0, 255, 0, 255, 255),
        (180.0, 255, 0, 255, 255),
        (240.0, 255, 0, 255, 255),
        (300.0, 255, 0, 255, 255),
        (359.999, 255, 0, 1, 255),
        (360.0, 255, 0, 255, 255),
        (361.0, 255, 0, 255, 255),
        (720.0, 255, 0, 255, 255),
        (-1.0, 255, 0, 255, 255),
        (181.0, 255, 0, 255, 255),
    ];
    for (hue, r, g, b, a) in cases {
        let c = get_color_from_hue(hue);
        assert_eq!(
            (c.r, c.g, c.b, c.a),
            (r, g, b, a),
            "getColorFromHue({hue}) mismatch"
        );
    }
    // The game always calls it with `hue / 360.f` (i in [0, 6)).
    let red = get_color_from_hue(0.0f32 / 360.0f32);
    assert_eq!((red.r, red.g, red.b), (255, 0, 0));
}

#[test]
fn transform_hue_matches_cpp() {
    let in_color = Color::new(200, 100, 50, 255);
    // (hue, r, g, b, a) from the C++ probe.
    let cases: [(f32, u8, u8, u8, u8); 6] = [
        (0.0, 75, 232, 182, 255),
        (90.0, 41, 209, 134, 255),
        (180.0, 181, 24, 74, 255),
        (270.0, 215, 47, 122, 255),
        (30.0, 86, 212, 2, 255),
        (-30.0, 44, 2, 125, 255),
    ];
    for (h, r, g, b, a) in cases {
        let c = transform_hue(&in_color, h);
        assert_eq!(
            (c.r, c.g, c.b, c.a),
            (r, g, b, a),
            "transformHue({in_color:?}, {h}) mismatch"
        );
    }
}

#[test]
fn get_color_darkened_matches_cpp() {
    let c = get_color_darkened(Color::new(200, 100, 50, 255), 1.4f32);
    assert_eq!((c.r, c.g, c.b, c.a), (142, 71, 35, 255));
    // Multiplier 0: unchanged.
    let c0 = get_color_darkened(Color::new(200, 100, 50, 255), 0.0f32);
    assert_eq!((c0.r, c0.g, c0.b), (200, 100, 50));
}

#[test]
fn component_clamp_matches_cpp() {
    let cases: [(f32, u8); 7] = [
        (-1.0, 0),
        (0.0, 0),
        (0.4, 0),
        (254.9, 254),
        (255.0, 255),
        (255.5, 255),
        (300.0, 255),
    ];
    for (v, expected) in cases {
        assert_eq!(component_clamp(v), expected, "componentClamp({v})");
    }
}

// ---------------------------------------------------------------------------
// Geometry (C++ probe)
// ---------------------------------------------------------------------------

#[test]
fn get_line_circle_intersection_matches_cpp() {
    // Two intersections (C++: lci: n=2, (1, 3.87298322), (1, -3.87298322)).
    let mut i1 = Vec2::default();
    let mut i2 = Vec2::default();
    let n = get_line_circle_intersection(
        &mut i1,
        &mut i2,
        &Vec2::new(1.0, 0.0),
        &Vec2::new(1.0, 10.0),
        16.0f32,
    );
    assert_eq!(n, 2);
    assert!((i1.x - 1.0f32).abs() < 1e-6f32);
    assert!((i1.y - 3.87298322f32).abs() < 1e-6f32);
    assert!((i2.x - 1.0f32).abs() < 1e-6f32);
    assert!((i2.y + 3.87298322f32).abs() < 1e-6f32);

    // Degenerate: both intersections at distinct points (C++: lci2).
    let mut i1 = Vec2::default();
    let mut i2 = Vec2::default();
    let n = get_line_circle_intersection(
        &mut i1,
        &mut i2,
        &Vec2::new(2.0, 0.0),
        &Vec2::new(8.0, 0.0),
        4.0f32,
    );
    assert_eq!(n, 2);
    assert_eq!((i1.x, i1.y), (2.0f32, 0.0f32));
    assert_eq!((i2.x, i2.y), (-2.0f32, 0.0f32));

    // No intersections (line y = 100 misses the r = 2 circle).
    let mut i1 = Vec2::default();
    let mut i2 = Vec2::default();
    let n = get_line_circle_intersection(
        &mut i1,
        &mut i2,
        &Vec2::new(0.0, 100.0),
        &Vec2::new(100.0, 100.0),
        4.0f32,
    );
    assert_eq!(n, 0);
}

#[test]
fn get_line_circle_closest_intersection_matches_cpp() {
    // Tie (both intersections equally close) -> v1 (C++: lccl).
    let mut inter = Vec2::default();
    let ok = get_line_circle_closest_intersection(
        &mut inter,
        &Vec2::new(1.0, 0.0),
        &Vec2::new(1.0, 0.0),
        &Vec2::new(1.0, 10.0),
        16.0f32,
    );
    assert!(ok);
    assert!((inter.x - 1.0f32).abs() < 1e-6f32);
    assert!((inter.y - 3.87298322f32).abs() < 1e-6f32);

    // Two intersections; closest to (0, -1) (C++: lccl2 -> (3, 0)).
    let mut inter = Vec2::default();
    let ok = get_line_circle_closest_intersection(
        &mut inter,
        &Vec2::new(0.0, -1.0),
        &Vec2::new(-5.0, 0.0),
        &Vec2::new(5.0, 0.0),
        9.0f32,
    );
    assert!(ok);
    assert!((inter.x - 3.0f32).abs() < 1e-6f32);
    assert_eq!(inter.y, 0.0f32);

    // No intersections -> false.
    let mut inter = Vec2::default();
    let ok = get_line_circle_closest_intersection(
        &mut inter,
        &Vec2::new(0.0, 0.0),
        &Vec2::new(0.0, 100.0),
        &Vec2::new(100.0, 100.0),
        4.0f32,
    );
    assert!(!ok);
}

// ---------------------------------------------------------------------------
// Easing (C++ probe)
// ---------------------------------------------------------------------------

#[test]
fn easing_matches_cpp() {
    for x in [-1.0f32, 0.0, 0.25, 0.5, 0.75, 1.0, 2.0] {
        let sat = get_saturated(x);
        assert_eq!(
            sat,
            match x {
                -1.0 => 0.0,
                0.0 => 0.0,
                0.25 => 0.25,
                0.5 => 0.5,
                0.75 => 0.75,
                _ => 1.0,
            },
            "getSaturated({x})"
        );
        // 9-significant-digit C++ values, compared as exact f32.
        let sstep = get_smooth_step(0.0, 1.0, x);
        let ssmo = get_smoother_step(0.0, 1.0, x);
        let (exp_sstep, exp_ssmo) = match x {
            -1.0 => (0.0, 0.0),
            0.0 => (0.0, 0.0),
            0.25 => (0.15625, 0.103515625),
            0.5 => (0.5, 0.5),
            0.75 => (0.84375, 0.896484375),
            _ => (1.0, 1.0),
        };
        assert_eq!(sstep, exp_sstep, "getSmoothStep(0,1,{x})");
        assert_eq!(ssmo, exp_ssmo, "getSmootherStep(0,1,{x})");
    }
}

// ---------------------------------------------------------------------------
// MoveTowards (C++ probe)
// ---------------------------------------------------------------------------

#[test]
fn move_towards_matches_cpp() {
    // (value, step, getMoveTowards(value, 3, step), getMoveTowardsZero(value, step))
    let cases: [(f32, f32, f32, f32); 15] = [
        (-5.0, 0.0, -5.0, -5.0),
        (-5.0, 2.0, -3.0, -3.0),
        (-5.0, 10.0, 3.0, 0.0),
        (-1.0, 0.0, -1.0, -1.0),
        (-1.0, 2.0, 1.0, 0.0),
        (-1.0, 10.0, 3.0, 0.0),
        (0.0, 0.0, 0.0, 0.0),
        (0.0, 2.0, 2.0, 0.0),
        (0.0, 10.0, 3.0, 0.0),
        (1.0, 0.0, 1.0, 1.0),
        (1.0, 2.0, 3.0, 0.0),
        (1.0, 10.0, 3.0, 0.0),
        (5.0, 0.0, 5.0, 5.0),
        (5.0, 2.0, 3.0, 3.0),
        (5.0, 10.0, 3.0, 0.0),
    ];
    for (v, step, exp_mt, exp_mt0) in cases {
        assert_eq!(
            get_move_towards(v, 3.0, step),
            exp_mt,
            "getMoveTowards({v}, 3, {step})"
        );
        assert_eq!(
            get_move_towards_zero(v, step),
            exp_mt0,
            "getMoveTowardsZero({v}, {step})"
        );
    }
}

// ---------------------------------------------------------------------------
// Split (C++ probe)
// ---------------------------------------------------------------------------

#[test]
fn split_matches_cpp() {
    assert_eq!(
        split("  a ,b,,c  d  ", " ,"),
        vec!["a", "b", "c", "d"],
    );
    assert_eq!(split("", " "), Vec::<&str>::new());
    assert_eq!(split("single", " "), vec!["single"]);
    // Default delimiter (space).
    assert_eq!(split("a  b", " "), vec!["a", "b"]);
}

// ---------------------------------------------------------------------------
// String (game Utils, C++ probe)
// ---------------------------------------------------------------------------

#[test]
fn string_utils_match_cpp() {
    let mut s = String::from("  \tx\t  ");
    oh_math::hg::string::l_trim(&mut s);
    assert_eq!(s, "x\t  ");
    s = String::from("abc \n\t ");
    oh_math::hg::string::r_trim(&mut s);
    assert_eq!(s, "abc");
    s = String::from("  abc  ");
    oh_math::hg::string::lr_trim(&mut s);
    assert_eq!(s, "abc");
    assert_eq!(get_l_trim(String::from("  abc  ")), "abc  ");
    assert_eq!(get_r_trim(String::from("  abc  ")), "  abc");
    assert_eq!(get_lr_trim(String::from("  abc  ")), "abc");

    let mut u = String::from("Hello World 123");
    uppercasify(&mut u);
    assert_eq!(u, "HELLO WORLD 123");
    assert_eq!(to_uppercase(String::from("ab cd")), "AB CD");

    let mut l = String::from("AbC-123_xY");
    lowercasify(&mut l);
    assert_eq!(l, "abc-123_xy");
    assert_eq!(to_lowercase(String::from("AaBb")), "aabb");
}

// ---------------------------------------------------------------------------
// ssvu::math (C++ probe)
// ---------------------------------------------------------------------------

#[test]
fn ssvu_math_matches_cpp() {
    // getMod (2-arg).
    assert_eq!(get_mod(-2i32, 12), 10);
    assert_eq!(get_mod(26i32, 12), 2);
    // getMod (3-arg).
    assert_eq!(get_mod_range(-2i32, 0, 12), 10);
    assert_eq!(get_mod_range(15i32, 0, 12), 3);
    assert_eq!(get_mod_range(7i32, 0, 12), 7);
    assert_eq!(get_mod_range(13i32, 5, 20), 13);
    assert_eq!(get_mod_range(-3i32, 5, 20), 12);
    // Unsigned form (C++ unsigned overload: `0 < mX`).
    assert_eq!(get_sign(5u32), 1);
    assert_eq!(get_sign(0u32), 0);

    // getSign.
    assert_eq!(get_sign(3.0f32), 1);
    assert_eq!(get_sign(-3.0f32), -1);
    assert_eq!(get_sign(0.0f32), 0);

    // clamp (in place) + getClamped.
    let mut x = 5.0f32;
    clamp(&mut x, 0.0, 10.0);
    assert_eq!(x, 5.0);
    let mut y = 15.0f32;
    clamp(&mut y, 0.0, 10.0);
    assert_eq!(y, 10.0);
    let mut z = -5.0f32;
    clamp(&mut z, 0.0, 10.0);
    assert_eq!(z, 0.0);
    assert_eq!(get_clamped(15.0f32, 0.0, 10.0), 10.0);
    assert_eq!(get_clamped(-15.0f32, 0.0, 10.0), 0.0);
    assert_eq!(get_clamped(5.0f32, 0.0, 10.0), 5.0);

    // Constants + toRad (C++: torad(180) = 3.14159274).
    assert_eq!(PI, 3.14159265359f32);
    assert_eq!(TAU, 6.28318530718f32);
    assert_eq!(RAD_DEG_RATIO, 0.01745329251f32);
    assert_eq!(to_rad(180.0f32).to_bits(), 3.14159274f32.to_bits());

    // Distances (C++: distsq=25, dist=5).
    assert_eq!(ssvu_get_dist_squared_euclidean(0.0, 0.0, 3.0, 4.0), 25.0);
    assert_eq!(ssvu_get_dist_euclidean(0.0, 0.0, 3.0, 4.0), 5.0);
}

// ---------------------------------------------------------------------------
// ssvu::containers (C++ probe)
// ---------------------------------------------------------------------------

#[test]
fn ssvu_containers_match_cpp() {
    let v = [10i32, 20, 30];
    assert!(contains(&v, &20));
    assert!(!contains(&v, &5));
    assert_eq!(*get_by_mod_idx(&v, 0i32), 10);
    assert_eq!(*get_by_mod_idx(&v, 3i32), 10); // 3 % 3 = 0
    assert_eq!(*get_by_mod_idx(&v, -1i32), 10); // (size_t)(-1) % 3 == 0
    assert_eq!(*get_by_mod_idx(&v, 5u32), 30); // 5 % 3 = 2
}

// ---------------------------------------------------------------------------
// ssvs::vector2 (C++ probe)
// ---------------------------------------------------------------------------

#[test]
fn vector2_helpers_match_cpp() {
    let v = Vec2::new(3.0, 4.0);
    assert_eq!(get_mag_squared(&v), 25.0);
    assert_eq!(get_mag(&v), 5.0);

    // (3,4) normalized: C++ 0.600000024, 0.800000012 (exact f32).
    let n = get_normalized(v);
    assert_eq!(n.x.to_bits(), 0.6f32.to_bits());
    assert_eq!(n.y.to_bits(), 0.8f32.to_bits());

    // Zero vector stays zero (the normalize guard).
    let z = get_normalized(ZERO_VEC);
    assert_eq!((z.x, z.y), (0.0, 0.0));

    // getRad: C++ 0.785398185.
    let r = get_rad(&Vec2::new(1.0, 1.0));
    assert!((r - 0.785398185f32).abs() < 1e-6f32);

    // getVecFromRad(pi_f32, 2): C++ -2, -1.74845553e-07.
    let p = get_vec_from_rad(3.14159265f32, 2.0);
    assert!((p.x - -2.0f32).abs() < 1e-6f32);
    assert!((p.y - -1.74845553e-07f32).abs() < 1e-6f32);

    // getOrbitRad((1,2), 0, 5): C++ (6, 2).
    let o = get_orbit_rad(&Vec2::new(1.0, 2.0), 0.0, 5.0);
    assert!((o.x - 6.0f32).abs() < 1e-6f32);
    assert!((o.y - 2.0f32).abs() < 1e-6f32);

    // Distances: C++ 5 / 25.
    let a = Vec2::new(0.0, 0.0);
    let b = Vec2::new(3.0, 4.0);
    assert_eq!(get_dist_squared_euclidean(&a, &b), 25.0);
    assert_eq!(get_dist_euclidean(&a, &b), 5.0);

    // zeroVec2f.
    assert_eq!((ZERO_VEC.x, ZERO_VEC.y), (0.0, 0.0));
}
