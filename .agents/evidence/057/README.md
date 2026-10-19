# Issue 057 — held-input regression: web reproduction evidence

The held keyboard / mouse input regression ("holding a key should work as
before"; "a keypress event that should have been a keydown" — the
operator's words), reproduced and verified in a real browser over the
wasm build.

## How it was driven

`tools/web-probe.mjs` (this lane's additions: the `h:KEY;MS` /
`m:BTN;MS` held-input items + the boot-installed focus counters) over
headless Chrome (SwiftShader), one build per port:

- **pre-fix**: lane 057 worktree at `3d395ce5` (the step-A commit —
  before the fix), `cargo build --release -p oh-web --target
  wasm32-unknown-emscripten` in the `emscripten/emsdk` container
  (the house recipe), served from a static directory.
- **post-fix**: the same worktree at the fix commit, same recipe,
  same page, same sequence.

Sequence (identical for both builds): two Enters (the boot screens),
`i:vee057` (the name), three Enters (profile → main menu → level
selection → start the level), then `h:Left;1500` (hold the
rotate-CCW default bind for 1500 ms — the browser sends no further
keydown for a held key; that IS the held-input test), then
`m:left;1500` (hold LButton — also rotate-CCW).

## The evidence

- **`held-key-verdict-{prefix,postfix}.png`** — the game's own
  verdict: the tutorial level's tracked text
  "ROTATED COUNTER-CLOCKWISE". Pre-fix: `NOPE` — the game's
  `onInput` never saw the held key's counter-clockwise movement in
  its tracked window. Post-fix: `YES!` — it did. This is the
  reproduction's load-bearing observable (game-native, not a
  pixel inference): pre-fix the held key's level was reverted to
  its pre-frame value every frame (the pre-fold restore), so the
  game saw the press for the fold's frame only; post-fix the
  level persists until the release's event, exactly the C++'s
  persistent `inputState` map (SSVStart `GameWindow.hpp:60-72`).
- **`held-mouse-{prefix,postfix}.png`** — the held mouse button's
  stills (LButton is the rotate-CCW default bind; the same
  mechanism — the pointer fold's `MouseButtonPressed` arm).
- **`01-smain-*` / `02-levelsel-*`** — the tap half of the
  reproduction: the Enter presses (down+up inside one frame's
  batch) advance every screen in BOTH builds — taps never broke;
  the fast-press visibility (issue 046) is preserved by the fix.
- **`probe-{prefix,postfix}*.log`** — the probe's full output.
  The lines that matter: the `state timeline` (…→ `game`), the
  `[dom]` lines (one `down:ArrowLeft` + one `up:ArrowLeft` — no
  OS repeats, no re-delivery), and the **focus counters**:
  `{"hidden":"visible","fullscreen":false,"blur":0,"focus":0,
  "vis":0,"fs":0}` — NO spurious window blur, tab-hide, or
  fullscreen change during the holds. The parent's blur /
  `LostFocus` hypothesis is ruled out by these: the DOM never
  went there, so the port's `LostFocus` fold (1:1 with the window
  blur + the `visibilitychange`→hidden) was never the culprit.
  The input was being reverted by the drain's own restore — the
  pre-fold snapshot's target.

## The host-side guards (the deterministic half)

In `crates/oh-render/src/window.rs` (`mod tests`) +
`crates/oh-app/tests/windowed.rs` — see the fix commit
(`fix(057)`):

- `a_held_key_button_and_finger_survive_n_frames_with_no_release` —
  a held key AND a held mouse button AND a held finger survive 8
  frames with no release; the releases drop the levels.
- `a_fast_press_inside_one_batch_is_visible_to_that_frame_only` —
  the 046 goal must-not-regress: a same-frame press+release is
  visible to that frame's poll and not stuck afterwards.
- `a_batch_ending_in_lost_focus_clears_the_persistent_level` —
  the `LostFocus`'s `reset()` semantics survive the extraction.
- `a_held_touch_in_gameplay_stays_continuous_and_the_menu_cadence_is_inert`
  (oh-app windowed) — the F35 boundary: a touch held on the
  gameplay's left half keeps driving the game's rotation input on
  every frame; the menu's `touch_delay` cadence field is provably
  inert in the game state.
- Menu taps still act once, not racing — the existing 054 guard
  (`oh-menu/tests/touch_grid.rs` EDGE test: a press fires exactly
  once; release fires nothing).

Negative check: with the pre-fold capture restored into the new
methods, the held-input guard fails exactly as the operator's
build did.
