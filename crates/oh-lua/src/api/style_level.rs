//! C++ `HexagonGame::initLua_StyleControl` (HGScripting.cpp:691-699):
//! `s_setStyle`. (The `StyleData` accessor pairs and color getters belong to
//! the shared `initStyleControl` group — see `style_control.rs`.)

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

use crate::add_lua_fn_try;
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn_try!(ctx,
        "s_setStyle",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |style_id: String| {
            // C++: `styleData = assets.getStyleData(packId, id);
            // styleData.computeColors();` (throws when the id is unknown).
            let mut s = lock(&st);
            s.style_data = s.assets.get_style_data(&s.pack_id, &style_id)?;
            s.style_data.compute_colors();
            Ok(())
        },
    )?
    .arg("styleId")
    .doc("Set the currently active style to the style with id `$0`. Styles can be defined as `.json` files in the `<pack>/Styles/` folder.");

    Ok(())
}
