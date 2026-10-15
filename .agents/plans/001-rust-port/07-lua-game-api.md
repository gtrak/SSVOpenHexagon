# Issue 07 — `oh-lua` game API: all globals + callbacks

## Objective

Register the **complete** Lua API surface from `HGScripting.cpp` (79 functions)
and `LuaScripting.cpp` (61 functions, including all `makeLuaAccessor`-generated
getters/setters), with exact names, argument types/order, return types, semantics,
and doc strings — plus the script-callback dispatch (`onInit`, `onUpdate`,
`onInput`, `onIncrement`, `onSwap`, `onDeath`, `onEvent`, `onMessage`, ...) and the
script include mechanism (`u_execScript`, `u_execDependencyScript`).

## Key decisions

- The full function table is authoritative in
  `.agents/plans/001-rust-port/mapping/lua-api.md` (transcription worker output) —
  **re-derive the count from source** before relying on any number in it.
- `makeLuaAccessor` generates `*_get<Name>` / `*_set<Name>` pairs from a struct
  member pointer + doc strings; in Rust, express these as explicit registrations
  (a macro or a table) — enumerate them individually, do not hand-wave.
- Callbacks are invoked via `Utils::runLuaFunctionIfExists`; port that helper and
  the `luaExceptionLippincottHandler` warning behavior.
- `execScriptPackPathContext`: the C++ keeps a stack of pack-path contexts for
  dependency resolution; port exactly.
- The Lua API is a **content contract**: every existing pack must run unmodified.

## Files

| File | Change |
|---|---|
| `crates/oh-lua/src/api/*.rs` | new — one module per C++ init group (utils, audio, timelines, level, style, walls, steam, custom walls, deprecated, random) |
| `crates/oh-lua/src/callbacks.rs` | new — callback dispatch |
| `crates/oh-lua/src/docs.rs` | new — metadata collection + `printDocs`/`getAllFunctionNames`/`getDocsForFunction` |
| `crates/oh-lua/tests/api_surface.rs` | new — assert every name is registered with the right arity |
| `.agents/skills/lib-lua-api.md` | new — the API contract, generated from the mapping doc + source |

## Steps

1. Port `initLua_*` groups function-by-function from `HGScripting.cpp`.
2. Port the `LuaScripting.cpp` groups: `u_*` (utils, exiting in 06 for random),
   `l_*` (level), `a_*` (audio), `m_*`/`e_*`/`t_*`/`ct_*` (messages/events/timelines/
   custom timelines), `s_*` (style), `cw_*` (custom walls), `shdr_*` (shaders),
   `w_*` (wall creation), `steam_*`.
3. Port every `makeLuaAccessor` accessor explicitly (enumerate them).
4. Port callback dispatch + `runLuaFunctionIfExists`.
5. Port `u_execScript` / `u_execDependencyScript` + pack-path resolution.
6. Port the deprecated-function warnings (`calledDeprecatedFunctions`,
   `raiseWarning`).

## Verification

- A test that registers the full API on a fresh context and asserts the exact set
  of global names equals the source-derived list (fail on any missing/extra).
- A test that arity mismatches produce a Lua error, not a panic.
- A **pack smoke test**: `executeCode` each `_RELEASE/Packs/base/Scripts/*.lua`
  (with the dependency resolution working) and assert no error.
- `tools/gate.sh` green.

## Notes / risks

- Some functions depend on game state (issue 08/09) — define a trait/struct
  interface so `oh-lua` can be compiled and tested with mocks before the game loop
  exists; the real wiring is issue 09.
- Float/double crossing: `u_getDifficultyMult()` returns `float`; `u_rndReal`
  returns `float`; Lua numbers are doubles. Match the C++ conversion exactly.
- This is the highest-risk issue for content compatibility — the gate must compare
  the registered-name set against a list **derived from source**, not from prose.
