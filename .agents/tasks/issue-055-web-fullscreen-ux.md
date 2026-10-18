# Task 055 — the web fullscreen button: top-right, always present, and the canvas scaling that never recovers

Read `.agents/WORKER-HYGIENE.md` first, then `.agents/tasks/issue-046-web-ime.md`'s F33 section (the
button you are changing) and the register's F33 row. Read `.agents/tasks/issue-032-web-input.md` for the
canvas real-size path (the "config size != real size" lesson) — that is the code the scaling bug lives in.

## Operator report, verbatim, on Android Chrome

*"also the full screen button needs to be on top right and stay present. sometimes the browser itself
gets out of fullscreen and i can no longer reach the button. also when that happens the scaling gets out
of sync and never recovers."* Three items, one lane, because they are one mechanism.

## Item 1 — move it to the top-right and keep it present EVERYWHERE

Today it is bottom-right 12px and `display: none` outside the menu states (the `ohstatechange` gate at
`crates/oh-web/site/index.html:267`), and 046 hid it during gameplay deliberately — to protect 039's
RotateCW corner hit-test. The operator's decision overrides that: **top-right, and visible during
gameplay too**, because losing it strands them (see item 2). CONSTRAINT: the F31 action row draws its
own REPLAY/RESTART/PAUSE buttons in the TOP-RIGHT area while a level is running
(`touch_overlay.rs`: the buttons are top-row, and `windowed.rs:4208` gates them on for the whole game
state). A DOM button over them would swallow their touches — the exact failure 046 was avoiding. So
resolve the collision explicitly (offset the button below the row, or move the row's right buttons), and
PROVE it the way 046 did: `document.elementFromPoint` at the relevant corners, in the menu state AND in
the game state, asserting nothing that the game owns is swallowed. State the chosen resolution and why.

## Item 2 — the state desync (the concrete bug the operator describes)

The click sink toggles the CONFIG (`cfg.set_fullscreen(!cfg.fullscreen())`, the LAlt+Enter twin —
`windowed.rs:3192-3215`), and its own docs already note the hole: an Esc press or Android's back gesture
leaves fullscreen without the game asking, so the config still says `true`. The next tap therefore sets
it to `false` and calls exitFullscreen — a no-op — so the FIRST tap after an external exit does nothing
visible. Fix it properly: derive the intent from the browser's ACTUAL state (`document.fullscreenElement`)
and re-sync the ported `fullscreen` config on `fullscreenchange`, so the config, the button's appearance
and the browser cannot disagree. Keep the config as the single source of truth for the *setting* (F33's
rule) — reconcile it, do not abandon it.

## Item 3 — the canvas scaling that never recovers (the real bug)

Leaving fullscreen externally resizes the viewport, and the operator reports the scaling goes out of sync
and never recovers. The port sizes the canvas from the CSS box x devicePixelRatio through the real-size
path (`crates/oh-render/src/window.rs`'s `resized_edge` and the `gl.resize` arm — note its own comment:
"the `resize` listener catches the layout when it lands", so a listener firing BEFORE layout settles is
the first suspect, and Android's `devicePixelRatio` changing across a fullscreen transition is the
second). Diagnose it rather than guessing, then fix it so the invariant holds: the canvas backing store
== its CSS box x DPR. REPRODUCE it first: a CDP viewport/metrics override can simulate the change, so
this is testable headlessly — do that, and keep the reproduction as a probe or test rather than a
one-off (tools/web-probe.mjs already has `--phone`).

## Acceptance and boundaries

- A headless reproduction of the stale-size bug BEFORE the fix, and the same run showing it correct
  after — that pair is the acceptance, plus the hit-test assertions from item 1.
- The button: top-right, present in the game state, and out of the way of everything the game draws.
- State plainly what only a real device can confirm (the browser's own fullscreen behaviour under a real
  gesture — headless cannot grant a transient activation, as 046 and 041 both recorded).
- Boundaries: `crates/oh-web/site/index.html`, `crates/oh-render/src/window.rs`'s web sizing, and the
  click sink in `windowed.rs`. Do NOT touch the touch overlay or the menu grid (lane 054 owns those).
- Host gate before each commit, the wasm build in the emsdk container (§4 — this is web code), explicit
  paths, `timeout` everywhere, and a `## Report`.

## Report

All three items done, all three proven by `tools/web-fs-probe.mjs` (the 046
`tools/web-probe.mjs`'s sibling — CDP headless Chrome, `--phone 390x844@3`,
per-PID port/user-data-dir/cleanup). Evidence: BEFORE `/tmp/fs-probe-055-before-
scale.log`, AFTER `/tmp/fs-probe-055-after-full3.log` (scale / hit / wiring all
PASS on the post-fix package).

### Item 1 — top-right, present everywhere, out of the way (proved by hit-tests, BOTH states)

- `crates/oh-web/site/index.html`: `#fs` moved bottom-right -> top-right
  (`right: 12px; top: 12px` pre-JS fallback); the 046 `ohstatechange` gate
  (game-state `display: none` — the state that stranded the operator) replaced
  by the `placeFs()` gate: `top = 76 / devicePixelRatio + 12` px in EVERY
  state (below the F31 row's 12..76 REAL-px band — `touch_overlay.rs`'s
  `button_centers` top row, PAD 12 + BUTTON_HALF 32; the collision resolved by
  the OFFSET per the spec — the row is lane 054's, a DOM button over REPLAY /
  RESTART / PAUSE would swallow the PAUSE touches — the exact failure 046
  avoided by hiding), `display: block` in every state, re-applied on the
  `resize` edge (the DPR re-scales the band; event-driven, no poll).
  `display: none` stays the PRE-JS default (046's invariant). `#status` stays
  top-right, above the button at every DPR.
- `crates/oh-render/src/window.rs`: the `#fs` fallback's inline style follows
  (`top: 88px` — the dpr = 1 worst case of the 76 real-px band + 12 px gap,
  which clears the band at every dpr >= 1).
- PROOF (probe `--hit`, post-fix package): game state — the F31 row's REPLAY /
  RESTART / PAUSE centers hit-test to the canvas (`f31Hits: ["c","c","c"]`)
  with `#fs` present-but-below (`belowRow: true`) + `selfHit: "fs"`; menu
  state — `#fs` `display:block`, `inTopRight`, `selfHit: "fs"`; the
  bottom-right RotateCW corner stays a canvas hit (046's property, re-asserted).

### Item 2 — the desync (reconcile, do not abandon)

- `crates/oh-app/src/windowed.rs`: the `#fs` click sink now derives the
  direction from `document.fullscreenElement` (NOT the config's value — the
  pre-055 config-toggle left the config `true` after an external Esc/back exit,
  so the next tap set it `false` + called `exitFullscreen()`, a no-op: the
  operator's "i can no longer reach the button"). A rejected request (no
  transient user activation) fires no event and writes nothing.
- NEW: the `fullscreenchange` RE-SYNC — `Window::set_fullscreen_change_sink`
  (`crates/oh-render/src/window.rs`; the `Document`'s `fullscreenchange`
  listener in `Window::create`), the `oh-app`'s callback writes
  `Config::set_fullscreen` from the browser's ACTUAL state on EVERY transition
  (our API's grant/exit AND the external exits). F33's rule kept: the config
  stays the SETTING's single source of truth (the options menu's + the
  LAlt+Enter bind's writes unchanged — same shared `Arc<RwLock<Config>>`), the
  browser stays the STATE's source of truth (the `html:fullscreen` flip), and
  they cannot disagree — the config is written ONCE, from the browser.
- PROOF (probe `--wiring`, CDP-granted run): `[oh-web-fs] fullscreen -> true
  (browser: requestFullscreen() called (fullscreen_enabled=false))` +
  `[oh-web-fs] fullscreenchange -> true (config false -> true)` + CDP `Escape`
  (the external exit) -> `[oh-web-fs] fullscreenchange -> false (config true
  -> false)` — the pre-055 stale-`true` desync closed; the next tap's direction
  is correct in every case (the fast double-tap included).

### Item 3 — the scaling that never recovers (reproduced headlessly, BEFORE + AFTER pair)

- REPRO (pre-fix package, `--scale`, the log kept): the DPR-only viewport
  change (800x600 @3 -> @2.5, CSS box unchanged) fires NO `resize` and NO
  `matchMedia` change (the page-side `__reslog` event-time diagnostic is the
  measured fact — the browser delivers no event for a `devicePixelRatio`
  change that leaves the CSS box unchanged: the Android DPR change across a
  fullscreen transition, the task's second suspect), so the backing store
  stayed stuck at 2940x2205 vs expected 2450x1837 at +20/+150/+1200 ms —
  "never recovers" (no event re-fires the `resize` once the layout settles).
  The boot edge reproduced too (zero-rect at module init -> the requested
  640x480 fallback x3 = 1920x1440, no correcting event).
- FIX (`crates/oh-render/src/window.rs`, emscripten-only, in the rAF pump's
  closure): (a) the SETTLE re-measure — a folded `Resized` (or the boot) primes
  a post-layout re-measure on the next tick (the event-time measurement may be
  pre-settle — the 100vw/100vh box + the DPR settle in the frame's layout
  pass; the dedup against the folded bookkeeping makes a settled-already
  measurement a no-op — the event stream never carries a duplicate `Resized`);
  (b) the PER-FRAME DPR sensor — a passive `devicePixelRatio` read (one DOM
  getter + one compare per frame, no event, no side effect while the DPR
  holds) re-measures on drift (the layout box is unchanged in the DPR-only
  case, so the read is the settled box).
- AFTER (post-fix package, same probe): the invariant (canvas backing store ==
  its CSS box x DPR) holds at +20/+150/+1200 ms at every trigger — boot, the
  windowed resize, the DPR-only, the DPR-return — and in the CDP-granted
  fullscreen size.

### Gate, build, limits

- Host gate 5/5 green on the final tree (fmt / build / clippy / test 1033
  passed / lua-parity); wasm build in the emsdk container green (the
  `tools/package-web.sh` recipe, `CARGO_TARGET_DIR` in-lane); the post-build
  probe re-run PASSes.
- Real-gesture fullscreen is the operator's to confirm: headless CDP grants
  the transient user activation run-dependently (046/041's recorded limit —
  the operator's real tap on Android Chrome always does). The wiring, the
  re-sync and the external-exit path are proven by the CDP-granted run above;
  the on-device feel of the transition is the operator's confirmation.
- Register: the F33 row amended (the 055 overrides — the relocation, the
  re-sync, the scaling fix, the probe's acceptance — marked as 055
  amendments; the row's original 046 text kept, superseded clauses marked).
- Commits: `9cd9476b` the probe; `986bbf4a` the window layer (items 2 + 3 +
  the fallback style); `9aa58dac` the shell (item 1); this report + the
  register follow.
