-- Lua 5.1 compat prelude over Luau (lane 030c prototype).
--
-- Task: .agents/tasks/issue-030c-three-way-parity.md §2.2 (issue 30 §12's
-- "Removed 5.1-isms" blocker row: "implement them as a Lua-level 5.1 compat
-- prelude over Luau"). This file is the PROTOTYPE that classifies each of
-- the seven isms named in §12 — see the classification table in
-- `tests/lua51_compat_prelude.rs` (faithful / approximated / not
-- emulatable). It is Lua-level only: no Rust, no VM change; it runs in any
-- context BEFORE a 5.1-style script does.
--
-- FINDING OF RECORD (the reason this prototype differs from the plan's
-- assumption that "Luau removed the 5.1-isms"): it did not, at least not
-- in the version this port builds. Luau 0.663 (luau0-src 0.12.3, via
-- mlua 0.10.5) re-added most of the 5.1 surface with REAL semantics —
-- verified against the vendored source (luau/VM/src/lbaselib.cpp,
-- ltablib.cpp) AND the running VM:
--
--   * `unpack`    — NATIVE: the table library reinstalls it as a global
--                   ("Lua 5.1 compat", ltablib.cpp luaopen_table);
--   * `getfenv` / `setfenv` — NATIVE, with real environment semantics
--                   (lbaselib.cpp luaB_getfenv/luaB_setfenv: setfenv(f, t)
--                   re-binds the function's environment, setfenv(0, t) the
--                   thread's; errors on C functions — 5.1 behaviour); the
--                   Luau compiler even tracks getfenv/setfenv use
--                   (Compiler.cpp "inlining failed: module uses
--                   getfenv/setfenv");
--   * `newproxy`  — NATIVE (lbaselib.cpp luaB_newproxy: zero-size tagged
--                   userdata + empty metatable, like LuaJIT);
--   * `gcinfo`    — NATIVE as a GLOBAL FUNCTION on BOTH backends of this
--                   build (Luau: lbaselib.cpp luaB_gcinfo, integer KB via
--                   LUA_GCCOUNT; the vendored LuaJIT fork: a lib_base.c
--                   addition, `gc.total >> 10`) — but the 5.1
--                   `collectgarbage("gcinfo")` OPTION is absent on BOTH
--                   (mlua replaces Luau's collectgarbage with a wrapper
--                   that knows only collect/stop/restart/count/step/
--                   isrunning; and LuaJIT's own collectgarbage never
--                   implemented the option either).
--
--   The genuinely ABSENT constructs on this Luau build are:
--
--   * `loadstring` AND `load` — Luau's base library has no chunk
--     compiler at all (lbaselib.cpp base_funcs: no load/loadstring);
--   * `module` — absent (the 5.2-style package.module LuaJIT has; Luau
--     never had it).
--
-- Install contract:
--   * every shim is guarded by the target global's absence, so on any VM
--     that already has the construct the prelude installs nothing for it
--     (on this Luau: unpack/getfenv/setfenv/newproxy; on LuaJIT: all
--     seven except the gcinfo option);
--   * the `collectgarbage` wrapper (installed on BOTH backends — neither
--     implements the 5.1 "gcinfo" option) forwards every native option
--     unmodified;
--   * the `loadstring` line installs ONLY when a `load` function exists.
--     On this Luau build `load` is absent and NO Lua-level code can create
--     a chunk compiler — the mitigation is a ONE-LINE RUST-SIDE binding
--     (expose `mlua::Lua::load` as the global `load`; the exact line is
--     demonstrated test-locally in lua51_compat_prelude.rs, not shipped
--     here). Without that companion, `loadstring` stays absent: that is
--     the not-emulatable-at-Lua-level cell of the classification.

do
	-- 1. unpack — 5.1 global. NATIVE on this Luau build ("Lua 5.1 compat"
	-- re-install in the table library) and on LuaJIT; the guard keeps this
	-- a no-op there and provides the shim only where it is missing.
	if not rawget(_G, "unpack") then
		_G.unpack = table.unpack
	end

	-- 2. loadstring — the 5.1 string-source alias of load. Installs only
	-- when a `load` function exists (LuaJIT: native; a host that installed
	-- the Rust-side `load` binding on Luau). On a bare Luau build there is
	-- no chunk compiler in the language surface, so there is nothing to
	-- alias and the line stays inert (the classification's not-emulatable
	-- cell, pending the one-line Rust-side companion).
	if not rawget(_G, "loadstring") and type(load) == "function" then
		_G.loadstring = load
	end

	-- 3. gcinfo — wrap collectgarbage only on a VM whose collectgarbage
	-- lacks the option (the pcall probe). That is BOTH backends here:
	-- LuaJIT 2.1 never implemented "gcinfo", and on Luau mlua's
	-- collectgarbage wrapper knows only the Luau-native options. The
	-- wrapper forwards every native option unmodified ("collect" keeps
	-- LuaJIT's number return / Luau's nil return; no normalisation) and
	-- adds the 5.1 form: integer KB = floor(count). (The global gcinfo()
	-- function already exists natively on this Luau build — this wrapper
	-- covers the 5.1 collectgarbage-option access form, which is what the
	-- census pattern for third-party packs actually is.)
	local has_gcinfo = pcall(collectgarbage, "gcinfo")
	if not has_gcinfo then
		local base = _G.collectgarbage
		_G.collectgarbage = function(op, ...)
			if op == "gcinfo" then
				return math.floor(base("count"))
			end
			return base(op, ...)
		end
	end

	-- 4+5. getfenv / setfenv — NATIVE with real environment semantics on
	-- this Luau build (and native on LuaJIT): the guards keep them
	-- untouched. The fallback shims below exist only for a build where
	-- they are absent (the issue-30 §12 assumption): getfenv returns the
	-- only environment Luau has; setfenv honours the one target it can
	-- (the global environment — a faithful no-op) and REFUSES the rest
	-- with an explicit error, because silently ignoring a re-bind would
	-- resolve globals against the wrong table with no diagnostic.
	if not rawget(_G, "getfenv") then
		_G.getfenv = function(_f)
			return _G
		end
	end
	if not rawget(_G, "setfenv") then
		_G.setfenv = function(f, env)
			if env == _G then
				return f -- the one target an env-less VM can honour
			end
			error(
				"Luau 5.1 compat prelude: setfenv to a non-global environment is not emulatable (this build has no per-function global environments)",
				2
			)
		end
	end

	-- 6. module — ABSENT on this Luau build (present on LuaJIT as the
	-- 5.2-style package.module). The shim restores the 5.1 contract as
	-- far as the VM allows:
	--   (a) the module table exists at _G[name] (+ package.loaded when the
	--       VM has a package table);
	--   (b) the DEFINING-CHUNK redirect: when native setfenv exists (this
	--       Luau build has it), the shim re-binds the CALLING chunk's
	--       environment to the module table (setfenv level 3 as seen from
	--       inside the shim's pcall: 0 = Luau's C setfenv, 1 = the pcall,
	--       2 = the shim, 3 = the shim's caller — exactly what 5.1's
	--       module does to its caller). Function definitions in the
	--       defining chunk then land IN the module table, as in 5.1.
	--       (Same calling-contract caveat as 5.1 itself: a module() call
	--       nested inside another function re-binds THAT function.)
	--   (c) read side: the table carries __index = _G, so the defining
	--       chunk (and consumers) see module fields PLUS the globals.
	--       5.1's own module table self-indexes (own fields only) and
	--       LuaJIT's has no metatable at all — the __index = _G
	--       fall-through is a deliberate LENIENT superset: a defining
	--       chunk that reads a global after module() keeps working,
	--       instead of breaking on a nil global as it would under a
	--       strict 5.1 replica.
	-- The one un-closable gap (recorded, not hidden): without (b) — i.e.
	-- on a build that also lacks setfenv — the defining chunk keeps
	-- writing to _G and only the read side is emulated.
	if not rawget(_G, "module") then
		_G.module = function(name, ...)
			local existing = rawget(_G, name)
			if existing ~= nil and type(existing) ~= "table" then
				-- 5.1 contract: "module name (%s) conflicts with an
				-- existing non-table value" — matched, not invented.
				error(string.format("module name (%s) conflicts with existing non-table value", tostring(name)), 2)
			end
			local m = existing
			if m == nil then
				m = {}
				rawset(_G, name, m)
			end
			-- Read side (c): once per table.
			if rawget(m, "__030c_module_pre") == nil then
				setmetatable(m, { __index = _G })
				rawset(m, "__030c_module_pre", true)
			end
			-- package publish (5.1 did this when package existed).
			local pkg = rawget(_G, "package")
			if type(pkg) == "table" and type(rawget(pkg, "loaded")) == "table" then
				pkg.loaded[name] = m
			end
			-- Defining-chunk redirect (b): re-bind the CALLING chunk's
			-- environment to the module table when the VM has a native
			-- level-based setfenv (this Luau build does). The pcall IS the
			-- detector: the prelude's own fallback setfenv (installed on a
			-- build without one) REFUSES the non-_G target, and the
			-- C-function/short-stack edge cases error too — in every
			-- refusal the read-side emulation above is what remains.
			-- Level 3: 0 = Luau's C setfenv, 1 = this pcall, 2 = this shim,
			-- 3 = the shim's caller (the defining chunk) — the same target
			-- 5.1's module sets (its immediate caller).
			pcall(setfenv, 3, m)
			return m
		end
	end

	-- 7. newproxy — NATIVE on this Luau build (zero-size tagged userdata
	-- + empty metatable, like LuaJIT's) and on LuaJIT; the guard keeps
	-- this a no-op there. The fallback shim (only for a build that lacks
	-- it) returns a table pre-flagged with an inert __gc, because such a
	-- build's __gc (if any) must be callable where 5.2's flag is the
	-- literal `true`. The finalizer idiom that runs on all observed VMs
	-- is metatable mutation: getmetatable(newproxy(true)).__gc = fn
	-- (LuaJIT's own profiler's usage; the Lua-level setmetatable refuses
	-- the userdata on BOTH LuaJIT and this Luau).
	if not rawget(_G, "newproxy") then
		_G.newproxy = function(flag)
			local t = {}
			if flag then
				setmetatable(t, { __gc = function() end })
			end
			return t
		end
	end
end
