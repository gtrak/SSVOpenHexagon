//! C++ `HexagonGame::initLua_EventTimeline` (HGScripting.cpp:353-459): the
//! 11 non-deprecated `e_*` functions (the 5 `e_event*` deprecated aliases
//! live in `deprecated.rs`).
//!
//! All schedule onto the EVENT timeline except `e_clearMessages`, which acts
//! immediately (C++ calls `clearMessages()` directly). `e_messageAdd` fires
//! only when `firstPlay` at *execution* time (the C++ lambda captures
//! `this` and reads `firstPlay` when the timeline reaches the action).

use super::main_timeline::duration_to_ms_truncated;
use super::{lock, State};
use crate::api::state::SharedLua;
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;
use std::time::Duration;

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State, shared: SharedLua) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(ctx,
        "e_eval",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |code: String| {
            let st2 = st.clone();
            lock(&st).event_timeline.append_do(move || {
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
    .doc("*Add to the event timeline*: evaluate the Lua code specified in `$0`. (This is the closest you'll get to 1.92 events)");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "e_kill",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let st2 = st.clone();
            lock(&st).event_timeline.append_do(move || {
                let s = lock(&st2);
                s.backend.death(true);
            });
        },
    )?
    .doc("*Add to the event timeline*: kill the player.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "e_stopTime",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            let st2 = st.clone();
            // C++: `status.pauseTime(ssvu::getFTToSeconds(duration))` —
            // FRAMETIME, not seconds (mapping §8.5).
            lock(&st).event_timeline.append_do(move || {
                let mut s = lock(&st2);
                s.status
                    .pause_time(crate::api::state::ft_to_seconds(duration));
            });
        },
    )?
    .arg("duration")
    .doc("*Add to the event timeline*: pause the game timer for `$0` frames (under the assumption of a 60 FPS frame rate).");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "e_stopTimeS",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            let st2 = st.clone();
            lock(&st).event_timeline.append_do(move || {
                let mut s = lock(&st2);
                s.status.pause_time(duration);
            });
        },
    )?
    .arg("duration")
    .doc("*Add to the event timeline*: pause the game timer for `$0` seconds.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "e_wait",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            lock(&st)
                .event_timeline
                .append_wait_for_sixths(duration);
        },
    )?
    .arg("duration")
    .doc("*Add to the event timeline*: wait for `$0` frames (under the assumption of a 60 FPS frame rate).");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "e_waitS",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            lock(&st).event_timeline.append_wait_for_seconds(duration);
        },
    )?
    .arg("duration")
    .doc("*Add to the event timeline*: wait for `$0` seconds.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "e_waitUntilS",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            let st2 = st.clone();
            lock(&st).event_timeline.append_wait_until_fn(move || {
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
    .arg("duration")
    .doc("*Add to the event timeline*: wait until the timer reaches `$0` seconds.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "e_messageAdd",
        FnMeta {
            ret: "void",
            arg_types: &["std::string", "double"],
        },
        move |message: String, duration: f64| {
            let st2 = st.clone();
            lock(&st).event_timeline.append_do(move || {
                let s = lock(&st2);
                // C++: fires ONLY on the first run of the level.
                if s.first_play {
                    s.backend.add_message(&message, duration, true);
                }
            });
        },
    )?
    .arg("message")
    .arg("duration")
    .doc("*Add to the event timeline*: print a message with text `$0` for `$1` seconds. The message will only be printed during the first run of the level.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "e_messageAddImportant",
        FnMeta {
            ret: "void",
            arg_types: &["std::string", "double"],
        },
        move |message: String, duration: f64| {
            let st2 = st.clone();
            lock(&st).event_timeline.append_do(move || {
                let s = lock(&st2);
                s.backend.add_message(&message, duration, true);
            });
        },
    )?
    .arg("message")
    .arg("duration")
    .doc("*Add to the event timeline*: print a message with text `$0` for `$1` seconds. The message will be printed during every run of the level.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "e_messageAddImportantSilent",
        FnMeta {
            ret: "void",
            arg_types: &["std::string", "double"],
        },
        move |message: String, duration: f64| {
            let st2 = st.clone();
            lock(&st).event_timeline.append_do(move || {
                let s = lock(&st2);
                s.backend.add_message(&message, duration, false);
            });
        },
    )?
    .arg("message")
    .arg("duration")
    .doc("*Add to the event timeline*: print a message with text `$0` for `$1` seconds. The message will only be printed during every run of the level, and will not produce any sound.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "e_clearMessages",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let s = lock(&st);
            s.backend.clear_messages();
        },
    )?
    .doc("Remove all previously scheduled messages.");

    Ok(())
}
