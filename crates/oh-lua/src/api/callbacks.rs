//! Engine-side dispatch of the 12 script callbacks (mapping/lua-api.md §5).
//!
//! Every call goes through the C++ `Utils::runLuaFunctionIfExists<T>`
//! behavior: `doesVariableExist(name)` first — a missing callback is
//! skipped *silently* (no error, no return); a defined callback is called
//! with the exact C++ argument types, and a `nil` return is *coerced* to
//! the C++ type's zero (`false` / `0` / `0.0`).
//!
//! `onInput`'s `true` return is a VETO: it suppresses the default player
//! input for that frame (`HGUpdate.cpp:244`). `onRenderStage`'s `int`
//! return is discarded by the engine; the hook exists for side effects.
//!
//! Error handling: C++ `luaExceptionLippincottHandler` prints
//! `[runLuaFunctionIfExists] Runtime error on "<name>" ...` and (non-debug)
//! `goToMenu(false, true)`. In the port the caller (issue 009 game loop)
//! receives the `Err` and performs that behavior — these helpers report the
//! callback name in the error message.

use mlua::{MultiValue, Value};

use crate::context::LuaContext;
use crate::error::LuaError;

/// `mlua::Integer` is `i32` on the shipping Luau build (D-037: the port
/// has one VM — Luau, whose `lua_Integer` is 32-bit), so an `i32`
/// callback argument is already exactly the FFI integer width: no
/// conversion.
#[inline]
fn callback_int(v: i32) -> mlua::Integer {
    v
}

/// C++ `runLuaFunctionIfExists<T>`: skip (→ `None`) when the global does not
/// exist or is `nil`; otherwise call it with `args` and read one return
/// value (a `nil` return is `None`, matching the C++ nil coercion to the
/// type's zero value).
fn call_lua_function<T: mlua::FromLua>(
    ctx: &LuaContext,
    name: &str,
    args: MultiValue,
) -> Result<Option<T>, LuaError> {
    // C++ `doesVariableExist`: plain global, must be non-nil.
    if !ctx.does_variable_exist(name)? {
        return Ok(None);
    }
    let globals = ctx.lua().globals();
    let func: mlua::Function = globals.get(name).map_err(|e| LuaError::ExecutionError {
        message: format!("[runLuaFunctionIfExists] Runtime error on \"{name}\": {e}"),
    })?;
    let result: Option<T> = func
        .call::<Option<T>>(args)
        .map_err(|e| LuaError::ExecutionError {
            message: format!("[runLuaFunctionIfExists] Runtime error on \"{name}\": {e}"),
        })?;
    Ok(result)
}

/// `onInit()` — level/menu setup; runs once per load (also on restart).
pub fn call_on_init(ctx: &LuaContext) -> Result<(), LuaError> {
    call_lua_function::<mlua::Value>(ctx, "onInit", MultiValue::new()).map(|_| ())
}

/// `onLoad()` — "level started playing" hook.
pub fn call_on_load(ctx: &LuaContext) -> Result<(), LuaError> {
    call_lua_function::<mlua::Value>(ctx, "onLoad", MultiValue::new()).map(|_| ())
}

/// `onUpdate(mFT: float)` — per-frame update; the float return (if any) is
/// discarded by the engine.
pub fn call_on_update(ctx: &LuaContext, ft: f32) -> Result<(), LuaError> {
    let args = MultiValue::from_vec(vec![Value::Number(ft.into())]);
    call_lua_function::<mlua::Value>(ctx, "onUpdate", args).map(|_| ())
}

/// `onStep()` — "pattern step finished" (main timeline drained).
pub fn call_on_step(ctx: &LuaContext) -> Result<(), LuaError> {
    call_lua_function::<mlua::Value>(ctx, "onStep", MultiValue::new()).map(|_| ())
}

/// `onInput(mFT: float, mMovement: int, mFocus: bool, mSwap: bool) -> bool`
/// — returning `true` **vets** (prevents) the default player input for this
/// frame. `movement` is −1 / 0 / 1. A missing or `nil`-returning callback
/// yields `false` (no veto).
pub fn call_on_input(
    ctx: &LuaContext,
    ft: f32,
    movement: i32,
    focused: bool,
    swap: bool,
) -> Result<bool, LuaError> {
    let args = MultiValue::from_vec(vec![
        Value::Number(ft.into()),
        Value::Integer(callback_int(movement)),
        Value::Boolean(focused),
        Value::Boolean(swap),
    ]);
    Ok(call_lua_function::<bool>(ctx, "onInput", args)?.unwrap_or(false))
}

/// `onPreDeath()` — fired even when the death is ignored by
/// invincibility/tutorial mode.
pub fn call_on_pre_death(ctx: &LuaContext) -> Result<(), LuaError> {
    call_lua_function::<mlua::Value>(ctx, "onPreDeath", MultiValue::new()).map(|_| ())
}

/// `onDeath()` — only when the death actually proceeds.
pub fn call_on_death(ctx: &LuaContext) -> Result<(), LuaError> {
    call_lua_function::<mlua::Value>(ctx, "onDeath", MultiValue::new()).map(|_| ())
}

/// `onIncrement()` — fired on every level increment (incl.
/// `u_forceIncrement`).
pub fn call_on_increment(ctx: &LuaContext) -> Result<(), LuaError> {
    call_lua_function::<mlua::Value>(ctx, "onIncrement", MultiValue::new()).map(|_| ())
}

/// `onPreUnload()` — last chance with the old context on restart
/// (`!firstPlay` only).
pub fn call_on_pre_unload(ctx: &LuaContext) -> Result<(), LuaError> {
    call_lua_function::<mlua::Value>(ctx, "onPreUnload", MultiValue::new()).map(|_| ())
}

/// `onUnload()` — restart path and exit-to-menu (the latter only on
/// non-error exits).
pub fn call_on_unload(ctx: &LuaContext) -> Result<(), LuaError> {
    call_lua_function::<mlua::Value>(ctx, "onUnload", MultiValue::new()).map(|_| ())
}

/// `onCursorSwap()` — fired on every 180° swap (incl. `u_swapPlayer`).
pub fn call_on_cursor_swap(ctx: &LuaContext) -> Result<(), LuaError> {
    call_lua_function::<mlua::Value>(ctx, "onCursorSwap", MultiValue::new()).map(|_| ())
}

/// `onRenderStage(rs: int, ft: float) -> int` — the return value is
/// discarded by the engine (side-effect hook); `render_stage` is 0–8
/// (`RenderStage`), `ft` = `60 / FPS`.
pub fn call_on_render_stage(ctx: &LuaContext, render_stage: i32, ft: f32) -> Result<i32, LuaError> {
    let args = MultiValue::from_vec(vec![
        Value::Integer(callback_int(render_stage)),
        Value::Number(ft.into()),
    ]);
    Ok(call_lua_function::<i32>(ctx, "onRenderStage", args)?.unwrap_or(0))
}
