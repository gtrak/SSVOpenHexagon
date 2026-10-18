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
