//! C++ `HexagonGame::initLua_MainTimeline` (HGScripting.cpp:301-349): the 6
//! `t_*` functions. All schedule onto the MAIN timeline — none of them act
//! at call time (mapping §8.5).
//!
//! * `t_wait` takes *frames at 60 FPS* (1/60 s each), `t_waitS` takes
//!   seconds, `t_waitUntilS` waits until
//!   `levelStartTP + int(duration * 1000) ms` (C++ int-millisecond
//!   truncation, reproduced).
//! * `t_eval` / `t_kill` append deferred closures that capture the Lua
//!   context (`SharedLua`) / the backend, exactly like the C++ `[=, this]`
//!   captures.

use std::time::Duration;

use super::{lock, State};
use crate::api::state::SharedLua;
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

/// C++ `static_cast<int>(mDuration * 1000.0)` — the int-millisecond
/// truncation of `waitUntilSImpl`.
use crate::add_lua_fn;
pub(crate) fn duration_to_ms_truncated(duration: f64) -> i64 {
    (duration * 1000.0) as i32 as i64
}

pub(crate) fn register(ctx: &LuaContext, state: &State, shared: SharedLua) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "t_eval",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |code: String| {
            let st2 = st.clone();
            lock(&st).main_timeline.append_do(move || {
                // C++ `Utils::runLuaCode(lua, mCode)` — on a fatal Lua error
                // the C++ path logs + rethrows (abandoning the update); the
                // port cannot rethrow from a `Fn()` action, so it logs the
                // same "Fatal Lua error" message instead.
                if let Err(err) = shared.execute_code(&code) {
                    lock(&st2).backend.log(
                        "hg::Utils::runLuaCode",
                        &format!("Fatal Lua error\nCode: {code}\nError: {err}\n"),
                    );
                }
            });
        },
    )?
    .arg("code")
    .doc("*Add to the main timeline*: evaluate the Lua code specified in `$0`.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "t_clear",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || lock(&st).main_timeline.clear(),
    )?
    .doc("Clear the main timeline.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "t_kill",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let st2 = st.clone();
            lock(&st).main_timeline.append_do(move || {
                let s = lock(&st2);
                s.backend.death(true);
            });
        },
    )?
    .doc("*Add to the main timeline*: kill the player.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "t_wait",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            lock(&st).main_timeline.append_wait_for_sixths(duration);
        },
    )?
    .arg("duration")
    .doc("*Add to the main timeline*: wait for `$0` frames (under the assumption of a 60 FPS frame rate).");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "t_waitS",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            lock(&st).main_timeline.append_wait_for_seconds(duration);
        },
    )?
    .arg("duration")
    .doc("*Add to the main timeline*: wait for `$0` seconds.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "t_waitUntilS",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            let st2 = st.clone();
            lock(&st).main_timeline.append_wait_until_fn(move || {
                // C++ `status.getLevelStartTP() + chrono::milliseconds(
                // static_cast<int>(mDuration * 1000.0))`.
                let ms = duration_to_ms_truncated(duration);
                let s = lock(&st2);
                if ms < 0 {
                    // C++ would target a time in the past (negative
                    // `chrono::milliseconds`); `Duration` can't be negative.
                    s.status.get_level_start_tp() - Duration::from_millis((-ms) as u64)
                } else {
                    s.status.get_level_start_tp() + Duration::from_millis(ms as u64)
                }
            });
        },
    )?
    .arg("duration")
    .doc("*Add to the main timeline*: wait until the timer reaches `$0` seconds.");

    Ok(())
}
