# Task 029b-nav-batch — menu navigation: wheel, scroll machinery, exit timer, cosmetics (29b2, 29b5, 29b7, 29b8)

Four audit findings in the menu's input/navigation path. Gate green at **817
tests**. **No worktree** (D-035). One commit per item.

Full evidence: `.agents/audits/menu.md` (items #2, #5, #6, #10, #11, #9). The
citations below were re-derived; read the cited C++ before changing anything.

---

## 1. 29b2 — the mouse wheel is dropped at the event layer  [uservisible-gameplay]

C++ handles `MouseWheelScrolled` in `MenuGame::onEvent` (`MenuGame.cpp:299-356`):
per-menu index navigation, and for `LevelSelection` it accumulates
`YScrollTo += deltaY * 48.f` (clamped `0 … -4000`) plus the
`a35_eagerformore` achievement when the list is pushed to the end (`error.ogg`),
with a bind-capture gate and cursor re-enable.

The port **discards the event entirely**: `Event::MouseWheelScrolled { .. } => {}`
at `crates/oh-render/src/window.rs:1040`. So the wheel works nowhere — not the
menus, not the level list.

Do: plumb the wheel delta from the platform event through to `MenuGame` (find how
the other mouse events reach it — the click path — and follow that; the fix spans
`oh-render`'s event translation + `oh-menu`'s handler), then port the C++ handler
with the exact scroll constants and clamp. Check the C++'s `deltaY` sign
convention against the platform event's (this is the classic wheel inversion bug —
read both, do not assume). Test the handler with synthetic deltas (including the
clamp ends).

## 2. 29b5 — the level-list scroll machinery is absent  [uservisible-gameplay]

The whole easing/scroll system is missing — `y_scroll_to` is written once and
never consumed, and `PackChange::Rest` does nothing:

- `PackChange::Rest → scrollLevelListToTargetY()` (`MenuGame.cpp:2884`), the
  easing function itself at `:4578-4598` (`yOffset → yScrollTo`);
- `calcLevelChangeScroll()` on up/down level change incl. the wraparound
  `scrollJump` (`:4404-4461`), called at `:1973`, `:1996`, `:2062`, `:2079`;
- single-pack top/bottom `YScrollTo` targets + `calcScrollSpeed()`
  (`:1964-1981`, `:2056-2070`);
- `changePackQuick`'s scroll compensation (`:2156-2181`);
- `calcPackChangeScrollFold` / `Stretch` (`:4463-4561`) and
  `quickPackFoldStretch()` (`:4563-4576`);
- the focus rising/falling-edge block (`:2655-2670`: `setIndex(0)` +
  `quickPackFoldStretch()`).

The port's state today: `y_scroll_to` is declared (`menu_game.rs:50`),
initialised (`:63`) and written by `set_y_offset` (`:939`) — **never read**;
`step_pack_change`'s `PackChange::Rest => {}` (`:785-786`); `change_pack_quick`
(`:916-925`) has no scroll math; `up_action`/`down_action` (`:1522-1560`,
`:1562-1598`) only `set_index`/`y_offset`; `update` (`:1846-1847`) mirrors
`was_focus_held` with no edge effect. Consequence: **the selected level can sit
off-screen** and pack switches don't animate.

Do: port each piece faithfully (these are pure float computations — they are
exactly the kind of thing that must match numerically). Test the pure functions
against hand-computed values from the C++ (easing progress, the wraparound jump,
the fold/stretch), not just "it moved".

## 3. 29b7 — hold-Exit-to-quit is dead  [uservisible-gameplay]

C++ `MenuGame.cpp:2852-2854`: when `exitTimer > 20.f`, the window is stopped.
The port accumulates `exit_timer` (`menu_game.rs:97`, `:210`, `:1185-1195`) and
wires the bind (`windowed.rs:1785-1786`) but **never compares it to 20** — so the
quit gesture does nothing. Port the comparison + the stop call (find how the port
already requests a window stop — `quit_requested`-style — and use the existing
path). Test the threshold boundary (just-below does not stop, at/above does) and
that releasing the key resets the timer like the C++ does.

## 4. 29b8 — four small cosmetic gaps  [uservisible-cosmetic]

1. **`randomTip` is never populated** — C++ ctor (`MenuGame.cpp:752-766`) picks a
   random tip (and concatenates a second when the picked one is a single line);
   the port leaves `["", ""]` (`menu_game.rs:92`, `:207`), so
   `drawLoadingScreen` renders two blank lines (`menu_draw.rs:1954-1961`). Port
   the selection, including the RNG source the C++ uses (check whether it is the
   cosmetic RNG or a different one — this must match, so read it).
2. **Credits-bar texture cycling** — C++ `:2848-2850` cycles 3 textures every
   ~100 frames via `creditsIds`/`currentCreditsId` (`MenuGame.hpp:135-137`,
   `:190`); the port draws a static `credits_bar2`. Port the cycling.
3. **Shift-modifier in `leftRightActionImpl`** — C++ `:1860-1866`: holding
   LShift/RShift makes the slider step 2 instead of 1. The port
   (`menu_game.rs:1608-1640`) computes the amount purely from
   `focus_held`/`was_focus_held`. Port the shift check (the C++ reads
   `window.isKeyDown(LShift/RShift)` — find the port's equivalent key-state query).
4. **`a23_login`** — `Steam::steam_manager.unlockAchievement("a23_login")` on
   `ELoginSuccess` (`MenuGame.cpp:2591`). The port's `LoginSuccess` arm
   (`menu_game.rs:751`) plays the sound and shows the dialog but never unlocks.
   This is a plain missing call site, **not** a platform gate — the Steam manager
   is present and shared in the shipping app (`windowed.rs:2223-2227`). Wire it
   through the existing steam seam.

---

## Hygiene

- **The phone's ADB is DOWN** (`adb connect 192.168.10.138:38721` → refused; the
  host still pings it). No device work; do not add the temporary `debuggable`
  flag. The scroll/wheel work is visually confirmable on the device later —
  record that as **PENDING**, and make the host tests carry the correctness
  argument instead (pure float expectations).
- One commit per item, naming the row id. **Stage explicit paths — never
  `git add -A`** (shared checkout + index).
- Keep the gate green after each item (`bash tools/gate.sh`, expect 817 + yours).
- Refresh `deviations.md` rows (issue 28's rule); `.agents/audits/*.md` are
  committed reports — do not rewrite them.
- If the batch overruns your context, STOP after a coherent subset, commit what
  is green, and report which items remain — do not leave the tree mid-change.
- Append a `## Report`: per item — the fix, the verified citation, the tests, and
  what you could not establish.
