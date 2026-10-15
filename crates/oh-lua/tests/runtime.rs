//! issue 006 verification: conversion round-trips, error mapping, sandbox
//! (verbatim C++ semantics), RNG routing, io.open mode pass-through,
//! dotted-name path rules, `add_lua_fn` call semantics, metadata.
//!
//! Spec: `.agents/tasks/issue-006-impl.md` "Verification" (1-7) and
//! `.agents/plans/001-rust-port/06-lua-runtime.md`.

use std::sync::{Arc, Mutex};

use oh_lua::{
    destroy_malicious_functions, get_metadata, redefine_io_open, redefine_random, LuaContext,
    LuaError,
};

fn new_ctx() -> LuaContext {
    LuaContext::new()
}

/// C++ `_getGlobal` throws `VariableDoesntExistException(fullName)` when it
/// hits a `nil` at ANY path step — including the final key. So in the
/// C++ original, `doesVariableExist("io.write")` after the sandbox teardown
/// does not return `false`, it throws. This helper asserts exactly that.
fn gone(lua: &LuaContext, name: &str) {
    if !name.contains('.') {
        assert!(
            !lua.does_variable_exist(name).unwrap(),
            "{name} still exists"
        );
        return;
    }
    match lua.does_variable_exist(name) {
        Err(LuaError::VariableDoesntExist { variable }) => assert_eq!(variable, name),
        other => panic!("expected VariableDoesntExist(\"{name}\"), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 2. Conversion round-trips, both directions, for each type
// ---------------------------------------------------------------------------

#[test]
fn round_trip_integers() {
    let lua = new_ctx();

    // Rust -> Lua -> Rust, 64-bit integer path (C++ `lua_pushinteger`/`lua_tointeger`).
    // NOTE: the vendored LuaJIT (5.1-compat mode, like the C++ build) stores
    // every number as a double, so only values exactly representable as f64
    // round-trip bit-exactly — same as the C++ original.
    for v in [0i32, 42i32, -42i32, i32::MAX, i32::MIN] {
        lua.write_variable("iv", &v).unwrap();
        assert_eq!(lua.read_variable::<i64>("iv").unwrap(), v as i64);
    }
    for v in [
        2_000_000_000i64,
        2i64.pow(53),
        4_000_000_000i64,
        1_000_000_000_000i64,
        1_000_000_000_000_000u64 as i64,
    ] {
        lua.write_variable("iv", &v).unwrap();
        assert_eq!(lua.read_variable::<i64>("iv").unwrap(), v);
    }
    // typed reads go through the same C++ cast (`T(lua_tointeger(...))`)
    lua.write_variable("iv", &-1i64).unwrap();
    assert_eq!(lua.read_variable::<i32>("iv").unwrap(), -1);

    // the Lua side really sees numbers: the vendored LuaJIT runs in 5.1
    // mode where every number is a double (`type()` == "number" even for
    // integer values — same as the C++ build).
    lua.write_variable("iv", &7i64).unwrap();
    let t: String = lua.execute_code_typed("return type(iv)").unwrap();
    assert_eq!(t, "number");
}

#[test]
fn round_trip_floats() {
    let lua = new_ctx();

    for v in [0.5f32, -1.25f32, 0.1f32, f32::MAX] {
        lua.write_variable("fv", &v).unwrap();
        assert_eq!(lua.read_variable::<f64>("fv").unwrap(), v as f64);
    }
    for v in [std::f64::consts::PI, -0.25f64, 1e300f64] {
        lua.write_variable("fv", &v).unwrap();
        assert_eq!(lua.read_variable::<f64>("fv").unwrap(), v);
    }
    // C++ pushes floats through `lua_pushnumber` (64-bit double): LuaJIT 5.1
    // numbers are all doubles, so `type()` is "number".
    lua.write_variable("fv", &0.5f64).unwrap();
    let t: String = lua.execute_code_typed("return type(fv)").unwrap();
    assert_eq!(t, "number");
}

#[test]
fn round_trip_bools() {
    let lua = new_ctx();
    for v in [true, false] {
        lua.write_variable("bv", &v).unwrap();
        assert_eq!(lua.read_variable::<bool>("bv").unwrap(), v);
    }
    lua.write_variable("bv", &true).unwrap();
    let t: String = lua.execute_code_typed("return type(bv)").unwrap();
    assert_eq!(t, "boolean");
}

#[test]
fn round_trip_strings() {
    let lua = new_ctx();
    for s in [
        "hello world",
        "",
        "üñî©ødé ✓",
        "with \n newline and \"quotes\"",
    ] {
        lua.write_variable("sv", &s.to_string()).unwrap();
        assert_eq!(lua.read_variable::<String>("sv").unwrap(), s);
    }
}

#[test]
fn round_trip_arrays() {
    let lua = new_ctx();
    let v: Vec<i32> = vec![1, -2, 3];
    lua.write_variable("av", &v).unwrap();
    assert_eq!(lua.read_variable::<Vec<i32>>("av").unwrap(), v);

    let s: Vec<String> = vec!["a".into(), "b".into()];
    lua.write_variable("sv", &s).unwrap();
    assert_eq!(lua.read_variable::<Vec<String>>("sv").unwrap(), s);

    // Lua 1-based indexing, exactly as C++ table pushes produce.
    let one: i32 = lua.execute_code_typed("return av[1]").unwrap();
    assert_eq!(one, 1);
}

// ---------------------------------------------------------------------------
// 3. Error mapping
// ---------------------------------------------------------------------------

#[test]
fn execute_error_maps_to_execution_error() {
    let lua = new_ctx();
    let err = lua.execute_code("error('x')").unwrap_err();
    match &err {
        // LuaJIT's `error()` appends a `stack traceback:` section to the
        // error object — the C++ build shows the same payload.
        LuaError::ExecutionError { message } => {
            assert!(message.starts_with("x"), "unexpected message: {message:?}")
        }
        other => panic!("expected ExecutionError, got {other:?}"),
    }
}

#[test]
fn syntax_error_maps_to_syntax_error() {
    let lua = new_ctx();
    let err = lua.execute_code("return =").unwrap_err();
    assert!(
        matches!(&err, LuaError::SyntaxError { .. }),
        "expected SyntaxError, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. read_variable("nope") raises VariableDoesntExist
// ---------------------------------------------------------------------------

#[test]
fn read_missing_variable_raises_variable_doesnt_exist() {
    let lua = new_ctx();
    let err = lua.read_variable::<i32>("nope").unwrap_err();
    match &err {
        LuaError::VariableDoesntExist { variable } => assert_eq!(variable, "nope"),
        other => panic!("expected VariableDoesntExist, got {other:?}"),
    }
    // every read type raises the same
    let err = lua.read_variable::<String>("nope").unwrap_err();
    assert!(matches!(err, LuaError::VariableDoesntExist { .. }));

    assert!(!lua.does_variable_exist("nope").unwrap());
    lua.execute_code("nope = 1").unwrap();
    assert!(lua.does_variable_exist("nope").unwrap());
}

// ---------------------------------------------------------------------------
// Dotted-name path semantics (C++ `_getGlobal`/`_setGlobal`)
// ---------------------------------------------------------------------------

#[test]
fn dotted_read_walks_and_reports_full_name() {
    let lua = new_ctx();
    lua.execute_code("a = { b = { c = 5 } }").unwrap();
    assert_eq!(lua.read_variable::<i32>("a.b.c").unwrap(), 5);

    // missing leaf: full dotted name in the error
    let err = lua.read_variable::<i32>("a.b.z").unwrap_err();
    match &err {
        LuaError::VariableDoesntExist { variable } => assert_eq!(variable, "a.b.z"),
        other => panic!("expected VariableDoesntExist, got {other:?}"),
    }
    // non-table intermediate: full dotted name in the error
    lua.execute_code("x = 3").unwrap();
    let err = lua.read_variable::<i32>("x.y").unwrap_err();
    match &err {
        LuaError::VariableDoesntExist { variable } => assert_eq!(variable, "x.y"),
        other => panic!("expected VariableDoesntExist, got {other:?}"),
    }
}

#[test]
fn dotted_write_requires_existing_table() {
    let lua = new_ctx();
    // C++ `_setGlobal`: intermediate tables are NOT created; a missing table
    // part raises VariableDoesntExist carrying the table part ("a.b").
    let err = lua.write_variable("a.b.c", &1).unwrap_err();
    match &err {
        LuaError::VariableDoesntExist { variable } => assert_eq!(variable, "a.b"),
        other => panic!("expected VariableDoesntExist, got {other:?}"),
    }

    lua.execute_code("a = { b = {} }").unwrap();
    lua.write_variable("a.b.c", &7).unwrap();
    assert_eq!(lua.read_variable::<i32>("a.b.c").unwrap(), 7);
    // non-table intermediate: error carries the FULL name
    let err = lua.write_variable("a.b.c.d", &1).unwrap_err();
    match &err {
        LuaError::VariableDoesntExist { variable } => assert_eq!(variable, "a.b.c.d"),
        other => panic!("expected VariableDoesntExist, got {other:?}"),
    }
}

#[test]
fn clear_variable_nils_dotted_paths() {
    let lua = new_ctx();
    lua.execute_code("t = { u = 1 }").unwrap();
    lua.clear_variable("t.u").unwrap();
    // C++ `doesVariableExist("t.u")` after the clear: `_getGlobal` hits nil
    // at the final step and throws — it does not return false.
    gone(&lua, "t.u");
    assert!(lua.is_variable_array("t").unwrap());
}

// ---------------------------------------------------------------------------
// 5. Sandbox: os and io.write are nil (and the rest of the teardown)
// ---------------------------------------------------------------------------

#[test]
fn sandbox_destroys_malicious_functions() {
    let lua = new_ctx();
    let calls = Arc::new(Mutex::new(Vec::<(i32, i32, i32)>::new()));
    // C++ `init` order: RNG shims first, then io.open, math.random, teardown.
    let c2 = calls.clone();
    oh_lua::add_lua_fn!(
        lua,
        "u_rndSwitch",
        oh_lua::FnMeta {
            ret: "float",
            arg_types: &["int", "int", "int"],
        },
        move |m: i32, _lower: i32, _upper: i32| {
            c2.lock().unwrap().push((m, 0, 0));
            0.42f32
        },
    )
    .unwrap();
    redefine_io_open(&lua).unwrap();
    redefine_random(&lua).unwrap();
    destroy_malicious_functions(&lua).unwrap();

    // os: gone.
    assert!(!lua.does_variable_exist("os").unwrap());
    // io: read-only subset remains; write/popen/flush/setvbuf gone.
    gone(&lua, "io.write");
    gone(&lua, "io.popen");
    gone(&lua, "io.flush");
    gone(&lua, "io.setvbuf");
    assert!(lua.does_variable_exist("io.open").unwrap());
    // debug: gone.
    assert!(!lua.does_variable_exist("debug").unwrap());
    // math.randomseed: gone; math.random: the shim.
    gone(&lua, "math.randomseed");
    assert!(lua.does_variable_exist("math.random").unwrap());
    // package: require/package.path survive; the restore paths are gone.
    assert!(lua.does_variable_exist("package.path").unwrap());
    gone(&lua, "package.loadlib");
    gone(&lua, "package.searchpath");
    gone(&lua, "package.loaders");
    gone(&lua, "package.loaded.os");
    gone(&lua, "package.loaded.debug");

    // `require` keeps working (the C++ comment: "needed for the require
    // function to work properly").
    lua.execute_code("local s = require('string'); assert(s.rep('b', 3) == 'bbb')")
        .unwrap();
    // base stdlib still fine.
    lua.execute_code("assert(table.concat({'x', 'y'}, '-') == 'x-y')")
        .unwrap();

    // and the destroyed libraries really are unreachable: calling `os.time`
    // must fail as a Lua error.
    let ok = lua
        .execute_code("local ok = pcall(function() return os.time() end); assert(ok == false)")
        .is_ok();
    assert!(ok);
}

// ---------------------------------------------------------------------------
// 6. math.random routes to the injected u_rndSwitch stub (assert the args)
// ---------------------------------------------------------------------------

#[test]
fn math_random_routes_to_u_rnd_switch() {
    let lua = new_ctx();
    let calls = Arc::new(Mutex::new(Vec::<(i32, i32, i32)>::new()));
    let c2 = calls.clone();
    oh_lua::add_lua_fn!(
        lua,
        "u_rndSwitch",
        oh_lua::FnMeta {
            ret: "float",
            arg_types: &["int", "int", "int"],
        },
        move |mode: i32, lower: i32, upper: i32| {
            c2.lock().unwrap().push((mode, lower, upper));
            0.42f32
        },
    )
    .unwrap();
    redefine_random(&lua).unwrap();

    // math.random() -> u_rndSwitch(0, 0, 0)
    lua.execute_code("test_r = math.random()").unwrap();
    // math.random(n) -> u_rndSwitch(1, 0, n)
    lua.execute_code("test_r = math.random(7)").unwrap();
    // math.random(a, b) -> u_rndSwitch(2, a, b)
    lua.execute_code("test_r = math.random(3, 9)").unwrap();

    let got = calls.lock().unwrap().clone();
    assert_eq!(got, vec![(0, 0, 0), (1, 0, 7), (2, 3, 9)]);
    // the stub's return value passes through unchanged
    assert!((lua.read_variable::<f64>("test_r").unwrap() - 0.42f32 as f64).abs() < 1e-6);
}

// ---------------------------------------------------------------------------
// 7. io.open("rb") passes the mode through unchanged
// ---------------------------------------------------------------------------

#[test]
fn io_open_mode_is_passed_through_or_coerced() {
    let lua = new_ctx();

    let path = std::env::temp_dir().join("oh_lua_rb_test.bin");
    std::fs::write(&path, b"1\r\n2").unwrap();
    let p = path.to_string_lossy().to_string();

    // 1) Mode routing, observed through a spy: the wrapper captures
    //    `local open = io.open` at install time, so the spy's arguments
    //    are exactly the mode that reaches the real `io.open`.
    lua.execute_code(
        "seen = {}; local real = io.open; io.open = function(f, m) table.insert(seen, m); return real(f, m) end",
    )
    .unwrap();
    redefine_io_open(&lua).unwrap();

    lua.execute_code(&format!(
        "local f = assert(io.open('{p}', 'rb')); f:close()"
    ))
    .unwrap();
    lua.execute_code(&format!(
        "local f = assert(io.open('{p}', 'wb')); f:close()"
    ))
    .unwrap();
    lua.execute_code("assert(seen[1] == 'rb', seen[1]); assert(seen[2] == 'r', seen[2])")
        .unwrap();

    // 2) The read-only guarantee: a write mode on a missing file is
    //    coerced to 'r', so the real open fails (nil + error, and no file
    //    is created) — exactly what a pack script would hit.
    let r: (bool, String) = lua
        .execute_code_typed(
            "local f, e = io.open('/nonexistent/oh_lua_dir/x', 'wb') return f == nil, tostring(e)",
        )
        .unwrap();
    assert!(r.0, "expected open-for-write to be refused, got Some file");
    assert!(r.1.contains("No such file"), "unexpected error: {r:?}");

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// add_lua_fn: the C++ FunctionToPush call semantics
// ---------------------------------------------------------------------------

#[test]
fn registered_fn_arity_and_arg_window() {
    let lua = new_ctx();
    let seen = Arc::new(Mutex::new(Vec::<(i32, i32)>::new()));
    let s2 = seen.clone();
    oh_lua::add_lua_fn!(
        lua,
        "f2",
        oh_lua::FnMeta {
            ret: "void",
            arg_types: &["int", "int"],
        },
        move |a: i32, b: i32| {
            s2.lock().unwrap().push((a, b));
        },
    )
    .unwrap();

    // 0 args (N-2): "at least N" error.
    let err = lua
        .execute_code("local ok = pcall(f2); assert(ok == false)")
        .is_ok();
    assert!(err);
    let msg: String = lua
        .execute_code_typed("local _, e = pcall(f2); return tostring(e)")
        .unwrap();
    assert!(
        msg.contains("this function requires at least 2 parameter(s)"),
        "got: {msg}"
    );

    // 1 arg (N-1): C++ reads the callee itself as the last slot -> the
    // userdata WrongType error text.
    let msg: String = lua
        .execute_code_typed("local _, e = pcall(f2, 1); return tostring(e)")
        .unwrap();
    assert!(
        msg.contains("Trying to cast a lua variable to an invalid type"),
        "got: {msg}"
    );

    // 2 args: exact.
    lua.execute_code("f2(1, 2)").unwrap();
    // 3 args (extra): C++ reads the LAST N slots, not the first.
    lua.execute_code("f2(1, 10, 20)").unwrap();
    assert_eq!(*seen.lock().unwrap(), vec![(1, 2), (10, 20)]);
}

#[test]
fn registered_fn_nil_and_type_coercion() {
    let lua = new_ctx();
    let got_i = Arc::new(Mutex::new(0i32));
    let gi = got_i.clone();
    oh_lua::add_lua_fn!(
        lua,
        "ri",
        oh_lua::FnMeta {
            ret: "void",
            arg_types: &["int"],
        },
        move |a: i32| {
            *gi.lock().unwrap() = a;
        },
    )
    .unwrap();
    let got_b = Arc::new(Mutex::new(false));
    let gb = got_b.clone();
    oh_lua::add_lua_fn!(
        lua,
        "rb",
        oh_lua::FnMeta {
            ret: "void",
            arg_types: &["bool"],
        },
        move |a: bool| {
            *gb.lock().unwrap() = a;
        },
    )
    .unwrap();
    let got_s = Arc::new(Mutex::new(String::new()));
    let gs = got_s.clone();
    oh_lua::add_lua_fn!(
        lua,
        "rs",
        oh_lua::FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |a: String| {
            *gs.lock().unwrap() = a;
        },
    )
    .unwrap();
    let got_f = Arc::new(Mutex::new(0.0f64));
    let gf = got_f.clone();
    oh_lua::add_lua_fn!(
        lua,
        "rf",
        oh_lua::FnMeta {
            ret: "void",
            arg_types: &["double"],
        },
        move |a: f64| {
            *gf.lock().unwrap() = a;
        },
    )
    .unwrap();

    // nil args coerce (C++ `lua_tointeger(nil)` == 0 etc.), not error.
    lua.execute_code("ri(nil); rb(nil); rf(nil)").unwrap();
    assert_eq!(*got_i.lock().unwrap(), 0);
    assert!(!*got_b.lock().unwrap());
    assert_eq!(*got_f.lock().unwrap(), 0.0);
    // C++ nil string arg is UB (std::string(nullptr)); the port errors.
    let msg: String = lua
        .execute_code_typed("local _, e = pcall(rs, nil); return tostring(e)")
        .unwrap();
    assert!(
        msg.contains("Trying to cast a lua variable to an invalid type"),
        "got: {msg}"
    );

    // numbers truncate toward zero; floats for int args.
    lua.execute_code("ri(2.9); ri(-2.9)").unwrap();
    assert_eq!(*got_i.lock().unwrap(), -2);
    // string -> number.
    lua.execute_code("ri('41'); rf('2.5')").unwrap();
    assert_eq!(*got_i.lock().unwrap(), 41);
    assert!((*got_f.lock().unwrap() - 2.5).abs() < 1e-12);
    // 5.1 hex literals parse through the arg path too.
    lua.execute_code("ri('0x1A')").unwrap();
    assert_eq!(*got_i.lock().unwrap(), 26);

    // bool rule: `lua_toboolean` — in Lua 5.1 only nil/false are falsy,
    // so 0 and "" read as TRUE (C++ `lua_toboolean(0)` == 1).
    lua.execute_code("rb(0); rb(1); rb(''); rb(true); rb(false)")
        .unwrap();
    assert!(!*got_b.lock().unwrap()); // false last
    lua.execute_code("rb(0)").unwrap();
    assert!(*got_b.lock().unwrap()); // 0 is truthy in Lua 5.1
    lua.execute_code("rb('')").unwrap();
    assert!(*got_b.lock().unwrap()); // non-nil non-false, even ""
    lua.execute_code("rb(nil)").unwrap();
    assert!(!*got_b.lock().unwrap());
    lua.execute_code("rb(false)").unwrap();
    assert!(!*got_b.lock().unwrap());

    // Lua number -> string args use Lua's own formatting (1e+20, not
    // 100000000000000000000).
    lua.execute_code("rs(100000000000000000000); rs(true)")
        .unwrap();
    assert_eq!(*got_s.lock().unwrap(), "true");
    lua.execute_code("rs(100000000000000000000)").unwrap();
    assert_eq!(*got_s.lock().unwrap(), "1e+20");
    lua.execute_code("rs(42)").unwrap();
    assert_eq!(*got_s.lock().unwrap(), "42");
}

#[test]
fn registered_fn_wrong_type_on_userdata() {
    let lua = new_ctx();
    let got = Arc::new(Mutex::new(0i32));
    let g = got.clone();
    oh_lua::add_lua_fn!(
        lua,
        "ri",
        oh_lua::FnMeta {
            ret: "void",
            arg_types: &["int"],
        },
        move |a: i32| {
            *g.lock().unwrap() = a;
        },
    )
    .unwrap();
    // A full userdata argument is userdata for the C++ `lua_isuserdata`
    // pre-check -> WrongType, before any coercion. (`newproxy` is the 5.1
    // way to make one; `lightuserdata` doesn't exist pre-5.2.)
    lua.execute_code("lu = newproxy(false)").unwrap();
    let msg: String = lua
        .execute_code_typed("local _, e = pcall(ri, lu); return tostring(e)")
        .unwrap();
    assert!(
        msg.contains("Trying to cast a lua variable to an invalid type"),
        "got: {msg}"
    );
}

#[test]
fn registered_fn_returns_scalars_and_tuples() {
    let lua = new_ctx();
    oh_lua::add_lua_fn!(
        lua,
        "g1",
        oh_lua::FnMeta {
            ret: "int",
            arg_types: &[],
        },
        || 7i32,
    )
    .unwrap();
    oh_lua::add_lua_fn!(
        lua,
        "g2",
        oh_lua::FnMeta {
            ret: "tuple<float, float>",
            arg_types: &[],
        },
        || (1.5f32, 2.5f32),
    )
    .unwrap();
    oh_lua::add_lua_fn!(
        lua,
        "g4",
        oh_lua::FnMeta {
            ret: "tuple<int, int, int, int>",
            arg_types: &[],
        },
        || (1i32, 2i32, 3i32, 4i32),
    )
    .unwrap();
    oh_lua::add_lua_fn!(
        lua,
        "g8",
        oh_lua::FnMeta {
            ret: "tuple<float, float, float, float, float, float, float, float>",
            arg_types: &[],
        },
        || (0f32, 1f32, 2f32, 3f32, 4f32, 5f32, 6f32, 7f32),
    )
    .unwrap();

    let v: i32 = lua.execute_code_typed("return g1()").unwrap();
    assert_eq!(v, 7);
    let v: (f32, f32) = lua.execute_code_typed("return g2()").unwrap();
    assert_eq!(v, (1.5, 2.5));
    let v: (i32, i32, i32, i32) = lua.execute_code_typed("return g4()").unwrap();
    assert_eq!(v, (1, 2, 3, 4));
    let v: (f32, f32, f32, f32, f32, f32, f32, f32) =
        lua.execute_code_typed("return g8()").unwrap();
    assert_eq!(v.0, 0.0);
    assert_eq!(v.7, 7.0);
}

#[test]
fn write_variable_stores_functions() {
    use mlua::Function;
    let lua = new_ctx();
    let f: Function = lua.lua().create_function(|_lua, x: i32| Ok(x * 2)).unwrap();
    lua.write_variable("double_it", &f).unwrap();
    let r: i32 = lua.execute_code_typed("return double_it(21)").unwrap();
    assert_eq!(r, 42);
}

// ---------------------------------------------------------------------------
// 4b. Metadata collector (addLuaFn doc/arg builder)
// ---------------------------------------------------------------------------

#[test]
fn metadata_entries_recorded_with_docs_and_args() {
    {
        let mut meta = get_metadata();
        meta.clear();
    }
    let lua = new_ctx();
    {
        let mut proxy = oh_lua::add_lua_fn!(
            &lua,
            "u_testRnd",
            oh_lua::FnMeta {
                ret: "float",
                arg_types: &["int", "int"],
            },
            |lower: i32, upper: i32| {
                let _ = (lower, upper);
                0.5f32
            }
        )
        .unwrap();
        proxy.arg("lower").arg("upper");
        proxy.doc("Return a random integer number in the [$0; $1] range.");
    }
    let meta = get_metadata();
    // other tests register their own `u_*` shims in the same process, so
    // locate this test's entry by name rather than assuming a slot.
    let entries = &meta.entries()[0];
    let entry = entries
        .iter()
        .find(|e| e.fn_name == "u_testRnd")
        .expect("u_testRnd entry missing from category 0");
    assert_eq!(entry.fn_ret, "float");
    assert_eq!(entry.fn_args, "int lower, int upper");
    assert_eq!(
        entry.fn_docs,
        "Return a random integer number in the [lower; upper] range."
    );
}
