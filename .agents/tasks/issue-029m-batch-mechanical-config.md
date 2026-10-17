# Task 029m-batch — the mechanical/transcription + config batch (6 audit fixes)

Six small, verified-by-audit fixes, all **host-verifiable** (the phone's ADB is
down — see Hygiene). One lane, but **one commit per item** so each stays
reviewable. Master green: gate 4/4, **759 tests**. **No worktree** (D-035).

Each item below is an audit finding whose citation the orchestrator re-derived.
Read the cited C++ before changing anything; the audit reports
(`.agents/audits/update.md`, `config.md`, `replay.md`) hold the full evidence
tables with absence proofs.

---

## 1. 29d1 — `go_to_menu(false, false)` drops the score send/save  [uservisible-gameplay]

C++ declares `void goToMenu(bool mSendScores = true, bool mError = false);`
(`include/SSVOpenHexagon/Core/HexagonGame.hpp:417`) and BOTH transcribed call
sites call it with **no arguments** (`src/SSVOpenHexagon/Core/HGUpdate.cpp:399`
and `:576`), so the default runs `if(mSendScores && !mError) saveReplay();`
(`HexagonGame.cpp:1263-1265`) — which submits the local best and sends/saves the
replay.

The port's `go_to_menu` implements that branch faithfully
(`hexagon_game.rs:1622-1624`), and its own `trigger_go_to_menu`
(`hexagon_game.rs:2233-2234`) already passes `(true, false)` — but the two sites
transcribed from HGUpdate.cpp pass `(false, false)`
(`crates/oh-game/src/update.rs:442`, `:707`). **Fix: `(true, false)` at both.**

Test it: after a death-triggered `go_to_menu`, assert `save_replay` ran (and
after a joystick-Exit one); keep an `mError`-path test showing the score is
NOT sent.

## 2. 29d2 — the PB-particle cosmetic RNG draw order  [uservisible-cosmetic]

C++ `makePBParticle` draws from ONE global `pcg32_fast` stream in the order
**x → rotation → scale → alpha → vx → vy → angVel**
(`HGUpdate.cpp:944-955`). The port draws **alpha first**, then x/rot/scale/vx/vy/
angVel (`update.rs:1160` before `:1164`), so every particle takes C++'s 4th
value as its x. The in-code comment at `update.rs:1158` already documents the
*correct* C++ order — make the code match it (and keep the comment honest).

No replay impact (this is `cosmetic_rng`, a separate stream), but the port's own
comment being right while the code is wrong is exactly what to fix.

## 3. 29s1 — the `scoreOverride` `readVariable` error path  [internal]

C++ throws out of `update()` (uncaught — there is no try/catch in the SSVStart
timer chain), which is a crash. The port does `host_error` + skip
(`update.rs:316-321`) and its comment claims "the same Lippincott behavior" —
which is wrong: Lippincott only wraps `runLuaFunctionIfExists`, not a direct
`readVariable`. **Either match the C++ (and say so) or keep the port's safer
behaviour and correct the comment + record it as a deviation** — your call, but
the comment must stop asserting something false. Justify whichever you pick.

## 4. 29f1 — RESET CONFIG keeps the custom binds (and skips the recompute)  [uservisible-gameplay]

C++ `resetConfigToDefaults()` resets **all 101** `LinkedValue`s — its
`resetAllFromDefault()` iterates the full `X_LINKEDVALUES` macro whose last line
is `X_LINKEDVALUES_BINDS` (`src/SSVOpenHexagon/Utils/Config.cpp:373`,
`:455-460`, `:538-544`) — and ends with `reapplyResolution()` /
`recalculateSizes()` (`:535`).

The port's `reset_config_to_defaults` (`crates/oh-config/src/config.rs:919-994`)
resets only the **73** non-bind values, so custom keyboard/joystick binds survive
a "RESET CONFIG", and it never recomputes `sizeX`/`sizeY`/`zoomFactor` (no
`recalculate_sizes` caller exists anywhere). Fix both; test that after a reset a
custom bind is back to its default AND the sizes were recomputed. Note the doc
comment mislabels the current behaviour — fix that too.

## 5. 29l1 — a coerced-type `config.json` EXITS the app  [data-format]

C++ reads the config with jsoncpp, which **coerces** (`"vsync": 1`,
a numeric element in a combo array, etc.) and loads fine
(`extlibs` jsoncpp `jsoncpp.inl:1177-1322`). The port's `load_config`
**hard-fails** on the same file (`crates/oh-data/src/json.rs:336-381`), so the
app exits on a config the C++ accepts — affecting all 101 keys.

Match the C++ coercion behaviour for the cases jsoncpp handles, and add tests for
the coercions (at minimum: a boolean key written as `1`/`0`, and a string-typed
value where a number is expected, or whatever `jsoncpp` actually coerces — read
it, don't guess). A user hand-editing their config must not brick the game.

## 6. 29r1 — the config desktop-mode placeholder + the absent Config logging  [uservisible-cosmetic]

C++ reads `sf::VideoMode::getDesktopMode()` (`Config.cpp:480,487`); the port
hardcodes 1920×1080 (`crates/oh-app/src/lifecycle.rs:19-20`), so automatic
resolution sizing is wrong on any non-1080p desktop. Fix it using the available
monitor query (winit/glutin — whatever the window layer already exposes), and
match the C++'s fallback behaviour when there is no monitor (headless).

Also: **all** Config console logging is absent — the C++ logs found/override/
saving/resetting lines, the invalid-bind warning, and parse errors. Port them to
the port's logging convention on the same channel the C++ uses (`ssvu::lo`
→ the port's `backend.log(...)` pattern used elsewhere).

---

## Hygiene

- **The phone's ADB listener is DOWN** (`adb connect 192.168.10.138:38721` →
  connection refused; the host can ping it). **Do not spend time on device work —
  none of the six items needs a device.** Everything here is host-verifiable.
- Commit **once per item** (6 commits), messages naming the row id. Never
  `git add -A` if anything else might be mid-flight; stage explicit paths.
- Keep the gate green after each item: `bash tools/gate.sh` (expect 759 + yours).
- `.agents/audits/*.md` are committed audit reports — do not rewrite them.
- Add/refresh the `deviations.md` rows for the fixes that are deviations (issue
  28's rule): 29d1, 29f1, 29l1 are the ones that matter most.
- Append a short `## Report` to this file at the end: per item — the fix, the
  citation you verified, the test, and anything you could not establish.
