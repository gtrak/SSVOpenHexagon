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
ADDITION rather than a fork. The operator's examples already have C++ knobs to drive — `timescale` for
slomo and `invincible` for zen are EXISTING config values, and a synthesizer belongs behind
`seams::Audio`, i.e. our code attached at a seam like the audio and Steam backends.

AND ONE MORE RULE FOR THE CRATE'S DOCS, which is the acceptance for any RUN-AFFECTING addition: **an
addition that changes what happens in a run must be driven through RECORDED INPUTS (so the replay
reproduces the run) or through config — never through silent state.** A slomo or zen toggle nobody records
would diverge a replay, and the port's determinism tests would catch it; the docs should hand that
constraint forward instead of leaving it to be rediscovered.

The classification table's "OURS" category therefore means "additions", not "UI".
