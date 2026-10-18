# Task 054 — touch navigation: the racing grid, and the way out of a level

Read `.agents/WORKER-HYGIENE.md` first; then 031's and 039's specs (`.agents/tasks/issue-031-*.md`,
`issue-039-*.md`) because both are the precedent for deliberate touch behavioural additions, and the
deviations register's F28/F31 rows (`crates/oh-app/src/touch_overlay.rs`).

## Operator report, verbatim, on Android Chrome

*"some ux nits, i never see the menu button, and the repeated touch events are a non starter"* — the
first half of a five-item batch; the other items are other lanes (the web fullscreen button and the
canvas scaling in 055, the music content in 056). Do not stray into those.

## Item 1 — the repeated touch dispatch (the "non starter")

The menu's 3x3 grid (`MenuGame.cpp:2801-2834`, mirrored at `crates/oh-menu/src/menu_game.rs:3030`) acts
on the finger's position EVERY FRAME while it is down, and 039 MEASURED it on device: one 400 ms held
press produced 18 dispatches, roughly one per frame. 039 correctly called that C++ parity and left it —
but the operator has now judged it unusable, twice ("the input is really wonky", and now "a non
starter"). **So this is a deliberate, sanctioned DEVIATION now**, in the F28/F31 class, and it needs its
own register row (F35): state what is added, why the C++ has no equivalent *and* why we are departing
from it anyway, and that the desktop keyboard path is unchanged.

Shape it like the port's own edge-triggered work (031's overlay and 039's F31 action buttons are the
precedents: act on the transition, not the level). A press should act ONCE, with an optional
auto-repeat after a deliberate delay if held — and note, while reading the C++, that SFML's `touchDelay`
member (written at a dozen sites, decremented at `MenuGame.cpp:2787-2790` and never used as a guard) was
almost certainly INTENDED to be exactly this debounce. If you can show that, say so in the row: we are
completing a mechanism the original left dead.

Verify on the real surfaces you can reach: the port's own tests, plus the web via `tools/web-probe.mjs`
(its `--phone` viewport exists). Do NOT weaken the C++-faithful grid geometry (the thirds mapping) —
only the cadence.

## Item 2 — the way out of a level ("i never see the menu button")

The code says a labeled MENU button exists: `Button::Exit` sits at the top-left
(`touch_overlay.rs:202`), its label is "MENU" (`:502`), and it is gated on for the whole game state
(`windowed.rs:4208`: `set_action_active(engine.current() == GameSlot::Game && status.started)`), which
039 verified on device on the GAME-OVER screen. So either it is not visible while PLAYING on the
operator's build, or it is unrecognisable, or (most likely, and worth checking first) they simply cannot
reach or trust it because the racing grid makes the menus unusable — the two items are probably the same
bug from the operator's side.

Find out which, with evidence rather than reasoning: take a screenshot while a level is RUNNING (the web
probe, or the device), and state what is actually on screen. Then make the way out unmistakable and
reachable from every state the operator can get stuck in — including the level-selection and
profile-selection screens, where "back" is currently only the grid's left-third tap. Keep whatever you
add inside the existing overlay/grid machinery (031's design: the same per-`Tid` inputs) rather than
inventing a second input path, and if an addition is a platform-only deviation, give it a register row
too.

## Acceptance and boundaries

- Item 1: the debounce/edge behaviour proven by test (the arithmetic or the edge), the operator-visible
  effect described, and the register row written. Where a decision is a judgement call, state it.
- Item 2: a screenshot with the level RUNNING, on at least one touch surface, showing the way out; plus
  the reachability fix for any state that lacks one.
- Do not touch `crates/oh-web/site/index.html` or the fullscreen/canvas code (lane 055 owns those), and
  do not touch the music content (056).
- Host gate before each commit, `tools/check-android.sh` (§4), explicit paths, `timeout` everywhere, and
  a `## Report` with the screenshot paths and what the operator should now see.

## Report

Stopping point, per the operator's direction ("i do see the menu button" / "get to a stopping
point"): the in-flight work is coherent and host-tested, so it is committed (one commit, explicit
paths); the gates are green; nothing exploratory was left behind or reverted.

### Item 1 — the racing-grid cadence (F35): committed, then corrected

- Committed as `f28737f5` (verified, not redone): the C++'s dead `touchDelay` guard — written,
  never read as a guard (its only read was its own decay) — completed as a deliberate 50 ms
  auto-repeat cadence on the touch grid, a sanctioned deviation from the C++'s per-frame
  dispatch (register row F35).
- This commit corrects a unit bug in that row's arithmetic: `ft` is the port's `mFT` unit — FT,
  60 per second (`getSecondsToFT`: seconds × 60, SSVStart `Timers/TimerBase.hpp:42`) — not
  seconds. The 240 Hz fixed tick passes `TIME_STEP` = 0.25 FT per call, so the committed decay
  (`ft * 1000.0`) subtracted 250 "ms" per tick: the gate opened every tick and the "50 ms
  cadence" actually dispatched on every tick (~240/s). That flood is what the operator felt as
  "wonky" and what the probe's console measured (hundreds of `grid dispatch` lines per held
  press; ~107 dispatches in one 300 ms hold). The decay is now `ft * 1000.0 / 60.0` (4.17 ms per
  tick; the 50 ms gate opens every 12 ticks), the field's docs state FT, and the four cadence
  tests were rewritten in the honest unit (`update(1.5)` = 25 ms per call, so 50 ms = 2 calls
  EXACTLY in f32): a 300 ms held finger = exactly 6 dispatches; a press = 1; a jittering finger
  = 6, not per-call; the decay-while-skipped test pins the decay position.
- Proven: the arithmetic (host tests, exact). NOT proven on-device post-fix: the final bundle's
  probe run timed out before completing its navigation sequence (see Evidence below). The
  pre-fix flood IS documented (console counts + the held-press screenshot).

### Item 2 — the way out ("i never see the menu button")

- The operator now confirms the MENU button IS seen (in a running level) on the deployed build;
  the lane's own screenshots agree — the in-game label was never the broken half:
  `superseded-20261018/03-level-running.png` (predecessor) and `level-running.png` (this lane)
  both show the MENU label top-left while a level runs. No credit is claimed for fixing a
  visibility problem that did not exist in the game state.
- What was actually missing, and what this commit keeps (F36 register row): a way out of the
  MENU states the operator can get stuck in — level selection and profile selection — where
  "back" was only the invisible grid's left-third tap (and, pre-item-1, a per-tick flood).
  F36: the overlay's `Exit` (MENU) button gains a menu-side arm — `menu_active` (the
  `GameSlot::Menu && menu_exit_relevant()` feed: `LevelSelection`, `MOpts`, `MOnline`,
  `SLPSelect`, `SLPSelectBoot`; `SMain` excluded — the C++'s "main"-category no-op), its
  press-edge drained every frame and applied as the C++'s `exit_action` under the bind's own
  text-entry guard, the label drawn under `action || menu_active`, and the menu grid's
  `MenuTouchSink` now excludes the overlay-captured pointers (the §2b double-count hazard's
  menu half: the button sits in the grid's `up` zone, so without the exclusion a held MENU
  press would also step the selection up on every dispatch). Edge-triggered (the C++'s
  `t::Once` bind's shape): holding it never re-fires.
- Why kept rather than reverted: it is coherent, host-tested (the `MenuSlotAdapter` drain test,
  the `menu_exit_relevant` tests, the full workspace test stage green), and it is the task's
  explicit ask ("reachable from every state the operator can get stuck in — including the
  level-selection and profile-selection screens").

### Gates

- Host gate (`tools/gate.sh`: fmt, build, clippy `-D warnings`, `test --workspace`, lua-parity):
  all 5 stages green.
- `tools/check-android.sh`: passes (the overlay/window glue is platform code, §4).
- The web bundle was rebuilt through the emsdk container (`tools/package-web.sh`) for the probe
  runs; the `tools/web-probe.mjs` additions (held-touch `t:X;Y;DUR` / named-zone presets,
  `s:NAME` per-step screenshots, viewport convergence) are the probe-side evidence driver.

### Evidence (`.agents/evidence/054/`)

Captured over the lane's INTERMEDIATE bundle (F36 + pre-fix F35 decay; the label rendering and
the menu transitions are independent of the decay fix): `level-running.png` (a level RUNNING,
MENU top-left — the acceptance screenshot, on the CDP-touched web surface),
`level-selection.png` / `profile-selection.png` (the two stuck states the task names, MENU
top-left in each), `exit-from-game.png` (MENU in a running level → LevelSelection, the C++'s
`returnToLevelSelection` parity), `exit-from-level-selection.png` /
`exit-from-profile-selection.png` (MENU → SMain from each), `held-cadence-300ms.png` (pre-fix:
~107 zone dispatches in that 300 ms hold — the flood the cadence ends). The predecessor's
earlier captures are archived in `superseded-20261018/`.

What the operator should now see (lane build): in a running level — the MENU button top-left as
before; in level selection and profile selection — the same MENU button top-left; pressing it
exits exactly as the C++'s `exit_action` (running level → LevelSelection, LevelSelection /
profile selection → SMain). Grid presses act at the 50 ms cadence (one press = one action; a
held finger re-acts every 50 ms) instead of every tick.

Honest gaps: the post-fix web-side cadence has no completed probe run (the final run timed out
mid-navigation, and its boot did not reach SMain within the sequence — not diagnosed, per the
stopping-point instruction); the SMain-exclusion (no MENU label in the main menu) is
unit-tested, not screenshot-captured in this lane.
