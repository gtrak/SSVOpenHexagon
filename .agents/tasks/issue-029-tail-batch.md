# Task 029-tail-batch — the last four 29 rows (29v1, 29g1, 29w1, 29q1)

Read `.agents/WORKER-HYGIENE.md` first. Gate green at **908+** (check the current
baseline). **No worktree** (D-035). One commit per item.

These close out issue 29's fix queue apart from K6/`29n1` (the pack-shader
subsystem, which is its own lane).

---

## 1. 29v1 — sweep every spliced/embedded Lua snippet, and make the class uncatchable

**Why this exists:** `029m`'s 29s1 found that `l_overrideScore`'s spliced
validation string carried an **extra `)`** — `if (type(x) ) ~= "number") then` — a
**Lua syntax error on every call**, so custom score override never worked. The C++
splices `if (type(<var> ) ~= "number") then` (`HGScripting.cpp:630-634`). It was
found by accident, while writing an unrelated test.

That is a whole class the module audits could not see: **hand-transcribed Lua
source embedded in Rust**. The lua pair audit checked the 207 registered globals'
*behaviour*; it cannot catch a typo inside a snippet.

**Do:**
- Find **every** embedded/spliced Lua string that gets loaded or executed:
  validation snippets per API function, and the sandbox snippets
  (`redefineIoOpen`, `redefineRandom`, `destroyMaliciousFunctions` — the latter is
  a list of clears, so check how it is built), plus anything in `oh-game`/`oh-lua`
  that hands a string to the Lua context. Search for multi-line string literals
  containing Lua keywords (`if `, `then`, `return`, `function`, `local`), and for
  `format!`/`+`-built snippets.
- **Diff each against the C++ verbatim** (the originals are in
  `src/SSVOpenHexagon/Core/LuaScripting.cpp`, `Utils/LuaWrapper.cpp`, and the
  per-function registration sites). Report any other divergence you find — a second
  instance of this bug class is a realistic outcome.
- **Add a compile test per snippet** — load it in a `LuaContext` and assert it
  compiles (no call needed). That is the mechanical guard: it catches this entire
  class, including future edits. Prefer a table-driven test so adding a snippet
  without a test is obvious.

## 2. 29g1 — the Lua error surface

Three items (evidence: `.agents/audits/lua.md` MISSING #1–#3, DIFFERENT #1):

- **`shdr_getDependencyShaderId`** — C++ **aborts the running script** and logs
  `"Fatal error while looking for Lua dependency"` when the
  (disambiguator, name, author) triple fails validation
  (`Utils.cpp:116-160`, `:177-207` → `LuaScripting.cpp:1267-1311`). The port's
  `if let Ok(dependency)` (`crates/oh-lua/src/api/shaders.rs:78`) **silently
  returns `size_t(-1)`** and lets the script continue. Port the abort + the log
  (the sibling `u_execDependencyScript` propagates its error correctly, so only
  this path deviates — follow that one's pattern).
- The two missing diagnostic log lines: the `u_execDependencyScript` failure path,
  and the `redefineIoOpen`/`redefineRandom` catch-block lines
  (`LuaScripting.cpp:150-153`, `:171-174`) — the port currently ends in an
  `expect()` panic with no equivalent diagnostic.
- **`cw_setVertexColor*` out-of-range channels**: the C++ wraps mod-256
  (`sf::Color`; `LuaScripting.cpp:326`, `:374`, `:405`). The port is
  **internally inconsistent**: the single setter → 255, the `4`/`4Same` variants →
  `clamp(0,255)` (`crates/oh-lua/src/api/custom_walls.rs:141-144`, `:391-392`).
  Make all three match the C++'s wrap.

## 3. 29w1 — the credits-bar textures still share the 1×1 placeholder

`29b8` ported the credits-bar texture **cycling** (state-only), and flagged that
the three credits textures still resolve to the **1×1 white placeholder** — the
same class `29e1` fixed for the key/particle textures (it also noted "the 15c
texture table").

**Do:** route the credits textures through the same asset load `29e1` established,
and add the **same guard**: assert the loaded dimensions are not 1×1, so a
placeholder regression cannot come back silently. Find the C++'s credit-bar
texture ids (`.agents/audits/menu.md` item #10 and `MenuGame.cpp:2848-2850`) and
mirror the cycling order.

## 4. 29q1 — the citation/comment drift sweep

**D-014 requires verified citations**, and every audit reported wrong or drifted
ones. Fix each against the source (re-derive, do not trust the audit's own line
numbers):

- `crates/oh-app/src/windowed.rs:2431-2434` — claims the C++ leaves a null game
  state and crashes on `-p/-l`; true only of the C++'s *failure* path (the success
  path sets it at `main.cpp:446-448`). **Check whether the 29b4 lane already fixed
  this** (`git log -p --follow` that hunk) before redoing it.
- `crates/oh-menu/src/real.rs:74` / `:204` — cite `MenuGame.cpp` 5545/5494 for
  calls that are actually at 5470/5570.
- `crates/oh-render/src/graphics.rs:352-355`; `crates/oh-app/src/lua_console_ui.rs:384-385`.
- `crates/oh-game/src/update.rs:1497-1498` — says the Discord half is "issue 21
  (absent)" while the Discord retry IS implemented at `:1519-1535` (the file's own
  second doc block contradicts it).
- `crates/oh-lua/src/api/style_control.rs:398-401` — claims the C++ uses
  `std::vector::at`; it uses `ssvu::getByModIdx` (`StyleData.cpp:320-323`). The
  behaviour is ported correctly (mod-wrap) but the `ok_or` error branch is then
  dead code — say so.
- the three `a22_swapdeath` citations each off by +1; and the three drifts the
  menu audit listed.

**Then:** sweep for the same class rather than fixing only the named instances —
grep for `// C++` / `// issue` comments citing a file:line and spot-check ones in
the files you touch. Report the count you checked and how many were wrong.

---

## Hygiene
- One commit per item, naming the row id. **Stage explicit paths — never
  `git add -A`** (shared checkout + index).
- `cargo fmt --all` before committing; gate green after each item
  (`bash tools/gate.sh`).
- Refresh `deviations.md` rows (issue 28's rule); do not rewrite
  `.agents/audits/*.md`. **When code and audit disagree, code wins** — and say so.
- The phone's ADB is still down: no device work, nothing here needs it.
- Append a `## Report`: per item — the fix, the citation you verified, the tests,
  and what you could not establish.

## Report

**29v1** (commit `49c23991`) — Fix: every executable embedded Lua string now lives in ONE place (`crates/oh-lua/src/snippets.rs`: `REDEFINE_IO_OPEN`, `REDEFINE_RANDOM`, `OVERRIDE_SCORE_VALIDATION`, the `{variable}` splice point = the C++'s `mVar`), and a table-driven test compiles every row through the C++ `executeCode` path (load + pcall, chunk `"chunk"`, no `LuaError::SyntaxError`); the negative half pins 29s1's extra-`)` splice as a `SyntaxError` so the guard has teeth. The sweep: those three snippets are the COMPLETE executable-embedded set — every other `executeCode` call site in the C++ tree is user input (the console, HGUpdate.cpp:1518/1522) or file contents (`runLuaCode`/`runLuaFile`, HGUpdate.cpp:1595, Utils.cpp:29/:95); `destroyMaliciousFunctions` is a fixed clear sequence, mirrored 1:1 incl. the double `package.loadlib` clear. One residual divergence found + fixed: the `l_overrideScore` splice carried 9 tabs where the C++ carries 8 (byte-identical now). No second 29s1-class bug. Citations verified: LuaScripting.cpp:144-147 / :159-168 / :178-219, HGScripting.cpp:635-638, HGUpdate.cpp:1518/1522/:1595, Utils.cpp:29/:95. Tests: `snippets.rs` `every_embedded_snippet_compiles` + `the_29s1_extra_paren_splice_does_not_compile`. Not established: nothing host-relevant (mlua 0.10.5 has no LuaJIT compile-only handle — the guard asserts on the `execute_code` result, which is the C++-faithful assertion for these snippets since the C++ itself executes them at init/call time).

**29g1** (commit `5607a122`) — Fix: harvested from the failed lane's WIP + one repair (the missing `use crate::add_lua_fn_try;` import that broke the build). (1) `shdr_getDependencyShaderId` now aborts the script on a dependency-validation failure (the `add_lua_fn_try!` match arm → `log_dependency_lookup_failure` + `Err`), matching the C++'s "Fatal error while looking for Lua dependency" + rethrow, instead of silently returning `size_t(-1)`; (2) the two missing diagnostic lines — the `u_execDependencyScript` failure path and the `redefineIoOpen`/`redefineRandom` catch-block lines, logged at `init_lua_api` (the single production call site with the `ssvu::lo` sink in reach) before the `Err` propagates to the caller's `expect()` = the C++'s uncaught-exception abort; (3) the three `cw_setVertexColor*` setters now share the C++'s mod-256 channel wrap (`rem_euclid(256)`, `sf::Color`'s `int -> int16_t -> uint8_t` narrowing) instead of the internally inconsistent pair (255 / `clamp(0,255)`). Citations verified: Utils.cpp:116-160 / :177-207 / :193-207, LuaScripting.cpp:1267-1311 / :150-153 / :171-174 / :326 / :374 / :405. Tests: NO new behaviour tests — the 38 pre-existing `oh-lua` tests hold; the recording `MockGame` + `game_state_with` harness landed in `tests/api.rs` so the `ssvu::lo` lines CAN be pinned by a future test. Not established: a test pin for each of the three new error-surface behaviours (the changes are citation-pinned, not test-pinned).

**29w1** (commit `f6ca5ad3`) — Fix: the credits-bar textures are the real `Assets/` PNGs (the 29e1 class, the menu's half): `MenuDraw::new(assets_dir, ...)` loads `creditsBar1.png` + the three `CREDITS_BAR_IDS` frames through `bridge::load_texture_or_null` (the 1×1 `nullTexture` fallback on a missing file) + the C++ `initAssets` `setSmooth(true)`; `draw()` sets the `creditsBar2` frame every frame from the id it borrows BEFORE the refresh (the C++'s update-before-draw order — `refreshCamera`'s `getLocalWidth(creditsBar2)` reads the current texture); the written-unread `SharedSnapshot.credits_bar2_id` field is gone. The not-1×1 guard: three new tests — real dimensions (1024×213 / 1024×116×3) + smooth, the missing-`Assets/` 1×1 fallback, and the cycle driving the sprite (0/150/250/350 → frames 0/1/2/0 by pixel equality, after a distinct-decodes self-check so a placeholder regression fails the pre-check). Also fixed the three clippy `-D warnings` the 29g1 commit left at HEAD (two `blocks_in_conditions`, one `type_complexity`) in the minimal shape. Citations verified: MenuGame.cpp:170-171 / :821-828 / :2848-2850 / :3157, MenuGame.hpp:135-137 (= `CREDITS_BAR_IDS`). Tests: `menu_draw` `credits_bar_textures_are_the_real_assets_pngs_not_placeholders` / `credits_bar_missing_assets_fall_back_to_the_placeholders` / `draw_applies_the_credits_bar2_cycle_to_the_sprite` (+ all 9 `MenuDraw::new` call sites updated). Not established: the device visual (no device work this batch — the phone's ADB listener is down; host-pinned as above). Gate: 920 passed.

**29q1** (commit `4b9b2ee0`) — Fix: the named citation/comment drifts, each re-derived against the C++ source — `windowed.rs`'s adapter comment (the 29b4 fix was already in per `git log -p --follow`; this item re-derived its `main.cpp:442-449` / `:448-461-479-502` cites); `real.rs` `getMusicData` (audit's 5545 → actual 5469-5470) + `getScore` (5494 → 5570); `graphics.rs:352` ("HGGraphics.cpp 122–141" → MenuGame.cpp:5772-5785 + the inverted parenthetical); `lua_console_ui.rs` :334 → :333; `update.rs` (the "Discord half is issue 21 (absent)" stacked doc block was FALSE — the Discord retry IS implemented at :1519-1535; block removed, surviving cites 477-507 → 477-508, hpp:90-92 → hpp:90,92); `style_control.rs` (`std::vector::at` → `ssvu::getByModIdx`, StyleData.cpp:320-325; the C++ binding verified at LuaScripting.cpp:1178-1189; the `ok_or` branch documented as defensive dead code); the three `a22_swapdeath` +1-off cites → 448-451 / 478-481 / 496-499 (`seams.rs` :450/:480/:498 re-verified correct, untouched); the three menu-audit drifts (`mustTakeScreenshot` 5974-5978 → 5962-5966, 2 sites; `setIgnoreAllInputs(1)` :357 → :358; the handler registrations :518/:617/:674 → :519/:616/:673). **Code wins over the audit:** `getFPSMult` 5985-5989 was already correct — the audit's own 5984-5988 was wrong; the code's cite stands. Then the whole class: every `// C++` file:line citation in the nine touched files extracted — **642 occurrences / 477 unique** — each re-derived against `src/SSVOpenHexagon` (+ `Global`); **88 citation-bearing comment blocks corrected** (`windowed.rs` 29, `menu_game.rs` 25, `menu_draw.rs` 21, `lua_console_ui.rs` 9, `real.rs` 4). Comment-only: zero code changes, gate unchanged at 920. Tests: none (comments only — the gate is the check). Not established (reported, NOT "fixed"): the citations into the SSVStart / SSVUtils / SSVMenuSystem headers (GameWindow.hpp, GameTimer.hpp, GameEngine.hpp, GameState.hpp, Manager.hpp, TimerBase.hpp, Rnd.hpp, Math.hpp, ToStr.hpp, StringifierBase.hpp, Input.hpp, TimerStatic.hpp, Geometry.hpp, Category.hpp, Items/Single.hpp, …) — `extlibs/SSVStart`, `extlibs/SSVUtils`, `extlibs/SSVMenuSystem` are NOT vendored in this worktree (uninitialized submodules, empty directories), so their line numbers could not be re-derived locally; left unchanged on purpose — an unverified "fix" would be a new drift, not a correction.

**Batch hygiene:** one commit per item, each named for its row id (`49c23991` / `5607a122` / `f6ca5ad3` / `4b9b2ee0`); explicit-path staging throughout (no `git add -A`); `cargo fmt` scoped to the touched files (the 030a-owned `crates/oh-lua/Cargo.toml` / `src/convert.rs` untouched); `.agents/audits/*.md` untouched; the C++ tree (030b) never modified; gate green after every item — starting baseline 915 (the post-029misc-remainder state) + 29v1's 2 snippet tests + 29w1's 3 credits-bar tests = **920 passed, 0 failed** (fmt + build + clippy `-D warnings` + workspace tests); no device work (the phone's ADB listener is down for the whole batch — none of the four items needs the device; 29w1's device visual is PENDING, host-pinned as above). `deviations.md` refreshed: F25 (29v1) / F26 (29g1) / F27 (29w1) rows + the F20 credits-bar pointer + this batch's Report section.
