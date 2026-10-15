//! C++ `HexagonGame::initLua_CustomTimelines` (HGScripting.cpp:462-591):
//! the 8 `ct_*` functions.
//!
//! `ct_create` returns a `CustomTimelineHandle` (opaque `int`). Every other
//! call first validates the handle (`_customTimelineManager.isHandleValid`):
//! an invalid handle logs to the `CustomTimelineManager` channel and
//! no-ops — no Lua error (mapping §8.9).

use std::time::Duration;

use super::main_timeline::duration_to_ms_truncated;
use super::{lock, State};
use crate::api::state::SharedLua;
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

/// C++ `checkHandle(cth, title)`: valid handle → `true`; otherwise log
/// `"Invalid handle '<cth>' during '<title>'\n"` and return `false`.
fn check_handle(state: &State, handle: i32, title: &str) -> bool {
    let valid = lock(state).custom_timelines.is_handle_valid(handle);
    if !valid {
        lock(state).backend.log(
            "CustomTimelineManager",
            &format!("Invalid handle '{handle}' during '{title}'\n"),
        );
    }
    valid
}

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State, shared: SharedLua) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "ct_create",
        FnMeta {
            ret: "int",
            arg_types: &[],
        },
        move || {
            let mut s = lock(&st);
            s.custom_timelines.create()
        },
    )?
    .doc("Create a new custom timeline and return a integer handle to it.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "ct_eval",
        FnMeta {
            ret: "void",
            arg_types: &["int", "std::string"],
        },
        move |handle: i32, code: String| {
            let st2 = st.clone();
            if !check_handle(&st, handle, "ct_eval") {
                return;
            }
            lock(&st)
                .custom_timelines
                .get(handle)
                .timeline_mut()
                .append_do(move || {
                    if let Err(err) = shared.execute_code(&code) {
                        lock(&st2).backend.log(
                            "hg::Utils::runLuaCode",
                            &format!("Fatal Lua error\nCode: {code}\nError: {err}\n"),
                        );
                    }
                });
        },
    )?
    .arg("handle")
    .arg("code")
    .doc("*Add to the custom timeline with handle `$0`*: evaluate the Lua code specified in `$1`.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "ct_kill",
        FnMeta {
            ret: "void",
            arg_types: &["int"],
        },
        move |handle: i32| {
            let st2 = st.clone();
            if !check_handle(&st, handle, "ct_kill") {
                return;
            }
            lock(&st)
                .custom_timelines
                .get(handle)
                .timeline_mut()
                .append_do(move || {
                    let s = lock(&st2);
                    s.backend.death(true);
                });
        },
    )?
    .arg("handle")
    .doc("*Add to the custom timeline with handle `$0`*: kill the player.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "ct_stopTime",
        FnMeta {
            ret: "void",
            arg_types: &["int", "double"],
        },
        move |handle: i32, duration: f64| {
            let st2 = st.clone();
            if !check_handle(&st, handle, "ct_stopTime") {
                return;
            }
            lock(&st).custom_timelines.get(handle).timeline_mut().append_do(
                move || {
                    let mut s = lock(&st2);
                    s.status
                        .pause_time(crate::api::state::ft_to_seconds(duration));
                },
            );
        },
    )?
    .arg("handle")
    .arg("duration")
    .doc("*Add to the custom timeline with handle `$0`*: pause the game timer for `$1` frames (under the assumption of a 60 FPS frame rate).");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "ct_stopTimeS",
        FnMeta {
            ret: "void",
            arg_types: &["int", "double"],
        },
        move |handle: i32, duration: f64| {
            let st2 = st.clone();
            if !check_handle(&st, handle, "ct_stopTimeS") {
                return;
            }
            lock(&st)
                .custom_timelines
                .get(handle)
                .timeline_mut()
                .append_do(move || {
                    let mut s = lock(&st2);
                    s.status.pause_time(duration);
                });
        },
    )?
    .arg("handle")
    .arg("duration")
    .doc("*Add to the custom timeline with handle `$0`*: pause the game timer for `$1` seconds.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "ct_wait",
        FnMeta {
            ret: "void",
            arg_types: &["int", "double"],
        },
        move |handle: i32, duration: f64| {
            if !check_handle(&st, handle, "ct_wait") {
                return;
            }
            lock(&st)
                .custom_timelines
                .get(handle)
                .timeline_mut()
                .append_wait_for_sixths(duration);
        },
    )?
    .arg("handle")
    .arg("duration")
    .doc("*Add to the custom timeline with handle `$0`*: wait for `$1` frames (under the assumption of a 60 FPS frame rate).");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "ct_waitS",
        FnMeta {
            ret: "void",
            arg_types: &["int", "double"],
        },
        move |handle: i32, duration: f64| {
            if !check_handle(&st, handle, "ct_waitS") {
                return;
            }
            lock(&st)
                .custom_timelines
                .get(handle)
                .timeline_mut()
                .append_wait_for_seconds(duration);
        },
    )?
    .arg("handle")
    .arg("duration")
    .doc("*Add to the custom timeline with handle `$0`*: wait for `$1` seconds.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "ct_waitUntilS",
        FnMeta {
            ret: "void",
            arg_types: &["int", "double"],
        },
        move |handle: i32, duration: f64| {
            let st2 = st.clone();
            if !check_handle(&st, handle, "ct_waitUntilS") {
                return;
            }
            lock(&st)
                .custom_timelines
                .get(handle)
                .timeline_mut()
                .append_wait_until_fn(move || {
                    let ms = duration_to_ms_truncated(duration);
                    let s = lock(&st2);
                    if ms < 0 {
                        s.status.get_level_start_tp() - Duration::from_millis((-ms) as u64)
                    } else {
                        s.status.get_level_start_tp() + Duration::from_millis(ms as u64)
                    }
                });
        },
    )?
    .arg("handle")
    .arg("duration")
    .doc(
        "*Add to the custom timeline with handle `$0`*: wait until the timer reaches `$1` seconds.",
    );

    Ok(())
}
