//! `Lua::LuaContext` analogue over `mlua` (issue 006).
//!
//! Ports `include/SSVOpenHexagon/Utils/LuaWrapper.hpp` +
//! `src/SSVOpenHexagon/Utils/LuaWrapper.cpp`: state creation, `executeCode`
//! (`_load`/`_call`), the dotted-name `_getGlobal`/`_setGlobal` path walk,
//! `readVariable`/`writeVariable`/`clearVariable`/`doesVariableExist` and the
//! `addLuaFn` registration mechanism (`FunctionToPush`), whose exact
//! argument-count/argument-window behavior is preserved (see
//! `LuaContext::add_lua_fn`).
//!
//! `mlua::Lua` is not `Sync`; like the C++ context (single stack, no mutex)
//! this context must be used from one thread at a time.

use crate::convert::{FnReturn, FromLuaValue, IntoLuaValue};
use crate::error::LuaError;
use crate::metadata::{FnMeta, LuaMetadataProxy};
use mlua::Value;

/// The Lua context: a LuaJIT state with the standard libraries open
/// (C++: `lua_newstate` + `luaL_openlibs`), not yet sandboxed.
///
/// # Sandboxing
///
/// The C++ game/menu contexts are created with the default libs and then
/// sandboxed by `hg::LuaScripting::init` (mapping/lua-api.md §3.3):
/// register the RNG shims (`u_rnd*`), then call
/// [`sandbox::redefine_io_open`](crate::redefine_io_open),
/// [`sandbox::redefine_random`](crate::redefine_random) and
/// [`sandbox::destroy_malicious_functions`](crate::destroy_malicious_functions)
/// **in that order** (`u_rndSwitch` must exist before `math.random` is
/// redefined).
pub struct LuaContext {
    lua: mlua::Lua,
}

impl LuaContext {
    /// C++ `LuaContext(bool openDefaultLibs = true)` — the game and menu both
    /// use `true` (`luaL_openlibs`).
    ///
    /// `mlua::Lua::new()` loads the safe stdlib set: on the `luajit` feature
    /// that is every C++ `luaL_openlibs` library **except** `debug` and `ffi`.
    /// The C++ build compiles LuaJIT with `LUAJIT_DISABLE_FFI` (CMakeLists.txt
    /// CPM pin) so `ffi` is absent there too, and `destroy_malicious_functions`
    /// clears `debug` — the post-sandbox stdlib surface is identical.
    /// (Creation failure is fatal: `mlua::Lua::new` panics, like a C++
    /// `bad_alloc` from `lua_newstate`.)
    #[must_use]
    pub fn new() -> Self {
        Self {
            lua: mlua::Lua::new(),
        }
    }

    /// C++ `executeCode(std::string_view)` (void): load with chunk name
    /// `"chunk"` (the literal C++ chunk name) and `pcall` with zero results
    /// (return values are discarded, exactly like C++ `nresults = 0`).
    pub fn execute_code(&self, code: &str) -> Result<(), LuaError> {
        // `Chunk::eval` loads with a protected `pcall`; `()` consumes no
        // return values, matching C++ `lua_pcall(state, 0, 0, 0)`.
        self.lua
            .load(code)
            .set_name("chunk")
            .eval::<()>()
            .map_err(LuaError::from_mlua)
    }

    /// C++ templated `executeCode<T>`: load + `pcall`, converting the return
    /// values (`T` may be a scalar or a tuple; `mlua::FromLuaMulti`).
    pub fn execute_code_typed<T: mlua::FromLuaMulti>(&self, code: &str) -> Result<T, LuaError> {
        self.lua
            .load(code)
            .set_name("chunk")
            .eval::<T>()
            .map_err(LuaError::from_mlua)
    }

    /// C++ `readVariable<T>`: resolve the (possibly dotted) name and convert
    /// with the exact `_read` coercion rules (`FromLuaValue`).
    ///
    /// A nil/missing variable raises `VariableDoesntExist`. Note: the C++
    /// `readVariable` on a missing *plain* global silently coerced `nil`
    /// (int 0 / float 0.0 / bool false / string UB); raising
    /// `VariableDoesntExist` for any missing global is a deliberate port
    /// decision (mapping/lua-api.md §8.4 leaves this to the port). The
    /// nil-coercion table still applies to *function arguments* exactly.
    pub fn read_variable<T: FromLuaValue>(&self, name: &str) -> Result<T, LuaError> {
        let value = self
            .get_path_value(name)?
            .ok_or_else(|| LuaError::VariableDoesntExist {
                variable: name.to_string(),
            })?;
        T::from_lua_value(&value)
    }

    /// C++ `readVariableIfExists`: `None` if the variable does not exist,
    /// otherwise `read_variable`.
    pub fn read_variable_if_exists<T: FromLuaValue>(
        &self,
        name: &str,
    ) -> Result<Option<T>, LuaError> {
        if !self.does_variable_exist(name)? {
            return Ok(None);
        }
        Ok(Some(self.read_variable(name)?))
    }

    /// C++ `writeVariable(name, value)`: sets the (possibly dotted) global.
    ///
    /// Dotted names require the intermediate tables to exist (C++ `_setGlobal`
    /// walks to the last dot and throws `VariableDoesntException` when the
    /// table part is missing or not a table) — intermediate tables are NOT
    /// created.
    pub fn write_variable<V: IntoLuaValue>(&self, name: &str, value: &V) -> Result<(), LuaError> {
        let value = value
            .into_lua_value(&self.lua)
            .map_err(LuaError::from_mlua)?;
        self.set_path(name, &value)
    }

    /// C++ `clearVariable`: puts `nil` into the variable (same path rules as
    /// `write_variable`). Used by the sandbox.
    pub fn clear_variable(&self, name: &str) -> Result<(), LuaError> {
        self.set_path(name, &Value::Nil)
    }

    /// C++ `doesVariableExist` (not nil). Plain names are a direct global
    /// lookup (never fails); dotted names walk the path and can raise
    /// `VariableDoesntExist` for missing/non-table intermediates (C++
    /// `_getGlobal` semantics).
    pub fn does_variable_exist(&self, name: &str) -> Result<bool, LuaError> {
        if !name.contains('.') {
            let globals = self.lua.globals();
            return Ok(globals
                .get::<Option<Value>>(name)
                .map_err(LuaError::from_mlua)?
                .is_some());
        }
        Ok(self.get_path_value(name)?.is_some())
    }

    /// C++ `isVariableArray` (`lua_istable`; dotted names use the throwing
    /// path walk).
    pub fn is_variable_array(&self, name: &str) -> Result<bool, LuaError> {
        Ok(matches!(self.get_path_value(name)?, Some(Value::Table(_))))
    }

    /// C++ `writeArrayIntoVariable`: writes an empty table into the variable
    /// (same path rules as `write_variable`).
    pub fn write_array_into_variable(&self, name: &str) -> Result<(), LuaError> {
        let table = self.lua.create_table().map_err(LuaError::from_mlua)?;
        self.set_path(name, &Value::Table(table))
    }

    /// The underlying `mlua::Lua` state (escape hatch for raw Lua API
    /// operations; the single-thread, single-stack usage contract stays the
    /// caller's, exactly like the C++ context's raw `lua_State*`).
    #[must_use]
    pub fn lua(&self) -> &mlua::Lua {
        &self.lua
    }

    // ------------------------------------------------------------------
    // C++ `_getGlobal` / `_setGlobal` (dotted-name path walk)
    // ------------------------------------------------------------------

    /// Tolerant existence check for the sandbox (lane 030a): like
    /// `does_variable_exist` but a missing/non-table intermediate (or a
    /// nil key at any step) reports `false` instead of raising — the C++
    /// teardown only ever clears variables that exist on its VM (LuaJIT),
    /// while the Luau backend simply has no `io`/`ffi`/`jit` at all, so
    /// the ported teardown skips absent targets instead of failing.
    pub fn variable_exists(&self, name: &str) -> bool {
        if !name.contains('.') {
            return self
                .lua
                .globals()
                .get::<Option<Value>>(name)
                .map(|v| v.is_some())
                .unwrap_or(false);
        }
        let segments: Vec<&str> = name.split('.').collect();
        let mut current: Option<Value> = self
            .lua
            .globals()
            .get::<Option<Value>>(segments[0])
            .ok()
            .flatten();
        for part in &segments[1..] {
            let table = match current {
                Some(Value::Table(table)) => table,
                _ => return false,
            };
            current = table.get::<Option<Value>>(*part).ok().flatten();
        }
        current.is_some()
    }

    /// C++ `_getGlobal`: plain names resolve directly (nil is returned as
    /// `None` — never an error); dotted names walk the path and raise
    /// `VariableDoesntExist` (carrying the *resolved* name) when an
    /// intermediate is missing/non-table **or when the final key is nil** —
    /// the C++ nil check runs at every path step, last one included, which
    /// is also why C++ `doesVariableExist("a.b")` raises for a missing `b`.
    fn get_path_value(&self, name: &str) -> Result<Option<Value>, LuaError> {
        if !name.contains('.') {
            let globals = self.lua.globals();
            return globals
                .get::<Option<Value>>(name)
                .map_err(LuaError::from_mlua);
        }

        let segments: Vec<&str> = name.split('.').collect();
        let globals = self.lua.globals();
        let mut current: Option<Value> = globals
            .get::<Option<Value>>(segments[0])
            .map_err(LuaError::from_mlua)?;
        for part in &segments[1..] {
            let table = match current {
                // C++: `if (!lua_istable(_state, -1)) throw
                // VariableDoesntExistException(std::string{mVarName})`
                Some(Value::Table(table)) => table,
                _ => {
                    return Err(LuaError::VariableDoesntExist {
                        variable: name.to_string(),
                    })
                }
            };
            current = table
                .get::<Option<Value>>(*part)
                .map_err(LuaError::from_mlua)?;
            // C++: `if (lua_isnil(_state, -1)) throw
            // VariableDoesntExistException(std::string{mVarName})` — at
            // *every* step, including the last key.
            if current.is_none() {
                return Err(LuaError::VariableDoesntExist {
                    variable: name.to_string(),
                });
            }
        }
        Ok(current)
    }

    /// C++ `_setGlobal`: plain names go through `lua_setglobal`; dotted names
    /// resolve the table part (throwing its error) and then require it to be
    /// a table (else `VariableDoesntExist` with the *full* name).
    fn set_path(&self, name: &str, value: &Value) -> Result<(), LuaError> {
        let Some(last_dot) = name.rfind('.') else {
            self.lua
                .globals()
                .set(name, value.clone())
                .map_err(LuaError::from_mlua)?;
            return Ok(());
        };
        let table_name = &name[..last_dot];
        let key = &name[last_dot + 1..];

        let table = match self.get_path_value(table_name)? {
            Some(Value::Table(table)) => table,
            // C++: `if (!lua_istable(_state, -1))
            // throw VariableDoesntExistException(varNameAsStr)` — full name.
            _ => {
                return Err(LuaError::VariableDoesntExist {
                    variable: name.to_string(),
                })
            }
        };
        table.set(key, value.clone()).map_err(LuaError::from_mlua)?;
        Ok(())
    }
}

impl Default for LuaContext {
    fn default() -> Self {
        Self::new()
    }
}

/// The exact C++ `FunctionToPush::operator()` call-frame rules (see
/// `add_lua_fn`'s docs): the "at least N" count check (the callee occupies
/// one stack slot), the `N - 1`-argument callee-hit `WrongType` error, and
/// the last-N argument window for extra arguments. Returns the window the
/// argument reads operate on (exactly `count` values, or zero for
/// zero-argument closures).
fn read_call_window(count: usize, values: &[Value]) -> Result<&[Value], mlua::Error> {
    let nargs = values.len();

    // C++: `if (lua_gettop(state) < TupledFunction::count)` — the callee
    // itself occupies one stack slot. (`count == 0` means zero-argument
    // functions: both checks are vacuous.)
    if count > 0 && nargs + 1 < count {
        return Err(mlua::Error::RuntimeError(format!(
            "this function requires at least {count} parameter(s)"
        )));
    }
    // Exactly `count - 1` arguments: the C++ read of slot `-count` is the
    // callee itself (userdata) -> WrongTypeException.
    if count > 0 && nargs == count - 1 {
        return Err(mlua::Error::RuntimeError(
            "Trying to cast a lua variable to an invalid type".to_string(),
        ));
    }
    // Extra arguments: C++ reads the last `count` slots (`count == 0` reads
    // nothing at all).
    Ok(if count == 0 {
        &values[..0]
    } else {
        &values[nargs - count..]
    })
}

/// Take the next value off the front of a call window (the window always
/// holds exactly the remaining argument count, so this only fails if the
/// call-frame bookkeeping above is wrong).
fn take_next(values: &[Value]) -> Result<(&Value, &[Value]), mlua::Error> {
    values.split_first().ok_or_else(|| {
        mlua::Error::RuntimeError("this function requires at least 1 parameter(s)".to_string())
    })
}

/// Read the next argument of type `A` off the call window, converting it
/// with the C++ `_read` coercion rules (`FromLuaValue`).
fn read_arg<A: FromLuaValue>(values: &[Value]) -> Result<(A, &[Value]), mlua::Error> {
    let (value, rest) = take_next(values)?;
    Ok((A::from_lua_value(value).map_err(to_lua_error)?, rest))
}

/// Convert a `LuaError` (argument coercion / fallible-closure error) to the
/// Lua runtime error the C++ exception propagation produces.
fn to_lua_error(err: LuaError) -> mlua::Error {
    mlua::Error::RuntimeError(err.to_string())
}

// ----------------------------------------------------------------------------
// Registered-function methods, one per arity (C++ `FunctionToPush` spread
// semantics: the closure is invoked with its N parameters as N separate
// values, never as one tuple — stable Rust cannot name a generic
// `Fn(A, B, ...)` for variable N in a trait bound, so the arity is a method
// parameter instead; the `add_lua_fn!` / `add_lua_fn_raw!` macros dispatch
// to the right method). The game's largest signature is
// `cw_setVertexColor4` (17 parameters); arities 0..=17 are provided.
// ----------------------------------------------------------------------------

impl LuaContext {
    /// Create a Rust function in the state (shared by every `add_lua_fn_*`
    /// / `add_lua_fn_raw_*` method).
    fn register_fn<F>(&self, f: F) -> Result<mlua::Function, LuaError>
    where
        F: Fn(&mlua::Lua, mlua::MultiValue) -> mlua::Result<mlua::MultiValue> + 'static,
    {
        self.lua.create_function(f).map_err(LuaError::from_mlua)
    }
}

/// Zero-argument closures (C++ `TupledFunction` with `count == 0`: every
/// call arity is accepted, nothing is read).
impl LuaContext {
    /// `add_lua_fn` for a zero-argument closure (see the `add_lua_fn!`
    /// macro).
    pub fn add_lua_fn_0<Ret, F>(
        &self,
        name: &str,
        meta: FnMeta,
        f: F,
    ) -> Result<LuaMetadataProxy, LuaError>
    where
        F: Fn() -> Ret + 'static,
        Ret: FnReturn,
    {
        debug_assert_eq!(
            meta.arg_types.len(),
            0,
            "FnMeta::arg_types must list exactly one entry per closure argument"
        );
        let func = self
            .register_fn(move |_lua: &mlua::Lua, _values: mlua::MultiValue| f().to_multi(_lua))?;
        self.write_variable(name, &func)?;
        Ok(LuaMetadataProxy::new(name, meta.ret, meta.arg_types))
    }

    /// `add_lua_fn_raw` for a zero-argument closure (see the
    /// `add_lua_fn_raw!` macro).
    pub fn add_lua_fn_raw_0<Ret, F>(&self, name: &str, f: F) -> Result<(), LuaError>
    where
        F: Fn() -> Ret + 'static,
        Ret: FnReturn,
    {
        let func = self
            .register_fn(move |_lua: &mlua::Lua, _values: mlua::MultiValue| f().to_multi(_lua))?;
        self.write_variable(name, &func)?;
        Ok(())
    }
}

// One method pair per arity (1..=17); `$t` are the C++ argument types,
// `$v` the local argument bindings.
macro_rules! impl_lua_fn_arity {
    ($fn_name:ident, $fn_name_raw:ident, $n:literal; $($t:ident => $v:ident),*) => {
        /// `add_lua_fn` for a `$n`-argument spread closure (see the
        /// `add_lua_fn!` macro).
        pub fn $fn_name<$($t),*, Ret, F>(
            &self,
            name: &str,
            meta: FnMeta,
            f: F,
        ) -> Result<LuaMetadataProxy, LuaError>
        where
            $($t: FromLuaValue,)*
            F: Fn($($t),*) -> Ret + 'static,
            Ret: FnReturn,
        {
            debug_assert_eq!(
                meta.arg_types.len(),
                $n,
                "FnMeta::arg_types must list exactly one entry per closure argument"
            );
            let func = self.register_fn(
                move |_lua: &mlua::Lua,
                      values: mlua::MultiValue| {
                    let values: Vec<Value> = values.into();
                    let window = read_call_window($n, &values)?;
                    $(
                        let ($v, window) = read_arg::<$t>(window)?;
                    )*
                    _ = window;
                    f($($v),*).to_multi(_lua)
                },
            )?;
            self.write_variable(name, &func)?;
            Ok(LuaMetadataProxy::new(name, meta.ret, meta.arg_types))
        }

        /// `add_lua_fn_raw` for a `$n`-argument spread closure (see the
        /// `add_lua_fn_raw!` macro).
        pub fn $fn_name_raw<$($t),*, Ret, F>(&self, name: &str, f: F) -> Result<(), LuaError>
        where
            $($t: FromLuaValue,)*
            F: Fn($($t),*) -> Ret + 'static,
            Ret: FnReturn,
        {
            let func = self.register_fn(
                move |_lua: &mlua::Lua,
                      values: mlua::MultiValue| {
                    let values: Vec<Value> = values.into();
                    let window = read_call_window($n, &values)?;
                    $(
                        let ($v, window) = read_arg::<$t>(window)?;
                    )*
                    _ = window;
                    f($($v),*).to_multi(_lua)
                },
            )?;
            self.write_variable(name, &func)?;
            Ok(())
        }
    };
}

impl LuaContext {
    impl_lua_fn_arity!(add_lua_fn_1, add_lua_fn_raw_1, 1; A0 => a0);
    impl_lua_fn_arity!(add_lua_fn_2, add_lua_fn_raw_2, 2; A0 => a0, A1 => a1);
    impl_lua_fn_arity!(add_lua_fn_3, add_lua_fn_raw_3, 3; A0 => a0, A1 => a1, A2 => a2);
    impl_lua_fn_arity!(add_lua_fn_4, add_lua_fn_raw_4, 4; A0 => a0, A1 => a1, A2 => a2, A3 => a3);
    impl_lua_fn_arity!(add_lua_fn_5, add_lua_fn_raw_5, 5; A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4);
    impl_lua_fn_arity!(add_lua_fn_6, add_lua_fn_raw_6, 6; A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5);
    impl_lua_fn_arity!(add_lua_fn_7, add_lua_fn_raw_7, 7; A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6);
    impl_lua_fn_arity!(
        add_lua_fn_8, add_lua_fn_raw_8, 8;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7
    );
    impl_lua_fn_arity!(
        add_lua_fn_9, add_lua_fn_raw_9, 9;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7, A8 => a8
    );
    impl_lua_fn_arity!(
        add_lua_fn_10, add_lua_fn_raw_10, 10;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7, A8 => a8, A9 => a9
    );
    impl_lua_fn_arity!(
        add_lua_fn_11, add_lua_fn_raw_11, 11;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7, A8 => a8, A9 => a9, A10 => a10
    );
    impl_lua_fn_arity!(
        add_lua_fn_12, add_lua_fn_raw_12, 12;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7, A8 => a8, A9 => a9, A10 => a10, A11 => a11
    );
    impl_lua_fn_arity!(
        add_lua_fn_13, add_lua_fn_raw_13, 13;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7, A8 => a8, A9 => a9, A10 => a10, A11 => a11, A12 => a12
    );
    impl_lua_fn_arity!(
        add_lua_fn_14, add_lua_fn_raw_14, 14;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7, A8 => a8, A9 => a9, A10 => a10, A11 => a11, A12 => a12, A13 => a13
    );
    impl_lua_fn_arity!(
        add_lua_fn_15, add_lua_fn_raw_15, 15;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7, A8 => a8, A9 => a9, A10 => a10, A11 => a11, A12 => a12, A13 => a13, A14 => a14
    );
    impl_lua_fn_arity!(
        add_lua_fn_16, add_lua_fn_raw_16, 16;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7, A8 => a8, A9 => a9, A10 => a10, A11 => a11, A12 => a12, A13 => a13, A14 => a14, A15 => a15
    );
    impl_lua_fn_arity!(
        add_lua_fn_17, add_lua_fn_raw_17, 17;
        A0 => a0, A1 => a1, A2 => a2, A3 => a3, A4 => a4, A5 => a5, A6 => a6, A7 => a7, A8 => a8, A9 => a9, A10 => a10, A11 => a11, A12 => a12, A13 => a13, A14 => a14, A15 => a15, A16 => a16
    );
}

// ----------------------------------------------------------------------------
// `add_lua_fn!` / `add_lua_fn_try!` / `add_lua_fn_raw!` — the C++ `addLuaFn`
// registration (LuaScripting.cpp:39-53), dispatching the spread-argument
// closure to the matching arity method. A registration:
// (1) creates the Rust function in the state, (2) stores it under the global
// `name` (plain `writeVariable`), and (3) for `add_lua_fn!` /
// `add_lua_fn_try!`, returns the metadata proxy (`.arg` / `.doc`), which
// records the docs entry in the global `LuaMetadata` on drop — exactly what
// the C++ `LuaMetadataProxy` destructor does.
//
// # Spread arguments (C++ `FunctionToPush` semantics)
//
// The closure takes its parameters as **separate** closure arguments,
// exactly like the C++ `TupledFunction` callable:
//
//     add_lua_fn!(ctx, "f", FnMeta { ret: "void", arg_types: &[] },
//         move || { /* zero arguments */ });
//     add_lua_fn!(ctx, "g", FnMeta { ret: "void", arg_types: &["std::string"] },
//         move |x: String| { /* one argument */ });
//     add_lua_fn!(ctx, "h", FnMeta { ret: "void", arg_types: &["int", "float"] },
//         move |a: i32, b: f32| { /* two arguments */ });
//
// A closure returning `Result<T, LuaError>` is accepted too (use
// `add_lua_fn_try!` for those; it documents intent — the call semantics are
// identical); an `Err` aborts the enclosing protected call as a Lua runtime
// error.
//
// # Exact C++ call semantics (`FunctionToPush::operator()`)
//
// The C++ wrapper checks `lua_gettop(state) < N` where the stack holds the
// callee itself plus its arguments, then reads the slots `-N .. -1`.
// Consequences, all preserved (see `read_call_window`):
//
// * a call with `N - 2` or fewer arguments raises the Lua error
//   `this function requires at least N parameter(s)` (C++ builds it with
//   `luaL_where` + `lua_concat`; the port omits the location prefix);
// * a call with exactly `N - 1` arguments makes the last read hit the callee
//   itself, which every C++ `_read` rejects: the Lua error is `Trying to
//   cast a lua variable to an invalid type`;
// * a call with *more* than `N` arguments reads the **last** `N` arguments
//   (stack positions `-N .. -1`), not the first `N`;
// * each argument is converted with the `_read` coercion rules
//   (`FromLuaValue`), so explicit `nil` args read as 0 / 0.0 / false
//   (string: `WrongType`, the deliberate replacement of the C++ UB).
//
// The closure must be `'static` (mlua stores Rust functions in the state).
// Captured state therefore needs `Arc`s or the engine's own
// lifetime-extension pattern; `meta.arg_types` must list exactly one C++
// `typeToStr` spelling per closure argument (see `FnMeta`).
// ----------------------------------------------------------------------------

#[macro_export]
macro_rules! add_lua_fn {
    // strip `move` and re-dispatch
    ($ctx:expr, $name:expr, $meta:expr, move $($tail:tt)*) => {
        $crate::add_lua_fn!($ctx, $name, $meta, $($tail)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, || $($body:tt)*) => {
        $ctx.add_lua_fn_0($name, $meta, move || $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_1($name, $meta, move |$a0: $t0| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_2($name, $meta, move |$a0: $t0, $a1: $t1| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_3($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_4($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_5($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_6($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_7($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_8($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_9($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_10($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_11($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_12($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_13($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty, $a13:ident: $t13:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_14($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12, $a13: $t13| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty, $a13:ident: $t13:ty, $a14:ident: $t14:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_15($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12, $a13: $t13, $a14: $t14| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty, $a13:ident: $t13:ty, $a14:ident: $t14:ty, $a15:ident: $t15:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_16($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12, $a13: $t13, $a14: $t14, $a15: $t15| $($body)*)
    };
    ($ctx:expr, $name:expr, $meta:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty, $a13:ident: $t13:ty, $a14:ident: $t14:ty, $a15:ident: $t15:ty, $a16:ident: $t16:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_17($name, $meta, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12, $a13: $t13, $a14: $t14, $a15: $t15, $a16: $t16| $($body)*)
    };
}

/// `add_lua_fn!` for fallible closures (the port of C++ lambdas that can
/// throw — e.g. `u_execDependencyScript` dependency validation, `a_setMusic`
/// asset lookup, `s_setStyle`): the closure returns
/// `Result<Ret, LuaError>`, and an `Err` aborts the enclosing protected
/// call as a Lua runtime error (exactly what the plain `add_lua_fn!` does
/// too — this spelling documents intent and keeps the call sites readable).
#[macro_export]
macro_rules! add_lua_fn_try {
    ($($args:tt)*) => {
        $crate::add_lua_fn!($($args)*)
    };
}

/// Raw registration — same exact call semantics as `add_lua_fn!` but **no**
/// metadata entry (the C++ `addLuaFn`-free registration used for
/// `u_impl_addTrackedResult`, HGScripting.cpp:49).
#[macro_export]
macro_rules! add_lua_fn_raw {
    ($ctx:expr, $name:expr, move $($tail:tt)*) => {
        $crate::add_lua_fn_raw!($ctx, $name, $($tail)*)
    };
    ($ctx:expr, $name:expr, || $($body:tt)*) => {
        $ctx.add_lua_fn_raw_0($name, move || $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_1($name, move |$a0: $t0| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_2($name, move |$a0: $t0, $a1: $t1| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_3($name, move |$a0: $t0, $a1: $t1, $a2: $t2| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_4($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_5($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_6($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_7($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_8($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_9($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_10($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_11($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_12($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_13($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty, $a13:ident: $t13:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_14($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12, $a13: $t13| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty, $a13:ident: $t13:ty, $a14:ident: $t14:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_15($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12, $a13: $t13, $a14: $t14| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty, $a13:ident: $t13:ty, $a14:ident: $t14:ty, $a15:ident: $t15:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_16($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12, $a13: $t13, $a14: $t14, $a15: $t15| $($body)*)
    };
    ($ctx:expr, $name:expr, |$a0:ident: $t0:ty, $a1:ident: $t1:ty, $a2:ident: $t2:ty, $a3:ident: $t3:ty, $a4:ident: $t4:ty, $a5:ident: $t5:ty, $a6:ident: $t6:ty, $a7:ident: $t7:ty, $a8:ident: $t8:ty, $a9:ident: $t9:ty, $a10:ident: $t10:ty, $a11:ident: $t11:ty, $a12:ident: $t12:ty, $a13:ident: $t13:ty, $a14:ident: $t14:ty, $a15:ident: $t15:ty, $a16:ident: $t16:ty| $($body:tt)*) => {
        $ctx.add_lua_fn_raw_17($name, move |$a0: $t0, $a1: $t1, $a2: $t2, $a3: $t3, $a4: $t4, $a5: $t5, $a6: $t6, $a7: $t7, $a8: $t8, $a9: $t9, $a10: $t10, $a11: $t11, $a12: $t12, $a13: $t13, $a14: $t14, $a15: $t15, $a16: $t16| $($body)*)
    };
}
