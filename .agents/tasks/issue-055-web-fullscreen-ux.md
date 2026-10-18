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
