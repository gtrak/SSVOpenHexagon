//! C++ `HexagonGame::initLua_Utils` (HGScripting.cpp:48-179): the game
//! context's utility functions, including `u_impl_addTrackedResult`
//! (registered with a *raw* `writeVariable`, i.e. no metadata/docs entry —
//! see `add_lua_fn_raw`).

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

use crate::{add_lua_fn, add_lua_fn_raw};
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    // ------------------------------------------------------------------
    // Used internally to track values in the console (C++ HGScripting.cpp:49
    // — plain `lua.writeVariable`, NOT `addLuaFn`).
    // ------------------------------------------------------------------
    let st = state.clone();
    add_lua_fn_raw!(ctx, "u_impl_addTrackedResult", move |result: String| {
        lock(&st).ilc_lua_tracked_results.push(result);
    },)?;

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_setFlashEffect",
        FnMeta {
            ret: "void",
            arg_types: &["float"],
        },
        move |value: f32| lock(&st).status.flash_effect = value,
    )?
    .arg("value")
    .doc("Flash the screen with `$0` intensity (from 0 to 255).");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_setFlashColor",
        FnMeta {
            ret: "void",
            arg_types: &["int", "int", "int"],
        },
        move |r: i32, g: i32, b: i32| {
            let s = lock(&st);
            s.backend.init_flash_effect(r, g, b);
        },
    )?
    .arg("r")
    .arg("g")
    .arg("b")
    .doc("Set the color of the flash effect to `{$0, $1, $2}`.");

    let st = state.clone();
    let headless = lock(state).headless;
    add_lua_fn!(
        ctx,
        "u_log",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |message: String| {
            // C++: `if (window == nullptr) return;` — headless no-op.
            if headless {
                return;
            }
            let mut s = lock(&st);
            s.backend.log("lua", &message);
            s.ilc_cmd_log.push(format!("[lua]: {message}\n"));
        },
    )?
    .arg("message")
    .doc("Print out `$0` to the console.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "u_isKeyPressed",
        FnMeta {
            ret: "bool",
            arg_types: &["int"],
        },
        move |key_code: i32| {
            let mut s = lock(&st);
            s.raise_warning(
            "u_isKeyPressed",
            "This function will be removed in a future version of Open Hexagon. Please replace uses of this function with the `onInput` callback.");
            s.backend.is_key_pressed(key_code)
        },
    )?
    .arg("keyCode")
    .doc("Return `true` if the keyboard key with code `$0` is being pressed, `false` otherwise. The key code must match the definition of the SFML `sf::Keyboard::Key` enumeration. **This function is deprecated and will be removed in a future version.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_haltTime",
        FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |duration: f64| {
            // C++: `status.pauseTime(ssvu::getFTToSeconds(duration))` —
            // `duration` is FRAMETIME (1.0 == 1/60 s), not seconds
            // (mapping §8.5).
            let mut s = lock(&st);
            s.status
                .pause_time(crate::api::state::ft_to_seconds(duration));
        },
    )?
    .arg("duration")
    .doc("Pause the game timer for `$0` seconds.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_clearWalls",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || lock(&st).walls.clear(),
    )?
    .doc("Remove all existing walls.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_getPlayerAngle",
        FnMeta {
            ret: "float",
            arg_types: &[],
        },
        move || lock(&st).backend.get_player_angle(),
    )?
    .doc("Return the current angle of the player, in radians.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_setPlayerAngle",
        FnMeta {
            ret: "void",
            arg_types: &["float"],
        },
        move |angle: f32| {
            let s = lock(&st);
            s.backend.set_player_angle(angle);
        },
    )?
    .arg("angle")
    .doc("Set the current angle of the player to `$0`, in radians.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "u_isMouseButtonPressed",
        FnMeta {
            ret: "bool",
            arg_types: &["int"],
        },
        move |button_code: i32| {
            let mut s = lock(&st);
            s.raise_warning(
            "u_isMouseButtonPressed",
            "This function will be removed in a future version of Open Hexagon. Please replace uses of this function with the `onInput` callback.");
            s.backend.is_mouse_button_pressed(button_code)
        },
    )?
    .arg("buttonCode")
    .doc("Return `true` if the mouse button with code `$0` is being pressed, `false` otherwise. The button code must match the definition of the SFML `sf::Mouse::Button` enumeration. **This function is deprecated and will be removed in a future version.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_isFastSpinning",
        FnMeta {
            ret: "bool",
            arg_types: &[],
        },
        move || lock(&st).status.fast_spin > 0.0,
    )?
    .doc("Return `true` if the camera is currently \"fast spinning\", `false` otherwise.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "u_forceIncrement",
        FnMeta {
            ret: "void",
            arg_types: &[],
        },
        move || {
            let s = lock(&st);
            s.backend.increment_difficulty();
        },
    )?
    .doc("Immediately force a difficulty increment, regardless of the chosen automatic increment parameters.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_getDifficultyMult",
        FnMeta {
            ret: "float",
            arg_types: &[],
        },
        move || lock(&st).difficulty_mult,
    )?
    .doc("Return the current difficulty multiplier.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_getSpeedMultDM",
        FnMeta {
            ret: "float",
            arg_types: &[],
        },
        move || lock(&st).get_speed_mult_dm(),
    )?
    .doc("Return the current speed multiplier, adjusted for the chosen difficulty multiplier.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_getDelayMultDM",
        FnMeta {
            ret: "float",
            arg_types: &[],
        },
        move || lock(&st).get_delay_mult_dm(),
    )?
    .doc("Return the current delay multiplier, adjusted for the chosen difficulty multiplier.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "u_swapPlayer",
        FnMeta {
            ret: "void",
            arg_types: &["bool"],
        },
        move |play_sound: bool| {
            let s = lock(&st);
            s.backend.perform_player_swap(play_sound);
        },
    )?
    .arg("playSound")
    .doc("Force-swaps (180 degrees) the player when invoked. If `$0` is `true`, the swap sound will be played.");

    Ok(())
}
