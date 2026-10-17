//! C++ `hg::LuaScripting::initCustomWalls` (LuaScripting.cpp:254-504): the
//! 20 `cw_*` functions over `CCustomWallManager`. Handles are opaque ints;
//! the manager's own validation (log + sentinel) applies, exactly as in C++.

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;
use oh_math::hg::color::Color;
use oh_math::ssvs::Vec2;

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "cw_create",
        FnMeta {
            ret: "int",
            arg_types: &[],
        },
        move || {
            let mut s = lock(&st);
            s.cw_manager.create(|_cw| {})
        },
    )?
    .doc("Create a new custom wall and return a integer handle to it.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "cw_createDeadly",
        FnMeta {
            ret: "int",
            arg_types: &[],
        },
        move || {
            let mut s = lock(&st);
            s.cw_manager.create(|cw| cw.set_deadly(true))
        },
    )?
    .doc("Create a new deadly custom wall and return a integer handle to it.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "cw_createNoCollision",
        FnMeta {
            ret: "int",
            arg_types: &[],
        },
        move || {
            let mut s = lock(&st);
            s.cw_manager.create(|cw| cw.set_can_collide(false))
        },
    )?
    .doc("Create a new custom wall without collision and return a integer handle to it.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "cw_destroy",
        FnMeta {
            ret: "void",
            arg_types: &["int"],
        },
        move |handle: i32| {
            let mut s = lock(&st);
            s.cw_manager.destroy(handle);
        },
    )?
    .arg("cwHandle")
    .doc("Destroy the custom wall represented by `$0`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_setVertexPos",
        FnMeta {
            ret: "void",
            arg_types: &["int", "int", "float", "float"],
        },
        move |handle: i32, vertex_index: i32, x: f32, y: f32| {
            let mut s = lock(&st);
            s.cw_manager.set_vertex_pos(handle, vertex_index, Vec2::new(x, y));
        },
    )?
    .arg("cwHandle")
    .arg("vertexIndex")
    .arg("x")
    .arg("y")
    .doc("Given the custom wall represented by `$0`, set the position of its vertex with index `$1` to `{$2, $3}`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_moveVertexPos",
        FnMeta {
            ret: "void",
            arg_types: &["int", "int", "float", "float"],
        },
        move |handle: i32, vertex_index: i32, x: f32, y: f32| {
            let mut s = lock(&st);
            s.cw_manager.move_vertex_pos(handle, vertex_index, Vec2::new(x, y));
        },
    )?
    .arg("cwHandle")
    .arg("vertexIndex")
    .arg("offsetX")
    .arg("offsetY")
    .doc("Given the custom wall represented by `$0`, add `{$2, $3}` to the position of its vertex with index `$1`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_moveVertexPos4Same",
        FnMeta {
            ret: "void",
            arg_types: &["int", "float", "float"],
        },
        move |handle: i32, x: f32, y: f32| {
            let mut s = lock(&st);
            s.cw_manager.move_vertex_pos4_same(handle, Vec2::new(x, y));
        },
    )?
    .arg("cwHandle")
    .arg("offsetX")
    .arg("offsetY")
    .doc("Given the custom wall represented by `$0`, add `{$1, $2}` to the position of its vertex with indices `0`, `1`, `2`, and `3`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_setVertexColor",
        FnMeta {
            ret: "void",
            arg_types: &["int", "int", "int", "int", "int", "int"],
        },
        move |handle: i32, vertex_index: i32, r: i32, g: i32, b: i32, a: i32| {
            let mut s = lock(&st);
            // C++ `sf::Color(r, g, b, a)` (LuaScripting.cpp:326) — the
            // mod-256 channel wrap, exactly like the `4`/`4Same` variants
            // (the shared `color` helper below — 29g1).
            s.cw_manager.set_vertex_color(handle, vertex_index, color(r, g, b, a));
        },
    )?
    .arg("cwHandle")
    .arg("vertexIndex")
    .arg("r")
    .arg("g")
    .arg("b")
    .arg("a")
    .doc("Given the custom wall represented by `$0`, set the color of its vertex with index `$1` to `{$2, $3, $4, $5}`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_setVertexPos4",
        FnMeta {
            ret: "void",
            arg_types: &[
                "int", "float", "float", "float", "float", "float", "float",
                "float", "float",
            ],
        },
        move |handle: i32, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32,
              y3: f32| {
            let mut s = lock(&st);
            s.cw_manager.set_vertex_pos4(
                handle,
                Vec2::new(x0, y0),
                Vec2::new(x1, y1),
                Vec2::new(x2, y2),
                Vec2::new(x3, y3),
            );
        },
    )?
    .arg("cwHandle")
    .arg("x0")
    .arg("y0")
    .arg("x1")
    .arg("y1")
    .arg("x2")
    .arg("y2")
    .arg("x3")
    .arg("y3")
    .doc("Given the custom wall represented by `$0`, set the position of its vertex with index `0` to `{$1, $2}`, index `1` to `{$3, $4}`, index `2` to `{$5, $6}`, index `3` to `{$7, $8}`. More efficient than invoking `cw_setVertexPos` four times in a row.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_setVertexColor4",
        FnMeta {
            ret: "void",
            arg_types: &[
                "int", "int", "int", "int", "int", "int", "int", "int", "int",
                "int", "int", "int", "int", "int", "int", "int", "int",
            ],
        },
        move |handle: i32, r0: i32, g0: i32, b0: i32, a0: i32, r1: i32, g1: i32, b1: i32,
              a1: i32, r2: i32, g2: i32, b2: i32, a2: i32, r3: i32, g3: i32, b3: i32,
              a3: i32| {
            let mut s = lock(&st);
            s.cw_manager.set_vertex_color4(
                handle,
                color(r0, g0, b0, a0),
                color(r1, g1, b1, a1),
                color(r2, g2, b2, a2),
                color(r3, g3, b3, a3),
            );
        },
    )?
    .arg("cwHandle")
    .arg("r0")
    .arg("g0")
    .arg("b0")
    .arg("a0")
    .arg("r1")
    .arg("g1")
    .arg("b1")
    .arg("a1")
    .arg("r2")
    .arg("g2")
    .arg("b2")
    .arg("a2")
    .arg("r3")
    .arg("g3")
    .arg("b3")
    .arg("a3")
    .doc("Given the custom wall represented by `$0`, set the color of its vertex with index `0` to `{$1, $2, $3, $4}`, index `1` to `{$5, $6, $7, $8}`, index `2` to `{$9, $10, $11, $12}`, index `3` to `{$13, $14, $15, $16}`. More efficient than invoking `cw_setVertexColor` four times in a row.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_setVertexColor4Same",
        FnMeta {
            ret: "void",
            arg_types: &["int", "int", "int", "int", "int"],
        },
        move |handle: i32, r: i32, g: i32, b: i32, a: i32| {
            let mut s = lock(&st);
            s.cw_manager
                .set_vertex_color4_same(handle, color(r, g, b, a));
        },
    )?
    .arg("cwHandle")
    .arg("r")
    .arg("g")
    .arg("b")
    .arg("a")
    .doc("Given the custom wall represented by `$0`, set the color of its vertices with indiced `0`, `1`, `2`, and `3' to `{$1, $2, $3, $4}`. More efficient than invoking `cw_setVertexColor` four times in a row.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_setCollision",
        FnMeta {
            ret: "void",
            arg_types: &["int", "bool"],
        },
        move |handle: i32, collision: bool| {
            let mut s = lock(&st);
            s.cw_manager.set_can_collide(handle, collision);
        },
    )?
    .arg("cwHandle")
    .arg("collision")
    .doc("Given the custom wall represented by `$0`, set the collision of the custom wall to `$1`. If false, the player cannot die from this wall and can move through the wall. By default, all custom walls can collide with the player.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_setDeadly",
        FnMeta {
            ret: "void",
            arg_types: &["int", "bool"],
        },
        move |handle: i32, deadly: bool| {
            let mut s = lock(&st);
            s.cw_manager.set_deadly(handle, deadly);
        },
    )?
    .arg("cwHandle")
    .arg("deadly")
    .doc("Given the custom wall represented by `$0`, set wherever it instantly kills player on touch. This is highly recommended for custom walls that are either very small or very thin and should definitively kill the player.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_setKillingSide",
        FnMeta {
            ret: "void",
            arg_types: &["int", "unsigned int"],
        },
        move |handle: i32, side: u32| {
            let mut s = lock(&st);
            s.cw_manager.set_killing_side(handle, side as u8);
        },
    )?
    .arg("cwHandle")
    .arg("side")
    .doc("Given the custom wall represented by `$0`, set which one of its sides should beyond any doubt cause the death of the player. Acceptable values are `0` to `3`. In a standard wall, side `0` is the side closer to the center. This parameter is useless if the custom wall is deadly.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "cw_getCollision",
        FnMeta {
            ret: "bool",
            arg_types: &["int"],
        },
        move |handle: i32| lock(&st).cw_manager.get_can_collide(handle),
    )?
    .arg("cwHandle")
    .doc(
        "Given the custom wall represented by `$0`, get whether it can collide with player or not.",
    );

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_getDeadly",
        FnMeta {
            ret: "bool",
            arg_types: &["int"],
        },
        move |handle: i32| lock(&st).cw_manager.get_deadly(handle),
    )?
    .arg("cwHandle")
    .doc("Given the custom wall represented by `$0`, get whether it instantly kills the player on touch or not.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_getKillingSide",
        FnMeta {
            ret: "unsigned int",
            arg_types: &["int"],
        },
        move |handle: i32| i32::from(lock(&st).cw_manager.get_killing_side(handle)) as u32,
    )?
    .arg("cwHandle")
    .doc("Given the custom wall represented by `$0`, get which one of its sides always causes the death of the player.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_getVertexPos",
        FnMeta {
            ret: "tuple<float, float>",
            arg_types: &["int", "int"],
        },
        move |handle: i32, vertex_index: i32| {
            let pos = lock(&st).cw_manager.get_vertex_pos(handle, vertex_index);
            (pos.x, pos.y)
        },
    )?
    .arg("cwHandle")
    .arg("vertexIndex")
    .doc("Given the custom wall represented by `$0`, return the position of its vertex with index `$1`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "cw_getVertexPos4",
        FnMeta {
            ret: "tuple<float, float, float, float, float, float, float, float>",
            arg_types: &["int"],
        },
        move |handle: i32| {
            let [p0, p1, p2, p3] = lock(&st).cw_manager.get_vertex_pos4(handle);
            (p0.x, p0.y, p1.x, p1.y, p2.x, p2.y, p3.x, p3.y)
        },
    )?
    .arg("cwHandle")
    .doc("Given the custom wall represented by `$0`, return the position of its vertics with indices `0`, `1`, `2`, and `3`, as a tuple.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "cw_clear",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let mut s = lock(&st);
            s.cw_manager.clear();
        },
    )?
    .doc("Remove all existing custom walls.");

    Ok(())
}

/// C++ `sf::Color(r, g, b, a)` (LuaScripting.cpp:326 / :374 / :405) —
/// the C++ ctor's `int` channels narrow (`int` -> `int16_t` ->
/// `uint8_t`), which wraps mod 256 (e.g. 300 -> 44, 256 -> 0, -1 -> 255).
/// `rem_euclid(256)` is that exact two's-complement mod-256 for every
/// `i32`. (29g1: the single setter's `try_from(...).unwrap_or(255)` and
/// this helper's `clamp(0, 255)` were each wrong AND mutually
/// inconsistent — all three setters now share this C++ wrap.)
fn color(r: i32, g: i32, b: i32, a: i32) -> Color {
    let wrap = |v: i32| v.rem_euclid(256) as u8;
    Color::new(wrap(r), wrap(g), wrap(b), wrap(a))
}
