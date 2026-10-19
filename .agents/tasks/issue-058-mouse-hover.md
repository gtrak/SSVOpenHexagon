# Task 058 — the menu's mouse hover (`mouseHovering`), a C++ behaviour the port never ported

Read `.agents/WORKER-HYGIENE.md` first, then the tracker row **29m** in `.agents/plans/STATUS.md` — it is
the brief, because no task file was ever written for it. Its text: a FIX for the menu mouse hover
(`mouseHovering`), "4 C++ sites; gates left/right actions", from the 029 audit series.

## Why it matters right now

The operator reported, verbatim: **"mousing also doesn't work."** We have already fixed one real
mechanism behind that — the emscripten rAF drain's `saved_input` restore, which reverted held key and
mouse-button state after one frame (issue 057, landed) — but this row suggests there is a SECOND, older
gap: the C++'s `mouseHovering` was never ported, and it GATES the left/right actions. A menu whose
left/right behaviour depends on a hover state that does not exist in the port would feel exactly like
"the mouse does nothing", independently of the held-state bug.

## What to do

- Find the C++'s sites (grep `mouseHovering` in `cpp-original/src/SSVOpenHexagon/Core/MenuGame.cpp` — the
  row says four) and read what each does, INCLUDING its effect on the left/right actions and any place it
  is set or cleared. The pinned tree is the main checkout; hygiene §11 covers the submodules.
- Port it faithfully — this is a FIX, i.e. the port must MATCH the C++, so it belongs inside the ported
  menu logic rather than in `oh-additions` (see issue 057's classification: FIX stays inside; OURS moves
  to the crate).
- Careful with the port's own input model: the port feeds a per-frame input snapshot where the C++ reads
  SFML's live `isKeyPressed`/mouse state, so establish WHERE the hover value must be computed for the
  observable to match, and say which you chose and why.

## Acceptance

- The C++, cited, and the behaviour observable: a mouse move over the menu must produce the C++'s own
  hover behaviour, and the left/right-action gating must match. Drive it with the existing probe tooling
  (`tools/web-probe.mjs` has CDP mouse dispatch) or on the host under Xvfb.
- A TEST for the hover/gating logic, so this cannot silently regress — the class of bug that started this
  thread was a change nobody's test covered.
- The register/tracker: close row 29m with the landing hash, and if anything about this turns out to be a
  deliberate port decision rather than an omission, say so instead of closing it.
- Gates: host gate before each commit, `tools/check-android.sh` and `tools/build-windows.sh --check` if
  you touch shared or platform paths (§4), the wasm check if you touch the web path, and the oracle/corpus
  must stay green — the fingerprint parity is the acceptance that the simulation did not move.

If reading the C++ shows the row is stale (the behaviour already exists under another name), that is a
perfectly good outcome: report it with the evidence and close the row that way instead.
