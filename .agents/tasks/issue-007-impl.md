# Task spec — issue 007: `oh-lua` game API (all globals + callbacks)

You are a worker. Implement exactly this issue. Do not redesign or widen scope.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port. Issues 001–006 and
008 are LANDED; `bash tools/gate.sh` passes 4/4. `crates/oh-lua` provides
`LuaContext` (`execute_code`, `read_variable`, `write_variable`, `add_lua_fn`,
`LuaMetadata`).

This issue registers the **complete Lua API** — the content contract that every
shipped pack depends on.

Read first:
1. `.agents/plans/001-rust-port/PLAN.md`, `DECISIONS.md` (esp. D-009, D-010)
2. `.agents/plans/001-rust-port/07-lua-game-api.md` (the contract)
3. `.agents/plans/001-rust-port/mapping/lua-api.md` — **THE AUTHORITATIVE
   FUNCTION TABLE**: 285 names across `u_`/`a_`/`t_`/`e_`/`l_`/`s_`/`w_`/`cw_`/
   `ct_`/`shdr_`/`m_`/`steam_`, including all 72 accessor-generated getters and
   setters. §1 is the table, §2 the exact spellings, §3 conversions, §4 RNG, §5
   callbacks, §6 the include mechanism, §7 corpus usage, §8 the subtleties.
4. The real C++: `src/SSVOpenHexagon/Core/HGScripting.cpp` (79 registrations),
   `src/SSVOpenHexagon/Core/LuaScripting.cpp` (the rest, incl. the
   `makeLuaAccessor` calls), `include/.../Core/LuaScripting.hpp`
5. `.agents/skills/lib-luajit.md`, `lib-mlua.md`, `lib-gameplay.md`

## Working agreement

- **Write first**: create `crates/oh-lua/src/api/*.rs` skeletons and a registry
  immediately, compile early, iterate.
- **The API surface is a contract.** Every name and arity must match. Re-derive the
  name set from source (do not trust the mapping doc's count — confirm it) and
  make the test assert the **exact set**, so a missing or extra name fails.
- **Game state**: this crate must not depend on the not-yet-existing game loop.
  Define a trait/struct interface for the state the API touches (level status,
  style data, RNG, walls, audio, timelines, messages) with the exact C++ semantics;
  issue 009 supplies the real implementation. Keep the interface minimal but
  faithful (the C++ lambdas' exact behavior is the spec).
- **Deferred functions** (`w_*`, `t_*`, `e_*`, `ct_*`): the mapping doc §8.5
  documents that these enqueue deferred actions rather than acting immediately —
  reproduce that.
- **Commit at the first green build.**

## What to build

1. The registry + metadata wiring (`add_lua_fn` + `LuaMetadataProxy`), one module
   per C++ init group.
2. All `u_*` (incl. the 5 RNG functions, `u_execScript`,
   `u_execDependencyScript`, `u_impl_addTrackedResult`, the deprecated ones).
3. All `l_*` (direct + the 43 `LevelStatus` accessor pairs + the 6
   `HexagonGameStatus` pairs), `s_*` (incl. the 23 `StyleData` pairs and the
   `HueInc`/`PulseInc` backward-compat aliases), `a_*`, `m_*`, `e_*`, `t_*`,
   `ct_*`, `cw_*`, `shdr_*`, `w_*`, `steam_*`.
4. The callback dispatch (`onInit`, `onLoad`, `onUpdate(FT)`, `onStep`,
   `onInput(FT,int,bool,bool)->bool`, `onPreDeath`, `onDeath`, `onIncrement`,
   `onPreUnload`, `onUnload`, `onCursorSwap`, `onRenderStage(int,float)->int`)
   with the exact argument types and veto semantics.
5. `u_execScript` / `u_execDependencyScript` and the exec-script pack-path context.
6. The menu-context registration behavior (stubs for `u_log`, pinned getters,
   the no-op list) as documented in the mapping doc §8.10.

## Verification

1. `bash tools/gate.sh` — paste all `GATE_EXIT=` lines.
2. **Exact-set test**: register the full API on a fresh context and assert the set
   of global names equals the source-derived list (fail on any missing/extra).
   Print the count.
3. An arity test: a wrong-arity call produces a Lua error, not a panic.
4. **Pack smoke test**: with a mock state, load each
   `_RELEASE/Packs/base/Scripts/*.lua` through the dependency resolution and assert
   no error.
5. `u_rndInt`/`u_rndReal`/`u_rndSwitch` consume the injected RNG in the exact order
   (assert against the `oh-rng` oracle values).

## Report format

Module map, the name-set count (with the source command used to derive it), gate
outputs, skill corrections, deviations, known gaps.
