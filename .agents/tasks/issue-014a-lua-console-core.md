# Task spec — issue 014 (step a): Lua console CORE (docs + metadata flag + headless console state machine)

You are a worker. Implement exactly this. Do not redesign or widen scope.
**No egui, no window code, no `oh-app` changes** — those are step (b)/issue 15.

## Why this task exists (read this first)

The 014 contract (`.agents/plans/001-rust-port/14-lua-console.md`) was written
before four source findings, all recorded in `DECISIONS.md` — read **D-021,
D-022, D-023, D-024** first:

1. **D-023(a):** the console belongs to `HexagonGame`, NOT `MenuGame`. The
   contract's `crates/oh-menu/src/lua_console.rs` file row is wrong; the port
   lives in `crates/oh-game` (+ `oh-lua` for docs). Do not touch `oh-menu`.
2. **D-023(b):** the C++ metadata pipeline is behind
   `#ifdef SSVOH_PRODUCE_LUA_METADATA`, and `CMakeLists.txt:11-17` sets it via a
   literal `if(FALSE)` — **OFF in the shipped build**. With it off,
   `LuaMetadataProxy::~LuaMetadataProxy` is an empty body
   (`LuaMetadataProxy.cpp:160-179`), so `addFnEntry` is never called: the
   metadata store has ZERO entries, `printDocs` prints the 11 category headers
   and nothing else, `getAllFunctionNames()` returns an empty vector, and
   `getDocsForFunction` returns `"UNKNOWN FUNCTION"` for every input.
   `_RELEASE/luadocs.md` is a stale 2021 capture from a flag-on build (D-021) —
   **never** pin output against it.
3. **D-024:** the Rust port currently ALWAYS records metadata
   (`crates/oh-lua/src/metadata.rs:279-286` — the flag-ON behaviour). This task
   makes the flag a **cargo feature** so the default build matches the shipped
   C++ exactly.
4. **D-022:** the egui overlay (step b) is deferred. egui 0.37 is verified
   compatible with the D-011 stack (winit 0.30), with a glow-0.17-vs-0.18 caveat
   — that integration work lands with issue 15's window wiring, not here.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`, branch `master`, HEAD `0e89d57c`, tree
clean, `bash tools/gate.sh` 4/4 green (306 tests).

Read first:
1. `.agents/plans/001-rust-port/DECISIONS.md` — D-021, D-022, D-023, D-024.
2. C++ sources (the oracle):
   - `src/SSVOpenHexagon/Core/LuaScripting.cpp:1600-1690` — `printDocs`,
     `getAllFunctionNames`, `getDocsForFunction` (incl. the `"UNKNOWN FUNCTION"`
     fallback).
   - `src/SSVOpenHexagon/Core/HGUpdate.cpp:1148-1286` — `ilcTextEditCallback`
     (completion + history; the ONLY two callback cases).
   - `src/SSVOpenHexagon/Core/HGUpdate.cpp:1287-1631` —
     `postUpdate_ImguiLuaConsole` (window/debug gates, toggle, command
     `InputText` at `:1425`, interpretation `:1443-1545`, tracked variables
     `:1562-1631`).
   - `src/SSVOpenHexagon/Core/HGScripting.cpp:1149-1153` —
     `initLuaAndPrintDocs`.
   - `src/SSVOpenHexagon/Core/HexagonGame.cpp:411-419` — the `Tid::LuaConsole`
     trigger (debug-gated `ilcShowConsoleNext = true`).
   - `include/SSVOpenHexagon/Core/HexagonGame.hpp:106-121` — the `ilc*` state.
   - `src/SSVOpenHexagon/Utils/LuaMetadataProxy.cpp` — the flag-gated pipeline.
3. Rust: `crates/oh-lua/src/metadata.rs` (the collector + `PREFIX_HEADERS`),
   `crates/oh-lua/tests/runtime.rs:645-691` (the one test reading the store),
   `crates/oh-game/src/hexagon_game.rs:2053` + `:2054` (the already-landed
   `imgui_lua_console_has_input` + `ilc_show_console`), `update.rs:473`
   (`post_update` placeholder), `update.rs:594` (the input gate).

## Scope (what to implement)

### 1. `oh-lua`: cargo feature `produce-lua-metadata`, default OFF

Mirror `CMakeLists.txt`'s `if(FALSE)` as a cargo feature:

- Add `[features] produce-lua-metadata = []` to `crates/oh-lua/Cargo.toml`
  (NOT in default features).
- Gate the `Drop for LuaMetadataProxy` recording on it:
  `#[cfg(feature = "produce-lua-metadata")]` records exactly as today;
  the `#[cfg(not(...))]` drop body is empty (the C++ `#else {}` at
  `LuaMetadataProxy.cpp:176-178`). `arg()`/`doc()` keep accumulating on the
  proxy in both modes — only the final `add_fn_entry` is gated (this matches
  C++, where `arg()`/`doc()` bodies are no-ops but the proxy still exists).
- `crates/oh-lua/tests/runtime.rs`'s
  `metadata_entries_recorded_with_docs_and_args` (`:651`) asserts flag-ON
  behaviour: gate it `#[cfg(feature = "produce-lua-metadata")]` and add a
  default-mode sibling test asserting the store stays EMPTY after registering
  functions (clear it first as `:648-650` does — tests share the process-global
  store).
- The 285-name test (`api.rs`) uses a hardcoded array, NOT the store — verify
  that claim yourself, it must keep passing in both modes.

### 2. `oh-lua/src/docs.rs`: port `LuaScripting.cpp:1600-1690`

- `get_all_function_names() -> Vec<String>` — port of `getAllFunctionNames`
  (`:1632-1652`): category order 0..NUM_CATEGORIES, entry order within category.
- `get_docs_for_function(fn_name) -> String` — port of `getDocsForFunction`
  (`:1654-1688`): first matching entry (category order, then entry order),
  formatted `"{ret} {name}({args}):\n{docs}\n\n"`, else `"UNKNOWN FUNCTION"`.
- `print_docs(out: &mut impl Write)` — port of `printDocs` (`:1613-1630`) to a
  writer (testability; C++ writes to `std::cout`). Byte-exact format per
  entry: `"\n" + prefixHeaders[i] + "\n\n"` then per fn
  `"* **`" + ret + " " + name + "(" + args + ")`**: " + docs + "\n\n"`.
  Provide `print_docs_to_stdout()` for the future CLI (issue 15).
- Unit tests: default mode → 11 headers, zero entries, empty name list,
  `"UNKNOWN FUNCTION"`; feature-on mode → entries present. Clear the global
  store before populating (same-process hazard as above).

### 3. `oh-game`: the console state + headless command interpreter

Add the `ilc*` fields to the engine state, mirroring
`HexagonGame.hpp:106-121` field-for-field (names snake_cased, same defaults —
`ilc_history_pos: i32 = -1`): `ilc_delta_clock` (see note), `ilc_cmd_log`,
`ilc_history`, `ilc_history_pos`, `ilc_cmd_buffer`, `ilc_track_buffer`,
`ilc_show_console` (already exists at `hexagon_game.rs:310` — keep it),
`ilc_show_console_next`, `ilc_lua_tracked`, `ilc_lua_tracked_names`,
`ilc_lua_tracked_results`. `ilc_delta_clock` is `sf::Clock` used only by the
windowed draw path — model it as whatever the port's clock helper provides
(`clock.rs`); if nothing needs it headless, keep the field with a comment
saying the read sites are step (b).

Port the command logic of `postUpdate_ImguiLuaConsole`
(`HGUpdate.cpp:1408-1545`) as a headless-testable method —
`post_update_imgui_lua_console_command(&mut self, cmd: &str)` or a small
struct; keep C++ control flow exactly, including:

- `!CLEAR` / `!HELP` (the exact help text block, `:1452-1464`, verbatim),
- `!ff <seconds>` → parse `std::stod`-equivalent (`f64`), set
  `fast_forward_target`, the two distinct error strings
  (`invalid argument for <seconds>` / `out of range for <seconds>`),
- `!advt <ticks>` → `std::stoi`-equivalent (`i32`),
  `advance_tick_count = ticks >= 0 ? ticks : 0`, same two error strings,
- `?fn` → r-trimmed `get_docs_for_function`, log `"[?]: " + docs`,
- fallback: `u_log(<cmd>)` first, retry bare `<cmd>` on error, catch into
  `"[error]: " + what + "\n"` / `"[error]: unknown\n"`.
- History push happens BEFORE interpretation (`:1443` —
  `ilcHistory.emplace_back(cmdString)` precedes the split/dispatch at `:1445`);
  push unconditionally, even for `!CLEAR`.
- Tracked variables (`:1562-1631`): track/untrack-all + the execute-and-erase-
  on-error loop with its exact error strings, and the results gathering.

Port `ilcTextEditCallback` (`HGUpdate.cpp:1148-1286`) as pure functions over
the state (no ImGui): completion (word-start scan over `' ' '\t' ',' ';'`,
skip a leading `?`, case-insensitive prefix match via `Strnicmp` semantics →
0/1/many branches: no-match log line `"No match for \"...\"!\n"`, single
replace + trailing `" "`, many → longest-common-prefix completion + the
`"Possible matches:\n"` + `"- {name}\n"` log lines) and history
(Up/Down with the `-1` sentinel semantics, `:1250-1272`). Model the
`ImGuiInputTextCallbackData` effect on the buffer as explicit
delete/insert operations your function returns or applies, so it is unit-
testable; keep the C++ ordering of buffer mutations.

The gates: extend `post_update` (`update.rs:473`) to port the
`postUpdate_ImguiLuaConsole` PRE-UI prefix faithfully — the `window == nullptr`
return (in the port: the headless engine has no window, so document that the
UI tail is step (b), but the debug/official gate
`!Config::getDebug() || Config::getOfficial()` →
`ilc_show_console = ilc_show_console_next = false` MUST run, and the
`ilc_show_console_next` toggle swap `ilcShowConsole = !ilcShowConsole;
ilcShowConsoleNext = false` must run). Add the debug-gated
`Tid::LuaConsole`-equivalent setter (`HexagonGame.cpp:411-419`) as a small
public method the input wiring (issue 15) will call — do NOT build an input
system here. `imgui_lua_console_has_input` (`hexagon_game.rs:2053`): keep its
current behaviour, add a comment that the `wantCaptureKeyboard/Mouse` conjunct
is the step-(b) egui piece.

### 4. `initLuaAndPrintDocs` + the printLuaDocs mode

`HGScripting.cpp:1149-1153` is two lines: `initLua(); printDocs();`. Expose
the same on the port's engine/script-host seam. The `-printLuaDocs` CLI flag
itself is issue 15 (no `oh-app` yet) — provide the function + a headless test
that mirrors `mainPrintLuaDocs` (`main.cpp:167-189`) closely enough to assert:
default mode → output contains the 11 `prefixHeaders` strings in order and
ZERO `* **\`` entry lines; feature-on mode → entry lines present and
`u_rndReal`-style lookups resolve.

## Verification (required, in this order)

1. **Citations:** every C++ line reference above re-verified by you against the
   real source before you rely on it (D-014). If one is off, fix the spec in
   your commit and say so.
2. **Feature matrix:** `cargo test -p oh-lua` (default) AND
   `cargo test -p oh-lua --features produce-lua-metadata` both green; same for
   `oh-game`. The gate (`tools/gate.sh`) runs defaults only — it must stay 4/4.
3. **Discrimination:** prove the feature actually flips behaviour — with the
   feature off, `print_docs` output has no `* **` lines and
   `get_docs_for_function` returns `"UNKNOWN FUNCTION"`; with it on, both are
   populated. This must be an asserted test pair, not a manual check.
4. **Fidelity check on the interpreter:** for each command branch, one test
   with the C++-expected log strings (copy them from the source, including the
   trailing `\n`s).
5. Full `bash tools/gate.sh` (all 4 stages, print `GATE_EXIT=<rc>`) on your
   tree, then a clean-checkout gate:
   `rm -rf /tmp/v14a && mkdir -p /tmp/v14a && git archive HEAD | tar -x -C /tmp/v14a && cd /tmp/v14a && CARGO_TARGET_DIR=/tmp/v14a/target bash tools/gate.sh`.
6. Update `.agents/plans/STATUS.md` row 14 (state SPECD→in-flight→LANDED per
   stage, commit hash, notes saying "step (a) core landed; egui overlay = step
   (b) with issue 15"). Commit with a message referencing issue 014 step (a).

## Out of scope (do NOT do)

- Any egui/ImGui/window/drawing code (step b, with issue 15).
- The `-printLuaDocs` CLI flag in a binary (issue 15 owns `oh-app`).
- Changes to the 285-name API surface, replay paths, or `set_level_data`
  behaviour. The D-019 fix and the determinism harness are untouchable.
- New dependencies in `oh-game` (egui must NOT be added to `oh-game` — that
  would break headless capability; docs live in `oh-lua`).

## PROGRESS NOTE (orchestrator, post-timeout of run 046e3463)

Scope items 1 and 2 are DONE and committed at `b85f7004`: the cargo feature
`produce-lua-metadata` (default off) with the gated `Drop`, `docs.rs`
(`print_docs`/`get_all_function_names`/`get_docs_for_function`), and the
discrimination test pair — both modes 30/30 green, workspace 309 passed.
A resumed worker picks up at **scope item 3 (oh-game console core) and item 4
(initLuaAndPrintDocs)**, plus the STATUS.md row update. Do not redo item 1/2;
read `b85f7004` first. Two lessons from the timed-out run: (a) NEVER hold the
`get_metadata()` guard across a `docs::*` call — the store lock is
non-reentrant and it self-deadlocks (already fixed in `runtime.rs`); (b) run
`cargo test -p <crate>` for EACH mode right after writing tests, before moving
to the next file, and commit early.
