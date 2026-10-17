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
