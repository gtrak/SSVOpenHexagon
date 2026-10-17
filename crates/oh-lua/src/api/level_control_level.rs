//! C++ `HexagonGame::initLua_LevelControl` (HGScripting.cpp:625-688): the 4
//! direct level-control functions (the 43+6 accessor pairs are registered in
//! `level_control.rs` by the shared `initLevelControl` group).

use super::{lock, State};
use crate::api::state::SharedLua;
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;
use crate::snippets::OVERRIDE_SCORE_VALIDATION;

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
            // The validation snippet is the verbatim C++ splice
            // ("if (type(" + mVar + R"( ) ~= \"number\") then...";
            // HGScripting.cpp:635-638), the single source of truth in
            // `crate::snippets` (29v1's table compiles every embedded
            // snippet — the 29s1 extra-`)` regression guard).
            // `{variable}` is the C++'s `mVar` splice point.
            let code =
                OVERRIDE_SCORE_VALIDATION.replace("{variable}", &variable);
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
