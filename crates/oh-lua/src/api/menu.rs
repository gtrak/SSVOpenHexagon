//! C++ `MenuGame::initLua` overrides (MenuGame.cpp:1063-1128), applied on
//! top of the shared `init_lua_api` registration (mapping §8.10):
//!
//! * `u_log` is overridden to the menu implementation (`ssvu::lo("lua-menu")`
//!   — no `ilcCmdLog` append, no headless check);
//! * `u_getDifficultyMult` / `u_getSpeedMultDM` / `u_getDelayMultDM` /
//!   `u_getPlayerAngle` are pinned to constants (`1`, `1`, `1`, `0`);
//! * ~70 "unused" level functions are replaced with no-ops. The list is the
//!   authoritative menu-spelling checklist and contains the legacy
//!   `u_timelineWait` (a vestige of an older API, never registered as a real
//!   function) and a duplicated `u_setFlashEffect` entry.
//!
//! All menu registrations are raw `writeVariable`s — no metadata entries.

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use mlua::{MultiValue, Value};

/// The C++ no-op list, verbatim (including the historical `u_timelineWait`
/// and the duplicate `u_setFlashEffect`).
const MENU_NO_OP_FUNCTIONS: &[&str] = &[
    "u_isKeyPressed",
    "u_isMouseButtonPressed",
    "u_isFastSpinning",
    "u_setPlayerAngle",
    "u_forceIncrement",
    "u_haltTime",
    "u_timelineWait",
    "u_clearWalls",
    "u_setFlashEffect",
    "a_setMusic",
    "a_setMusicSegment",
    "a_setMusicSeconds",
    "a_playSound",
    "a_playPackSound",
    "a_syncMusicToDM",
    "a_setMusicPitch",
    "a_overrideBeepSound",
    "a_overrideIncrementSound",
    "a_overrideSwapSound",
    "a_overrideDeathSound",
    "t_eval",
    "t_kill",
    "t_clear",
    "t_wait",
    "t_waitS",
    "t_waitUntilS",
    "e_eval",
    "e_kill",
    "e_stopTime",
    "e_stopTimeS",
    "e_wait",
    "e_waitS",
    "e_waitUntilS",
    "e_messageAdd",
    "e_messageAddImportant",
    "e_messageAddImportantSilent",
    "e_clearMessages",
    "ct_create",
    "ct_eval",
    "ct_kill",
    "ct_stopTime",
    "ct_stopTimeS",
    "ct_wait",
    "ct_waitS",
    "ct_waitUntilS",
    "l_overrideScore",
    "l_setRotation",
    "l_getRotation",
    "l_getOfficial",
    "s_setStyle",
    "w_wall",
    "w_wallAdj",
    "w_wallAcc",
    "w_wallHModSpeedData",
    "w_wallHModCurveData",
    "steam_unlockAchievement",
    "u_kill",
    "u_eventKill",
    "u_playSound",
    "u_playPackSound",
    "u_setFlashEffect",
    "u_setFlashColor",
    "e_eventStopTime",
    "e_eventStopTimeS",
    "e_eventWait",
    "e_eventWaitS",
    "e_eventWaitUntilS",
    "m_messageAdd",
    "m_messageAddImportant",
    "m_messageAddImportantSilent",
    "m_clearMessages",
];

pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    // ------------------------------------------------------------------
    // Menu `u_log` override: `ssvu::lo("lua-menu") << mLog` (no ilcCmdLog).
    // ------------------------------------------------------------------
    let st = state.clone();
    let u_log = ctx
        .lua()
        .create_function(
            move |_lua: &mlua::Lua, message: String| -> mlua::Result<MultiValue> {
                lock(&st).backend.log("lua-menu", &message);
                Ok(MultiValue::new())
            },
        )
        .map_err(LuaError::from_mlua)?;
    ctx.write_variable("u_log", &u_log)?;

    // ------------------------------------------------------------------
    // Pinned getters (menu: no level -> the constants below).
    // ------------------------------------------------------------------
    macro_rules! pin {
        ($name:literal, $value:expr) => {
            let f = ctx
                .lua()
                .create_function(
                    |_lua: &mlua::Lua, _args: mlua::MultiValue| -> mlua::Result<MultiValue> {
                        Ok(MultiValue::from_vec(vec![$value]))
                    },
                )
                .map_err(LuaError::from_mlua)?;
            ctx.write_variable($name, &f)?;
        };
    }
    pin!("u_getDifficultyMult", Value::Number(1.0));
    pin!("u_getSpeedMultDM", Value::Number(1.0));
    pin!("u_getDelayMultDM", Value::Number(1.0));
    pin!("u_getPlayerAngle", Value::Number(0.0));

    // ------------------------------------------------------------------
    // Unused functions -> zero-argument no-ops (C++ `lua.writeVariable(un,
    // [] {})`): calling them with ANY number of arguments is accepted.
    // ------------------------------------------------------------------
    for name in MENU_NO_OP_FUNCTIONS {
        let f = ctx
            .lua()
            .create_function(
                |_lua: &mlua::Lua, _args: mlua::MultiValue| -> mlua::Result<MultiValue> {
                    Ok(MultiValue::new())
                },
            )
            .map_err(LuaError::from_mlua)?;
        ctx.write_variable(name, &f)?;
    }

    Ok(())
}
