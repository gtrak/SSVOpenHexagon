//! Lane 030c — the 5.1 compat prelude prototype (task §2.2): the proof that
//! issue 30 §12's only plausible HARD blocker ("removed 5.1-isms appearing
//! in third-party packs") is either mitigable or genuinely hard, per-ism.
//!
//! The prelude (`lua51_prelude.lua`) is a Lua-level shim set installed in
//! the context before a 5.1-style script runs — no Rust, no VM change,
//! idempotent, every shim guarded by the target global's absence. The
//! workload (`lua51_snippet.lua`) uses the seven isms named in §12 in
//! their canonical forms.
//!
//! FINDING OF RECORD (rewrote the plan's assumption): the Luau this port
//! builds (0.663, luau0-src 0.12.3 via mlua 0.10.5) did NOT remove the
//! 5.1-isms the way issue 30 assumed — it re-added most of them with real
//! semantics (verified in the vendored source AND the running VM):
//! `unpack` (5.1-compat global reinstall), `getfenv`/`setfenv` (real
//! environment re-binding), `newproxy` (userdata + empty metatable, like
//! LuaJIT), and a global `gcinfo()`. Genuinely absent on this Luau:
//! `load`/`loadstring` (no chunk compiler in the language surface at all)
//! and `module`. And `collectgarbage("gcinfo")` — the 5.1 OPTION form — is
//! absent on BOTH backends (LuaJIT 2.1 never implemented it; mlua's
//! Luau-side collectgarbage wrapper doesn't know it).
//!
//! The tests, run under BOTH backends (one build = one VM):
//!
//! * `five_1_snippet_without_prelude_reports_the_gap` — the A/B: bare Luau
//!   fails at `loadstring` (the first construct it lacks); bare LuaJIT
//!   fails at `collectgarbage("gcinfo")` (the one 5.1 option it never got).
//! * `five_1_snippet_runs_with_prelude` — the deliverable's core assertion:
//!   the 5.1-style snippet runs with the prelude installed, on either VM.
//!   On Luau the host must ALSO supply the one missing Rust-side binding
//!   — the global `load` (this test installs it locally via mlua's own
//!   `Lua::load` API — that one line IS the loadstring mitigation; it is
//!   not shipped in the Lua-level prelude, which cannot write it).
//! * `each_51_ism_individually` — the per-ism classification table: each
//!   ism probed alone, with and without the prelude. Bare Luau: everything
//!   passes natively EXCEPT `loadstring` and `module` (and the
//!   `collectgarbage("gcinfo")` option form). Bare LuaJIT: everything
//!   passes natively EXCEPT `gcinfo`. With the prelude: all pass on both.
//! * `loadstring_needs_the_rust_side_companion` — the not-emulatable cell,
//!   pinned three ways: bare (absent), prelude-only (STILL absent — no
//!   Lua-level code can create a chunk compiler), binding+prelude (works).
//! * `prelude_approximation_boundaries` — where the remaining differences
//!   live, observed rather than assumed: `setfenv` re-binding is native
//!   AND faithful on both backends (the plan's "not emulatable" verdict
//!   does not apply to this build); the module shim's lenient `__index =
//!   _G` superset (the defining chunk keeps global visibility — a
//!   deliberate divergence from 5.1's self-index and LuaJIT's none); the
//!   `newproxy` flag (neither VM stores it in the metatable); the
//!   collectgarbage wrapper's unnormalised forwarding; install-nothing
//!   identity for the native globals.

use oh_lua::LuaContext;

const PRELUDE: &str = include_str!("lua51_prelude.lua");
const SNIPPET: &str = include_str!("lua51_snippet.lua");

/// Is Luau the selected backend? (mlua permits one VM per build; the
/// feature is the build-level selector — same idiom as lua_engine_parity.)
fn is_luau() -> bool {
    cfg!(feature = "backend-luau")
}

/// The ONE-LINE RUST-SIDE companion the prelude cannot write itself: on a
/// VM whose standard surface has no chunk compiler (this Luau build), the
/// host installs the global `load` from mlua's own `Lua::load` (the exact
/// line a shipped prelude would need; here it lives test-locally so no
/// product code changes). A no-op where `load` already exists (LuaJIT).
fn install_load_binding_if_missing(lua: &LuaContext) {
    let has_load: bool = lua
        .execute_code_typed("return type(load) == 'function'")
        .expect("type(load) probe");
    if has_load {
        return;
    }
    let l = lua.lua();
    let load_fn = l
        .create_function(
            |lu: &mlua::Lua, code: String| -> mlua::Result<mlua::Function> {
                lu.load(code.as_str())
                    .set_name("loadstring-chunk")
                    .into_function()
            },
        )
        .expect("create the load binding");
    l.globals()
        .set("load", load_fn)
        .expect("install the load binding");
}

/// The A/B arm: the 5.1-style snippet on a BARE context (no prelude).
/// Both backends fail — at different gaps:
///   * Luau   — `loadstring`: Luau's base library has no chunk compiler
///     (no `load` either). Note the snippet gets PAST `unpack` — this Luau
///     re-installed the 5.1 global natively, so the plan's "Luau removed
///     the 5.1-isms" premise does not hold for this build;
///   * LuaJIT — `collectgarbage("gcinfo")`: the 5.1 option LuaJIT 2.1
///     never implemented.
#[test]
fn five_1_snippet_without_prelude_reports_the_gap() {
    let lua = LuaContext::new();
    let r = lua.execute_code(SNIPPET);
    let msg = r.expect_err(
        "the bare context must lack something the snippet needs (the gap the prelude fills)",
    );
    let msg_owned = msg.to_string();
    let first_line = msg_owned.lines().next().unwrap_or("");
    if is_luau() {
        eprintln!("030c without-prelude [luau]: the 5.1 snippet fails as expected (at loadstring — no chunk compiler in the language surface): {first_line}");
        // Luau's error does not name the nil global, so prove the first-gap
        // ordering directly: the construct BEFORE loadstring in the snippet
        // (unpack) is NATIVE on this build, and loadstring itself is the
        // first missing one — so the failure lands on loadstring, not
        // earlier (the plan's premise that "Luau removed the 5.1-isms" does
        // not hold for unpack on this build).
        let unpack_t: String = LuaContext::new()
            .execute_code_typed("return type(unpack)")
            .expect("type(unpack) probe");
        let loadstring_t: String = LuaContext::new()
            .execute_code_typed("return type(loadstring)")
            .expect("type(loadstring) probe");
        assert_eq!(
            unpack_t, "function",
            "luau: unpack is NATIVE on this build (the 5.1 snippet gets past it) — the plan's premise does not hold"
        );
        assert_eq!(
            loadstring_t, "nil",
            "luau: loadstring is the first gap (no chunk compiler in the language surface)"
        );
    } else {
        eprintln!("030c without-prelude [luajit]: the 5.1 snippet fails as expected (at the gcinfo option): {first_line}");
        assert!(
            first_line.contains("gcinfo") || first_line.contains("collectgarbage"),
            "luajit: the first gap must be the collectgarbage \"gcinfo\" option (never implemented in 2.1)"
        );
    }
}

/// The core deliverable assertion: the 5.1-style snippet runs WITH the
/// prelude installed, on either backend. On Luau the test-local `load`
/// binding is installed first (the documented Rust-side companion — the
/// prelude's `loadstring = load` line then activates).
#[test]
fn five_1_snippet_runs_with_prelude() {
    let lua = LuaContext::new();
    if is_luau() {
        install_load_binding_if_missing(&lua);
    }
    lua.execute_code(PRELUDE)
        .expect("the prelude must install cleanly (idempotent guards)");
    lua.execute_code(SNIPPET).expect(
        "the 5.1-style snippet (unpack/loadstring/newproxy/gcinfo/getfenv/setfenv/module) must run with the prelude installed",
    );
    eprintln!(
        "030c with-prelude [{}]: the 5.1-style snippet ran to completion{}",
        if is_luau() { "luau" } else { "luajit" },
        if is_luau() {
            " (with the test-local load binding — the one Rust-side line)"
        } else {
            ""
        }
    );
}

/// The per-ism probes (canonical single-ism forms; each in its own context
/// so a prior probe's globals cannot mask a later one's absence).
///
/// `newproxy` uses the metatable-mutation spelling — the only one that
/// runs on all observed VMs (LuaJIT's own profiler's usage; the Lua-level
/// setmetatable refuses the proxy userdata on BOTH LuaJIT and this Luau).
///
/// The `module` probe is the single-chunk form that BOTH backends support
/// natively: after `module(name)` the defining chunk's env is the module
/// table (no globals readable — verified on this LuaJIT; the prelude's
/// Luau shim reproduces it via native setfenv), so the assertion and the
/// consumer-env closure are captured BEFORE the `module()` call, and the
/// read-back happens inside that closure (consumer env = _G).
///
/// `gcinfo` probes the 5.1 ACCESS FORM (`collectgarbage("gcinfo")` — the
/// census pattern for third-party packs), not the native global
/// `gcinfo()` function that BOTH backends of this build have (a different
/// access path, noted in the report).
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

/// The per-ism gap map, bare context (no prelude), pinned per backend:
///   * bare Luau:  every ism NATIVE except `loadstring` (no chunk
///     compiler) and `module` (never had it) — and the `gcinfo` 5.1
///     option form (the global gcinfo() exists on both backends of this
///     build, but that is not the collectgarbage option the census looks
///     for);
///   * bare LuaJIT: every ism native except `gcinfo` (the 5.1 option 2.1
///     never implemented; the fork does add the global gcinfo()).
fn bare_luau_gap(ism: &str) -> bool {
    matches!(ism, "loadstring" | "module" | "gcinfo")
}

fn bare_luajit_gap(ism: &str) -> bool {
    ism == "gcinfo"
}

/// The classification evidence: each 5.1-ism probed ALONE, with and
/// without the prelude. With the prelude (and, on Luau, the test-local
/// `load` binding): all pass on both.
#[test]
fn each_51_ism_individually() {
    let backend = if is_luau() { "luau" } else { "luajit" };
    eprintln!("030c per-ism table [{backend}]");
    eprintln!("  {:<12} | without prelude | with prelude", "ism");
    for (name, probe) in ISM_PROBES {
        let bare = LuaContext::new();
        let bare_r = bare.execute_code(probe);
        drop(bare);

        let with = LuaContext::new();
        if is_luau() {
            install_load_binding_if_missing(&with);
        }
        with.execute_code(PRELUDE).expect("prelude install");
        let with_r = with.execute_code(probe);

        let expected_gap = if is_luau() {
            bare_luau_gap(name)
        } else {
            bare_luajit_gap(name)
        };
        if expected_gap {
            assert!(
                bare_r.is_err(),
                "{backend}: the {name} probe must fail WITHOUT the prelude (that is the gap)"
            );
        } else {
            assert!(
                bare_r.is_ok(),
                "{backend}: the {name} probe must pass natively WITHOUT the prelude (this build has it)"
            );
        }
        assert!(
            with_r.is_ok(),
            "{backend}: the {name} probe must pass WITH the prelude installed"
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

/// The not-emulatable cell, pinned three ways (Luau arm only — on LuaJIT
/// `loadstring` is native and the classification does not apply):
///
/// 1. BARE: `loadstring` is absent (no chunk compiler in the language
///    surface — `load` is absent too);
/// 2. PRELUDE-ONLY: the prelude ALONE cannot fix it — `loadstring = load`
///    has nothing to alias (`load` is nil), so it stays absent. No
///    Lua-level code can create a chunk compiler;
/// 3. BINDING + PRELUDE: with the one Rust-side line (the global `load`
///    from mlua's own `Lua::load`, test-local), the prelude's line
///    activates and the 5.1 form works.
#[test]
fn loadstring_needs_the_rust_side_companion() {
    // (1) bare:
    let bare = LuaContext::new();
    let t: String = bare
        .execute_code_typed("return type(loadstring) .. '|' .. type(load)")
        .expect("bare probe");
    if is_luau() {
        assert_eq!(
            t, "nil|nil",
            "luau bare: loadstring AND load are both absent"
        );
    } else {
        assert_eq!(t, "function|function", "luajit bare: both native");
    }

    // (2) prelude-only: still absent on Luau (the prelude's line is inert
    // without a load to alias):
    let prelude_only = LuaContext::new();
    prelude_only.execute_code(PRELUDE).expect("prelude install");
    let t2: String = prelude_only
        .execute_code_typed("return type(loadstring)")
        .expect("prelude-only probe");
    if is_luau() {
        assert_eq!(
            t2, "nil",
            "luau: the prelude ALONE cannot create a chunk compiler — loadstring stays absent (the not-emulatable cell)"
        );
        eprintln!("030c loadstring [luau]: prelude-only = still absent (Lua-level emulation impossible — no chunk compiler in the language surface)");
    } else {
        assert_eq!(
            t2, "function",
            "luajit: native loadstring untouched by the prelude"
        );
    }

    // (3) binding + prelude: the one Rust-side line makes the 5.1 form
    // work:
    let with = LuaContext::new();
    install_load_binding_if_missing(&with);
    with.execute_code(PRELUDE).expect("prelude install");
    let v: i64 = with
        .execute_code_typed("return loadstring('return 6*7')()")
        .expect("loadstring with the binding + prelude");
    assert_eq!(v, 42, "the 5.1 form must run once the host supplies `load`");
    eprintln!(
        "030c loadstring [{}]: binding + prelude = works (loadstring('return 6*7')() == 42)",
        if is_luau() { "luau" } else { "luajit" }
    );
}

/// Where the remaining differences live, observed rather than assumed:
///
/// * `getfenv()` IS the global environment table (identity) on both
///   backends (native on both);
/// * `setfenv(f, _G)` is the faithful no-op on both; `setfenv(f, t)` to a
///   NON-global env is accepted natively on both backends and the re-bind
///   is REAL (a function's globals resolve from the new env) — the
///   plan's "setfenv not emulatable on Luau" verdict does NOT apply to
///   this build: Luau 0.663 has native environment semantics;
/// * `gcinfo` == `floor(count)` via the wrapper (installed on both
///   backends — neither implements the 5.1 option), and the wrapper
///   forwards the native options UNNORMALISED (`"collect"` returns a
///   number on LuaJIT and nothing on Luau);
/// * `module`'s defining-chunk redirect now works on BOTH backends (the
///   prelude's Luau shim via native setfenv; LuaJIT natively): the
///   function lands IN the module table on both — the approximation
///   boundary is the shim's LENIENT superset: the defining chunk keeps
///   global visibility through `__index = _G` (5.1 self-indexes; LuaJIT
///   has no metatable — both are stricter);
/// * the `newproxy` flag: neither observed VM stores it in the metatable
///   (LuaJIT: internal weak table; this Luau: empty metatable) —
///   `getmetatable(newproxy(true)).__gc` is nil on both; the prelude's
///   dormant fallback (table + inert __gc function) only activates on a
///   build that lacks newproxy entirely;
/// * install-nothing identity: the prelude must not replace a global the
///   VM already has (`getfenv`/`setfenv`/`unpack`/`newproxy` captured
///   before, compared after — on both backends; `type` == "C" is NOT the
///   marker, this LuaJIT build reports every base-stdlib function as
///   "function").
#[test]
fn prelude_approximation_boundaries() {
    let lua = LuaContext::new();
    // Capture pre-install identity for every global the VM already has
    // (the install-nothing property, checked after the install below):
    lua.execute_code(
        "pre_before = { getfenv = getfenv, setfenv = setfenv, unpack = unpack, newproxy = newproxy }",
    )
    .expect("capture pre-install identity");
    if is_luau() {
        install_load_binding_if_missing(&lua);
    }
    lua.execute_code(PRELUDE).expect("prelude install");

    // getfenv identity (the faithful subset):
    let env_is_g: bool = lua
        .execute_code_typed("return getfenv() == _G")
        .expect("getfenv identity");
    assert!(
        env_is_g,
        "getfenv() must be the global environment (both backends)"
    );

    // setfenv: the honoured no-op ...
    let noop: bool = lua
        .execute_code_typed("local f = function() end return setfenv(f, _G) == f")
        .expect("setfenv no-op");
    assert!(
        noop,
        "setfenv(f, _G) must be a faithful no-op returning f (both backends)"
    );

    // ... and the NON-global re-bind, which is native AND faithful on both
    // backends (the plan's not-emulatable verdict does not apply to this
    // Luau build — it has real environment semantics):
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
        "setfenv(f, non-global) must re-bind the function's globals (native 5.1 on LuaJIT; native real-env on this Luau)"
    );
    eprintln!("030c boundary [{}]: setfenv non-global re-bind accepted natively and works (faithful 5.1 — no approximation needed on this build)", if is_luau() { "luau" } else { "luajit" });

    // gcinfo == floor(count), and the wrapper forwards native options
    // UNNORMALISED: "collect" returns a number on LuaJIT (a 5.1 deviation
    // in LuaJIT itself) and nothing on Luau — the wrapper must forward
    // each, not normalise:
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
    if is_luau() {
        assert_eq!(
            gc_check.2, "nil",
            "luau: native collectgarbage('collect') returns nothing — the wrapper must forward it as-is"
        );
    } else {
        assert_eq!(
            gc_check.2, "number",
            "luajit: native collectgarbage('collect') returns a number — the wrapper must forward it, not normalise it to 5.1's nil"
        );
    }
    // And the access-form note, now a finding: BOTH backends have the
    // GLOBAL gcinfo() function natively (this Luau via lbaselib.cpp; this
    // LuaJIT fork via a lib_base.c addition — both integer KB), but
    // NEITHER implements the 5.1 collectgarbage("gcinfo") OPTION — the
    // wrapper's job on both:
    let gc_global: String = lua
        .execute_code_typed("return type(gcinfo)")
        .expect("global gcinfo type");
    assert_eq!(
        gc_global, "function",
        "both backends: the global gcinfo() is native (a different access form from the 5.1 collectgarbage option the wrapper adds)"
    );

    // module: the defining-chunk redirect now works on BOTH backends
    // (native on LuaJIT; the prelude's shim via native setfenv on Luau):
    // the function lands IN the module table on both. Two chunks, as in
    // real 5.1 code: the defining chunk (env = module table after
    // module()) and the consumer chunk (normal env).
    lua.execute_code(
        "module('boundary_mod')\n\
         function bm_fn() return 'ok' end",
    )
    .expect("module defining chunk (both backends)");
    let consumer: String = lua
        .execute_code_typed(
            "return tostring(boundary_mod.bm_fn()) .. '|' .. tostring(rawget(boundary_mod, 'bm_fn') ~= nil)",
        )
        .expect("module consumer chunk");
    let (read_side, raw_in_module) = consumer.split_once('|').expect("consumer output");
    assert_eq!(
        read_side, "ok",
        "the module read side must work on both backends"
    );
    assert!(
        raw_in_module == "true",
        "the module-defined function must land IN the module table on both backends (native on LuaJIT; the prelude's setfenv redirect on Luau)"
    );
    // The strictness difference the shim introduces, made observable:
    // AFTER module(), the defining chunk's env still sees globals through
    // __index = _G on Luau (the shim's LENIENT superset) — 5.1
    // (self-index) and LuaJIT (no metatable on its module table) are both
    // stricter there, and LuaJIT's strictness is pinned here too:
    let strictness: Result<bool, _> = lua.execute_code_typed(
        "module('strict_mod')\n\n\
         return type(_G) == 'table'",
    );
    if is_luau() {
        assert!(
            strictness.is_ok(),
            "luau: the shim's defining chunk keeps global visibility through __index = _G (a deliberate lenient superset of 5.1's self-index)"
        );
    } else {
        assert!(
            strictness.is_err(),
            "luajit: the native module table has no metatable — the defining chunk cannot read _G after module() (5.1-strict, observed)"
        );
    }

    // The newproxy flag: neither observed VM stores it in the metatable:
    let flag_type: String = lua
        .execute_code_typed("local t = newproxy(true) return type(getmetatable(t).__gc)")
        .expect("newproxy flag");
    assert_eq!(
        flag_type, "nil",
        "neither VM stores the newproxy flag in the metatable (LuaJIT: internal weak table; this Luau: empty metatable) — the idiom is metatable mutation, which both accept"
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
    let lua = LuaContext::new();
    if is_luau() {
        install_load_binding_if_missing(&lua);
    }
    lua.execute_code(PRELUDE).expect("first install");
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
