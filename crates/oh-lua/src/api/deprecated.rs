//! C++ `HexagonGame::initLua_Deprecated` (HGScripting.cpp:861-1106): the
//! deprecated aliases. Every one of them (except `l_forceSetPulse` /
//! `l_forceSetBeatPulse`, which live in this group but do **not** call
//! `raiseWarning`) raises the one-shot deprecation warning first.

use std::time::Duration;

use super::main_timeline::duration_to_ms_truncated;
use super::{lock, State};
use crate::api::state::{ft_to_seconds, SharedLua};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

use crate::add_lua_fn;
pub(crate) fn register(
    ctx: &LuaContext,
    state: &State,
    _shared: SharedLua,
) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(ctx,
        "u_kill",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let st2 = st.clone();
            let mut s = lock(&st);
            s.raise_warning(
            "u_kill",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"t_kill\" in your level files.");
            s.main_timeline
                .append_do(move || lock(&st2).backend.death(true));
        },
    )?
    .doc("*Add to the main timeline*: kill the player. **This function is deprecated and will be removed in a future version. Please use t_kill instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "u_eventKill",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let st2 = st.clone();
            let mut s = lock(&st);
            s.raise_warning(
            "u_eventKill",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_kill\" in your level files.");
            s.event_timeline
                .append_do(move || lock(&st2).backend.death(true));
        },
    )?
    .doc("*Add to the event timeline*: kill the player. **This function is deprecated and will be removed in a future version. Please use e_kill instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "u_playSound",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |sound_id: String| {
            let mut s = lock(&st);
            s.raise_warning(
            "u_playSound",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"a_playSound\" in your level files.");
            s.backend.play_sound_override(&sound_id);
        },
    )?
    .arg("soundId")
    .doc("Play the sound with id `$0`. The id must be registered in `assets.json`, under `\"soundBuffers\"`. **This function is deprecated and will be removed in a future version. Please use a_playSound instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "u_playPackSound",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |file_name: String| {
            let pack_id = {
                let mut s = lock(&st);
                s.raise_warning(
                "u_playPackSound",
                "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"a_playPackSound\" in your level files.");
                s.pack_id.clone()
            };
            let s = lock(&st);
            s.backend.play_pack_sound_override(&pack_id, &file_name);
        },
    )?
    .arg("fileName")
    .doc("Dives into the `Sounds` folder of the current level pack and plays the specified file `$0`. **This function is deprecated and will be removed in a future version. Please use a_playPackSound instead!**");

    // ------------------------------------------------------------------
    // e_event* (deprecated event-timeline aliases of the `e_*` functions).
    // ------------------------------------------------------------------
    let st = state.clone();
    add_lua_fn!(ctx,
        "e_eventStopTime",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            let st2 = st.clone();
            let mut s = lock(&st);
            s.raise_warning(
            "e_eventStopTime",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_stopTime\" in your level files.");
            s.event_timeline.append_do(move || {
                let mut s = lock(&st2);
                s.status.pause_time(ft_to_seconds(duration));
            });
        },
    )?
    .arg("duration")
    .doc("*Add to the event timeline*: pause the game timer for `$0` frames (under the assumption of a 60 FPS frame rate). **This function is deprecated and will be removed in a future version. Please use e_stopTime instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "e_eventStopTimeS",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            let st2 = st.clone();
            let mut s = lock(&st);
            s.raise_warning(
            "e_eventStopTimeS",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_stopTimeS\" in your level files.");
            s.event_timeline.append_do(move || {
                let mut s = lock(&st2);
                s.status.pause_time(duration);
            });
        },
    )?
    .arg("duration")
    .doc("*Add to the event timeline*: pause the game timer for `$0` seconds. **This function is deprecated and will be removed in a future version. Please use e_stopTimeS instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "e_eventWait",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            let mut s = lock(&st);
            s.raise_warning(
            "e_eventWait",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_wait\" in your level files.");
            s.event_timeline.append_wait_for_sixths(duration);
        },
    )?
    .arg("duration")
    .doc("*Add to the event timeline*: wait for `$0` frames (under the assumption of a 60 FPS frame rate). **This function is deprecated and will be removed in a future version. Please use e_wait instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "e_eventWaitS",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            let mut s = lock(&st);
            s.raise_warning(
            "e_eventWaitS",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_waitS\" in your level files.");
            s.event_timeline.append_wait_for_seconds(duration);
        },
    )?
    .arg("duration")
    .doc("*Add to the event timeline*: wait for `$0` seconds. **This function is deprecated and will be removed in a future version. Please use e_waitS instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "e_eventWaitUntilS",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            let st2 = st.clone();
            let mut s = lock(&st);
            s.raise_warning(
            "e_eventWaitUntilS",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_waitUntilS\" in your level files.");
            s.event_timeline.append_wait_until_fn(move || {
                let ms = duration_to_ms_truncated(duration);
                let s = lock(&st2);
                if ms < 0 {
                    s.status
                        .get_level_start_tp()
                        - Duration::from_millis((-ms) as u64)
                } else {
                    s.status
                        .get_level_start_tp()
                        + Duration::from_millis(ms as u64)
                }
            });
        },
    )?
    .arg("duration")
    .doc("*Add to the event timeline*: wait until the timer reaches `$0` seconds. **This function is deprecated and will be removed in a future version. Please use e_waitUntilS instead!**");

    // ------------------------------------------------------------------
    // m_* (deprecated message aliases of the `e_*` message functions).
    // ------------------------------------------------------------------
    let st = state.clone();
    add_lua_fn!(ctx,
        "m_messageAdd",
        FnMeta {
            ret: "void",
            arg_types: &["std::string", "double"],
        },
        move |message: String, duration: f64| {
            let st2 = st.clone();
            let mut s = lock(&st);
            s.raise_warning(
            "m_messageAdd",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_messageAdd\" in your level files and common.lua.");
            s.event_timeline.append_do(move || {
                let s = lock(&st2);
                if s.first_play {
                    s.backend.add_message(&message, duration, true);
                }
            });
        },
    )?
    .arg("message")
    .arg("duration")
    .doc("*Add to the event timeline*: print a message with text `$0` for `$1` seconds. The message will only be printed during the first run of the level. **This function is deprecated and will be removed in a future version. Please use e_messageAdd instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "m_messageAddImportant",
        FnMeta {
            ret: "void",
            arg_types: &["std::string", "double"],
        },
        move |message: String, duration: f64| {
            let st2 = st.clone();
            let mut s = lock(&st);
            s.raise_warning(
            "m_messageAddImportant",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_messageAddImportant\" in your level files and common.lua.");
            s.event_timeline.append_do(move || {
                let s = lock(&st2);
                s.backend.add_message(&message, duration, true);
            });
        },
    )?
    .arg("message")
    .arg("duration")
    .doc("*Add to the event timeline*: print a message with text `$0` for `$1` seconds. The message will be printed during every run of the level. **This function is deprecated and will be removed in a future version. Please use e_messageAddImportant instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "m_messageAddImportantSilent",
        FnMeta {
            ret: "void",
            arg_types: &["std::string", "double"],
        },
        move |message: String, duration: f64| {
            let st2 = st.clone();
            let mut s = lock(&st);
            s.raise_warning(
            "m_messageAddImportantSilent",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_messageAddImportantSilent\" in your level files.");
            s.event_timeline.append_do(move || {
                let s = lock(&st2);
                s.backend.add_message(&message, duration, false);
            });
        },
    )?
    .arg("message")
    .arg("duration")
    .doc("*Add to the event timeline*: print a message with text `$0` for `$1` seconds. The message will only be printed during every run of the level, and will not produce any sound. **This function is deprecated and will be removed in a future version. Please use e_messageAddImportantSilent instead!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "m_clearMessages",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let mut s = lock(&st);
            s.raise_warning(
            "m_clearMessages",
            "This function will be removed in a future version of Open Hexagon. Please replace all occurrences of this function with \"e_clearMessages\" in your level files.");
            s.backend.clear_messages();
        },
    )?
    .doc("Remove all previously scheduled messages. **This function is deprecated and will be removed in a future version. Please use e_clearMessages instead!**");

    // ------------------------------------------------------------------
    // l_forceSetPulse / l_forceSetBeatPulse — live in the C++
    // `initLua_Deprecated` group but do NOT call `raiseWarning` (mapping
    // §2 note).
    // ------------------------------------------------------------------
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "l_forceSetPulse",
        FnMeta {
            ret: "void",
            arg_types: &["float"],
        },
        move |value: f32| {
            let mut s = lock(&st);
            s.status.pulse = value;
            s.backend.refresh_pulse();
        },
    )?
    .doc("Immediately sets the current pulse value to `$0`.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "l_forceSetBeatPulse",
        FnMeta {
            ret: "void",
            arg_types: &["float"],
        },
        move |value: f32| {
            let mut s = lock(&st);
            s.status.beat_pulse = value;
            s.backend.refresh_beat_pulse();
            s.backend.update_player_position(s.status.radius);
        },
    )?
    .doc("Immediately sets the current beat pulse value to `$0`.");

    Ok(())
}
