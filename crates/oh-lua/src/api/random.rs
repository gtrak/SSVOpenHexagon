//! C++ `hg::LuaScripting::initRandom` (LuaScripting.cpp:81-140): the five
//! RNG functions. They draw from the SAME `random_number_generator` stream
//! the C++ engine advances per frame (mapping §4/§8.2 — draw order is
//! replay-critical), so all five lock the shared state.

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

use crate::add_lua_fn;
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_rndReal",
        FnMeta {
            ret: "float",
            arg_types: &[],
        },
        move || lock(&st).rng.get_real::<f32>(0.0, 1.0),
    )?
    .doc("Return a random real number in the [0; 1] range.");

    let st = state.clone();
    // C++ declares `-> float`; the value is always integer-valued (mapping
    // §8.1) — old pack code does `math.random(5) % 3`.
    add_lua_fn!(
        ctx,
        "u_rndIntUpper",
        FnMeta {
            ret: "float",
            arg_types: &["int"],
        },
        move |upper: i32| lock(&st).rng.get_int::<i32>(1, upper) as f32,
    )?
    .arg("upper")
    .doc("Return a random integer number in the [1; `$0`] range.");

    let st = state.clone();
    add_lua_fn!(
        ctx,
        "u_rndInt",
        FnMeta {
            ret: "float",
            arg_types: &["int", "int"],
        },
        move |lower: i32, upper: i32| { lock(&st).rng.get_int::<i32>(lower, upper) as f32 },
    )?
    .arg("lower")
    .arg("upper")
    .doc("Return a random integer number in the [`$0`; `$1`] range.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "u_rndSwitch",
        FnMeta {
            ret: "float",
            arg_types: &["int", "int", "int"],
        },
        move |mode: i32, lower: i32, upper: i32| {
            let mut s = lock(&st);
            match mode {
                0 => s.rng.get_real::<f32>(0.0, 1.0),
                1 => s.rng.get_int::<i32>(1, upper) as f32,
                2 => s.rng.get_int::<i32>(lower, upper) as f32,
                // C++: `SSVOH_ASSERT(false); return 0;` — consumes no RNG.
                _ => 0.0,
            }
        },
    )?
    .arg("mode")
    .arg("lower")
    .arg("upper")
    .doc("Internal replacement for `math.random`. Calls `u_rndReal()` with `$0 == 0`, `u_rndIntUpper($2)` with `$0 == 1`, and `u_rndInt($1, $2)` with `$0 == 2`.");

    let st = state.clone();
    // 64-bit seed exposed as a 64-bit Lua integer (mapping §8.1).
    add_lua_fn!(ctx,
        "u_getAttemptRandomSeed",
        FnMeta {
            ret: "unsigned long long",
            arg_types: &[],
        },
        move || lock(&st).rng.seed(),
    )?
    .doc("Obtain the current random seed, automatically generated at the beginning of the level. `math.randomseed` is automatically initialized with the result of this function at the beginning of a level.");

    Ok(())
}
