//! C++ `HexagonGame::initLua_WallCreation` (HGScripting.cpp:706-833): the 5
//! `w_*` functions.
//!
//! None of them creates a wall at call time — each schedules a closure onto
//! the MAIN timeline, and the speed data is computed from
//! `getSpeedMultDM()` / `difficultyMult` **at execution time** (mapping
//! §8.5), exactly like the C++ `[=, this]` captures.

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;
use oh_game::SpeedData;

/// The 5 C++ `createWall(...)` speed shapes (`HGScripting.cpp`:
/// `w_wall`, `w_wallAdj`, `w_wallAcc`, `w_wallHModSpeedData`,
/// `w_wallHModCurveData`). All values are DM-adjusted **at execution time**
/// exactly like the C++ `[=, this]` captures.
enum WallSpeedSpec {
    /// `w_wall`: `SpeedData{getSpeedMultDM()}`.
    Plain,
    /// `w_wallAdj`: `SpeedData{mSpeedAdj * getSpeedMultDM()}`.
    Adj(f32),
    /// `w_wallAcc`: `SpeedData{mSpeedAdj * getSpeedMultDM(),
    /// mAcceleration / pow(difficultyMult, 0.65f), mMinSpeed * getSpeedMultDM(),
    /// mMaxSpeed * getSpeedMultDM()}`.
    Acc {
        speed_mult: f32,
        acceleration: f32,
        min_speed: f32,
        max_speed: f32,
    },
    /// `w_wallHModSpeedData`: full `SpeedData{mSAdj * getSpeedMultDM(), mSAcc,
    /// mSMin, mSMax, mSPingPong}` (NO DM division on the acceleration).
    HModSpeed {
        speed_mult: f32,
        acceleration: f32,
        min_speed: f32,
        max_speed: f32,
        ping_pong: bool,
    },
    /// `w_wallHModCurveData`: straight `SpeedData{getSpeedMultDM()}` + curve
    /// `SpeedData{mCAdj, mCAcc, mCMin, mCMax, mCPingPong}` (curve values are
    /// NOT DM-adjusted).
    HModCurve {
        curve_speed_mult: f32,
        curve_acceleration: f32,
        curve_min_speed: f32,
        curve_max_speed: f32,
        curve_ping_pong: bool,
    },
}

/// The deferred `createWall` body: locks the state, computes the speed data
/// from the live `difficultyMult` / `getSpeedMultDM()` (execution time), and
/// calls the backend.
fn create_wall_action(
    st: State,
    side: i32,
    thickness: f32,
    spec: WallSpeedSpec,
    hue_mod: f32,
) -> impl Fn() + 'static {
    move || {
        let s = lock(&st);
        let (speed, curve) = match spec {
            WallSpeedSpec::Plain => (
                SpeedData::new(s.get_speed_mult_dm(), 0.0, 0.0, 0.0, false),
                SpeedData::default(),
            ),
            WallSpeedSpec::Adj(speed_mult) => (
                SpeedData::new(speed_mult * s.get_speed_mult_dm(), 0.0, 0.0, 0.0, false),
                SpeedData::default(),
            ),
            WallSpeedSpec::Acc {
                speed_mult,
                acceleration,
                min_speed,
                max_speed,
            } => (
                SpeedData::new(
                    speed_mult * s.get_speed_mult_dm(),
                    acceleration / s.difficulty_mult.powf(0.65),
                    min_speed * s.get_speed_mult_dm(),
                    max_speed * s.get_speed_mult_dm(),
                    false,
                ),
                SpeedData::default(),
            ),
            WallSpeedSpec::HModSpeed {
                speed_mult,
                acceleration,
                min_speed,
                max_speed,
                ping_pong,
            } => (
                SpeedData::new(
                    speed_mult * s.get_speed_mult_dm(),
                    acceleration,
                    min_speed,
                    max_speed,
                    ping_pong,
                ),
                SpeedData::default(),
            ),
            WallSpeedSpec::HModCurve {
                curve_speed_mult,
                curve_acceleration,
                curve_min_speed,
                curve_max_speed,
                curve_ping_pong,
            } => (
                SpeedData::new(s.get_speed_mult_dm(), 0.0, 0.0, 0.0, false),
                SpeedData::new(
                    curve_speed_mult,
                    curve_acceleration,
                    curve_min_speed,
                    curve_max_speed,
                    curve_ping_pong,
                ),
            ),
        };
        s.backend
            .create_wall(side, thickness, speed, curve, hue_mod);
    }
}

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(ctx,
        "w_wall",
        FnMeta {
            ret: "void",
            arg_types: &["int", "float"],
        },
        move |side: i32, thickness: f32| {
            lock(&st).main_timeline.append_do(create_wall_action(
                st.clone(),
                side,
                thickness,
                WallSpeedSpec::Plain,
                0.0,
            ));
        },
    )?
    .arg("side")
    .arg("thickness")
    .doc("Create a new wall at side `$0`, with thickness `$1`. The speed of the wall will be calculated by using the speed multiplier, adjusted for the current difficulty multiplier.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "w_wallAdj",
        FnMeta {
            ret: "void",
            arg_types: &["int", "float", "float"],
        },
        move |side: i32, thickness: f32, speed_mult: f32| {
            lock(&st).main_timeline.append_do(create_wall_action(
                st.clone(),
                side,
                thickness,
                WallSpeedSpec::Adj(speed_mult),
                0.0,
            ));
        },
    )?
    .arg("side")
    .arg("thickness")
    .arg("speedMult")
    .doc("Create a new wall at side `$0`, with thickness `$1`. The speed of the wall will be calculated by using the speed multiplier, adjusted for the current difficulty multiplier, and finally multiplied by `$2`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "w_wallAcc",
        FnMeta {
            ret: "void",
            arg_types: &["int", "float", "float", "float", "float", "float"],
        },
        move |side: i32, thickness: f32, speed_mult: f32, acceleration: f32, min_speed: f32, max_speed: f32| {
            lock(&st).main_timeline.append_do(create_wall_action(
                st.clone(),
                side,
                thickness,
                WallSpeedSpec::Acc {
                    speed_mult,
                    acceleration,
                    min_speed,
                    max_speed,
                },
                0.0,
            ));
        },
    )?
    .arg("side")
    .arg("thickness")
    .arg("speedMult")
    .arg("acceleration")
    .arg("minSpeed")
    .arg("maxSpeed")
    .doc("Create a new wall at side `$0`, with thickness `$1`. The speed of the wall will be calculated by using the speed multiplier, adjusted for the current difficulty multiplier, and finally multiplied by `$2`. The wall will have a speed acceleration value of `$3`. The minimum and maximum speed of the wall are bounded by `$4` and `$5`, adjusted  for the current difficulty multiplier.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "w_wallHModSpeedData",
        FnMeta {
            ret: "void",
            arg_types: &["float", "int", "float", "float", "float", "float", "float", "bool"],
        },
        move |hue_mod: f32, side: i32, thickness: f32, speed_mult: f32, acceleration: f32, min_speed: f32, max_speed: f32, ping_pong: bool| {
            // C++: full `SpeedData{mSAdj * getSpeedMultDM(), mSAcc, mSMin,
            // mSMax, mSPingPong}` — NO DM division on the acceleration.
            lock(&st).main_timeline.append_do(create_wall_action(
                st.clone(),
                side,
                thickness,
                WallSpeedSpec::HModSpeed {
                    speed_mult,
                    acceleration,
                    min_speed,
                    max_speed,
                    ping_pong,
                },
                hue_mod,
            ));
        },
    )?
    .arg("hueModifier")
    .arg("side")
    .arg("thickness")
    .arg("speedMult")
    .arg("acceleration")
    .arg("minSpeed")
    .arg("maxSpeed")
    .arg("pingPong")
    .doc("Create a new wall at side `$1`, with thickness `$2`. The speed of the wall will be calculated by using the speed multiplier, adjusted for the current difficulty multiplier, and finally multiplied by `$3`. The wall will have a speed acceleration value of `$4`. The minimum and maximum speed of the wall are bounded by `$5` and `$6`, adjusted  for the current difficulty multiplier. The hue of the wall will be adjusted by `$0`. If `$7` is enabled, the wall will accelerate back and forth between its minimum and maximum speed.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "w_wallHModCurveData",
        FnMeta {
            ret: "void",
            arg_types: &["float", "int", "float", "float", "float", "float", "float", "bool"],
        },
        move |hue_mod: f32, side: i32, thickness: f32, curve_speed_mult: f32, curve_acceleration: f32, curve_min_speed: f32, curve_max_speed: f32, curve_ping_pong: bool| {
            lock(&st).main_timeline.append_do(create_wall_action(
                st.clone(),
                side,
                thickness,
                WallSpeedSpec::HModCurve {
                    curve_speed_mult,
                    curve_acceleration,
                    curve_min_speed,
                    curve_max_speed,
                    curve_ping_pong,
                },
                hue_mod,
            ));
        },
    )?
    .arg("hueModifier")
    .arg("side")
    .arg("thickness")
    .arg("curveSpeedMult")
    .arg("curveAcceleration")
    .arg("curveMinSpeed")
    .arg("curveMaxSpeed")
    .arg("pingPong")
    .doc("Create a new curving wall at side `$1`, with thickness `$2`. The curving speed of the wall will be calculated by using the speed multiplier, adjusted for the current difficulty multiplier, and finally multiplied by `$3`. The wall will have a curving speed acceleration value of `$4`. The minimum and maximum curving speed of the wall are bounded by `$5` and `$6`, adjusted  for the current difficulty multiplier. The hue of the wall will be adjusted by `$0`. If `$7` is enabled, the wall will accelerate back and forth between its minimum and maximum speed.");

    Ok(())
}
