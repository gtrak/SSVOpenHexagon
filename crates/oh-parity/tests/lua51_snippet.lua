-- The 5.1-style snippet (lane 030c): uses the seven isms named in issue 30
-- §12 in their canonical forms, in a single chunk. It must PASS with
-- `lua51_prelude.lua` installed (the shipped prelude, now with the product
-- `load` binding in every context — D-037), and on a bare context (the
-- product `load` binding present, the prelude NOT installed) it FAILS at
-- the first gap:
--   * bare Luau (0.663, this build) — at `loadstring`: the prelude's
--     `loadstring = load` alias line is not installed (the `load` binding
--     alone gives the 5.2 spelling only); `unpack` passes because this
--     Luau re-installed it natively as a 5.1-compat global.
--
-- (Historical, pre-unification: bare LuaJIT failed one construct later,
-- at `collectgarbage("gcinfo")` — the one 5.1 option LuaJIT never
-- implemented. D-037 deleted the LuaJIT backend; that cell is recorded in
-- the 030c report.)
--
-- FINDINGS recorded here (evidence for the report, not worked around):
--
--   * This Luau build is NOT the "Luau removed the 5.1-isms" picture issue
--     30 assumed: `unpack`, `getfenv`, `setfenv` (with REAL environment
--     semantics), `newproxy`, and a global `gcinfo()` are all NATIVE in
--     Luau 0.663 (verified in luau0-src's lbaselib.cpp/ltablib.cpp and in
--     the running VM). The prelude's guards therefore leave them untouched;
--     its live work here is `module` (absent) + the `collectgarbage`
--     wrapper (the 5.1 "gcinfo" option is absent on BOTH backends).
--   * `newproxy` is a 5.2+ construct, and the VMs agree on more than the
--     plan assumed: BOTH this LuaJIT and this Luau return a zero-size
--     USERDATA, and BOTH refuse the Lua-level setmetatable on it
--     ("table expected, got userdata"). The finalizer idiom that runs on
--     all observed VMs is metatable mutation:
--     `getmetatable(newproxy(true)).__gc = fn` — LuaJIT's own profiler's
--     usage. (Reference 5.2+ differs: it returns a table, allows
--     setmetatable, and flags the metatable with the literal `true`; this
--     LuaJIT keeps the flag in an internal weak table instead.)
--   * After `module(name)`, the DEFINING chunk's environment IS the module
--     table — it cannot read any global (not even `assert`, `_G` or the
--     module's own name; verified on this LuaJIT; the prelude's Luau shim
--     reproduces it via native setfenv). The snippet therefore captures
--     `assert` and a consumer-env closure BEFORE `module()`, and the
--     read-back happens inside that closure (consumer env = _G) — exactly
--     how a 5.1 consumer chunk sees the module.

-- 1. unpack — list unpacking (the 5.1 global; NATIVE on this Luau build —
--    the table library reinstalls it as a global "Lua 5.1 compat").
local a, b = unpack({ 10, 20 })
assert(a == 10 and b == 20, "unpack")

-- 2. loadstring — compile+run a string chunk (the 5.1 alias of load; ABSENT
--    on this Luau build until the prelude's `loadstring = load` line
--    aliases the product's Rust-side `load` binding — the one line the
--    prelude cannot write itself, no Lua-level code can create a chunk
--    compiler).
local f = loadstring("return 3 + 4")
assert(type(f) == "function", "loadstring must yield a function")
assert(f() == 7, "the loadstring chunk must run")

-- 3. newproxy — the __gc finalization idiom (see the finding above).
local t = newproxy(true)
assert(t ~= nil, "newproxy must yield an object")
local mt = getmetatable(t)
assert(mt ~= nil, "newproxy(true) must yield an object with a reachable metatable")
mt.__gc = function() end -- the idiom: install the finalizer (LuaJIT's own usage)
t = nil
collectgarbage("collect")
collectgarbage("collect") -- finalization may need a second step; the idiom
-- must not ERROR regardless of when the VM runs it

-- 4. gcinfo — heap size in KB, in the 5.1 access FORM (the
--    collectgarbage option, absent on this backend — the prelude's
--    wrapper adds it; the global gcinfo() function is native on this
--    build, but is not the 5.1 form the census pattern looks for).
local kb = collectgarbage("gcinfo")
assert(type(kb) == "number" and kb >= 0, "gcinfo must yield the heap size in KB")

-- 5+6. getfenv / setfenv — the faithful subset (the global environment;
--     on this Luau build both are native with real env semantics, and the
--     prelude leaves them untouched).
local env = getfenv()
assert(type(env) == "table", "getfenv must yield the environment table")
local g = function()
	return a -- reads a global (through the function's environment)
end
assert(g() == 10, "the function must resolve its globals")
setfenv(g, env) -- the no-op case: a function's default env IS the global env
assert(g() == 10, "setfenv(f, _G) must leave the function callable and correct")

-- 7. module — the canonical 5.1 pattern (see the finding above): capture
--    the assertion + a consumer-env closure first, then the defining chunk
--    writes its functions into the module table through its (new)
--    environment, and the read-back happens in the consumer closure.
local check = assert
local consumer = function()
	return lane030c_probe_mod.greet() == "hello" and lane030c_probe_mod.use_greet() == "hello"
end
module("lane030c_probe_mod")
function greet()
	return "hello"
end
function use_greet()
	return greet()
end
check(consumer(), "module: the canonical 5.1 read pattern (consumer env)")
