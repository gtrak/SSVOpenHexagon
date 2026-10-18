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
