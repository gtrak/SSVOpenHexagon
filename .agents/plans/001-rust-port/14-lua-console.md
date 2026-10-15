# Issue 14 — Lua console overlay + `-printLuaDocs`

## Objective

Port the ImGui-based Lua console (`postUpdate_ImguiLuaConsole`, `drawImguiLuaConsole`,
the `ilc*` state machine, `ilcTextEditCallback`, tracked-variable display) and the
`-printLuaDocs` mode (`LuaScripting::printDocs`).

## Key decisions

- ImGui analogue (D-004 ImGui entry): `egui` (with `egui-winit` + `egui_glow`) is
  the primary candidate; assess the multi-line `InputText` with callback + tab
  completion before committing (`misc/cpp/imgui_stdlib` is used for `std::string`
  inputs).
- The console is a developer tool, not a content contract — exact pixel layout is
  not required, but **behavior** (command history, tracked variables, tab
  completion, error display, F1 toggle) must match.
- `-printLuaDocs` output format must match (it writes `luadocs.md`); pin against the
  existing `_RELEASE/luadocs.md` if present.

## Files

| File | Change |
|---|---|
| `crates/oh-menu/src/lua_console.rs` | new |
| `crates/oh-lua/src/docs.rs` | extend — `print_docs`, `get_all_function_names`, `get_docs_for_function` |
| `.agents/skills/lib-egui.md` | new — from the crate's real source |

## Steps

1. Port the console state (`ilcCmdLog`, `ilcHistory`, `ilcHistoryPos`,
   `ilcCmdBuffer`, tracked vars) and its input handling (up/down history, enter,
   tab completion via `LuaMetadata`).
2. Port `ilcTextEditCallback` semantics.
3. Port `postUpdate_ImguiLuaConsole` (evaluate input, run it in Lua, capture
   errors/output, update tracked results).
4. Port `printDocs` and the function-name/doc lookup from `LuaMetadata`.

## Verification

- Test `-printLuaDocs` output equals the expected doc list (function count derived
  from source; each function's doc string present).
- A scripted console test: feed a Lua expression, assert the result string and that
  an error is displayed for a bad expression.
- `tools/gate.sh` green.

## Notes / risks

- The C++ has `#ifndef SSVOH_ANDROID` guards around ImGui; the Rust console can be
  always-on or feature-gated — pick and document.
