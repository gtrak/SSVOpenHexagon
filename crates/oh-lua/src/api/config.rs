//! C++ `hg::LuaScripting::initConfig` (LuaScripting.cpp:1565-1577):
//! `u_getWidth` / `u_getHeight` (live `Config` reads -> `GameApi`).

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_getWidth",
        FnMeta {
            ret: "unsigned int",
            arg_types: &[],
        },
        move || lock(&st).backend.get_width(),
    )?
    .doc("Return the width of the game window in pixels.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_getHeight",
        FnMeta {
            ret: "unsigned int",
            arg_types: &[],
        },
        move || lock(&st).backend.get_height(),
    )?
    .doc("Return the height of the game window in pixels.");

    Ok(())
}
