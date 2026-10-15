//! C++ `hg::LuaScripting::initUtils` (LuaScripting.cpp:220-252):
//! `u_inMenu`, `u_isHeadless`, and the four version getters.
//!
//! `inMenu` / `headless` are CAPTURED at registration time in C++ (plain
//! booleans), so this module reads them once and captures the values.

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;
use oh_data::version::{GAME_VERSION, GAME_VERSION_STR};

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let in_menu = lock(state).in_menu;
    add_lua_fn!(
        ctx,
        "u_inMenu",
        FnMeta {
            ret: "bool",
            arg_types: &[],
        },
        move || in_menu,
    )?
    .doc("Returns `true` if the script is being executed in the menu, `false` otherwise.");

    let headless = lock(state).headless;
    add_lua_fn!(ctx,
        "u_isHeadless",
        FnMeta {
            ret: "bool",
            arg_types: &[],
        },
        move || headless,
    )?
    .doc("Returns `true` if the script is being executed in an headless context (e.g. online replay validation server), `false` otherwise.");

    add_lua_fn!(
        ctx,
        "u_getVersionMajor",
        FnMeta {
            ret: "int",
            arg_types: &[],
        },
        || GAME_VERSION.major,
    )?
    .doc("Returns the major of the current version of the game");

    add_lua_fn!(
        ctx,
        "u_getVersionMinor",
        FnMeta {
            ret: "int",
            arg_types: &[]
        },
        || GAME_VERSION.minor,
    )?
    .doc("Returns the minor of the current version of the game");

    add_lua_fn!(
        ctx,
        "u_getVersionMicro",
        FnMeta {
            ret: "int",
            arg_types: &[]
        },
        || GAME_VERSION.micro,
    )?
    .doc("Returns the micro of the current version of the game");

    add_lua_fn!(
        ctx,
        "u_getVersionString",
        FnMeta {
            ret: "std::string",
            arg_types: &[]
        },
        || GAME_VERSION_STR.to_string(),
    )?
    .doc("Returns the string representing the current version of the game");

    Ok(())
}
