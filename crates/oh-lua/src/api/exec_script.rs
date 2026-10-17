//! C++ `hg::LuaScripting::initExecScript` (LuaScripting.cpp:1193-1232) +
//! `Utils.cpp` dependency resolution (`getDependentScriptFilename`,
//! `withDependencyScriptFilename`, `findDependencyPackDataOrThrow`).
//!
//! Reproduced semantics (mapping §6):
//! * `u_execScript(x)` runs `<context>/Scripts/x` where the context is the
//!   *innermost active dependency folder* (top of
//!   `exec_script_pack_path_context`) or the level's own pack path when the
//!   stack is empty.
//! * `u_execDependencyScript` validates `(disambiguator, name, author)`
//!   against the **level's own pack** dependencies (+ min version), pushes
//!   the dependee's `folderPath` onto the context stack, runs
//!   `<dependeeFolder>/Scripts/<script>`, and pops it afterwards (scope
//!   guard) — so nested `u_execScript` calls inside the dependency script
//!   resolve relative to the dependency pack.
//! * Validation failures throw C++ `runtime_error`s with exact messages;
//!   the port surfaces them as Lua runtime errors (script aborts).

use super::{lock, State};
use crate::api::state::SharedLua;
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

use crate::add_lua_fn_try;
pub(crate) fn register(ctx: &LuaContext, state: &State, shared: SharedLua) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn_try!(
        ctx,
        "u_execScript",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |script_name: String| {
            let (path, run) = {
                let s = lock(&st);
                (
                    s.get_dependent_script_filename(&script_name),
                    s.run_lua_file.clone(),
                )
            };
            run(&path, &shared)
        },
    )?
    .arg("scriptFilename")
    .doc("Execute the script located at `<pack>/Scripts/$0`.");

    let st = state.clone();
    add_lua_fn_try!(ctx,
        "u_execDependencyScript",
        FnMeta {
            ret: "void",
            arg_types: &["std::string", "std::string", "std::string", "std::string"],
        },
        move |pack_disambiguator: String, pack_name: String, pack_author: String, script_name: String| {
            // C++ `withDependencyScriptFilename` (scope-guarded):
            // `withDependencyAssetFilename` (Utils.cpp:176-207) — a
            // validation failure logs the "Fatal error while looking
            // for Lua dependency" line and rethrows: the script ABORTS
            // (the port returns the error to the Lua call frame — the
            // propagation was already faithful; 29g1 adds the missing
            // diagnostic line). Note: a dependency SCRIPT's own failure
            // never reaches that catch in the C++ either — the
            // `HexagonGame::runLuaFile` catch (HGScripting.cpp:1127-1147)
            // swallows it (`goToMenu` unless debug) before it could
            // rethrow, exactly like the port's `run_lua_file` closure.
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
            let run = {
                let mut s = lock(&st);
                s.exec_script_pack_path_context
                    .push(dependency.folder_path.clone());
                s.run_lua_file.clone()
            };
            let path = format!("{}/Scripts/{script_name}", dependency.folder_path);
            let result = run(&path, &shared);
            lock(&st).exec_script_pack_path_context.pop();
            result
        },
    )?
    .arg("packDisambiguator")
    .arg("packName")
    .arg("packAuthor")
    .arg("scriptFilename")
    .doc("Execute the script provided by the dependee pack with disambiguator `$0`, name `$1`, author `$2`, located at `<dependeePack>/Scripts/$3`.");

    Ok(())
}
