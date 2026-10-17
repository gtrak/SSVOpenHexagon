//! Lane 030c — the 5.1 compat prelude (task §2.2), now the SHIPPED
//! capability of the unified-on-Luau port (D-037).
//!
//! Issue 30 §12's only plausible HARD blocker for unifying on Luau was a
//! 5.1-ism that Luau removed (`setfenv`, `getfenv`, `loadstring`, `unpack`,
//! `newproxy`, `module`, the `collectgarbage("gcinfo")` option) appearing in
//! a third-party pack. 030c's prototype classified each ism against the
//! exact Luau this port builds (0.663, luau0-src 0.12.3 via mlua 0.10.5):
//!
//! * `unpack` — NATIVE (5.1-compat global re-install in the table library);
//! * `getfenv` / `setfenv` — NATIVE, with real environment semantics;
//! * `newproxy` — NATIVE (zero-size userdata + empty metatable, like
//!   LuaJIT);
//! * `gcinfo` — the GLOBAL `gcinfo()` is NATIVE, but the 5.1
//!   `collectgarbage("gcinfo")` OPTION is absent (mlua's Luau-side
//!   collectgarbage wrapper knows only the Luau-native options);
//! * `loadstring` — NOT EMULATABLE AT LUA LEVEL: Luau's base library has no
//!   chunk compiler at all (no `load` either). The shipped companion is the
//!   ONE-LINE RUST-SIDE binding in `LuaContext::new`
//!   (`crates/oh-lua/src/context.rs`: mlua's own `Lua::load` exposed as the
//!   global `load`); the prelude's `loadstring = load` line then activates
//!   the 5.1 spelling;
//! * `module` — ABSENT; the prelude's shim restores the 5.1 contract as far
//!   as the VM allows.
//!
//! The product wiring (D-037, lane 030d): `LuaContext::new` installs the
//! `load` binding in EVERY context (the C++'s LuaJIT has `loadstring` in
//! every context — dropping it would be a parity regression for
//! third-party content), and `api::init_lua_api` installs the prelude
//! itself (`install_lua51_prelude` — the game/menu seam through which level
//! scripts run). These tests pin that wiring end to end:
//!
//! * `lua_context_ships_the_load_binding` — the one-line Rust binding on a
//!   bare context (chunk name, error shape included);
//! * `loadstring_runs_with_the_product_binding_and_prelude` — the pin of
//!   record: `loadstring('return 6*7')() == 42`;
//! * `each_51_ism_individually` — the per-ism classification table (bare
//!   context vs prelude-installed);
//! * `five_1_snippet_without_prelude_reports_the_gap` /
//!   `five_1_snippet_runs_with_prelude` — the 5.1-style workload;
//! * `prelude_approximation_boundaries` — where the remaining differences
//!   live, observed rather than assumed;
//! * `prelude_is_idempotent` — double install changes nothing.
//!
//! The prelude file under test is the SHIPPED one
//! (`crates/oh-lua/lua51_prelude.lua` — the same file the product includes);
//! installing it below is done exactly the way `install_lua51_prelude` does
//! (one `execute_code` of the file's text).

const PRELUDE: &str = include_str!("../../oh-lua/lua51_prelude.lua");
const SNIPPET: &str = include_str!("lua51_snippet.lua");

/// Install the shipped prelude the product way (`install_lua51_prelude`):
/// one `execute_code` of the file's text.
fn install_prelude(lua: &oh_lua::LuaContext) {
    lua.execute_code(PRELUDE)
        .expect("the prelude must install cleanly (idempotent guards)");
}

/// The one-line Rust binding of D-037: every `LuaContext` ships the global
/// `load` (mlua's own `Lua::load` — Luau's base library has no chunk
/// compiler at all, so without this line the C++'s `loadstring`
/// capability would be a parity regression for third-party content).
#[test]
fn lua_context_ships_the_load_binding() {
    let lua = oh_lua::LuaContext::new();
    let t: String = lua
        .execute_code_typed("return type(load)")
        .expect("type(load) probe");
    assert_eq!(
        t, "function",
        "every context must ship the `load` binding (D-037)"
    );

    // It compiles a chunk from a string and the chunk runs:
    let v: i64 = lua
        .execute_code_typed("return load('return 6*7')()")
        .expect("load('return 6*7')()");
    assert_eq!(v, 42);

    // The chunk name the binding sets (the 030c-demonstrated spelling — a
    // load chunk's error carries it):
    let err = match lua.execute_code("load('error(\"x\")')()") {
        Ok(()) => panic!("the load chunk must error"),
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("loadstring-chunk") && err.contains("x"),
        "the load binding must name its chunks 'loadstring-chunk': {err}"
    );
}

/// The pin of record: with the product's `load` binding (every context)
/// and the prelude's `loadstring = load` line, the 5.1 spelling runs.
#[test]
fn loadstring_runs_with_the_product_binding_and_prelude() {
    let lua = oh_lua::LuaContext::new();
    install_prelude(&lua);
    let v: i64 = lua
        .execute_code_typed("return loadstring('return 6*7')()")
        .expect("loadstring('return 6*7')() must run (D-037's preserved capability)");
    assert_eq!(v, 42, "loadstring('return 6*7')() == 42");
    eprintln!(
        "030d loadstring: product binding + prelude = works (loadstring('return 6*7')() == 42)"
    );
}

/// The not-emulatable cell, pinned against the PRODUCT wiring (D-037):
///
/// 1. BARE: `loadstring` is absent but `load` exists — the one-line Rust
///    binding ships in every context, and NO Lua-level code can create the
///    chunk compiler itself (the prelude only ALIASES it);
/// 2. PRELUDE: the `loadstring = load` line activates the 5.1 spelling
///    (idempotent guards: a second install must not double-alias or
///    change the value);
/// 3. the C++'s `loadstring` parity: the 5.1 spelling compiles and runs a
///    chunk from a string exactly as the C++'s LuaJIT does.
#[test]
fn loadstring_needs_the_rust_side_companion() {
    // (1) bare:
    let bare = oh_lua::LuaContext::new();
    let t: String = bare
        .execute_code_typed("return type(loadstring) .. '|' .. type(load)")
        .expect("bare probe");
    assert_eq!(
        t,
        "nil|function",
        "bare context: loadstring is absent (the prelude's alias line is not installed) but the product `load` binding is present"
    );

    // (2) prelude: the alias activates:
    let with = oh_lua::LuaContext::new();
    install_prelude(&with);
    let t2: String = with
        .execute_code_typed("return type(loadstring)")
        .expect("post-prelude probe");
    assert_eq!(
        t2, "function",
        "with the prelude + the product binding, loadstring is a function"
    );

    // (3) the 5.1 form runs (the pin of record, via the alias):
    let v: i64 = with
        .execute_code_typed("return loadstring('return 6*7')()")
        .expect("the 5.1 form");
    assert_eq!(
        v, 42,
        "the 5.1 form must run once the prelude aliases the product binding"
    );
    eprintln!("030c loadstring: not emulatable at Lua level — the product binding is the companion (bare = nil|function, prelude = works)");
}

/// The A/B arm, now against the product wiring: the 5.1-style snippet on a
/// BARE context (the product `load` binding present, the prelude NOT
/// installed). The snippet fails at `loadstring` — the first construct it
/// lacks: it gets PAST `unpack` (native on this Luau build — the plan's
/// premise that "Luau removed the 5.1-isms" does not hold for it on this
/// build), and the `load` binding alone does not give the 5.1 spelling
/// (the prelude's alias line is what activates it).
#[test]
fn five_1_snippet_without_prelude_reports_the_gap() {
    let lua = oh_lua::LuaContext::new();
    let r = lua.execute_code(SNIPPET);
    let msg = r.expect_err(
        "the bare context must lack something the snippet needs (the gap the prelude fills)",
    );
    let msg_owned = msg.to_string();
    let first_line = msg_owned.lines().next().unwrap_or("");
    eprintln!(
        "030c without-prelude: the 5.1 snippet fails as expected (at loadstring — the prelude's alias line is not installed): {first_line}"
    );
    // The first-gap ordering, proven directly: the construct BEFORE
    // loadstring in the snippet (unpack) is NATIVE on this build, and
    // loadstring itself is the first missing one.
    let unpack_t: String = oh_lua::LuaContext::new()
        .execute_code_typed("return type(unpack)")
        .expect("type(unpack) probe");
    let loadstring_t: String = oh_lua::LuaContext::new()
        .execute_code_typed("return type(loadstring)")
        .expect("type(loadstring) probe");
    assert_eq!(
        unpack_t, "function",
        "unpack is NATIVE on this build (the 5.1 snippet gets past it) — the plan's premise does not hold"
    );
    assert_eq!(
        loadstring_t, "nil",
        "loadstring is the first gap on a bare context (the prelude's alias line is what installs it)"
    );
}

/// The core deliverable assertion: the 5.1-style snippet runs WITH the
/// prelude installed (the product wiring: the `load` binding is already in
/// the context from `LuaContext::new`).
#[test]
fn five_1_snippet_runs_with_prelude() {
    let lua = oh_lua::LuaContext::new();
    install_prelude(&lua);
    lua.execute_code(SNIPPET).expect(
        "the 5.1-style snippet (unpack/loadstring/newproxy/gcinfo/getfenv/setfenv/module) must run with the prelude installed",
    );
    eprintln!("030c with-prelude: the 5.1-style snippet ran to completion (product load binding + shipped prelude)");
}

/// The per-ism probes (canonical single-ism forms; each in its own context
/// so a prior probe's globals cannot mask a later one's absence).
///
/// `newproxy` uses the metatable-mutation spelling — the only one that
/// runs on all observed VMs (LuaJIT's own profiler's usage; the Lua-level
/// setmetatable refuses the proxy userdata on LuaJIT and this Luau alike).
///
/// The `module` probe is the single-chunk form that runs with the prelude's
/// shim (defining-chunk redirect via the native setfenv, read side via
/// `__index = _G` — the deliberate lenient superset, see
/// `prelude_approximation_boundaries`): the assertion and the consumer-env
/// closure are captured BEFORE the `module()` call, and the read-back
/// happens inside that closure (consumer env = _G).
///
/// `gcinfo` probes the 5.1 ACCESS FORM (`collectgarbage("gcinfo")` — the
/// census pattern for third-party packs), not the native global `gcinfo()`
/// function this build has (a different access path, noted below).
const ISM_PROBES: [(&str, &str); 7] = [
    ("unpack", "local a, b = unpack({1, 2})\nassert(a == 1 and b == 2)"),
    ("loadstring", "assert(loadstring('return 41')() == 41)"),
    ("getfenv", "assert(type(getfenv()) == 'table')"),
    ("setfenv", "setfenv(function() end, getfenv())"),
    (
        "newproxy",
        "local t = newproxy(true)\nlocal mt = getmetatable(t)\nassert(mt ~= nil)\nmt.__gc = function() end",
    ),
    (
        "module",
        "local check = assert\n\n\
         local consumer = function() return ism_probe_mod.greet() == 42 end\n\n\
         module('ism_probe_mod')\n\n\
         function greet() return 42 end\n\n\
         check(consumer())",
    ),
    (
        "gcinfo",
        "local kb = collectgarbage('gcinfo')\nassert(type(kb) == 'number' and kb >= 0)",
    ),
];

/// The per-ism gap map, bare context (no prelude), pinned for the shipping
/// Luau: every ism NATIVE except `loadstring` (the prelude's alias line is
/// what installs it — the product `load` binding alone gives the 5.2
/// spelling only), `module` (never had it; the prelude's shim), and the
/// `gcinfo` 5.1 option form (the global gcinfo() exists natively, but that
/// is not the collectgarbage option the census looks for).
fn bare_luau_gap(ism: &str) -> bool {
    matches!(ism, "loadstring" | "module" | "gcinfo")
}

/// The classification evidence: each 5.1-ism probed ALONE, with and
/// without the prelude (the product `load` binding is present in both).
/// With the prelude: all pass.
#[test]
fn each_51_ism_individually() {
    eprintln!("030c per-ism table [luau, product wiring]");
    eprintln!("  {:<12} | without prelude | with prelude", "ism");
    for (name, probe) in ISM_PROBES {
        let bare = oh_lua::LuaContext::new();
        let bare_r = bare.execute_code(probe);
        drop(bare);

        let with = oh_lua::LuaContext::new();
        install_prelude(&with);
        let with_r = with.execute_code(probe);

        let expected_gap = bare_luau_gap(name);
        if expected_gap {
            assert!(
                bare_r.is_err(),
                "luau: the {name} probe must fail WITHOUT the prelude (that is the gap)"
            );
        } else {
            assert!(
                bare_r.is_ok(),
                "luau: the {name} probe must pass natively WITHOUT the prelude (this build has it)"
            );
        }
        assert!(
            with_r.is_ok(),
            "luau: the {name} probe must pass WITH the prelude installed"
        );
        eprintln!(
            "  {:<12} | {:<15} | ok",
            name,
            if bare_r.is_ok() {
                "ok (native)"
            } else {
                "FAILS (gap)"
            }
        );
    }
}

/// Where the remaining differences live, observed rather than assumed:
///
/// * `getfenv()` IS the global environment table (identity) — native on
///   this Luau build;
/// * `setfenv(f, _G)` is the faithful no-op; `setfenv(f, t)` to a
///   NON-global env is accepted natively and the re-bind is REAL (a
///   function's globals resolve from the new env) — the plan's "setfenv
///   not emulatable on Luau" verdict does NOT apply to this build: Luau
///   0.663 has native environment semantics;
/// * `gcinfo` == `floor(count)` via the wrapper (this Luau's
///   collectgarbage wrapper does not know the 5.1 option), and the wrapper
///   forwards the native options UNNORMALISED (`"collect"` returns
///   nothing on Luau);
/// * `module`'s defining-chunk redirect works through the prelude's shim
///   (native setfenv): the function lands IN the module table. The
///   approximation boundary is the shim's LENIENT superset: the defining
///   chunk keeps global visibility through `__index = _G` (5.1
///   self-indexes; LuaJIT has no metatable — both are stricter);
/// * the `newproxy` flag: this VM does not store it in the metatable
///   (empty metatable) — `getmetatable(newproxy(true)).__gc` is nil; the
///   prelude's dormant fallback (table + inert __gc function) only
///   activates on a build that lacks newproxy entirely;
/// * install-nothing identity: the prelude must not replace a global the
///   VM already has (`getfenv`/`setfenv`/`unpack`/`newproxy` captured
///   before, compared after; `type` == "C" is NOT the marker — this VM
///   reports every base-stdlib function as "function").
#[test]
fn prelude_approximation_boundaries() {
    let lua = oh_lua::LuaContext::new();
    // Capture pre-install identity for every global the VM already has
    // (the install-nothing property, checked after the install below):
    lua.execute_code(
        "pre_before = { getfenv = getfenv, setfenv = setfenv, unpack = unpack, newproxy = newproxy }",
    )
    .expect("capture pre-install identity");
    install_prelude(&lua);

    // getfenv identity (the faithful subset):
    let env_is_g: bool = lua
        .execute_code_typed("return getfenv() == _G")
        .expect("getfenv identity");
    assert!(env_is_g, "getfenv() must be the global environment");

    // setfenv: the honoured no-op ...
    let noop: bool = lua
        .execute_code_typed("local f = function() end return setfenv(f, _G) == f")
        .expect("setfenv no-op");
    assert!(noop, "setfenv(f, _G) must be a faithful no-op returning f");

    // ... and the NON-global re-bind, which is native AND faithful on
    // this build (the plan's not-emulatable verdict does not apply — it
    // has real environment semantics):
    let rebound: bool = lua
        .execute_code_typed(
            "local env2 = { special = 42 }\n\
             local h = function() return special end\n\
             setfenv(h, env2)\n\
             return h() == 42",
        )
        .expect("setfenv re-bind");
    assert!(
        rebound,
        "setfenv(f, non-global) must re-bind the function's globals (native real-env on this Luau)"
    );
    eprintln!("030c boundary [luau]: setfenv non-global re-bind accepted natively and works (faithful 5.1 — no approximation needed on this build)");

    // gcinfo == floor(count), and the wrapper forwards native options
    // UNNORMALISED: "collect" returns nothing on Luau — the wrapper must
    // forward it as-is, not normalise it:
    let gc_check: (u64, u64, String) = lua
        .execute_code_typed(
            "local kb = collectgarbage('gcinfo')\n\
             local c = math.floor(collectgarbage('count'))\n\
             local r = collectgarbage('collect')\n\
             return kb, c, type(r)",
        )
        .expect("gcinfo boundary");
    assert!(
        gc_check.0 == gc_check.1,
        "gcinfo must equal floor(count): gcinfo={} vs floor(count)={}",
        gc_check.0,
        gc_check.1
    );
    assert_eq!(
        gc_check.2, "nil",
        "luau: native collectgarbage('collect') returns nothing — the wrapper must forward it as-is"
    );
    // And the access-form note, now a finding: this build has the GLOBAL
    // gcinfo() function natively (integer KB via LUA_GCCOUNT), but the
    // 5.1 collectgarbage("gcinfo") OPTION is the one the wrapper adds:
    let gc_global: String = lua
        .execute_code_typed("return type(gcinfo)")
        .expect("global gcinfo type");
    assert_eq!(
        gc_global, "function",
        "the global gcinfo() is native (a different access form from the 5.1 collectgarbage option the wrapper adds)"
    );

    // module: the defining-chunk redirect works through the prelude's
    // shim (native setfenv on this build): the function lands IN the
    // module table. Two chunks, as in real 5.1 code: the defining chunk
    // (env = module table after module()) and the consumer chunk (normal
    // env).
    lua.execute_code(
        "module('boundary_mod')\n\
         function bm_fn() return 'ok' end",
    )
    .expect("module defining chunk");
    let consumer: String = lua
        .execute_code_typed(
            "return tostring(boundary_mod.bm_fn()) .. '|' .. tostring(rawget(boundary_mod, 'bm_fn') ~= nil)",
        )
        .expect("module consumer chunk");
    let (read_side, raw_in_module) = consumer.split_once('|').expect("consumer output");
    assert_eq!(read_side, "ok", "the module read side must work");
    assert!(
        raw_in_module == "true",
        "the module-defined function must land IN the module table (the prelude's setfenv redirect)"
    );
    // The strictness difference the shim introduces, made observable:
    // AFTER module(), the defining chunk's env still sees globals through
    // __index = _G (the shim's LENIENT superset) — 5.1 (self-index) and
    // LuaJIT (no metatable on its module table) are both stricter there:
    let strictness: Result<bool, _> = lua.execute_code_typed(
        "module('strict_mod')\n\n\
         return type(_G) == 'table'",
    );
    assert!(
        strictness.is_ok(),
        "luau: the shim's defining chunk keeps global visibility through __index = _G (a deliberate lenient superset of 5.1's self-index)"
    );

    // The newproxy flag: this VM does not store it in the metatable
    // (empty metatable — LuaJIT's internal weak table is the only other
    // observed behavior):
    let flag_type: String = lua
        .execute_code_typed("local t = newproxy(true) return type(getmetatable(t).__gc)")
        .expect("newproxy flag");
    assert_eq!(
        flag_type, "nil",
        "the VM does not store the newproxy flag in the metatable — the idiom that runs is metatable mutation, which the VM accepts"
    );

    // Install-nothing identity: every global the VM already had keeps its
    // pre-install identity after the prelude (the rawget guards):
    let same: (bool, bool, bool, bool) = lua
        .execute_code_typed(
            "return getfenv == pre_before.getfenv, setfenv == pre_before.setfenv, unpack == pre_before.unpack, newproxy == pre_before.newproxy",
        )
        .expect("post-install identity");
    assert!(
        same.0 && same.1 && same.2 && same.3,
        "the prelude must not replace globals the VM already has (getfenv/setfenv/unpack/newproxy identity lost) — the install-nothing property"
    );
}

/// Idempotency: installing the prelude twice must not double-wrap or
/// double-install anything (the rawget guards + the pcall capability
/// probe), and the snippet must still run afterwards.
#[test]
fn prelude_is_idempotent() {
    let lua = oh_lua::LuaContext::new();
    install_prelude(&lua);
    lua.execute_code("cg_after_first = collectgarbage")
        .expect("capture the post-first-install collectgarbage");
    lua.execute_code(PRELUDE)
        .expect("second install (idempotent)");
    lua.execute_code(SNIPPET)
        .expect("the snippet must still run after a double install");
    // No-stacking, pinned by identity (floor-of-floor would pass a value
    // check but not this one): the second install sees the wrapper's
    // working gcinfo through the pcall probe and must NOT re-wrap:
    let same: bool = lua
        .execute_code_typed("return collectgarbage == cg_after_first")
        .expect("collectgarbage identity after double install");
    assert!(
        same,
        "the collectgarbage wrapper must not stack across installs"
    );
    let floor_ok: bool = lua
        .execute_code_typed(
            "return collectgarbage('gcinfo') == math.floor(collectgarbage('count'))",
        )
        .expect("gcinfo after double install");
    assert!(
        floor_ok,
        "gcinfo must still equal floor(count) after a double install"
    );
}
