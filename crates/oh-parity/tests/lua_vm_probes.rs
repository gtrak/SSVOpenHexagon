//! Lane 030a: the explicit Lua-VM probes for the "unify everything on
//! Luau?" decision (task §4). D-037 landed: the port ships ONE Lua VM
//! (Luau), so the probe DIFF TABLE against the LuaJIT build is gone with
//! it — each probe is now a **regression pin** against its fixed reference
//! value (`probe_pins`): the values the Luau VM of this exact build
//! produces, recorded from the 030a/030c measurement. The pins are the
//! standing gate: a Luau version bump (or any VM-side behavior drift)
//! changes a pinned value and fails loudly here. The two pins D-037's
//! parity argument rests on are called out in the table below:
//! `string_format_%.14g` and `port_number_to_string` are IDENTICAL to the
//! C++'s (LuaJIT's) `lua_number2str` spelling — the only two probe paths
//! that can reach game-visible text (030c: measured identical).
//!
//! Each probe pins a CONCRETE, observable dialect fact (or agreement with
//! the C++ reference) of the shipping VM — the exact questions the
//! differential simulation cannot answer on its own (the simulation's
//! fingerprints proved the shipped corpus runs byte-identically; these
//! probes document WHY, at the dialect level):
//!
//! * number spellings (`tostring` / `tostring` of int-ish vs float values)
//! * `string.format('%.14g')`
//! * the port's number→string path (the C++ `readString` /
//!   `lua_number2str` equivalent: `read_variable::<String>` on numbers)
//! * the `error('x')` message shape
//! * dotted-path / nil-index semantics
//! * the post-sandbox global surface
//! * `pairs` ordering (the per-process hash-randomization question, made
//!   observable on a fixed key set)
//! * the `mlua::Integer` width (Luau's 32-bit `lua_Integer`)
//!
//! `emit_probes` writes `target/lua-parity/luau/run-<k>/probes.json`
//! (artifact convention — under `target/`, never committed) for human /
//! report inspection; `probe_pins` is the self-contained gate form (it
//! re-runs the probes in-process and asserts the fixed values — no
//! artifact, no cross-test race, runs under plain `cargo test`).

use std::fs;
use std::path::PathBuf;

use oh_lua::{destroy_malicious_functions, redefine_io_open, redefine_random, LuaContext};

const FINGERPRINT_DEF: &str = "lua-vm-probes/v1";

/// The artifact root (`target/lua-parity/luau/run-<k>/probes.json`).
fn artifact_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("../target"));
    target.join("lua-parity").join("luau").join(format!(
        "run-{}",
        std::env::var("LUA_PARITY_RUN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    ))
}

/// The probe results: name -> observed value (one string per probe; the
/// probe's Lua/Rust code lives next to the name so the report can quote it).
fn run_all_probes() -> Vec<(String, String)> {
    let lua = LuaContext::new();

    // -- number spellings --------------------------------------------------
    let spell = |vals: &[&str]| {
        let code = format!(
            "return table.concat({{{}}}, '|')",
            vals.iter()
                .map(|v| format!("tostring({v})"))
                .collect::<Vec<_>>()
                .join(",")
        );
        lua.execute_code_typed::<String>(&code)
            .unwrap_or_else(|e| format!("<error: {e}>"))
    };
    // The spec's value set + the wide-int spellings.
    let spellings = spell(&[
        "1e20",
        "0.5",
        "1234567.89",
        "0.0001",
        "9999999999999.999",
        "1",
        "1.0",
        "0.1",
        "1e30",
        "1e10",
        "-0.0",
        "2^53 + 1",
        "10^16",
    ]);

    // -- string.format('%.14g') --------------------------------------------
    let fmt14g = {
        let code = "return table.concat({string.format('%.14g', 1e20), string.format('%.14g', 1234567.89), string.format('%.14g', 0.0001), string.format('%.14g', 0.1), string.format('%.14g', 1e16), string.format('%.14g', 1.5e10), string.format('%.14g', 2^53 + 1)}, '|')";
        lua.execute_code_typed::<String>(code)
            .unwrap_or_else(|e| format!("<error: {e}>"))
    };

    // -- the port's number->string path (C++ readString / lua_number2str
    //     equivalent: `read_variable::<String>` on a numeric variable) -----
    let port_number_to_string = {
        let read_back = |lua: &LuaContext| {
            lua.read_variable::<String>("n")
                .unwrap_or_else(|e| format!("<error: {e}>"))
        };
        let mut parts = Vec::new();
        lua.write_variable("n", &1i64).unwrap();
        parts.push(read_back(&lua));
        lua.write_variable("n", &1e20f64).unwrap();
        parts.push(read_back(&lua));
        lua.write_variable("n", &-0.0f64).unwrap();
        parts.push(read_back(&lua));
        lua.write_variable("n", &42.5f64).unwrap();
        parts.push(read_back(&lua));
        parts.join("|")
    };

    // -- the error('x') message shape ---------------------------------------
    let error_shape = match lua.execute_code("error('x')") {
        Ok(()) => "<no error>".to_string(),
        Err(e) => e.to_string(),
    };

    // -- dotted-path / nil-index semantics ----------------------------------
    let dotted = {
        lua.execute_code("dp = { a = { b = 3 } }").unwrap();
        let ok_dotted: bool = lua.execute_code_typed("return dp.a.b == 3").unwrap();
        lua.execute_code("dpn = nil").unwrap();
        let nil_index_err = match lua.execute_code("return dpn.a") {
            Ok(()) => "<no error>".to_string(),
            Err(e) => e.to_string().lines().next().unwrap_or("").to_string(),
        };
        // the port's own dotted walk (C++ doesVariableExist semantics)
        let port_dotted = format!(
            "exists(dp.a.b)={} exists(dp.a.missing)={} exists(dp.a.missing.deep)={:?}",
            lua.does_variable_exist("dp.a.b").unwrap_or(false),
            lua.does_variable_exist("dp.a.missing").unwrap_or(false),
            lua.does_variable_exist("dp.a.missing.deep")
                .map_err(|e| e.to_string())
        );
        format!("ok_dotted={ok_dotted}; nil_index=[{nil_index_err}]; {port_dotted}")
    };

    // -- the post-sandbox global surface (key -> type) -----------------------
    let sandboxed = {
        redefine_io_open(&lua).unwrap();
        redefine_random(&lua).unwrap();
        destroy_malicious_functions(&lua).unwrap();
        let surface = lua
            .execute_code_typed::<Vec<String>>(
                "return (function() local t = {} for k in pairs(_G) do t[#t+1] = type(k) .. ':' .. k end table.sort(t) return t end)()",
            )
            .unwrap_or_else(|e| vec![format!("<error: {e}>")]);
        // The spec's post-state checklist, observed directly: the
        // restore-paths and the require behaviour after the sandbox.
        let require_ffi = match lua.execute_code("require('ffi')") {
            Ok(()) => "<ok!?>".to_string(),
            Err(e) => e.to_string().lines().next().unwrap_or("").to_string(),
        };
        let restore_paths: Vec<String> = lua
            .execute_code_typed::<Vec<String>>(
                "return (function() local t = {} for _, k in ipairs({'os','debug','io.popen','io.flush','io.write','io.setvbuf','math.randomseed','package.loadlib','package.searchpath','package.loaders','package.loaded.os','package.loaded.debug'}) do local ok, v = pcall(function() local cur = _G for part in k:gmatch('[%w_]+') do cur = cur[part] end return cur end) t[#t+1] = k .. '=' .. (ok and tostring(v == nil) or '<index-err>') end return t end)()",
            )
            .unwrap_or_default();
        format!(
            "{} | require(ffi)=[{}] | {}",
            surface.join(" "),
            require_ffi,
            restore_paths.join(" ")
        )
    };

    // -- pairs ordering (fixed key set: the hash-randomization probe) --------
    let pairs_order = {
        // 16 string keys chosen to collide on common hash layouts if a VM
        // ever bucketed them deterministically by value; 1..8 integer keys
        // (the array part, ordered in every conforming VM).
        lua.execute_code_typed::<Vec<String>>(
            "return (function()
                local t = {}
                for _, k in ipairs({'alpha','bravo','charlie','delta','echo','foxtrot','golf','hotel','india','juliet','kilo','lima','mike','november','oscar','papa'}) do t[k] = true end
                for i = 1, 8 do t[i] = true end
                local out = {}
                for k in pairs(t) do out[#out+1] = type(k) .. ':' .. tostring(k) end
                return out
            end)()",
        )
        .unwrap_or_else(|e| vec![format!("<error: {e}>")])
        .join(" ")
    };

    // -- the mlua::Integer width ---------------------------------------------
    let integer_width = {
        // `mlua::Integer` is `ffi::lua_Integer`: 8 bytes on the LuaJIT
        // backend, 4 on Luau — the observable width + the round-trip
        // consequence of a too-wide value.
        let width = std::mem::size_of::<mlua::Integer>();
        let mut rt = Vec::new();
        for v in [i32::MAX as i64, i32::MAX as i64 + 1, 2i64.pow(62)] {
            lua.write_variable("w", &v).unwrap();
            rt.push(lua.read_variable::<i64>("w").unwrap());
        }
        // The spec's integer-semantics set: math.floor / % / integral
        // float tostring — the constructs a level script can actually hit.
        let semantics: String = lua
            .execute_code_typed(
                "return table.concat({tostring(math.floor(4.9)), tostring(4.5 % 2), tostring(-4.5 % 2), tostring(42.0), type(42.0 + 0)}, '|')",
            )
            .unwrap_or_else(|e| format!("<error: {e}>"));
        format!(
            "width={width}B; roundtrips[i32MAX={}+1={}, 2^62={}]; floor4.9|mod4.5|mod-4.5|tostr42.0|type42 = {}",
            rt.first().copied().unwrap_or(-1),
            rt.get(1).copied().unwrap_or(-1),
            rt.get(2).copied().unwrap_or(-1),
            semantics
        )
    };

    vec![
        ("tostring_spellings".to_string(), spellings),
        ("string_format_%.14g".to_string(), fmt14g),
        ("port_number_to_string".to_string(), port_number_to_string),
        ("error_x_shape".to_string(), error_shape),
        ("dotted_path_semantics".to_string(), dotted),
        ("post_sandbox_surface".to_string(), sandboxed),
        ("pairs_order_fixed_keyset".to_string(), pairs_order),
        ("mlua_integer_width".to_string(), integer_width),
    ]
}

/// Emit the probe artifact + print the probe table (artifact form — for
/// human / report inspection; the gate form is `probe_pins`).
#[test]
fn emit_probes() {
    let dir = artifact_dir();
    fs::create_dir_all(&dir).expect("artifact dir");
    let probes = run_all_probes();
    let doc = serde_json::json!({
        "definition": FINGERPRINT_DEF,
        "backend": "luau",
        "probes": probes,
    });
    let path = dir.join("probes.json");
    fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).expect("write probes.json");
    eprintln!(
        "lane 030a: emitted {} probes to {}",
        probes.len(),
        path.display()
    );
    for (name, value) in &probes {
        eprintln!("  {name:32} = {value}");
    }
}

// ---------------------------------------------------------------------------
// The regression pins (D-037: the A/B diff table is gone - the shipping VM's
// probe values are the fixed reference)
// ---------------------------------------------------------------------------

/// The pinned probe values: what the shipping VM of this exact build
/// (Luau 0.663, `luau0-src 0.12.3`, mlua 0.10.5) produces - recorded from
/// the 030a/030c measurement (the pre-unification A/B table in
/// `.agents/plans/001-rust-port/30a-lua-parity.md` sec.3: `%.14g` and the
/// port's number->string are IDENTICAL to the C++'s `lua_number2str`
/// spelling - the two probe paths that can reach game-visible text - and
/// the remaining probes carry the measured Luau dialect, pinned here so a
/// Luau version bump or any VM-side drift fails loudly instead of drifting
/// silently).
const PINS: [(&str, &str); 8] = [
    (
        r#"tostring_spellings"#,
        r#"100000000000000000000|0.5|1234567.89|0.0001|9999999999999.998|1|1|0.1|1e+30|10000000000|-0|9007199254740992|10000000000000000"#,
    ),
    (
        r#"string_format_%.14g"#,
        r#"1e+20|1234567.89|0.0001|0.1|1e+16|15000000000|9.007199254741e+15"#,
    ),
    (r#"port_number_to_string"#, r#"1|1e+20|0|42.5"#),
    (
        r#"error_x_shape"#,
        r#"[string "chunk"]:1: x
stack traceback:
	[C]: in ?
	[C]: in function 'error'
	[string "chunk"]:1: in ?"#,
    ),
    (
        r#"dotted_path_semantics"#,
        r#"ok_dotted=true; nil_index=[[string "chunk"]:1: attempt to index nil with 'a']; exists(dp.a.b)=true exists(dp.a.missing)=false exists(dp.a.missing.deep)=Err("Variable \"dp.a.missing.deep\" doesn't exist in lua context")"#,
    ),
    (
        r#"post_sandbox_surface"#,
        r#"string:_G string:_VERSION string:assert string:bit32 string:buffer string:collectgarbage string:coroutine string:dp string:error string:gcinfo string:getfenv string:getmetatable string:ipairs string:load string:math string:n string:newproxy string:next string:package string:pairs string:pcall string:print string:rawequal string:rawget string:rawlen string:rawset string:require string:select string:setfenv string:setmetatable string:string string:table string:tonumber string:tostring string:type string:typeof string:unpack string:utf8 string:vector string:xpcall | require(ffi)=[[string "chunk"]:1: module 'ffi' not found] | os=true debug=true io.popen=<index-err> io.flush=<index-err> io.write=<index-err> io.setvbuf=<index-err> math.randomseed=true package.loadlib=true package.searchpath=true package.loaders=true package.loaded.os=true package.loaded.debug=true"#,
    ),
    (
        r#"pairs_order_fixed_keyset"#,
        r#"number:1 number:2 number:3 number:4 number:5 number:6 number:7 number:8 string:foxtrot string:papa string:november string:mike string:oscar string:lima string:delta string:hotel string:echo string:alpha string:india string:kilo string:juliet string:golf string:bravo string:charlie"#,
    ),
    (
        r#"mlua_integer_width"#,
        r#"width=4B; roundtrips[i32MAX=2147483647+1=2147483647, 2^62=2147483647]; floor4.9|mod4.5|mod-4.5|tostr42.0|type42 = 4|0.5|1.5|42|number"#,
    ),
];

/// The self-contained gate form: re-runs every probe in-process and asserts
/// the fixed values (no artifact, no cross-test race, no env gate - runs
/// under plain `cargo test`). A changed value is a VM-drift finding:
/// re-measure, re-review against the C++ reference, and update the pin with
/// the reason.
#[test]
fn probe_pins() {
    let probes = run_all_probes();
    assert_eq!(probes.len(), PINS.len(), "probe count drifted");
    let mut failures = Vec::new();
    for (name, expected) in PINS {
        let actual = probes
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("probe {name} missing from the emitted set"))
            .1
            .as_str();
        if actual != expected {
            failures.push(format!(
                "  {name}:\n    expected: {expected}\n    actual:   {actual}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "PROBE PIN DIVERGENCE ({} probe(s) drifted from the pinned Luau reference - VM version bump or behavior drift):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "lane 030a: all {} probes match the pinned Luau reference (the shipping VM's dialect is unchanged)",
        PINS.len()
    );
}
