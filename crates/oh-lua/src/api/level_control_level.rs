//! C++ `HexagonGame::initLua_LevelControl` (HGScripting.cpp:625-688): the 4
//! direct level-control functions (the 43+6 accessor pairs are registered in
//! `level_control.rs` by the shared `initLevelControl` group).

use super::{lock, State};
use crate::api::state::SharedLua;
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State, shared: SharedLua) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(ctx,
        "l_overrideScore",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |variable: String| {
            // C++: set the override fields, then run the string-spliced
            // validation snippet (the variable name is spliced in — it must
            // be a GLOBAL).
            {
                let mut s = lock(&st);
                s.level_status.score_override = variable.clone();
                s.level_status.score_overridden = true;
            }
            // C++ raw string, verbatim: `"if (type(" + mVar + R"( ) ~=
            // "number") then\n<8 tabs>error("Score override must be a number
            // value")\n<9 tabs>end\n)"`. (The lone `)` right after the
            // variable closes `type(`; the `if (` paren closes after
            // "number" — a second `)` there was a Lua syntax error on
            // every call, diverging from the C++ splice.)
            let code = format!(
                "if (type({variable} ) ~= \"number\") then\n\t\t\t\t\t\t\t\t\terror(\"Score override must be a number value\")\n\t\t\t\t\t\t\t\t\tend\n"
            );
            // C++: on `runtime_error`, print + `goToMenu(false, true)` unless
            // debug mode.
            if let Err(err) = shared.execute_code(&code) {
                let s = lock(&st);
                let level_name = s.level_name.clone();
                let message = format!(
                    "[l_overrideScore] Runtime error on overriding score with level \"{level_name}\": \n{err}\n"
                );
                s.backend.log("l_overrideScore", &message);
                if !s.backend.get_debug() {
                    s.backend.go_to_menu(false, true);
                }
            }
        },
    )?
    .arg("variable")
    .doc("Overrides the default scoring method and determines score based off the value of `$0`. This allows for custom scoring in levels. *Avoid using strings, otherwise scores won't sort properly. NOTE: Your variable must be global for this to work.*");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "l_setRotation",
        FnMeta {
            ret: "void",
            arg_types: &["float"],
        },
        move |value: f32| {
            let s = lock(&st);
            // C++: only acts when `backgroundCamera.has_value()`.
            if s.backend.background_camera_rotation().is_some() {
                s.backend.set_background_camera_rotation(value);
            }
        },
    )?
    .arg("angle")
    .doc("Set the background camera rotation to `$0` degrees.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "l_getRotation",
        FnMeta {
            ret: "float",
            arg_types: &[],
        },
        move || lock(&st)
            .backend
            .background_camera_rotation()
            .unwrap_or(0.0),
    )?
    .doc("Return the background camera rotation, in degrees.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "l_getOfficial",
        FnMeta {
            ret: "bool",
            arg_types: &[],
        },
        move || lock(&st).backend.get_official(),
    )?
    .doc("Return `true` if \"official mode\" is enabled, `false` otherwise.");

    Ok(())
}
