//! C++ `hg::LuaScripting::initStyleControl` (LuaScripting.cpp:959-1190):
//! the 23 `StyleData` accessor pairs (including the `HueInc`/`PulseInc`
//! backward-compat aliases that register BOTH spellings), the 3 cap-color
//! setters, and the 6 color getters (5 via the `sdColorGetter` helper +
//! `s_getColor`).
//!
//! (`s_setStyle` lives in the game-group `style_level` module, C++
//! `HexagonGame::initLua_StyleControl`.)

use super::{lock, GameApiState, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;
use oh_data::cap_color::CapColor;

/// `s_get<Name>` / `s_set<Name>` over the `style_data.<field>` — the Rust
/// form of the C++ `makeLuaAccessor(lua, styleData, "s")` pair. The call
/// site passes the field as a getter and setter closure over
/// `&mut GameApiState`; the macro handles the `State` cloning and locking.
macro_rules! reg_style_accessor {
    (
        $ctx:expr, $st:expr, $Name:literal, $Ty:ty, $ret:literal,
        $get:expr, $set:expr, $getter:expr, $setter:expr
        ) => {{
        // The `st` clone is hoisted out of the closure argument so the
        // closure itself is the 4th macro argument (`add_lua_fn!` matches on
        // the closure tokens).
        let st = { $st.clone() };
        $crate::add_lua_fn!(
            $ctx,
            concat!("s_get", $Name),
            crate::metadata::FnMeta {
                ret: $ret,
                arg_types: &[],
            },
            move || $get(&mut lock(&st)),
        )?
        .doc($getter);

        let st = { $st.clone() };
        $crate::add_lua_fn!(
            $ctx,
            concat!("s_set", $Name),
            crate::metadata::FnMeta {
                ret: "void",
                arg_types: &[$ret],
            },
            move |value: $Ty| {
                $set(&mut lock(&st), value);
            },
        )?
        .arg("value")
        .doc($setter);
    }};
}

/// C++ `colorToTuple(const sf::Color&)` — `(r, g, b, a)` as `int`s.
fn color_to_tuple(c: &oh_math::hg::color::Color) -> (i32, i32, i32, i32) {
    (
        i32::from(c.r),
        i32::from(c.g),
        i32::from(c.b),
        i32::from(c.a),
    )
}

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    reg_style_accessor!(
        ctx, state, "HueMin", f32, "float",
        |s: &mut GameApiState| s.style_data.hue_min,
        |s: &mut GameApiState, v| s.style_data.hue_min = v,
        "Gets the minimum value for the hue range of a level style. The hue attribute is an important attribute that is dedicated specifically to all colors that have the ``dynamic`` property enabled.",
        "Sets the minimum value for the hue range to `$0`. Usually you want this value at 0 to start off at completely red."
    );
    reg_style_accessor!(
        ctx, state, "HueMax", f32, "float",
        |s: &mut GameApiState| s.style_data.hue_max,
        |s: &mut GameApiState, v| s.style_data.hue_max = v,
        "Gets the maximum value for the hue range of a level style. Only applies to all colors with the ``dynamic`` property enabled.",
        "Sets the maximum value for the hue range to `$0`. Usually you want this value at 360 to end off at red, to hopefully loop the colors around."
    );
    // backwards-compatible alias pair: same member as `HueIncrement`.
    reg_style_accessor!(
        ctx,
        state,
        "HueInc",
        f32,
        "float",
        |s: &mut GameApiState| s.style_data.hue_increment,
        |s: &mut GameApiState, v| s.style_data.hue_increment = v,
        "Alias to ``s_getHueIncrement``. Done for backwards compatibility.",
        "Alias to ``s_setHueIncrement``. Done for backwards compatibility."
    );
    reg_style_accessor!(
        ctx, state, "HueIncrement", f32, "float",
        |s: &mut GameApiState| s.style_data.hue_increment,
        |s: &mut GameApiState, v| s.style_data.hue_increment = v,
        "Gets how fast the hue increments from ``HueMin`` to ``HueMax``. The hue value is added by this value every 1/60th of a second.",
        "Sets how fast the hue increments from ``HueMin`` to ``HueMax`` by `$0`. Be careful with high values, as this can make your style induce epileptic seizures."
    );
    reg_style_accessor!(
        ctx, state, "PulseMin", f32, "float",
        |s: &mut GameApiState| s.style_data.pulse_min,
        |s: &mut GameApiState, v| s.style_data.pulse_min = v,
        "Gets the minimum range for the multiplier of the ``pulse`` attribute in style colors. By default, this value is set to 0.",
        "Sets the minimum range for the multiplier of the ``pulse`` attribute to `$0`."
    );
    reg_style_accessor!(
        ctx, state, "PulseMax", f32, "float",
        |s: &mut GameApiState| s.style_data.pulse_max,
        |s: &mut GameApiState, v| s.style_data.pulse_max = v,
        "Gets the maximum range for the multiplier of the ``pulse`` attribute in style colors. By default, this value is set to 0, but ideally it should be set to 1.",
        "Sets the maximum range for the multiplier of the ``pulse`` attribute to `$0`."
    );
    // backwards-compatible alias pair: same member as `PulseIncrement`.
    reg_style_accessor!(
        ctx,
        state,
        "PulseInc",
        f32,
        "float",
        |s: &mut GameApiState| s.style_data.pulse_increment,
        |s: &mut GameApiState, v| s.style_data.pulse_increment = v,
        "Alias to ``s_getPulseIncrement``. Done for backwards compatibility.",
        "Alias to ``s_setPulseIncrement``. Done for backwards compatibility."
    );
    reg_style_accessor!(
        ctx, state, "PulseIncrement", f32, "float",
        |s: &mut GameApiState| s.style_data.pulse_increment,
        |s: &mut GameApiState, v| s.style_data.pulse_increment = v,
        "Gets how fast the pulse increments from ``PulseMin`` to ``PulseMax``. The pulse value is added by this value every 1/60th of a second.",
        "Sets how fast the pulse increments from ``PulseMin`` to ``PulseMax`` by `$0`. Be careful with high values, as this can make your style induce epileptic seizures."
    );
    reg_style_accessor!(
        ctx, state, "HuePingPong", bool, "bool",
        |s: &mut GameApiState| s.style_data.hue_ping_pong,
        |s: &mut GameApiState, v| s.style_data.hue_ping_pong = v,
        "Gets whether the hue should go ``Start-End-Start-End`` or ``Start-End, Start-End`` with the hue cycling.",
        "Toggles ping ponging in the hue cycling (``Start-End-Start-End``) with `$0`."
    );
    reg_style_accessor!(
        ctx, state, "MaxSwapTime", f32, "float",
        |s: &mut GameApiState| s.style_data.max_swap_time,
        |s: &mut GameApiState, v| s.style_data.max_swap_time = v,
        "Gets the amount of time that has to pass (in 1/100th of a second) before the background color offset alternates. The background colors by default alternate between 0 and 1. By default, this happens every second.",
        "Sets the amount of time that has to pass (in 1/100th of a second) to `$0` before the background color alternates."
    );
    reg_style_accessor!(
        ctx,
        state,
        "3dDepth",
        f32,
        "float",
        |s: &mut GameApiState| s.style_data._3d_depth,
        |s: &mut GameApiState, v| s.style_data._3d_depth = v,
        "Gets the current amount of 3D layers that are present in the style.",
        "Sets the amount of 3D layers in a style to `$0`."
    );
    reg_style_accessor!(
        ctx, state, "3dSkew", f32, "float",
        |s: &mut GameApiState| s.style_data._3d_skew,
        |s: &mut GameApiState, v| s.style_data._3d_skew = v,
        "Gets the current value of where the 3D skew is in the style. The Skew is what gives the 3D effect in the first place, showing the 3D layers and giving the illusion of 3D in the game.",
        "Sets the 3D skew at value `$0`."
    );
    reg_style_accessor!(
        ctx, state, "3dSpacing", f32, "float",
        |s: &mut GameApiState| s.style_data._3d_spacing,
        |s: &mut GameApiState, v| s.style_data._3d_spacing = v,
        "Gets the spacing that is done between 3D layers. A higher number leads to more separation between layers.",
        "Sets the spacing between 3D layers to `$0`."
    );
    reg_style_accessor!(
        ctx, state, "3dDarkenMult", f32, "float",
        |s: &mut GameApiState| s.style_data._3d_darken_mult,
        |s: &mut GameApiState, v| s.style_data._3d_darken_mult = v,
        "Gets the darkening multiplier applied to the 3D layers in a style. This is taken from the ``main`` color.",
        "Sets the darkening multiplier to `$0` for the 3D layers."
    );
    reg_style_accessor!(
        ctx, state, "3dAlphaMult", f32, "float",
        |s: &mut GameApiState| s.style_data._3d_alpha_mult,
        |s: &mut GameApiState, v| s.style_data._3d_alpha_mult = v,
        "Gets the alpha (transparency) multiplier applied to the 3D layers in a style. Originally references the ``main`` color.",
        "Sets the alpha multiplier to `$0` for the 3D layers. A higher value makes the layers more transparent."
    );
    reg_style_accessor!(
        ctx, state, "3dAlphaFalloff", f32, "float",
        |s: &mut GameApiState| s.style_data._3d_alpha_falloff,
        |s: &mut GameApiState, v| s.style_data._3d_alpha_falloff = v,
        "Gets the alpha (transparency) multiplier applied to the 3D layers consecutively in a style. Takes reference from the ``main`` color.",
        "Sets the alpha multiplier to `$0` for for the 3D layers and applies them layer after layer. This property can get finnicky."
    );
    reg_style_accessor!(
        ctx,
        state,
        "3dPulseMax",
        f32,
        "float",
        |s: &mut GameApiState| s.style_data._3d_pulse_max,
        |s: &mut GameApiState, v| s.style_data._3d_pulse_max = v,
        "Gets the highest value that the ``3DSkew`` can go in a style.",
        "Sets the highest value the ``3DSkew`` can go to `$0`."
    );
    reg_style_accessor!(
        ctx,
        state,
        "3dPulseMin",
        f32,
        "float",
        |s: &mut GameApiState| s.style_data._3d_pulse_min,
        |s: &mut GameApiState, v| s.style_data._3d_pulse_min = v,
        "Gets the lowest value that the ``3DSkew`` can go in a style.",
        "Sets the lowest value the ``3DSkew`` can go to `$0`."
    );
    reg_style_accessor!(
        ctx,
        state,
        "3dPulseSpeed",
        f32,
        "float",
        |s: &mut GameApiState| s.style_data._3d_pulse_speed,
        |s: &mut GameApiState, v| s.style_data._3d_pulse_speed = v,
        "Gets how fast the ``3DSkew`` moves between ``3DPulseMin`` and ``3DPulseMax``.",
        "Sets how fast the ``3DSkew`` moves between ``3DPulseMin`` and ``3DPulseMax`` by `$0`."
    );
    reg_style_accessor!(
        ctx, state, "3dPerspectiveMult", f32, "float",
        |s: &mut GameApiState| s.style_data._3d_perspective_mult,
        |s: &mut GameApiState, v| s.style_data._3d_perspective_mult = v,
        "Gets the 3D perspective multiplier of the style. Works with the attribute ``3DSpacing`` to space out layers.",
        "Sets the 3D perspective multiplier to `$0`."
    );
    reg_style_accessor!(
        ctx, state, "BGTileRadius", f32, "float",
        |s: &mut GameApiState| s.style_data.bg_tile_radius,
        |s: &mut GameApiState, v| s.style_data.bg_tile_radius = v,
        "Gets the distances of how far the background panels are drawn. By default, this is a big enough value so you do not see the border. However, feel free to shrink them if you'd like.",
        "Sets how far the background panels are drawn to distance `$0`."
    );
    reg_style_accessor!(
        ctx, state, "BGColorOffset", u32, "unsigned int",
        |s: &mut GameApiState| s.style_data.bg_color_offset,
        |s: &mut GameApiState, v| s.style_data.bg_color_offset = v,
        "Gets the offset of the style by how much the colors shift. Usually this sits between 0 and 1, but can easily be customized.",
        "Shifts the background colors to have an offset of `$0`."
    );
    reg_style_accessor!(
        ctx, state, "BGRotationOffset", f32, "float",
        |s: &mut GameApiState| s.style_data.bg_rot_off,
        |s: &mut GameApiState, v| s.style_data.bg_rot_off = v,
        "Gets the literal rotation offset of the background panels in degrees. This usually stays at 0, but can be messed with to make some stylish level styles.",
        "Sets the rotation offset of the background panels to `$0` degrees."
    );

    // ------------------------------------------------------------------
    // Cap-color setters (C++ LuaScripting.cpp:1132-1152).
    // ------------------------------------------------------------------
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "s_setCapColorMain",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let mut s = lock(&st);
            s.style_data.set_cap_color(&CapColor::Main);
        },
    )?
    .doc("Set the color of the center polygon to match the main style color.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "s_setCapColorMainDarkened",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let mut s = lock(&st);
            s.style_data.set_cap_color(&CapColor::MainDarkened);
        },
    )?
    .doc("Set the color of the center polygon to match the main style color, darkened.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "s_setCapColorByIndex",
        FnMeta {
            ret: "void",
            arg_types: &["int"],
        },
        move |index: i32| {
            let mut s = lock(&st);
            s.style_data.set_cap_color(&CapColor::ByIndex { index });
        },
    )?
    .arg("index")
    .doc("Set the color of the center polygon to match the style color with index `$0`.");

    // ------------------------------------------------------------------
    // Color getters (C++ `sdColorGetter` registrations, :1160-1173).
    // ------------------------------------------------------------------
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "s_getMainColor",
        FnMeta {
            ret: "tuple<int, int, int, int>",
            arg_types: &[],
        },
        move || color_to_tuple(lock(&st).style_data.get_main_color()),
    )?
    .doc("Return the current main color computed by the level style.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "s_getPlayerColor",
        FnMeta {
            ret: "tuple<int, int, int, int>",
            arg_types: &[],
        },
        move || color_to_tuple(lock(&st).style_data.get_player_color()),
    )?
    .doc("Return the current player color computed by the level style.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "s_getTextColor",
        FnMeta {
            ret: "tuple<int, int, int, int>",
            arg_types: &[],
        },
        move || color_to_tuple(lock(&st).style_data.get_text_color()),
    )?
    .doc("Return the current text color computed by the level style.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "s_get3DOverrideColor",
        FnMeta {
            ret: "tuple<int, int, int, int>",
            arg_types: &[],
        },
        move || color_to_tuple(lock(&st).style_data.get_3d_override_color()),
    )?
    .doc("Return the current 3D override color computed by the level style.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "s_getCapColorResult",
        FnMeta {
            ret: "tuple<int, int, int, int>",
            arg_types: &[],
        },
        move || {
            let s = lock(&st);
            // C++ `getCapColorResult()` resolves the cap color; the `Option`
            // form here falls back to the current main color when nothing has
            // been resolved (the C++ accessor always has a value after
            // `computeColors`).
            color_to_tuple(
                s.style_data
                    .get_cap_color_result()
                    .as_ref()
                    .unwrap_or(s.style_data.get_main_color()),
            )
        },
    )?
    .doc("Return the current cap color result computed by the level style.");

    // ------------------------------------------------------------------
    // s_getColor (C++ LuaScripting.cpp:1178-1190).
    // ------------------------------------------------------------------
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "s_getColor",
        FnMeta {
            ret: "tuple<int, int, int, int>",
            arg_types: &["int"],
        },
        move |index: i32| -> Result<(i32, i32, i32, i32), LuaError> {
            let s = lock(&st);
            if s.style_data.get_colors().is_empty() {
                // C++: `colorToTuple(sf::Color{0, 0, 0, 0})`.
                Ok((0, 0, 0, 0))
            } else {
                // C++ `StyleData::getColor(index)` uses `std::vector::at`
                // (throws out-of-range); the port surfaces the same failure
                // as a Lua runtime error.
                Ok(color_to_tuple(s.style_data.get_color(index as u32).ok_or(
                    LuaError::ExecutionError {
                        message: format!("style color index {index} out of range"),
                    },
                )?))
            }
        },
    )?
    .arg("index")
    .doc("Return the current color with index `$0` computed by the level style.");

    Ok(())
}
