//! C++ `HexagonGame::initLua_Steam` (HGScripting.cpp:838-848):
//! `steam_unlockAchievement` (no prefix -> the C++ "Miscellaneous" metadata
//! category).

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "steam_unlockAchievement",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |achievement_id: String| {
            let s = lock(&st);
            // C++: no-op while replaying; only fires with a live steam
            // manager AND official mode.
            if s.backend.in_replay() {
                return;
            }
            if s.backend.get_official() {
                s.backend.unlock_achievement(&achievement_id);
            }
        },
    )?
    .arg("achievementId")
    .doc("Unlock the Steam achievement with id `$0`.");

    Ok(())
}
