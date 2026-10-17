//! C++ `hg::LuaScripting::initShaders` (LuaScripting.cpp:1235-1561): the 13
//! `shdr_*` functions.
//!
//! Faithful to the C++ guard structure: every uniform setter goes through
//! `withValidShaderId` (headless → no-op; invalid id → log + no-op), and
//! the render-stage setters check the stage index against
//! `RenderStage::Count == 9` (out of range → log + no-op; headless → no-op).
//! Fragment-shader activation lives in `status.fragment_shader_ids`
//! (`[Option<u64>; 9]` — the 9 C++ `RenderStage` values).

use super::{lock, State};
use crate::add_lua_fn_try;
use crate::api::state::SharedLua;
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

/// C++ `RenderStage::Count`.
const RENDER_STAGE_COUNT: u64 = 9;

/// C++ `static_cast<std::size_t>(-1)` — the "no id" sentinel.
const NO_SHADER_ID: u64 = u64::MAX;

use crate::add_lua_fn;
pub(crate) fn register(
    ctx: &LuaContext,
    state: &State,
    _shared: SharedLua,
) -> Result<(), LuaError> {
    let headless = lock(state).headless;

    // ------------------------------------------------------------------
    // Shader id retrieval.
    // ------------------------------------------------------------------
    let st = state.clone();
    add_lua_fn!(ctx,
        "shdr_getShaderId",
        FnMeta {
            ret: "size_t",
            arg_types: &["std::string"],
        },
        move |shader_filename: String| {
            if headless {
                // C++: always return early in headless mode.
                return NO_SHADER_ID;
            }
            let s = lock(&st);
            let path = s.get_dependent_shader_filename(&shader_filename);
            match s.assets.get_shader_id_by_path(&path) {
                Some(id) => id,
                None => {
                    s.backend.log(
                        "hg::LuaScripting::initShaders",
                        &format!(
                            "`u_getShaderId` failed, no id found for '{shader_filename}'\n"
                        ),
                    );
                    NO_SHADER_ID
                }
            }
        },
    )?
    .arg("shaderFilename")
    .doc("Retrieve the id of the shader with name `$0` from the current pack. The shader will be searched for in the `Shaders/` subfolder. The extension of the shader should be included in `$0` and it will determine its type (i.e. `.frag`, `.vert`, and `.geom`).");

    let st = state.clone();
    add_lua_fn_try!(ctx,
        "shdr_getDependencyShaderId",
        FnMeta {
            ret: "size_t",
            arg_types: &["std::string", "std::string", "std::string", "std::string"],
        },
        move |pack_disambiguator: String, pack_name: String, pack_author: String,
              shader_filename: String| -> Result<u64, LuaError> {
            if headless {
                // C++: always return early in headless mode.
                return Ok(NO_SHADER_ID);
            }
            // C++ `Utils::withDependencyShaderFilename` →
            // `withDependencyAssetFilename` (Utils.cpp:176-207): a
            // validation failure (the (disambiguator, name, author)
            // triple is not a dependency of the level's pack / the
            // pack is missing / the version is too low) logs the
            // "Fatal error while looking for Lua dependency" line and
            // RETHROWS — the running script ABORTS (the port returns
            // the error to the Lua call frame, exactly the
            // `u_execDependencyScript` sibling's pattern — LuaScripting.cpp:1267-1311).
            let dependency_result = {
                let s = lock(&st);
                s.find_dependency_pack_data_or_throw(
                    &pack_disambiguator,
                    &pack_name,
                    &pack_author,
                )
            };
            let dependency = match dependency_result {
                Ok(d) => d,
                Err(err) => {
                    lock(&st).log_dependency_lookup_failure(&err);
                    return Err(err);
                }
            };
            // C++ `withDependencyShaderFilename` (scope-guarded context
            // push/pop around the resolution).
            let id = {
                let mut s = lock(&st);
                s.exec_script_pack_path_context
                    .push(dependency.folder_path.clone());
                let path =
                    format!("{}/Shaders/{shader_filename}", dependency.folder_path);
                let id = s.assets.get_shader_id_by_path(&path);
                s.exec_script_pack_path_context.pop();
                id
            };
            match id {
                Some(v) => Ok(v),
                None => {
                    let path = format!(
                        "{}/Shaders/{shader_filename}",
                        dependency.folder_path
                    );
                    lock(&st).backend.log(
                        "hg::LuaScripting::initShaders",
                        &format!(
                            "`u_getDependencyShaderId` failed, no id found for '{path}'\n"
                        ),
                    );
                    // C++: the not-found arm does NOT abort — it logs
                    // and returns `size_t(-1)`, the script continues.
                    Ok(NO_SHADER_ID)
                }
            }
        },
    )?
    .arg("packDisambiguator")
    .arg("packName")
    .arg("packAuthor")
    .arg("shaderFilename")
    .doc("Retrieve the id of the shader with name `$3` provided by the dependee pack with disambiguator `$0`, name `$1`, author `$2`. The shader will be searched for in the `Shaders/` subfolder of the dependee. The extension of the shader should be included in `$0` and it will determine its type (i.e. `.frag`, `.vert`, and `.geom`).");

    // ------------------------------------------------------------------
    // Uniform setters (C++ `withValidShaderId` guard around each).
    // ------------------------------------------------------------------
    macro_rules! with_valid_shader_id_inner {
        ($st:expr, $s:ident, $caller:literal, $shader_id:expr, $body:expr) => {
            if headless || !$s.assets.is_valid_shader_id($shader_id) {
                if !headless {
                    let caller = $caller;
                    let shader_id = $shader_id;
                    $s.backend.log(
                        "hg::LuaScripting::initShaders",
                        &format!("`{caller}` failed, invalid shader id '{shader_id}'\n"),
                    );
                }
            } else {
                $body
            }
        };
    }

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "shdr_setUniformF",
        FnMeta {
            ret: "void",
            arg_types: &["size_t", "std::string", "float"],
        },
        move |shader_id: u64, name: String, a: f32| {
            let s = lock(&st);
            with_valid_shader_id_inner!(
                st,
                s,
                "shdr_setUniformF",
                shader_id,
                s.assets.set_shader_uniform_f(shader_id, &name, a)
            );
        },
    )?
    .arg("shaderId")
    .arg("name")
    .arg("a")
    .doc("Set the float uniform with name `$1` of the shader with id `$0` to `$2`.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "shdr_setUniformFVec2",
        FnMeta {
            ret: "void",
            arg_types: &["size_t", "std::string", "float", "float"],
        },
        move |shader_id: u64, name: String, a: f32, b: f32| {
            let s = lock(&st);
            with_valid_shader_id_inner!(
                st,
                s,
                "shdr_setUniformFVec2",
                shader_id,
                s.assets.set_shader_uniform_f_vec2(shader_id, &name, a, b)
            );
        },
    )?
    .arg("shaderId")
    .arg("name")
    .arg("a")
    .arg("b")
    .doc("Set the float vector2 uniform with name `$1` of the shader with id `$0` to `{$2, $3}`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "shdr_setUniformFVec3",
        FnMeta {
            ret: "void",
            arg_types: &["size_t", "std::string", "float", "float", "float"],
        },
        move |shader_id: u64, name: String, a: f32, b: f32, c: f32| {
            let s = lock(&st);
            with_valid_shader_id_inner!(
                st, s, "shdr_setUniformFVec3", shader_id,
                s.assets.set_shader_uniform_f_vec3(shader_id, &name, a, b, c)
            );
        },
    )?
    .arg("shaderId")
    .arg("name")
    .arg("a")
    .arg("b")
    .arg("c")
    .doc("Set the float vector3 uniform with name `$1` of the shader with id `$0` to `{$2, $3, $4}`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "shdr_setUniformFVec4",
        FnMeta {
            ret: "void",
            arg_types: &["size_t", "std::string", "float", "float", "float", "float"],
        },
        move |shader_id: u64, name: String, a: f32, b: f32, c: f32, d: f32| {
            let s = lock(&st);
            with_valid_shader_id_inner!(
                st, s, "shdr_setUniformFVec4", shader_id,
                s.assets.set_shader_uniform_f_vec4(shader_id, &name, a, b, c, d)
            );
        },
    )?
    .arg("shaderId")
    .arg("name")
    .arg("a")
    .arg("b")
    .arg("c")
    .arg("d")
    .doc("Set the float vector4 uniform with name `$1` of the shader with id `$0` to `{$2, $3, $4, $5}`.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "shdr_setUniformI",
        FnMeta {
            ret: "void",
            arg_types: &["size_t", "std::string", "int"],
        },
        move |shader_id: u64, name: String, a: i32| {
            let s = lock(&st);
            with_valid_shader_id_inner!(
                st,
                s,
                "shdr_setUniformI",
                shader_id,
                s.assets.set_shader_uniform_i(shader_id, &name, a)
            );
        },
    )?
    .arg("shaderId")
    .arg("name")
    .arg("a")
    .doc("Set the integer uniform with name `$1` of the shader with id `$0` to `$2`.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "shdr_setUniformIVec2",
        FnMeta {
            ret: "void",
            arg_types: &["size_t", "std::string", "int", "int"],
        },
        move |shader_id: u64, name: String, a: i32, b: i32| {
            let s = lock(&st);
            with_valid_shader_id_inner!(
                st,
                s,
                "shdr_setUniformIVec2",
                shader_id,
                s.assets.set_shader_uniform_i_vec2(shader_id, &name, a, b)
            );
        },
    )?
    .arg("shaderId")
    .arg("name")
    .arg("a")
    .arg("b")
    .doc(
        "Set the integer vector2 uniform with name `$1` of the shader with id `$0` to `{$2, $3}`.",
    );

    let st = state.clone();
    add_lua_fn!(ctx,
        "shdr_setUniformIVec3",
        FnMeta {
            ret: "void",
            arg_types: &["size_t", "std::string", "int", "int", "int"],
        },
        move |shader_id: u64, name: String, a: i32, b: i32, c: i32| {
            let s = lock(&st);
            with_valid_shader_id_inner!(
                st, s, "shdr_setUniformIVec3", shader_id,
                s.assets.set_shader_uniform_i_vec3(shader_id, &name, a, b, c)
            );
        },
    )?
    .arg("shaderId")
    .arg("name")
    .arg("a")
    .arg("b")
    .arg("c")
    .doc("Set the integer vector3 uniform with name `$1` of the shader with id `$0` to `{$2, $3, $4}`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "shdr_setUniformIVec4",
        FnMeta {
            ret: "void",
            arg_types: &["size_t", "std::string", "int", "int", "int", "int"],
        },
        move |shader_id: u64, name: String, a: i32, b: i32, c: i32, d: i32| {
            let s = lock(&st);
            with_valid_shader_id_inner!(
                st, s, "shdr_setUniformIVec4", shader_id,
                s.assets.set_shader_uniform_i_vec4(shader_id, &name, a, b, c, d)
            );
        },
    )?
    .arg("shaderId")
    .arg("name")
    .arg("a")
    .arg("b")
    .arg("c")
    .arg("d")
    .doc("Set the integer vector4 uniform with name `$1` of the shader with id `$0` to `{$2, $3, $4, $5}`.");

    // ------------------------------------------------------------------
    // Active fragment shader per render stage.
    // ------------------------------------------------------------------
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "shdr_resetAllActiveFragmentShaders",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let mut s = lock(&st);
            for id in s.status.fragment_shader_ids.iter_mut() {
                *id = None;
            }
        },
    )?
    .doc("Reset all active fragment shaders in all render stages.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "shdr_resetActiveFragmentShader",
        FnMeta {
            ret: "void",
            arg_types: &["size_t"],
        },
        move |render_stage: u64| {
            if headless {
                return;
            }
            let mut s = lock(&st);
            if render_stage >= RENDER_STAGE_COUNT {
                s.backend.log(
                    "hg::LuaScripting::initShaders",
                    &format!(
                        "`shdr_resetActiveFragmentShader` failed, invalid render stage id '{render_stage}'\n"
                    ),
                );
                return;
            }
            s.status.fragment_shader_ids[render_stage as usize] = None;
        },
    )?
    .arg("renderStage")
    .doc("Reset the currently active fragment shader for the render stage `$0`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "shdr_setActiveFragmentShader",
        FnMeta {
            ret: "void",
            arg_types: &["size_t", "size_t"],
        },
        move |render_stage: u64, shader_id: u64| {
            if headless {
                return;
            }
            let mut s = lock(&st);
            if render_stage >= RENDER_STAGE_COUNT {
                s.backend.log(
                    "hg::LuaScripting::initShaders",
                    &format!(
                        "`shdr_setActiveFragmentShader` failed, invalid render stage id '{render_stage}'\n"
                    ),
                );
                return;
            }
            s.status.fragment_shader_ids[render_stage as usize] = Some(shader_id);
        },
    )?
    .arg("renderStage")
    .arg("shaderId")
    .doc("Set the currently active fragment shader for the render stage `$0` to the shader with id `$1`.");

    Ok(())
}
