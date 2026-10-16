# Issue 28 — The deviations audit (register + verification)

## Why this exists

The operator's goal is a **1:1 Rust port as the starting base**. A
deviation that was "documented and assumed harmless" — the menu 9-zone
touch grid (C++ `MenuGame.cpp:2795-2835`), dropped by the 25b lane as
"out of scope" — turned out to be **load-bearing**: without it a phone
cannot submit a name and the game is unplayable past the first screen.
Found only at the end, by device testing, at the cost of a lane.

This issue makes deviations **first-class and auditable** instead of
prose scattered through comments:

1. **A single register** (`deviations.md`) — every place the port does not
   behave like the C++, with: what the C++ does (file:line), what the port
   does, why, the risk (user-visible? determinism? data?), and the tracking
   issue/decision. No deviation may exist only as a code comment.
2. **A machine check** (`tools/check-deviations.sh`) — greps the crates for
   the deviation markers (`not ported`, `unported`, `documented no-op`,
   `no-op (the C++`, `placeholder`, `TODO(`, `divergenc`, `approximat`,
   `simplif`, `out of scope`) and fails when a hit has no register entry
   (match by file + the register's `path:line`/`path:marker` keys). This is
   the mechanical half of issue 27's guard, for deviations specifically.
3. **A triage verdict per deviation** — accepted (with reason) vs FIX
   (scheduled) vs needs-user-decision. The register is the input to that.

## What the orchestrator's initial sweep already found

Marker counts in `crates/` excluding tests: `no-op` 100, `placeholder` 26,
`TODO(` 23, `unported` 18, `documented no-op` 14, `not ported` 7,
`out of scope` 5, `divergenc` 2, `approximat` 1, `simplif` 2.
(Counts overstate intent — many are legitimate C++ no-ops or test doubles —
so the sweep is a *starting list* for triage, not a verdict list.)

Categories visible from the sample:

| Category | Examples seen | Likely verdict |
|---|---|---|
| Platform seams (deliberate) | `NoJoystick` (gamepad), `NoSteam`/`NoDiscordCore` feature-off, `NoAudio` before 025e | accepted (documented), but each needs a line |
| Dead C++ code | `custom_wall_manager` "no definition (dead/unused)", `setForceLeaderboardRefresh` "dead in ..." | accepted — verify the C++ claim, cite it |
| Not-yet-ported surface | `dispatch.rs:263` sodium init, `compose.rs:456` (issue 17 era), `print_lua_docs` mutation API, `mustRecreate→recreateWindow` | FIX or decision |
| Draw-path remainder | `windowed.rs` 13b/`MenuGame::draw` notes, `mouseHovering` unported, favorites drawer | FIX (13b lineage) |
| **Load-bearing omissions (the dangerous class)** | **the 9-zone grid** (025f is fixing), possibly others like it | **FIX — this is why the audit exists** |
| Determinism/data risks | any deviation touching arithmetic, RNG, file formats, or ordering | must be explicitly cleared (D-000/D-006 class) |

## Method (workers; keep each lane small)

**Lane 28a — inventory (mechanical):** produce the raw list. For every hit
of each marker in `crates/` (excluding `tests/`), emit a register skeleton
row: file:line, the exact comment text, the surrounding function, and the
C++ counterpart if the comment names one (verify the citation exists — the
D-014 rule). No judgement, no fixes. Output: `deviations.md` with ~N rows
grouped by crate. Must be reproducible: the row order = the marker-script
output order.

**Lane 28b — triage + checker:** for each row, assign a category and a
verdict (accepted / FIX / decision), *verifying* the "accepted" claims
against the C++ where the comment asserts something about it (e.g. "dead
in C++" must be shown dead). Write `tools/check-deviations.sh` and wire it
into `tools/gate.sh` as a stage (or a separate `tools/audit.sh` if gate
runtime matters). Report the FIX list, ordered by user-visible risk.

**Lane 28c…n — the FIX list, one lane per cluster** (the load-bearing ones
first). The 9-zone grid is the first (already in flight as 025f).

## Standing rules (to prevent recurrence)

1. A deviation that changes user-visible behavior, gameplay, data formats,
   or arithmetic is **not** "out of scope" — it needs a decision entry and
   a tracker row before the lane lands (D-025-style landing discipline).
2. Code comments may not be the only record (issue 27's rule, restated).
3. Any lane that drops a C++ behavior must add its register row **in the
   same commit** (the register is edited by the lane, not later).
4. Device/desktop "we could not test this" is not a deviation verdict — it
   is a MANUAL check that must be listed.

## Status

- Orchestrator sweep: done (above). D-032 recorded (platform-neutral input).
- 025f in flight (the grid — the first FIX).
- 28a/28b: not started (worker-sized).

## Verification log (orchestrator — the 9-zone grid claim)

Before dispatching the FIX, the claim behind it was verified in BOTH
directions (the D-014 rule applied to a decision, not just a report):

- **C++: the grid is UNGATED.** In `MenuGame::update()` (starts
  `MenuGame.cpp:2510`) the state-dependent blocks are at :2647/:2679/:2702
  (`state ==` tests) and the event switch at :2875; the
  `if(window.getFingerDownCount() == 1)` grid block sits at **:2802**, i.e.
  a TOP-LEVEL statement reachable in every state — including
  `ETLPNewBoot`/`ETLPNew` (name entry).
- **Port: the handlers exist.** `oh-menu/src/menu_game.rs:1578`
  `ok_action()` already implements the `ETLPNewBoot | ETLPNew` arm:
  `create_profile_from_entered()` → `play_sound_override("openHexagon.ogg")`
  → `change_state_to(States::SMain)`. So the port has the *action*, and only
  the *dispatch* (the grid that calls it) was missing — which is why
  entering a name on the phone did nothing: the sole device caller was a
  keyboard Enter that Gboard's Done key never sends.

This is the ideal outcome for a deviation fix: the C++ reference is
unambiguous, the port's handler is already 1:1, and the missing piece is a
faithful transcription of one block (plus its finger seam).
