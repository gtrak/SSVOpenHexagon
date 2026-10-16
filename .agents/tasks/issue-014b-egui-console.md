# Task spec — issue 014b: the egui Lua console overlay (the last optional lane)

You are a worker. Implement exactly this. Do not redesign or widen scope.

## Why this exists

Issue 014 step (a) landed the console CORE (`crates/oh-game/src/lua_console.rs`:
the `ilc*` state, the headless command interpreter, completion/history pure
functions, the docs) with the UI explicitly deferred. This lane adds the
windowed overlay. It is a dev tool (debug-gated, window-gated — D-022/D-023);
pixel layout need not match, behaviour must.

## What exists (verified — read first)

- D-022 (`DECISIONS.md`): egui 0.37 + egui-winit 0.37 + egui_glow 0.37 verified
  via a real scratch build to coexist with the D-011 stack (winit 0.30.13).
  **Caveat**: egui_glow 0.37 pulls glow 0.17 while `oh-render` uses glow 0.18 —
  different Rust types over the same GL context; give the console its own
  glow-0.17 context/loader (or bridge) — do NOT pass `oh-render`'s context.
- `crates/oh-game/src/lua_console.rs` — `interpret_command`, `complete`,
  `history`, `track_code`/`untrack_all`/`update_tracked_results`, the state
  readers on `HexagonGame` (`ilc_*` getters + setters), and
  `imgui_lua_console_has_input` (its `wantCapture*` conjunct is THIS lane).
- `crates/oh-app/src/windowed.rs` — the loop (`tick_fields`, per-frame order),
  `post_update` driving, the debug/official gates, and the input manager
  (`Tid::LuaConsole` → `set_ilc_show_console_next`, landed in 010b).
- C++ oracle (D-014 verified against the submodule at the final commit):
  `HGUpdate.cpp:1287-1646` (`postUpdate_ImguiLuaConsole` — the window /
  `ImGui::SFML::Update` tail; the UI tail itself is `:1314-1643`),
  `HGGraphics.cpp:318-336` (`drawImguiLuaConsole` — the console's GL paint),
  and `HexagonGame.cpp:270-274` (`imguiLuaConsoleHasInput`). (The WHOLE UI —
  cmd log with per-tag colours (`HGUpdate.cpp:1335-1363`), the InputText with
  completion/history callbacks (`HGUpdate.cpp:1118-1285`), Track field +
  Untrack All, the update/draw ms + Timescale DragFloat + Invincible
  Checkbox — lives in `postUpdate_ImguiLuaConsole`, not `HGGraphics.cpp`.)

## Scope

1. **(a) The overlay crate/module**: egui integration in the windowed path —
   egui-winit consumes events (coexisting with the game's input: the
   `imguiLuaConsoleHasInput` gate gains the real `wants_keyboard_input() ||
   wants_pointer_input()` conjunct so the game ignores input while the console
   has focus), egui_glow paints after the frame's game draws. Where it lives:
   `oh-app` (it is composition, like the bridge) — NOT `oh-game` (headless must
   stay free of egui). The state is the engine's `ilc*` fields via the landed
   getters/setters. Commit when a window-compiling increment is green.
2. **(b) The UI, behaviour-1:1** (citing the C++ for each): the
   `Lua Console` window (600x700 first-use), the scroll region with the log
   lines (the per-tag colours: `[error]` red, `[warning]` magenta, `[lua]`
   green, `# ` orange, `[?]` blue — `HGGraphics.cpp` cite), the InputText
   bound to `ilc_cmd_buffer` with Enter → the landed
   `post_update_imgui_lua_console_command` + completion/history wired to the
   landed pure functions (Tab completion + Up/Down history — the egui
   TextInput equivalents, cited), the Track field + Untrack All button
   (`ilc_track_buffer`/`ilc_track`), and the footer: update/draw ms,
   the Timescale DragFloat → `Config::setTimescale` analogue, the Invincible
   Checkbox. The F1/tilde toggle already works via 010b's trigger; the toggle
   swap fires the C++'s `ProcessEvent(GainedFocus)` equivalent — document what
   you do (egui manages focus; likely a no-op — cite).
3. **(c) Tests**: headless-testable pieces only (the egui layer itself is
   MANUAL — document the check: run with debug on, F1, type `!help` + a bad
   Lua expr + Tab completion). Assert: the input gate flips
   (`imgui_lua_console_has_input` true when the console has focus — unit-test
   the conjunct with a fake), the command path still routes through the landed
   interpreter (a headless test asserting the overlay's command plumbing calls
   it), and headless determinism is untouched (the console never runs headless:
   the `window == nullptr` gate — existing tests must stay green).
4. Final: full `bash tools/gate.sh` (4 stages, print `GATE_EXIT=<rc>`, never
   pipe to tail), `cargo test --workspace`,
   `cargo test --workspace --features oh-game/produce-lua-metadata`, STATUS
   row 14 → fully LANDED (14a+14b) in your final commit.

## Process rules

Budget 90 minutes. egui integration has sharp edges (paint ordering, the
glow version split, event routing) — compile+commit every ~50 lines; if the
egui_glow paint path fights back >30 min, land the egui-winit event layer +
the input gate + the UI logic driving the landed interpreter (a working
console with the game's own glyph rendering is NOT acceptable — if egui
painting truly cannot land, stop, commit the partial event/gate layer with
`TODO(014b-paint)`, and report). clippy `-D warnings` fixed immediately per
commit; verify every citation (D-014).

## Out of scope

`crates/oh-net/**`, `crates/oh-parity/**` (parallel 023 lane), the simulation
order, the D-019 fix, the determinism harness, headless behaviour (must stay
byte-identical — the console is window-gated and debug-gated).
