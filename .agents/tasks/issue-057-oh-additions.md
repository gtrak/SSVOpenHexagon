# Task 057 — `oh-additions`: give OUR UI a home, and stop patching the ported code piece by piece

Read `.agents/WORKER-HYGIENE.md` first, then the register's F-section
(`.agents/plans/001-rust-port/deviations.md`) — it is the authoritative list of everything that is "ours"
rather than the C++'s.

## The operator's direction, and the framing they approved

Verbatim: *"i would like to discuss separating our own ui changes from the c++ conventions, maybe in a
separate crate, instead of patching the original piece by piece. ours should be a true overlay over the
original, but actual fixes to the original ux can stay."* Then, on the proposal: *"let's call it
oh-additions and do some restructuring of what we already have, then I'll think about what improvements
to include. your framing is correct."*

So: **restructure what exists — no new features, no behaviour change** — into a new crate
`crates/oh-additions`. The improvements come later, on the operator's call, and will land there.

## Why, in measured terms (do not re-derive, but keep it in the row you add)

The deviation markers currently living inside PORTED files: `oh-render/src/window.rs` 59 (of 4,935
lines), `oh-app/src/windowed.rs` 39 (of 5,355), `oh-menu/src/menu_game.rs` 9 (of 3,629), plus
`oh-web/src/web.rs` 5, the audio backends 3, `oh-audio` 6, `hexagon_game.rs` 4. The F-section holds ~31
rows, and they are NOT all the same kind of thing — which is the crux.

## THE CONTRACT (the operator approved this wording; it is the crate's purpose)

> Our layer may **(1) inject inputs** through the existing per-`Tid` / input-state seams, **(2) draw**
> through the draw target, and **(3) own platform affordances**. It may **not** modify the ported
> simulation or menu logic — only *fixes* to that logic, i.e. correct ports, stay inside.

The port already has the right shape for this — the audio and Steam backends are "our code, attached at
a seam" — so `oh-additions` is the same proven pattern, made explicit and given a home.

## Deliverable 1 — classify the F-rows into three categories, in the register

For EVERY F-row, record one of:
- **OURS (additions / platform affordances)** → belongs in `oh-additions` (the touch overlay, the F31
  action row, F36's menu-exit arm, the web fullscreen button and IME — my initial guesses, VERIFY them).
- **INTEROP (platform plumbing at a seam)** → stays where it is, because it is already a separate file at
  a seam, not a patch of ported code: the gilrs poller (F15), the audio backends, the `ReplayStore` (F5),
  the json access layer (F9). Confirm each rather than assuming.
- **FIX (a correct port of the C++'s own behaviour)** → stays inside the ported code (F3's font probe,
  F6/F7, F10, F12, F13, F18/F19/F20, and 043's texture table). These make the port MATCH the C++; they
  are not "ours".
State the reason per row, and flag any row you find genuinely ambiguous instead of forcing it.

## Deliverable 2 — the crate, staged, each step its own commit with the gates green

- **Step A (mechanical, do this first):** lift `crates/oh-app/src/touch_overlay.rs` (1,242 lines) into
  `crates/oh-additions`. It is already one file behind a `set_active` gate, so the call sites should need
  almost nothing; keep the public surface identical and do not rename things gratuitously — the goal is
  the same behaviour in a better place.
- **Step B:** move the F31 action row's wiring and extract F36's menu-side arm OUT of `MenuGame::update`
  into an overlay-side **input injection** (the C++'s own `exit_action` stays the thing being invoked —
  that is what makes it injection rather than a logic fork). The `touch_grid` tests and the menu tests are
  your behavioural guard.
- **Step C:** the Rust halves of the platform affordances — the fullscreen click sink
  (`windowed.rs:3192-3215`) and the IME feed's Rust half. The SHELLS stay shells: the web `index.html`
  and the Android glue are already separate artifacts and are only *named*, not moved.

Do not move the INTEROP category: relocating the joystick poller or the audio backends would drag
platform dependencies into a UI crate for no benefit.

## Deliverable 3 — the ratchet (`tools/check-additions-boundary.sh`, wired into `tools/gate.sh`)

A cheap mechanical check so this cannot silently re-accrete: count the OURS-class deviation markers in
the PORTED files against a committed baseline and FAIL if the count grows, pointing the reader at
`oh-additions`. It must count only the OURS class — an INTEROP piece or a FIX may legitimately add a
marker to a ported file, and the ratchet must not punish a correct port. Keep the script tiny and say in
its header exactly what it does and does not mean.

## Acceptance — and it is the same one for every step

**No behaviour change**: the C++ fingerprint oracle, the corpus, `tools/lua-parity.sh`, the workspace
tests, and the touch/menu tests all stay green and unchanged; `tools/check-android.sh` and the wasm check
where a step touches a platform path. If a move cannot be done without changing behaviour, STOP and report
that rather than changing it — the operator wants the same program in a better shape, and a refactor that
alters behaviour is worse than the entanglement it fixed.

Boundaries: no new features, no behaviour changes, no `_RELEASE` content, no renames beyond what the move
requires, and update the register's rows with their new home as you go. Commit with explicit paths, bound
everything with `timeout`, and append a `## Report` with the classification table, what moved, the ratchet
baseline, and a plain statement of what you deliberately did NOT move and why.

## Then, and only then

A short section at the end of the report: "where the operator's improvements will go" — one or two lines
saying that new additions land in `oh-additions` under the contract above, so the next conversation has a
door to walk through.

## SCOPE CORRECTION (operator, after this lane began): additions, NOT just UI

Verbatim: *"not just ui by the way. i might add some more things like synth music and slomo or zen
mode."* So `oh-additions` is the home for **everything the C++ does not have**, of which UI is only the
first family. Name these in the crate's docs (without building empty modules for them): **UX/UI** · **modes
and features** (slomo, zen) · **platform affordances** · **content/audio generation** (e.g. synthesized
music instead of shipped third-party tracks).

THE CONTRACT GENERALISES — write this version into the crate's docs, because it is what makes features
possible without forking the port:

> Our layer may **drive the ported code through the knobs the C++ already has** — the config, the
> binds/input state, and the seams — may **draw over** it, and may **own platform affordances**. It may
> not edit the ported simulation or menu logic; only FIXES (correct ports of the C++'s own behaviour)
> stay inside.

This is the F36 precedent stated generally: invoking the original's own action is what keeps a feature an
ADDITION rather than a fork.

**The families above are the NATURE of what the operator wants to build — they are NOT designs, and no
feature design belongs in this lane or in the crate's docs.** Operator, verbatim: *"we're not implementing
those yet so don't make too many assumptions, but that is the nature of what i want to build."* So: name
the families, state the contract, stop. Where a concrete example would help a reader, mark it as an
ILLUSTRATION, and never let one harden into a plan — this project has already been bitten by prose that
outlived the thinking behind it.

AND ONE REQUIREMENT FOR THE CRATE'S DOCS — stated as a requirement to satisfy, not as a design: **any
addition that affects a run will have to be driven through RECORDED INPUTS (so a replay reproduces the
run) or through config — never through silent state.** That follows from the port's determinism promise
rather than from any particular feature: a toggle nobody records would diverge a replay, and the
determinism tests would catch it. Hand the constraint forward instead of leaving it to be rediscovered.

The classification table's "OURS" category therefore means "additions", not "UI".

## Report

Lane 057, branch `lane/057` (local-only). Commits, in order: `1c565181`
(deliverable 1 — the classification), `3d395ce5` (step A), `1226843b`
(the mid-run steer: the held keyboard / mouse input regression fix, its
own commit — see below), `c047a510` (the web repro tooling), `e1155def`
(the web repro evidence), `62ff07d6` (step B), `60212708` (step C),
`89cde68a` (deliverable 3 — the ratchet), this report. Every code step
landed with the full acceptance green AND UNCHANGED (the host gate's
1,045 passed / 0 failed / 1 ignored at each step; the C++ fingerprint
oracle + corpus `PARITY_OK` at each code step; `tools/lua-parity.sh`
green; the touch / menu behavioural guards pass unchanged;
`tools/check-android.sh` + the wasm release build where a platform
path was touched). All commands bounded with `timeout`; commits with
explicit paths.

### Deliverable 1 — the classification (in the register)

The canonical table is the register's section "057 — classification of
the F-rows (OURS / INTEROP / FIX)" (`.agents/plans/001-rust-port/
deviations.md`). The compact version:

| F-row | class | reason (one line) | home |
|---|---|---|---|
| F1 | FIX | the C++'s own layout space (actual render-target size) | stays |
| F2 | FIX | the C++'s own `--ignoreInputs` release-handler tail | stays |
| F3 | FIX | the C++'s ink-metric font-height probe | stays |
| F4 | FIX | the C++'s own swap-interval application | stays |
| F5 | INTEROP | the `ReplayStore` seam's real impl — already a separate file at the seam (the audio / Steam backend shape) | stays |
| F6 | FIX | the C++'s own `goToMenu()` default-argument port | stays |
| F7 | FIX | the C++'s own single-stream draw order | stays |
| F8 | FIX | a correct port of `resetConfigToDefaults` | stays |
| F9 | **FIX (diverges from the spec's INTEROP guess — flagged)** | the `as_*` coercions are a correct port of the vendored jsoncpp fork the C++ links: the json layer is the ported data layer (the C++'s own `ssvuj` value-access), not platform plumbing at a seam | stays (oh-data) |
| F10 | FIX | a correct port of the desktop-mode probe + console logging | stays |
| F11 | FIX | a correct port of the asset texture assignments | stays |
| F12 | FIX | a correct port of the per-frame `updateText` halves | stays |
| F13 | FIX | a correct port of the live FPS | stays |
| F14 | FIX | a correct port of the hook's frametime | stays |
| F15 | INTEROP (the poller half; the handler half is a FIX — noted) | the `RealJoystick` poller is our code in a separate file at the `Joystick` / `MenuJoystick` seams (moving it would drag `gilrs` in); the menu / engine handler blocks are a correct port of the C++'s own joystick controls | stays (both halves) |
| F16 | FIX | a correct port of the C++'s hint block | stays |
| F17 | FIX | a correct port of the mouse-wheel handler | stays |
| F18 | FIX | a correct port of the scroll machinery | stays |
| F19 | FIX | a correct port of the `exitTimer > 20` termination | stays |
| F20 | FIX | a correct port of the C++'s cosmetics | stays |
| F21 | FIX | a correct port of the favorites state machine | stays |
| F22 | FIX | a correct port of the profile item + `okAction` tail | stays |
| F23 | FIX | a correct port of the dialog input state machine | stays |
| F24 | FIX | a correct port of the dialog geometry (D1) | stays |
| F25 | FIX | the C++'s embedded Lua strings, byte-exact | stays |
| F26 | FIX | a correct port of the C++'s Lua error surface | stays |
| F27 | FIX | a correct port of the credit-bar textures | stays |
| F28 | OURS | the D-034-class sanctioned touch overlay (the C++'s touch controls are rotation-only; focus / swap have no C++ behaviour to match) | **oh-additions** (step A: the state machine + buttons; the per-frame feed / gate wiring stays in the composition — not named in the steps) |
| F29 | — | retired (deleted with the LuaJIT decision, D-037 — the number has no row) | — |
| F30 | FIX | parity RESTORATION (the port was the odd one out — the C++'s own compatibility profile) | stays |
| F31 | OURS | the D-034-class sanctioned action row (the C++'s end-of-level is unreachable on a touch-only device) | **oh-additions** (step A: the buttons; step B: the action-edge wiring as the overlay-side input injection; the `cfg(android)` hint override in hexagon_game.rs stays — a status-text half, not named in the steps) |
| F32 | OURS | the web-only IME text path (the C++ has no web text path) | **oh-additions** (step C: the `SoftInput` / `NoOpSoftInput` / `WebIme` controller half; the `#ime` get-or-create + feed listeners + keydown text gate stay in the emscripten window layer; the shell's `#ime` = named shell) |
| F33 | OURS | the web-only fullscreen button (the C++ had no such control) | **oh-additions** (step C: the click / change-sink install; the `#fs` get-or-create + listeners stay in the emscripten window layer; the per-frame `ohstatechange` signal stays in the rAF loop body; the shell's `#fs` = named shell) |
| F34 | OURS | the web's IndexedDB persistence (the C++'s emscripten build had none) | the named shells (the page glue + `index.html`'s IDBFS mount) + the emscripten-only exit-save re-fire at the platform boundary — NOT moved in 057 (step C names only the fs-sink + the IME Rust half) |
| F35 | OURS | the sanctioned menu-cadence DEVIATION (the operator ruled the C++'s per-frame grid "a non starter") | **stays in oh-menu** — the gate sits on the ported menu state (moving it would make the overlay drive the menu's grid: a logic fork, not an injection); not named in the steps; counted by the ratchet |
| F36 | OURS | the menu-side `Exit` arm (the C++'s menu "back" has no touch path) | **oh-additions** (step A: the `menu_active` gate + `gate_on`; step B: the menu-side arm as the overlay-side input injection — the C++'s own `exit_action` stays the thing invoked; the `menu_exit_relevant` accessor stays in oh-menu — C++ state, not an addition) |

The audio / Steam backends have no F-row: they are INTEROP-class by the
same seam pattern (our code, separate crates at the `Audio` /
`SteamSink` seams) and stay where they are.

### Deliverable 2 — what moved

* **Step A** (`3d395ce5`): `touch_overlay.rs` (1,242 lines) lifted from
  `oh-app` into `crates/oh-additions`; `load_texture_or_null` (the
  shared texture-null half) moved to `oh-render` (pub) to break the
  would-be cycle; the public surface preserved (`oh-app` re-exports
  `oh_additions::touch_overlay`).
* **The held-input regression fix** (`1226843b` — the mid-run steer,
  its own commit, EXEMPT from the no-behaviour-change rule as an
  authorised parity bug): the emscripten rAF drain's 046 fast-press
  cycle had saved the input state BEFORE folding the event batch and
  restored it after the frame — a HELD key / button / finger (down-only
  in a batch) got its folded level reverted every frame: one-frame
  press, then up, and the browser sends no further keydown. The fix
  moves the restore TARGET to the POST-FOLD state (the C++'s
  persistent `inputState` map, SSVStart `GameWindow.hpp:60-72`) — the
  046 fast-press OR-in is preserved, the restore is NOT deleted.
  Reproduced in a real browser over the wasm build (the probe's held
  items + focus counters; the game's own tutorial verdict flips
  NOPE → YES!; evidence in `.agents/evidence/057/`).
* **Step B** (`62ff07d6`): the F31 action row's edge apply + the F36
  menu-side arm move out of the slot adapters into `oh-additions`'
  `input_injection` (`apply_game_action_edges` / `apply_menu_exit`) —
  verbatim semantics (the unconditional every-frame take, the gated
  apply, the once-consulted console, the same `Tid` bodies /
  `exit_action` invoked — injection, not a logic fork).
* **Step C** (`60212708`): the `SoftInput` / `NoOpSoftInput` /
  `WebIme` controller half → `oh-additions/soft_input.rs` (re-exported
  by `oh-app::windowed` — the public surface unchanged, so the
  composition's construction + the android glue's `ImeController`
  impl are untouched); the `#fs` click / change-sink install →
  `oh-additions/fullscreen.rs` (`install_sinks` — the composition's
  `build` makes the one-line call).

### What was deliberately NOT moved (and why)

* **The INTEROP class** — the `RealJoystick` poller (F15), the
  `ReplayStore` impl (F5), the json layer (F9), the audio / Steam
  backends: already separate files / crates at a seam; moving them
  would drag platform dependencies into a UI crate for no benefit.
* **The shells stay shells** — the web `index.html` (the `#ime` /
  `#fs` elements + the IDBFS / gate glue) and the Android glue
  (`android.rs`'s `ImeController` + activity wiring) are separate
  artifacts, named not moved.
* **The emscripten window layer's DOM halves** — the `#ime`'s
  get-or-create + feed listeners + keydown text gate (the pure
  `WebImeFeed` / `web_keydown_ime_policy` / `WebKeydownState`) and the
  `#fs`'s get-or-create + click / `fullscreenchange` listeners stay in
  `oh-render`'s emscripten `Window`: the platform path, not a patch of
  ported logic, AND they are driven from the `Window`'s own listener
  attach — an oh-additions move would cycle (oh-additions already
  depends on oh-render).
* **The gate FEEDS** (`set_action_active` — `GameSlot::Game &&
  started`; `set_menu_active` — `GameSlot::Menu &&
  menu.menu_exit_relevant()`) stay in the composition's
  `tick_fields`: they read the app's slot state + the C++'s menu
  state — neither is oh-additions' to own.
* **The F28 held-inputs feed** (the `o.held` → `set_input_impl_*`
  block in `GameSlotAdapter::update_input`) stays in the composition:
  step B names the F31 / F36 wiring only; the F28 feed is the
  per-frame input injection of the C++-bound two-state inputs (the
  issue-31 §2b wiring) — the recorded-inputs rule's standing seam.
* **The `cfg(android)` F31 hint override** (hexagon_game.rs) stays: a
  status-text half of the addition, not named in the steps.
* **F34's persistence** stays at the platform boundary (the named
  shells + the emscripten-only exit-save re-fire): step C names only
  the fs-sink + the IME Rust half.
* **F35's cadence** stays in oh-menu (above — a logic-fork hazard).

### Deliverable 3 — the ratchet baseline

`tools/check-additions-boundary.sh` (gate stage 6) +
`tools/additions-boundary-baseline.txt`. The pattern: comment lines in
the ported files referencing the seven OURS rows' IDs (F28 / F31 /
F32 / F33 / F34 / F35 / F36), their lanes' issue references (031 /
039 / 046 / 048 / 054), or this lane's step notes. The baseline
(post-restructure): `window.rs` 44, `windowed.rs` 38,
`menu_game.rs` 9, `web.rs` 5, `hexagon_game.rs` 4, `oh-audio/lib.rs`
6. A grown count fails the gate (pointing at `oh-additions`); a
correct port (a FIX / INTEROP row's markers — their own lane / row
references) does not match the pattern; the header states what the
ratchet does and does not mean (marker LINES not code; it guards the
rows it names — a future lane classifying a new OURS row adds that
row's ID + lane to the pattern in the classifying commit). Verified
both directions (a planted `issue 039 (F31)` line fails; a
high-baseline shrink notes and passes; both temp changes reverted
clean).

### The four guard properties (the acceptance's guard list)

1. **The held key / mouse that started this** —
   `a_held_key_button_and_finger_survive_n_frames_with_no_release`
   (oh-render window tests): a held key AND a held mouse button AND a
   held finger survive 8 frames with no release; the releases drop the
   levels (the C++'s persistent `inputState`, `GameWindow.hpp:60-72`).
   Negative check: with the pre-fold capture restored, this guard
   fails exactly as the operator's build did.
2. **The press-and-release-in-one-frame case that must not regress** —
   `a_fast_press_inside_one_batch_is_visible_to_that_frame_only`
   (oh-render window tests): the 046 goal — a same-frame press+release
   is visible to that frame's poll and NOT stuck down afterwards
   (the restore to the post-fold state + the `Type::Once` latch's
   re-arm). `a_batch_ending_in_lost_focus_clears_the_persistent_level`
   keeps the `LostFocus` reset semantics.
3. **A held touch control in gameplay staying continuous** —
   `a_held_touch_in_gameplay_stays_continuous_and_the_menu_cadence_is_inert`
   (oh-app windowed test, the F35 boundary): a touch held on the
   gameplay's left half keeps driving the game's rotation input every
   frame (24 ticks), and the menu's `touch_delay` cadence field is
   provably inert in the game state (no gameplay reader exists).
4. **The menu's taps still acting once** — the existing 054 guard
   (oh-menu `touch_grid.rs`'s EDGE test: a press fires exactly once;
   the release fires nothing; a fresh press after the decay is a fresh
   dispatch) — pass unchanged through steps B / C.

### Where the operator's improvements will go

New additions land in `crates/oh-additions` under the boundary
contract (inject inputs through the existing per-`Tid` / input-state
seams, draw through the draw target, own platform affordances — never
modify ported simulation or menu logic; an addition that changes a
run is driven through recorded inputs or config, never silent
state). The ratchet (`tools/check-additions-boundary.sh`) is the
standing guard: a new OURS row gets its ID added to the pattern in
the same commit that classifies it.
