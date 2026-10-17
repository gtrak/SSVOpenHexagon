# Task 029i-batch — the asset reload path, the F3/F4 binds, the workshop cache (29i1, 29b9, 29p1)

Three findings that are one story: the port can't reload assets, has no key
bindings to trigger it, and never reads the cache the C++ keeps for installed
workshop content. Gate green at **877 tests**. **No worktree** (D-035). One
commit per item.

Evidence: `.agents/audits/assets.md` (the reload paths + the workshop-cache
fallback) and `.agents/audits/menu.md` (#12). Citations below were re-derived.

---

## 1. 29i1 — the reload paths are no-ops  [internal, but it is the point of the debug flow]

C++ `Assets.cpp:1159-1443` implements `reloadAllShaders`, `reloadPack`,
`reloadLevel`; the menu drives them from `MenuGame.cpp:3092-3125`. The port has
**none of it** in any crate's `src` (grep-verified) — `crates/oh-menu/src/real.rs`
`reload_assets` is a no-op (`:172`, and the seam trait at `seams.rs`).

**Do:** port the three reload operations into `oh-assets` with the C++
semantics — read the C++ for exactly what each re-reads and what state it
replaces (a pack reload re-reads that pack's level definitions; shaders re-read
their sources; what happens to the currently-selected level index, the level
list, and the load-results/error state). Then implement `RealMenuAssets::
reload_assets` over them. This is a debug feature, but it is the feature that
makes editing a pack level testable without relaunching — and it is exactly what
the audit's "edited pack files never take effect" means. Test: write a level
file, load, modify the file on disk, reload, observe the new content.

## 2. 29b9 — the F3/F4 binds are absent  [internal]

C++ binds F3/F4 (`MenuGame.cpp:1012`, `:1016`) to the `reloadAssets` handler
(`:3092-3127`), whose shape is:

- **guard**: `state == LevelSelection && dialogBox.empty() && Config::getDebug()`
  — so the whole thing is debug-gated;
- `reloadAllShaders` + either `reloadPack` or `reloadLevel` (F3 vs F4 — check
  which is which in the C++, do not assume);
- `setIndex(lvlDrawer->currentIndex)` to restore the selection afterwards;
- an **uppercase** result dialog + `select.ogg` + `setIgnoreAllInputs(2)`.

The port has no F3/F4 arm in `handle_key_pressed`, and the help text in
`create_options_menu` is missing its `"F3 - RELOAD LEVEL ASSETS"` line (check the
C++'s exact help string and any F4 line).

**Do:** port the bindings, the guard, the handler and the help line(s). Test the
guard (in debug vs not, with a dialog open vs not) and that the handler calls the
reload operations.

## 3. 29p1 — the workshop cache is written but never read  [internal]

C++ `Assets.cpp:662-710` falls back to a workshop cache (`workshopCache.json`)
when Steam enumeration is unavailable — that is the entire reason the cache
exists. `oh-steam` **writes** the file, but no Rust code ever reads it, so
installed workshop packs vanish in precisely the Steam-down case the cache was
built for. (`assets.rs:13-15` documents a seam, but it only covers live
enumeration.)

**Do:** port the fallback — read the cache when live enumeration is unavailable,
with the C++'s ordering/precedence between live and cached entries (read
`Assets.cpp:662-710` and mirror it). Test: with enumeration unavailable and a
cache present, the cached packs appear; with live enumeration available, live
wins (or whatever the C++ actually does — verify, do not assume).

---

## Hygiene

- **The phone's ADB is DOWN** (`adb connect 192.168.10.138:38721` → refused; the
  host still pings it). No device work; do not add the temporary `debuggable`
  flag. Record anything device-visual as **PENDING**.
- One commit per item, naming the row id. **Stage explicit paths — never
  `git add -A`** (shared checkout + index).
- Keep the gate green after each item (`bash tools/gate.sh`, expect 877 + yours).
- Refresh `deviations.md` rows (issue 28's rule); `.agents/audits/*.md` are
  committed reports — do not rewrite them.
- If the work overruns your context, STOP after a coherent item, commit what is
  green, and report what remains — never leave the tree mid-change.
- Append a `## Report`: per item — the fix, the verified citation, the tests, and
  what you could not establish.
