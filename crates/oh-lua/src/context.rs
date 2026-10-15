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

use crate::convert::{FromLuaValue, IntoLuaValue, PushValue};
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

    /// `addLuaFn` (LuaScripting.cpp:39-53): registers a Rust closure as a
    /// Lua global `name` and returns the metadata proxy (`.arg` / `.doc`),
    /// which records the docs entry in the global `LuaMetadata` on drop —
    /// exactly what the C++ `LuaMetadataProxy` destructor does.
    ///
    /// # Exact C++ call semantics (`FunctionToPush::operator()`)
    ///
    /// The C++ wrapper checks `lua_gettop(state) < N` where the stack holds
    /// the callee itself plus its arguments, then reads the slots
    /// `-N .. -1`. Consequences, all preserved here:
    ///
    /// * a call with `N - 2` or fewer arguments raises the Lua error
    ///   `this function requires at least N parameter(s)` (C++ builds it
    ///   with `luaL_where` + `lua_concat`; the port omits the location
    ///   prefix that `luaL_where` would add);
    /// * a call with exactly `N - 1` arguments makes the last read hit the
    ///   callee itself, which every C++ `_read` rejects: the Lua error is
    ///   `Trying to cast a lua variable to an invalid type`;
    /// * a call with *more* than `N` arguments reads the **last** `N`
    ///   arguments (stack positions `-N .. -1`), not the first `N`;
    /// * each argument is converted with the `_read` coercion rules
    ///   (`FromLuaValue`), so explicit `nil` args read as 0 / 0.0 / false
    ///   (string: `WrongType`, the deliberate replacement of the C++ UB).
    ///
    /// `f` must be `'static` (mlua stores Rust functions in the state).
    /// Captured state therefore needs `Arc`s or the engine's own
    /// lifetime-extension pattern; `meta.arg_types` must list exactly
    /// `Args::COUNT` C++ `typeToStr` spellings (see `FnMeta`).
    pub fn add_lua_fn<Ret, Args, F>(
        &self,
        name: &str,
        meta: FnMeta,
        f: F,
    ) -> Result<LuaMetadataProxy, LuaError>
    where
        F: Fn(Args) -> Ret + 'static,
        Args: ArgTuple,
        Ret: PushValue,
    {
        debug_assert_eq!(
            meta.arg_types.len(),
            Args::COUNT,
            "FnMeta::arg_types must list exactly one entry per closure argument"
        );

        let func = self
            .lua
            .create_function(
                move |_lua: &mlua::Lua,
                      values: mlua::MultiValue|
                      -> mlua::Result<mlua::MultiValue> {
                    let values: Vec<Value> = values.into();
                    let count = Args::COUNT;
                    let nargs = values.len();

                    // C++: `if (lua_gettop(state) < TupledFunction::count)` — the
                    // callee itself occupies one stack slot. (`count == 0` means
                    // zero-argument functions: both checks are vacuous.)
                    if count > 0 && nargs + 1 < count {
                        return Err(mlua::Error::RuntimeError(format!(
                            "this function requires at least {count} parameter(s)"
                        )));
                    }
                    // Exactly `count - 1` arguments: the C++ read of slot `-count`
                    // is the callee itself (userdata) -> WrongTypeException.
                    if count > 0 && nargs == count - 1 {
                        return Err(mlua::Error::RuntimeError(
                            "Trying to cast a lua variable to an invalid type".to_string(),
                        ));
                    }
                    // Extra arguments: C++ reads the last `count` slots
                    // (`count == 0` reads nothing at all).
                    let window = if count == 0 {
                        &values[..0]
                    } else {
                        &values[nargs - count..]
                    };
                    let args = Args::read_last_n(window)
                        .map_err(|err| mlua::Error::RuntimeError(err.to_string()))?;
                    f(args).push_value(_lua)
                },
            )
            .map_err(LuaError::from_mlua)?;

        self.write_variable(name, &func)?;
        Ok(LuaMetadataProxy::new(name, meta.ret, meta.arg_types))
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

// ----------------------------------------------------------------------------
// `ArgTuple` — registered-function argument extraction (C++ `FnTupleWrapper`
// + `FunctionToPush` argument reads). The game's largest signature is
// `cw_setVertexColor4` (17 parameters); arities 0..=17 are provided.
// ----------------------------------------------------------------------------

use crate::convert::ArgTuple;

impl ArgTuple for () {
    const COUNT: usize = 0;
    fn read_last_n(_values: &[Value]) -> Result<Self, LuaError> {
        Ok(())
    }
}

// Single-argument functions may take the scalar type directly (e.g.
// `|a: i32|`): COUNT 1, read the same single slot as `(A0,)`.
macro_rules! impl_arg_tuple_scalar {
    ($($t:ty),* $(,)?) => {
        $(impl ArgTuple for $t {
            const COUNT: usize = 1;
            fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
                <Self as FromLuaValue>::from_lua_value(&values[0])
            }
        })*
    };
}
impl_arg_tuple_scalar!(bool, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64, String);

// One impl per arity (the game's largest signature is `cw_setVertexColor4`,
// 17 parameters). Element reads use literal local names so the tuple
// construction stays in non-fragment positions.

impl<A0: FromLuaValue> ArgTuple for (A0,) {
    const COUNT: usize = 1;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        Ok((v0,))
    }
}

impl<A0: FromLuaValue, A1: FromLuaValue> ArgTuple for (A0, A1) {
    const COUNT: usize = 2;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        Ok((v0, v1))
    }
}

impl<A0: FromLuaValue, A1: FromLuaValue, A2: FromLuaValue> ArgTuple for (A0, A1, A2) {
    const COUNT: usize = 3;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        Ok((v0, v1, v2))
    }
}

impl<A0: FromLuaValue, A1: FromLuaValue, A2: FromLuaValue, A3: FromLuaValue> ArgTuple
    for (A0, A1, A2, A3)
{
    const COUNT: usize = 4;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        Ok((v0, v1, v2, v3))
    }
}

impl<A0: FromLuaValue, A1: FromLuaValue, A2: FromLuaValue, A3: FromLuaValue, A4: FromLuaValue>
    ArgTuple for (A0, A1, A2, A3, A4)
{
    const COUNT: usize = 5;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        Ok((v0, v1, v2, v3, v4))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
    > ArgTuple for (A0, A1, A2, A3, A4, A5)
{
    const COUNT: usize = 6;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        Ok((v0, v1, v2, v3, v4, v5))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
    > ArgTuple for (A0, A1, A2, A3, A4, A5, A6)
{
    const COUNT: usize = 7;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        Ok((v0, v1, v2, v3, v4, v5, v6))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
    > ArgTuple for (A0, A1, A2, A3, A4, A5, A6, A7)
{
    const COUNT: usize = 8;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        Ok((v0, v1, v2, v3, v4, v5, v6, v7))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
        A8: FromLuaValue,
    > ArgTuple for (A0, A1, A2, A3, A4, A5, A6, A7, A8)
{
    const COUNT: usize = 9;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        let v8 = <A8 as FromLuaValue>::from_lua_value(&values[8])?;
        Ok((v0, v1, v2, v3, v4, v5, v6, v7, v8))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
        A8: FromLuaValue,
        A9: FromLuaValue,
    > ArgTuple for (A0, A1, A2, A3, A4, A5, A6, A7, A8, A9)
{
    const COUNT: usize = 10;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        let v8 = <A8 as FromLuaValue>::from_lua_value(&values[8])?;
        let v9 = <A9 as FromLuaValue>::from_lua_value(&values[9])?;
        Ok((v0, v1, v2, v3, v4, v5, v6, v7, v8, v9))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
        A8: FromLuaValue,
        A9: FromLuaValue,
        A10: FromLuaValue,
    > ArgTuple for (A0, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10)
{
    const COUNT: usize = 11;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        let v8 = <A8 as FromLuaValue>::from_lua_value(&values[8])?;
        let v9 = <A9 as FromLuaValue>::from_lua_value(&values[9])?;
        let v10 = <A10 as FromLuaValue>::from_lua_value(&values[10])?;
        Ok((v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
        A8: FromLuaValue,
        A9: FromLuaValue,
        A10: FromLuaValue,
        A11: FromLuaValue,
    > ArgTuple for (A0, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11)
{
    const COUNT: usize = 12;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        let v8 = <A8 as FromLuaValue>::from_lua_value(&values[8])?;
        let v9 = <A9 as FromLuaValue>::from_lua_value(&values[9])?;
        let v10 = <A10 as FromLuaValue>::from_lua_value(&values[10])?;
        let v11 = <A11 as FromLuaValue>::from_lua_value(&values[11])?;
        Ok((v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
        A8: FromLuaValue,
        A9: FromLuaValue,
        A10: FromLuaValue,
        A11: FromLuaValue,
        A12: FromLuaValue,
    > ArgTuple for (A0, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12)
{
    const COUNT: usize = 13;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        let v8 = <A8 as FromLuaValue>::from_lua_value(&values[8])?;
        let v9 = <A9 as FromLuaValue>::from_lua_value(&values[9])?;
        let v10 = <A10 as FromLuaValue>::from_lua_value(&values[10])?;
        let v11 = <A11 as FromLuaValue>::from_lua_value(&values[11])?;
        let v12 = <A12 as FromLuaValue>::from_lua_value(&values[12])?;
        Ok((v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
        A8: FromLuaValue,
        A9: FromLuaValue,
        A10: FromLuaValue,
        A11: FromLuaValue,
        A12: FromLuaValue,
        A13: FromLuaValue,
    > ArgTuple for (A0, A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13)
{
    const COUNT: usize = 14;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        let v8 = <A8 as FromLuaValue>::from_lua_value(&values[8])?;
        let v9 = <A9 as FromLuaValue>::from_lua_value(&values[9])?;
        let v10 = <A10 as FromLuaValue>::from_lua_value(&values[10])?;
        let v11 = <A11 as FromLuaValue>::from_lua_value(&values[11])?;
        let v12 = <A12 as FromLuaValue>::from_lua_value(&values[12])?;
        let v13 = <A13 as FromLuaValue>::from_lua_value(&values[13])?;
        Ok((v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
        A8: FromLuaValue,
        A9: FromLuaValue,
        A10: FromLuaValue,
        A11: FromLuaValue,
        A12: FromLuaValue,
        A13: FromLuaValue,
        A14: FromLuaValue,
    > ArgTuple
    for (
        A0,
        A1,
        A2,
        A3,
        A4,
        A5,
        A6,
        A7,
        A8,
        A9,
        A10,
        A11,
        A12,
        A13,
        A14,
    )
{
    const COUNT: usize = 15;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        let v8 = <A8 as FromLuaValue>::from_lua_value(&values[8])?;
        let v9 = <A9 as FromLuaValue>::from_lua_value(&values[9])?;
        let v10 = <A10 as FromLuaValue>::from_lua_value(&values[10])?;
        let v11 = <A11 as FromLuaValue>::from_lua_value(&values[11])?;
        let v12 = <A12 as FromLuaValue>::from_lua_value(&values[12])?;
        let v13 = <A13 as FromLuaValue>::from_lua_value(&values[13])?;
        let v14 = <A14 as FromLuaValue>::from_lua_value(&values[14])?;
        Ok((
            v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14,
        ))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
        A8: FromLuaValue,
        A9: FromLuaValue,
        A10: FromLuaValue,
        A11: FromLuaValue,
        A12: FromLuaValue,
        A13: FromLuaValue,
        A14: FromLuaValue,
        A15: FromLuaValue,
    > ArgTuple
    for (
        A0,
        A1,
        A2,
        A3,
        A4,
        A5,
        A6,
        A7,
        A8,
        A9,
        A10,
        A11,
        A12,
        A13,
        A14,
        A15,
    )
{
    const COUNT: usize = 16;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        let v8 = <A8 as FromLuaValue>::from_lua_value(&values[8])?;
        let v9 = <A9 as FromLuaValue>::from_lua_value(&values[9])?;
        let v10 = <A10 as FromLuaValue>::from_lua_value(&values[10])?;
        let v11 = <A11 as FromLuaValue>::from_lua_value(&values[11])?;
        let v12 = <A12 as FromLuaValue>::from_lua_value(&values[12])?;
        let v13 = <A13 as FromLuaValue>::from_lua_value(&values[13])?;
        let v14 = <A14 as FromLuaValue>::from_lua_value(&values[14])?;
        let v15 = <A15 as FromLuaValue>::from_lua_value(&values[15])?;
        Ok((
            v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14, v15,
        ))
    }
}

impl<
        A0: FromLuaValue,
        A1: FromLuaValue,
        A2: FromLuaValue,
        A3: FromLuaValue,
        A4: FromLuaValue,
        A5: FromLuaValue,
        A6: FromLuaValue,
        A7: FromLuaValue,
        A8: FromLuaValue,
        A9: FromLuaValue,
        A10: FromLuaValue,
        A11: FromLuaValue,
        A12: FromLuaValue,
        A13: FromLuaValue,
        A14: FromLuaValue,
        A15: FromLuaValue,
        A16: FromLuaValue,
    > ArgTuple
    for (
        A0,
        A1,
        A2,
        A3,
        A4,
        A5,
        A6,
        A7,
        A8,
        A9,
        A10,
        A11,
        A12,
        A13,
        A14,
        A15,
        A16,
    )
{
    const COUNT: usize = 17;
    fn read_last_n(values: &[Value]) -> Result<Self, LuaError> {
        let v0 = <A0 as FromLuaValue>::from_lua_value(&values[0])?;
        let v1 = <A1 as FromLuaValue>::from_lua_value(&values[1])?;
        let v2 = <A2 as FromLuaValue>::from_lua_value(&values[2])?;
        let v3 = <A3 as FromLuaValue>::from_lua_value(&values[3])?;
        let v4 = <A4 as FromLuaValue>::from_lua_value(&values[4])?;
        let v5 = <A5 as FromLuaValue>::from_lua_value(&values[5])?;
        let v6 = <A6 as FromLuaValue>::from_lua_value(&values[6])?;
        let v7 = <A7 as FromLuaValue>::from_lua_value(&values[7])?;
        let v8 = <A8 as FromLuaValue>::from_lua_value(&values[8])?;
        let v9 = <A9 as FromLuaValue>::from_lua_value(&values[9])?;
        let v10 = <A10 as FromLuaValue>::from_lua_value(&values[10])?;
        let v11 = <A11 as FromLuaValue>::from_lua_value(&values[11])?;
        let v12 = <A12 as FromLuaValue>::from_lua_value(&values[12])?;
        let v13 = <A13 as FromLuaValue>::from_lua_value(&values[13])?;
        let v14 = <A14 as FromLuaValue>::from_lua_value(&values[14])?;
        let v15 = <A15 as FromLuaValue>::from_lua_value(&values[15])?;
        let v16 = <A16 as FromLuaValue>::from_lua_value(&values[16])?;
        Ok((
            v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14, v15, v16,
        ))
    }
}
