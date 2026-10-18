# Task 039 — Android device bugs: a tap fires multiple events, the end of a level locks up, and there is no music

Read `.agents/WORKER-HYGIENE.md` first (§6 is the device section, §5 is stop-and-report).
Operator report while testing on their phone, verbatim:

> *"doing some testing of the android, the input is really wonky, it sounds like a press creates
> multiple events. at the end of a level I can't do anything, and is there supposed to be music?
> there's no music"*

Three symptoms. **Reproduce on the device before fixing anything** — an un-reproduced fix is a
guess, and this file records what is already known so you do not start from zero.

---

## STEP 0 (mandatory, do this FIRST): make the device diagnosable — issue 35

Every symptom below is *blind* without the port's own logs. The port's `[channel]` lines do not
reach logcat on Android, which is why the previous device sessions had to navigate by OCR and
PIL screenshots. Since issue 026 landed the platform-neutral `oh-game` `log_sink` seam (the
engine's `log()`/`host_error` route through `emit_log`, with `oh-web` installing a
browser-console sink before `main`), the Android equivalent is small: **install an
`__android_log_print` sink in the Android entry point** (the same seam, a `cfg(target_os =
"android")` sink installed at boot, tagged so `adb logcat -s <tag>` works). Land it on its own
commit and confirm with logcat that the engine's lines appear.
**Do the rest of this task with logs in hand.** If you skip this, you will be guessing at
timings you cannot see.

## Symptom 1 — "a press creates multiple events"

What is known from the code, in priority order:

1. **The menu's 9-zone grid is PER-FRAME** (`crates/oh-menu/src/menu_game.rs:3030`:
   `if touch.finger_down_count() == 1 { …thirds dispatch… }`) — faithful to the C++
   (`MenuGame.cpp:2802-2835`). So while the touch sink holds a finger down, the grid dispatches
   `leftAction`/`rightAction`/`okAction`/`upAction`/`downAction` **every frame**. On desktop
   this is invisible (`WindowEvent::Touch` never arrives); on Android it is the hot path. The
   C++ has the same per-frame grid, so the question is **what differs**: the C++'s touch data
   (SFML) vs the port's sink. Check whether the finger-down slot is cleared on `TouchEnded`
   promptly, whether `finger_down_count()` ever reports 1 when no finger is down, and whether
   the Android fold emits *both* a touch event and a synthesized mouse event for one tap (the
   port already documents "SFML's window layer converts a touch release into a mouse event" at
   `crates/oh-app/src/windowed.rs:1437-1438` — if BOTH are dispatched, a tap is two inputs).
2. **The release-edge machinery**: 025g found and fixed a shared release-tail
   `--ignoreInputs` decrement (the menu's key binds stayed dead after a game→menu drop) — the
   touch path may have an equivalent gap, letting repeats through.
3. Evidence to gather: with STEP 0 in place, count dispatches per tap; log the touch sink's
   state (which slots, what positions, for how many frames) per frame across one tap; and
   compare that frame-by-frame trace against the C++ grid's semantics. Report the *numbers*
   (N events for one tap), not an impression.

## Symptom 2 — "at the end of a level I can't do anything"

Hypotheses, in priority order:

1. **`ignore_inputs` stuck above zero.** `crates/oh-menu/src/menu_game.rs:287` initialises
   `ignore_inputs: 1`; the C++ decrements it in the release tail (`MenuGame.cpp:576-601`,
   `644-666`, `694-700`) and 025g fixed ONE missing arm. A different arm — the end-of-level /
   load-results / death path — may still be missing, so input stays swallowed forever. Log
   `ignore_inputs` per frame through the death → results-screen → menu sequence: if it never
   returns to 0, that is the bug.
2. **The screen waits for an input a touch cannot produce** (a key-only affordance), which is
   the same class as the tutorial's focus key (issue 31).
3. **The game→menu transition** (`go_to_menu(true, false)`, 029d1) leaving the menu in a state
   whose actions are all no-ops.
Reproduce: die in a level, then try to navigate; log the menu state + `ignore_inputs` across
the whole sequence and report where input stops having an effect.

## Symptom 3 — "is there supposed to be music? there's no music"

**Yes, there is supposed to be level music** (the C++ streams the pack's music; the port has
`oh-audio` with `MusicCtx`, the asset music-path map, and `play_level_music()` at
`crates/oh-game/src/hexagon_game.rs:860`, gated by `should_play_music()` at `:1861`).

**A measured fact you should not ignore**: on the operator's phone right now,
`dumpsys media.audio_flinger` reports `numTracks=1`, `activeMask=0x1`, `writeErrors=0`,
`underruns=0` — **an audio track IS active and being written**. So the device opens and a
stream flows; the bug is in **what plays, or at what volume**, not in the output device. That
narrows it to:

1. **the music asset path on Android** — the pack's `Music/` file resolving differently in the
   extracted `_RELEASE` tree (log the resolved path + whether it exists);
2. **volume** — the level music volume (config / per-stream) being 0 (log the value at play
   time);
3. **`should_play_music()` returning false** for that level (the C++ gate: check what it reads);
4. **the music decode failing silently** while the SFX decode (a different asset) works.
**First triage question, answer it with the device, not by reasoning: do the SOUND EFFECTS
play?** (death/rot sounds, the menu blips). If the SFX are audible and only the music is
silent, it is a music-path bug; if *nothing* is audible despite the active track, it is a
volume/decode bug. Report which, with evidence.

## Device facts and technique (reuse — do not rediscover)

- The phone is connected at **192.168.10.138:41611** (the port is random per wireless-debugging
  enable; if it refuses, `adb mdns services` may find nothing here and a scan of 30000-50000
  with `nc -z` is the fallback). `run-as` needs a **debuggable** build: the release APK
  installed now is NOT (`flags=[ALLOW_CLEAR_USER_DATA ALLOW_BACKUP]`), so build a dev-profile
  APK when you need file-level evidence, and revert any temporary `debuggable = true` in its
  own commit.
- The driving technique is written up in `.agents/tasks/issue-029k1-luajit-version-and-anr.md`
  ("Device session 2026-10-17"): the boot text field opens the IME which then eats every later
  tap (`input keyevent 66` gets out); a short `input tap` is unreliable because the game
  samples per-frame, so use `input swipe X Y X Y 400` as a HELD press; the authoritative
  zone map is `MenuGame.cpp:2801-2834` (middle band → left/right thirds = left/right actions,
  MIDDLE third = ok; top band = up; bottom band = down); and navigation is done by
  DIFFERENTIAL SCREENSHOTS (a 16x16 brightness fingerprint), because the port's logs did not
  reach logcat — **which STEP 0 fixes**.
- Device evidence is TEXT (`logcat -d | grep`, `dumpsys`, `/proc/<pid>/stat`, PIL *analysis*),
  and it is capped at **ONE image read for the whole lane**. `adb shell input` can drive one
  pointer; multi-touch needs `sendevent`, which SELinux blocks for shell on this device
  (recorded) — so a two-finger acceptance is the operator's, not yours.

## Deliverable

One commit per fix (each with its reproduction and its evidence), STEP 0 first and separately,
then a `## Report` appended here: the reproduction steps per symptom, the root cause of each
(and the fix), the evidence lines, and **explicitly what you could NOT reproduce or establish**
— the operator may be describing timing or a level-specific path you cannot recreate, and a
precise negative ("could not reproduce symptom 2; here is the exact sequence I tried, with
logs") is a good outcome. Do not claim a fix for a symptom you could not reproduce.

## CORRECTION on Symptom 2 (operator, same session) — it is NOT the `ignore_inputs` bug

Operator, verbatim: *"symptom 2 isn't quite right, it's asking for 'Y' or 'joystick 6' inputs, and
there's no way to press it. In general the android build needs different input defaults than the
desktop build."*

Verified in the C++ tree, both halves:
- **'Y' is the REPLAY trigger**: `src/SSVOpenHexagon/Global/Config.cpp` has
  `X(triggerReplay, trig, "t_replay", cil{cmb{{k::Y}}})`, and the end-of-level prompt is built
  literally at `HexagonGame.cpp:806` (`status.replayInput += " OR JOYSTICK " + joystickButton`)
  and `:808` (`status.replayInput = "PRESS " + status.replayInput + " TO REPLAY\n"`). So the
  screen asks for a keyboard/joystick input a touch-only device cannot produce — **no swallowed
  input, just no affordance.** The `ignore_inputs` hypothesis is therefore DROPPED (it remains a
  real hypothesis only for a genuinely dead menu after a game→menu drop; do not spend the lane on
  it unless the enumeration in §Next shows that shape).
- **The C++ has NO Android-specific bind defaults**: `Config.cpp`'s bind table carries no
  platform `#if` (only unrelated `__clang__` pragmas). So the upstream Android build has this
  same gap — which makes the fix a **sanctioned DEVIATION (the F28 / issue-031 class)**, not a
  parity repair.

### The generalised requirement (the operator's actual ask)

Enumerate the C++'s entire default bind table (every `X(trigger…, trig, "t_…", cil{…})` row in
`Config.cpp`) and, per action, state whether it is reachable on a touch-only device TODAY — via
the issue-031 overlay (whose buttons are exactly `Exit, Focus, Swap, RotateCCW, RotateCW`), the
menu's 9-zone grid (`MenuGame.cpp:2801-2834`), the touch→mouse conversion, or nothing. **The
"nothing" rows are the work list**; a first read says the gameplay gaps are at least Replay,
Restart and Pause, with screenshot/lua-console being desktop-only by nature.

Implement the gaps through the mechanism that already exists rather than a new one: the overlay
drives the SAME per-`Tid` two-state inputs the keyboard drives
(`two_state(Tid::Focus, …)`, `crates/oh-app/src/windowed.rs:1648-1649`), so buttons driving
`Tid::Replay`/`Tid::Restart`/`Tid::Pause` inherit rebinding and the replay-bit record for free.
Keep the overlay platform-neutral (031's design: enabled on Android, inherited by wasm). And
settle "different defaults" explicitly — either the overlay covers everything (recommended:
discoverable, and F28's precedent) or `oh-config` diverges per platform; **desktop defaults must
not change**, whichever route is chosen, and the choice gets a `deviations.md` F-row citing that
the C++ has no Android bind defaults. The end-of-level hint should also stop telling a phone user
to press 'Y' (the C++ builds that string from the binds; say what you did about it).

## Report (2026-10-18 — lane 039, worktree `oh-lanes/039`, branch `lane/039` off master `5301c96e`)

Device: the operator's phone (moto g power 2021, Android 11, arm64-v8a, display 1600×720),
wireless ADB at `192.168.10.138:41611`. App `info.vittorioromeo.openhexagon`.

### Commits (one per fix, STEP 0 first)

| commit | content |
|---|---|
| `836be959` | **STEP 0** — the Android engine-log sink: the `oh_game::log_sink` seam's `__android_log_write` route (the `oh-web` browser-console sink's twin), installed at the top of `oh_app::android::run`, tag `OpenHexagon` for `adb logcat -s`; the stale D-026 `NoopAudio` module-doc bullet corrected (025e landed the real `oh-audio`) |
| `c73b75aa` | **s3 log half** — `oh-audio` logs the `set_sound_volume` / `set_music_volume` values at boot, the resolved music path + file existence + the music volume at play time, the re-open/decode failures; `play_level_music` logs `should_play_music`'s inputs + result + the resolved segment (the C++'s `playLevelMusic` logs nothing — the device-diagnostic half, issue 026's evidence-path class) |
| `4ec21cf1` | **s2** — the F31 touch action row (the `Replay` / `Restart` / `Pause` buttons + the `action_active` gate + the class-scoped clears + the Android end-of-level hint override) + the `deviations.md` F31 row |
| `cb2217a2` | **s1 log half** — the 9-zone grid's per-dispatch line (zone + finger position) |
| `93d86438` | **s2 cont.** — the F31 action row's draw gate (the device found it gameplay-gated: `o.active()` → `o.enabled() && (o.active() || o.action_active())`) + the sink's boot-confirmation line |

### STEP 0 — done, confirmed on device

The sink routes the engine's `[channel]` lines (`log()` / `host_error` → `emit_log`) to logcat
under the `OpenHexagon` tag. Confirmed lines (this build, live device):

```
I OpenHexagon: [android] engine boot: the OpenHexagon logcat sink is installed (issue 039 STEP 0)
I OpenHexagon: [playLevelMusic] shouldPlayMusic: window=true audio_some=true no_music=false => true
I OpenHexagon: [Replay] Attempting to send and save replay...
I OpenHexagon: [Replay] Failure sending replay
```

Note: the composition's own `eprintln!` diagnostics (the `[oh-audio] …`, `[oh-menu] …`,
`[Replay] Successfully saved …` lines below) reach logcat on the second, pre-existing route —
the `RustStdoutStderr` tag (stderr-to-logcat on the Android app). Both routes are live.

### Symptom 1 — "a press creates multiple events" — MEASURED: C++ parity, not a port bug

Reproduction (device, this build): one HELD press, `input swipe 800 100 800 100 400` (400 ms,
the menu's top band — `up` zone):

```
[oh-menu] grid dispatch zone=up pos=(730,100)   ×18, 22:24:53.317 → 22:24:53.713 (396 ms of held frames)
count: 18
```

**18 dispatches for one 400 ms held press ≈ one per rendered frame** (this device runs the game
at ~45 fps). Root cause of the "multiple events": the C++'s 9-zone grid
(`MenuGame.cpp:2802-2835`) dispatches the zone's action **every frame** while
`getFingerDownCount() == 1` — and the `touchDelay = 50` every action sets is WRITE-ONLY
(the sole read is the `:2787-2789` decay; it gates nothing on this path). The port's grid
(`menu_game.rs:3030`) is the verbatim port, so N held frames ⇒ N dispatches in the C++ too.
Ruled out, with the per-dispatch line as the instrument: (a) a post-release sink leak — the
18 lines span exactly the held window, none after the release; (b) a touch+mouse double-fire —
the port's winit fold synthesizes NO mouse-button event from a touch (the C++'s SFML does the
same on Android). The operator's "wonky" feeling is answered by the sanctioned-deviation route
(the F31 action row + the exit affordance, s2 below), NOT by changing the grid (a per-tap
dedup would be a deviation from the C++). Device coordinate note: the app window is 1530×720
offset +70 px in x (the display's top-left cutout) — grid `pos = (display_x − 70, display_y)`.

### Symptom 2 — "at the end of a level I can't do anything" (rescoped by the operator)

The operator's re-scope, verbatim: *"it's asking for 'Y' or 'joystick 6' inputs, and there's no
way to press it. In general the android build needs different input defaults than the desktop
build."* The spec's original hypothesis 1 (`ignore_inputs` stuck > 0) is **dropped, replaced by
the re-scope** — the input gates were not the problem; the problem is a missing touch
affordance.

**Root cause.** The C++'s end-of-level screen (`HexagonGame.cpp:806/:808` — "PRESS Y … TO
REPLAY", "PRESS SPACE … TO RESTART") demands the `Tid::Replay` / `Tid::Restart` keyboard
binds, and `Config.cpp`'s default bind table carries **no Android/platform `#if` at all**
(0 occurrences of `__ANDROID__` / `SSVOH_ANDROID` in `Config.cpp`) — so a touch-only device has
no Y/Space, and the game-over screen is unreachable on touch in the C++'s shipped shape too.
The C++'s own touch control (`HGUpdate.cpp:589-601` — rotation-only, the screen halves) has no
replay / restart / exit-at-death either, and this port's F28 overlay's single `active` gate
(playing only) hid even its own `Exit` button at death.

**Fix (F31 — `deviations.md` row, a D-034-class sanctioned addition, platform-neutral by
construction).** The overlay gains the `Replay` / `Restart` / `Pause` buttons — the C++'s
`Tid::Replay` / `Tid::Restart` / `Tid::Pause` bodies (`HexagonGame.cpp:399-407` / `:389-397` /
`:421-441` — the `death_input_ignore` + `has_died` gates, the `in_replay` replay-skip, the
debug-gated `debugPause`), edge-triggered like the C++'s `Type::Once` binds (so the replay's
bit record and the rebind paths are unaffected — the buttons drive the SAME `Tid` inputs the
keyboard drives). The two gates are CLASS-scoped: the gameplay buttons (`RotateCCW` /
`RotateCW` / `Focus` / `Swap`) stay under `active` (playing only, the spec §2.4/§2.5 half);
the action buttons (`Exit` / `Replay` / `Restart` / `Pause`) answer to `action_active`
(`GameSlot::Game && started` — the whole game state: playing, game-over, replay playback — the
keyboard binds' reachability, deliberately WITHOUT the `!in_replay` half). A death clears only
the gameplay captures; an end-of-game only the action ones. The action row: top — `Replay` /
`Restart` above the focus / swap x's, `Pause` top-right, 16 pt labels (the button LABEL is the
input name). `windowed.rs` applies the edges to `trigger_replay` / `trigger_restart` /
`trigger_pause` (the console-gating follows the C++'s `notInConsole` wrappers — `Tid::Pause`
the ungated one) and feeds `set_action_active`. The end-of-level hint's Android half (the
`cfg(target_os = "android")` override: "PRESS RESTART TO RESTART" / "PRESS REPLAY TO REPLAY";
the desktop strings stay C++ verbatim — the host test pins them).

**Device status — the input half is CONFIRMED; the draw half is CONFIRMED (051, 2026-10-18 —
see the results paragraph after the check list below).** Confirmed on device (this build, live): the in-game `MENU` (exit) button took
the game back to the menu mid-level; a level was started (the default tutorial level
`babysteps`, then the orthoplex pack's `bipolarity`); death was reached (the `death.ogg` +
`personalBest.ogg` decodes, the replay save — all in logcat); and the ONE image read of the
lane (the game-over screenshot, `/tmp/oh039-gameover.png`) shows the new hint text live:
`1.054 / OFFICIAL MODE / SWAP ENABLED / PRESS RESTART TO RESTART / PRESS REPLAY TO REPLAY /
NEW PERSONAL BEST! / BIPOLARITY ORTHOPLEX 1x by VITTORIO ROMEO`. What that screenshot did NOT
show: the action-row buttons themselves — the draw gate was still the gameplay half
(`o.active()`), so at `has_died` the row never painted (the INPUT path was live — the in-game
MENU tap worked — but the DRAW half was not). That is `93d86438`'s fix. Provable today,
stated plainly: host gate green (fmt/build/clippy/tests/lua-parity), the aarch64
`cargo check` green, the rebuilt release APK built after `93d86438`
(`target/release/apk/open-hexagon.apk` — the project's self-distribution keystore, verified
byte-identical fingerprint to the installed APK, so `adb install -r` is legal with no data
loss). **When the device returns, the precise check list is:** (1) `adb install -r
target/release/apk/open-hexagon.apk`; (2) reach a game-over screen (menu → hold `ok` zone
(795,360) 500 ms → level select → hold `down` zone (795,600) 500 ms → hold `ok` 500 ms →
hold the left half (400,360) ~4 s) and screenshot — the `MENU` (top-left) / `REPLAY` /
`RESTART` (top-middle) / `PAUSE` (top-right) buttons must be DRAWN over the results screen;
(3) hold `RESTART` (display (875,44)) 400 ms → the level restarts (the `playLevelMusic` log
line re-fires); (4) reach death again and hold `REPLAY` (display (795,44)) → the death
replay plays; (5) hold `MENU` (display (114,44)) → the main menu; (6) **leave the device at
the main menu**.

**051 (2026-10-18) — the check list ran, on the operator's device (ADB TCP
`192.168.10.138:40119`); the draw half is now CONFIRMED.** Step 1: the lane rebuilt the
release APK from master + 051's instrumentation commit `6fcfe13d` and `adb install -r`'d it
(same keystore — the pre-existing install's signature matched, no data loss). Steps 2 (draw):
the ONE image read of the 051 lane (`.agents/evidence/lane-051/oh051_d0.png`, the game-over
screen after a death on `examplelevel`) shows the action row drawn over the results screen —
`REPLAY` / `RESTART` (top-middle, flanking the focus/swap x's), `PAUSE` (top-right), the
`MENU` (Exit, top-left), plus the Android hint line bottom-centre ("swipe left/right =
CCW/CW", the C++'s `Packs/android.txt` → `android_menu_text` — the row's position oracle). The
earlier "buttons didn't paint" state (pre-`93d86438`) is gone: at `has_died` the row paints.
Steps 3–5 (function, held-press tests + text/pixel oracles): RESTART (875,44) held 400 ms →
the `playLevelMusic` line re-fired for the same level (the level restarted — re-confirmed in
a second cycle); REPLAY (795,44) → the replay scene played between the death screen and its
return (temporal brightness series: death 31.5 → replay 173.0 → death 30.8); MENU (114,44) →
back to the main menu (Level Selection, 0.0% pixel diff vs a known menu capture). The
`oh-menu` grid-dispatch log lines (the 039 STEP 2 instrument) confirm each held press landed
in the right zone. Step 6 honored: the device was left at the main menu.

**The C++ default bind table, as a touch-only reachability table** (`Config.cpp:255-288`'s
key/mouse defaults + `:245-252`'s joystick defaults; "C++ shipped" = the rotation-only touch
control + the menu's 9-zone grid; "port + F31" = this lane's build):

| `Tid` | C++ default (keys / mouse / joystick btn) | C++ shipped on a touch-only device | port + F31 |
|---|---|---|---|
| `RotateCCW` | `A` / `←` / left click / j=1 | screen left half (the C++ touch control) | overlay `CCW` + screen half |
| `RotateCW` | `D` / `→` / right click / (no j bind) | screen right half | overlay `CW` + screen half |
| `Focus` | `LShift` / XButton1 / j=4 | **none** (the F28 finding) | overlay `Focus` |
| `Select` | `Space` / middle / j=1 | menu grid `ok` zone | menu grid `ok` zone |
| `Exit` | `T` / XButton2 / j=2 | **none** in-game (the Android back key is the C++'s out) | overlay `MENU` (whole game state) |
| `ForceRestart` | `↑` / `R` / j=3 | **none** | none (C++ parity — the force-quit is a debug affordance) |
| `Restart` | `Space` / `Enter` / middle / j=0 | **none** — the lock-up | overlay `RESTART` |
| `Replay` | `Y` / j=6 | **none** — the lock-up | overlay `REPLAY` |
| `Screenshot` | `F12` / j=7 | none | none (C++ parity) |
| `Swap` | `Space` / middle / j=5 | **none** (the F28 finding) | overlay `Swap` |
| `Up` | `W` / (no j) | menu grid top band | menu grid top band |
| `Down` | `S` / (no j) | menu grid bottom band | menu grid bottom band |
| `NextPack` | `PageDown` / j=11 | none in-game | none (C++ parity) |
| `PreviousPack` | `PageUp` / j=10 | none in-game | none (C++ parity) |
| `LuaConsole` | `F1` / (no j) | none (the C++ Android build has no ImGui at all — `#ifndef SSVOH_ANDROID` in `Imgui.cpp`) | none (parity) |
| `Pause` | `F2` / (no j) | none | overlay `PAUSE` (a release no-op, exactly as the `F2` key — the body's debug gate) |

The "different input defaults" question, settled: the C++ has NO Android bind defaults (the
bind table is platform-blind), so the port's touch affordances are a documented deviation
(F31), not a port of a C++ behaviour — the desktop defaults are untouched.

### Symptom 3 — "is there supposed to be music? there's no music" — CONTENT GAP, not a port bug

Supervisor decision A: closed as a content gap, nothing added to the repo. The port's contract
is 1:1 parity with upstream, INCLUDING its content: upstream ships the `Music/*.json` metadata
and no OGGs (verified on GitHub `VittorioRomeo/SSVOpenHexagon` master + the release tags, the
fork history, and every local copy of the content), so the C++ with the same `_RELEASE` is
exactly as silent as the port. The port is CORRECT here; the operator's "is there supposed to
be music?" is answered: yes, the game intends to play it, this build SELECTS it, and the
shipped content has no audio to play.

Evidence (live device, this build — the three log lines the spec's direction 1-3 instrumented):

```
I RustStdoutStderr: [oh-audio] set sound volume 100 (0..=100, SFML units)
I RustStdoutStderr: [oh-audio] set music volume 100 (0..=100, SFML units)
I OpenHexagon: [playLevelMusic] shouldPlayMusic: window=true audio_some=true no_music=false => true
I OpenHexagon: [playLevelMusic] segment 0 @ 1.310s (music id 'drFinkelfracken', pack 'ohvrvanilla_vittorio_romeo_tutorial_1')
I RustStdoutStderr: [hg::AudioImpl::playMusic] No path for music id 'ohvrvanilla_vittorio_romeo_tutorial_1_drFinkelfracken'
```

(i) The volume chain is HEALTHY, with the measured device evidence: config 100/100 (SFML
units) → stored as 0-100 → applied to rodio as `(vol / 100).clamp(0, 1)` (`oh-audio/src/lib.rs`
— the explicit conversion; the SFML-0-100-vs-rodio-0-1 mismatch hypothesis is dead: 100 →
1.0). The operator's own "the android blips and death play" test confirms SFX audibility,
which kills the device/volume/decode hypotheses for the music too — the SFX decode/play fine
on the same live stream (the `select.ogg` / `go.ogg` / `death.ogg` / `personalBest.ogg`
decodes above). (ii) The port's music path is CORRECT: it builds the `{packId}_{musicId}`
asset id from the level's `musicId` + the pack's `BuildPackId` (the C++'s `Assets.cpp:936-962`
keying, byte-verified), resolves the path, and the `No path for music id …` line is the whole
story — the lookup runs and finds no file. (iii) The gap is upstream-faithful: `_RELEASE`
packs zero music OGGs (the `Packs/*/Music/` trees carry only the `.json` metadata; the
`Assets/` tree carries the 14 SFX OGGs — and not even `blip.ogg` / `swapBlip.ogg`, the
`decode failed for sound 'blip.ogg'` line in logcat being the SFX half of the same gap,
parity with upstream). A player who wants music adds the OGGs locally under
`_RELEASE/Packs/<pack>/Music/` (the official OHRM distribution, `vittorioromeo.it`, is the
source — noted once, not an action).

### What could NOT be reproduced / established (explicit negatives)

- **The spec's original symptom-2 hypothesis** (`ignore_inputs` stuck > 0 at the end of a
  level): dropped per the operator's re-scope, not pursued — the input gates were demonstrably
  live on device (the in-game MENU button, the grid dispatches after a game→menu return).
- **A port-side defect behind symptom 1** (an extra dispatch beyond the held frames, a
  post-release sink leak, a touch+mouse double-fire): not observed — 18 dispatches over a 396
  ms held window ≈ 1/frame, none after release, no synthesized mouse event (the fold's
  analysis + the dispatch line agree). The "multiple events" ARE the C++'s per-frame grid —
  parity, not a bug.
- **A port-side music code bug (symptom 3)**: not reproducible — the path resolution, the
  volume chain, and `should_play_music` all verified correct on the live device; the silence
  is the content.
- **The s2 draw fix, visually**: PENDING DEVICE VISUAL VERIFICATION — the phone's ADB listener
  died mid-run (the wireless-debugging port closed; `192.168.10.138:41611` refuses, the mdns
  record is stale, a 37000-45000 scan is empty; it needs the operator to re-enable wireless
  debugging on the phone). The fix is committed (`93d86438`), gate + aarch64 + APK-evidenced;
  the check list above is what remains.
- **The two-finger acceptance (focus held WHILE rotating, the tutorial's step-2 gate)**:
  `sendevent` is SELinux-blocked for the shell on this device (recorded in the spec), so the
  multi-touch proof is the operator's, not mine.

### Open items / next steps

1. Device return → the 5-step s2 draw verification + the device left at the main menu.
2. Optional (only if the operator asks): source the OHRM OGGs locally (the escalation's
   options B/C — a trivial follow-up, out of this lane's scope per decision A).
