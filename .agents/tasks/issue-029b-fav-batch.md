# Task 029b-fav-batch — the favorites drawer + the new-profile item (29b1, 29b6)

Two findings in the menu's list/drawer layer, both in `oh-menu` (+ the windowed
bindings). Gate green at **848 tests**. **No worktree** (D-035). One commit per
item.

Full evidence: `.agents/audits/menu.md` (items #1 and #7) and
`.agents/audits/dialog.md`. Citations below were re-derived.

---

## 1. 29b1 — the favorites feature is dead  [uservisible-gameplay]

**The data layer is already done** (029c1): the profile store persists favorites,
`is_level_favorite` consults the real current profile, and `ProfileData` has the
favorite-set mutations. What is missing is the **UI/state layer**, so F1/F2 and
the whole drawer do nothing:

- **F1/F2 binds**: `MenuGame.cpp:1019-1025`. In the port they are still TODO stubs
  in `crates/oh-app/src/windowed.rs:1870-1893`
  (`TODO(010b-menu-favorite)` `:1875`, `TODO(010b-menu-favorites-toggle)` `:1888`).
- **Add/remove favorite** — `MenuGame.cpp:4835-4931`: toggles the level in the
  current profile (`assets.getCurrentLocalProfile().addRemoveFavoriteLevel(...)`),
  plays the sound, and updates the star; **the toggle must round-trip through the
  same store 029c1 wired**, so a favorited level is still favorited after a
  restart (add a test for exactly that — it is the point of the feature).
- **Switch to/from the favorites view** — `:4933-4955`: `setIndex` +
  `adjustLevelsOffset` into the favorites list.
- **The favorites list rebuild** — `:4781-4833`: rebuild `favoriteLevelDataIds`
  from the current profile, `sort`, resize `lvlOffsets`, the drawer switch, and
  create the `"FAVORITES:"` submenu (`favSlct`).

Port state today: `favorites_active` is never set true anywhere (the only writer
is a test at `menu_draw.rs:4457`), `favoriteLevelDataIds` is never built, the
`"FAVORITES:"` submenu is never created, and `change_favorite_levels_to_profile`
is a stub (`menu_game.rs:643-650`) whose comment defers to "a 13b detail" — read
that comment, and check whether the 13b lane ever landed the piece it defers to
before you assume you must build it from scratch.

**Do:** port all of it, wiring the toggle to the 029c1 store. Test: toggle → the
profile's favorite set changes → `favorites_active`/the list rebuild reflects it →
and (critically) it survives a save/reload. Follow the C++ for the exact label
`"FAVORITES:"`, the sort, and the index/offset adjustments.

## 2. 29b6 — a created profile never appears in the menu  [uservisible-gameplay]

C++ `okAction`'s profile-creation branch (`MenuGame.cpp:2239-2247`) pushes a new
item for the created profile into `profileSelectionMenu` **at the sorted
position and re-sorts the whole list** (`sortByName`), so the new profile is
immediately selectable.

The port's `create_profile_from_entered` (`menu_game.rs:658-683`) calls
`assets.profile_create(...)` (now real, 029c1) and changes state — it **never
touches `profile_selection_menu`**. So a profile created in the UI is not
selectable until the menu is rebuilt.

Also check the guard the audit flagged: the SMain flow's
`if entered_str.is_empty()` appears to skip the `profile_create` call entirely —
confirm against the C++ whether that is faithful (the C++ may create in a case the
port skips) and fix it if not.

**Do:** push the item with the C++'s exact label format and comparator (read
`okAction` for both — the label is built from the name, and the sort is
`sortByName`), then re-sort. Test: create → the item exists → it is selectable →
and the ordering matches the C++'s comparator.

---

## Hygiene

- **The phone's ADB is DOWN** (`adb connect 192.168.10.138:38721` → refused; the
  host still pings it). No device work; do not add the temporary `debuggable`
  flag. Record the drawer's visual behaviour as **PENDING**; let host tests carry
  the correctness argument.
- One commit per item, naming the row id. **Stage explicit paths — never
  `git add -A`** (shared checkout + index).
- Keep the gate green after each item (`bash tools/gate.sh`, expect 848 + yours).
- Refresh `deviations.md` rows (issue 28's rule); `.agents/audits/*.md` are
  committed reports — do not rewrite them.
- If the batch overruns your context, STOP after a coherent item, commit what is
  green, and report what remains — never leave the tree mid-change.
- Append a `## Report`: per item — the fix, the verified citation, the tests, and
  what you could not establish.
