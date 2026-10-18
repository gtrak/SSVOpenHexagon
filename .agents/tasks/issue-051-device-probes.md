# Task 051 — the device probes: three lanes' PENDING halves, on the operator's phone

Read `.agents/WORKER-HYGIENE.md` first — **§6 is the device section and it is binding** (the phone is the
OPERATOR'S OWN DEVICE: no uninstall, no `pm clear`, no changing their settings, leave it where they left
it — and the standing rule that **device evidence is text**, not an impression). Then read the
device-driving technique already written down in `.agents/tasks/issue-029k1-luajit-version-and-anr.md`'s
"Device session" section: it contains the hard-won tricks (the boot text field opens the IME and then eats
later taps — `input keyevent 66` escapes it; the authoritative touch-zone mapping from
`MenuGame.cpp:2801-2834`; a short `input tap` is unreliable because the game samples per-frame, so use
`input swipe X Y X Y 400` as a HELD press; and the port's logs reach logcat under the tag **OpenHexagon**
since 039's STEP 0 sink).

**The operator has just said: "adb is back."** Three landed lanes each left exactly one PENDING half that
needs hardware, and this lane closes all three in ONE device session, in this order (cheapest first):

## Step 0 — install, from CURRENT master

Build the APK on this box with `bash tools/package-android.sh` (issue 045's fixes made the script
self-install the rustup target and resolve the NDK pin — read `tools/android-env.sh` if the NDK misbehaves).
039's APK was built in a worktree that has since been harvested for disk, and master now also carries
048/049/050 — so build fresh rather than reusing anything. Then `adb install -r` (the same keystore as the
installed app, so no data is lost — confirm that in the report). If the LOCAL build cannot produce an APK,
say so plainly and fall back to the CI artifact: `gh workflow run release.yml -R gtrak/SSVOpenHexagon
--ref master` builds all four artifacts without publishing anything, and the APK is downloadable with
`gh run download`.

## Probe A — 050's `GL_SAMPLES` (the cheapest: one boot, one logcat line)

050 plumbed `antialiasing_level` into all three GL contexts and reads the ACTUAL sample count back at
creation. After boot, `adb logcat -s OpenHexagon` must show its create-time `[oh-render] context MSAA` /
`GL_SAMPLES` line. Report the number verbatim. **A `GL_SAMPLES = 0` with a non-zero configured level is the
documented "no matching EGL config" case working as designed** (the pick falls back to the largest
available and warns) — that is a result, not a failure; report which happened.

## Probe B — 039's game-over action row (the visual half that was PENDING)

039 fixed the end-of-level lock-up (F31: Replay/Restart/Pause overlay buttons) and its INPUT half was
already confirmed on device; only the DRAW half is pending. On the game-over screen, with a screenshot as
evidence: the `MENU` / `REPLAY` / `RESTART` / `PAUSE` buttons are DRAWN; `RESTART` at display (875,44)
restarts the level; `REPLAY` at (795,44) plays the death replay; `MENU` at (114,44) returns to the main
menu. Remember the held-press rule from the technique above. **Leave the device at the main menu when you
are done with this probe.**

## Probe C — 049's frame cap (the one that needs a sustained run)

049 closed K2 by mirroring SFML's `Window::display` sleep on Android. With `limit_fps=true` and
`max_fps=200` in the config (put it there however is least invasive — prefer the app's own options menu over
poking their files, and if you do touch a file, restore it), measure: menu fps and in-game fps should be
≈200, NOT the ~500/~360 that K2 measured before the fix; CPU should settle from the 76–71 % that the K2
row recorded toward the C++'s lower burn; and the ANR dropbox count must stay FLAT (the 025i
non-regression — the previous method was 10 `data_app_anr` entries plus one crash in a session, so
`adb shell dumpsys dropbox --print data_app_anr | grep -c` before and after is the measurement). For fps,
prefer the same instrument the K2 row's author used if the technique file names it; otherwise
`dumpsys SurfaceFlinger --latency <layer>` gives per-frame timestamps you can convert, and say which you
used. **This is the probe where a negative (the cap still not enforced) is as valuable as a positive —
report the numbers you measured, not the numbers you expected.**

## Ground rules for the report

- **Raw evidence, not summaries**: the logcat lines, the screenshots' paths, the numbers. Every claim needs
  its command.
- Screenshots: read at most ONE image yourself per the hygiene rule; for the rest, state the file paths and
  what a PIL/alpha analysis would show rather than looking. The action-row probe needs at least one
  deliberate look, so spend it there and describe exactly what you saw.
- If a probe cannot be run (device busy, a command unavailable, the config not reachable), say so plainly
  and leave the device usable — a precise negative is a result.
- **Leave the device at the main menu, no app uninstalled, no settings changed.**
- Update the tracker rows for the three probes with what the device said (`049`'s K2 device half, `039`'s
  pending visual half, `050`'s pending GL_SAMPLES half) and append a `## Report`.

## Report (2026-10-18, operator's device — ADB TCP `192.168.10.138:40119`; the old port
`41611` refused, this one is live; the device was reachable and idle)

Device: moto g power (2021) (`motorola/borneo`, SD439 / Adreno 610), Android 11, display
1600x720 (landscape). App: `info.vittorioromeo.openhexagon` (installed 2026-10-16, the 025
series' install + updates). **APK:** this lane rebuilt the release APK from `lane/dev`
master (`a861f41c` + the instrumentation commit `6fcfe13d` below — `tools/package-android.sh`
green: signature verification, keystore match, content hashes, `APK OK`) and `adb install
-r`'d it — the pre-existing install's signature matched the project's self-distribution
keystore, so no data was lost (configs/profiles/users survived; the boot screen was skipped
past with the existing `no_profile` profile). The installed APK is this lane's build
(`lastUpdateTime 2026-10-18 10:11:47`), same signature as before.

### Probe A — `GL_SAMPLES` on the operator's device: **POSITIVE**

Logcat (tag `RustStdoutStderr` — the 039 STEP 0 sink routes the port's stdout there; the
`OpenHexagon` tag carries the JNI-callback log half), after boot:

```
[oh-render] context MSAA: requested level 4 — config with 4 samples
[oh-render] context GL_SAMPLES = 4
```

The device context honors the full requested 4× MSAA — no fallback, no
`GL_SAMPLES = 0` (the "no matching EGL config" case the 050 file described). The 050
antialiasing device-side residual is closed (the task file's bullet is updated).

### Probe B — the game-over action row: **DRAWN AND FUNCTIONAL**

One deliberate image read of the whole lane (`.agents/evidence/lane-051/oh051_d0.png`, the
game-over screen after a death on `workshopexample/examplelevel`) confirmed the draw half:
`REPLAY` / `RESTART` (top-middle, flanking the focus/swap x's), `PAUSE` (top-right), `MENU`
(Exit, top-left), plus the Android hint line bottom-centre ("swipe left/right = CCW/CW", the
C++'s `Packs/android.txt` → the port's `android_menu_text`) — the row's position oracle. The
pre-`93d86438` "buttons didn't paint" state is gone: at `has_died` the row paints.

Function, held-press tests (400 ms `input swipe X Y X Y 400` = a held touch at the button's
centre; a held press produces ~2–5 `oh-menu` grid-dispatch events at the device's input
rate — each registers the zone, which is what the text oracle shows) with text/pixel oracles
(no image reads beyond the one above):

- **RESTART (display 875,44):** the `playLevelMusic` log line re-fires for the same level —
  the level restarted. Re-confirmed in a second cycle during the sustained run.
- **REPLAY (display 795,44):** the death replay plays — temporal brightness series across
  the press: death screen mean 31.5 → during replay 173.0 (the bright replay scene) →
  death screen again 30.8.
- **MENU (display 114,44):** back to the main menu (Level Selection) — the final capture is
  a 0.0% pixel diff vs a known LevelSelection capture.
- The `oh-menu` grid-dispatch lines (the 039 STEP 2 instrument) confirm each held press
  landed in the right zone (`zone=restart` / `zone=replay` / the Exit tap).

039's pending visual half is confirmed (the task file's device-status section is updated).

### Probe C — fps / CPU / ANR on the capped build: **CAP ENFORCED; ANR FLAT**

**Instrument (and why the app's own log line is the instrument):** `SurfaceFlinger
--latency` (the standing 025j probe) saturates at ~16.7 ms in every state — the Adreno 610
display composites at its own 60 Hz whatever the app submits, so it measures the display,
not the app's loop. `dumpsys gfxinfo` reports 0 frames (the NativeActivity is not a
GraphicsStats-tracked app). The K2 row's standing probe names the instrument — "menu fps
and in-game fps from the port's own log lines (the msDraw/msUpdate-adjacent frame timing
the 025j session used)" — so this lane added one android-gated log line to
`crates/oh-render/src/window.rs` (commit `6fcfe13d`): `RunHandler::log_frame_period`
accumulates the per-frame period at the `pace_display` point (after the frame — the swap —
ran) and every 500 frames logs the average:

```
[oh-render] loop period 5.31 ms over 500 frames (~188 fps; limit in force: 200)
```

Pure instrumentation: no pacing behaviour changed (049's `pace_display` is untouched),
desktop/wasm byte-unchanged (cfg-gated), host gate + android-target check/clippy + the
release APK build all green. **It is kept** as a standing device diagnostic (the 039
log-sink class) — the next K2 re-measure needs it.

**Config in force — no config change was needed (the least-invasive option of STEP 3, zero
edits):** the shipped config (`limit_fps=true, max_fps=200, vsync=false`,
`antialiasing_level=4`, `show_fps=true`) was already in force on the device — the line's
"limit in force: 200" proves `limit_fps=true` + `max_fps=200` effective, and the 5.3 ms
period (not 16.7 ms) proves vsync off. (The options menu was not touched; the on-device
`config.json` was left exactly as found.)

**Measurements (loop line + whole-process `utime+stime` from `/proc/<pid>/stat`, 12 s
windows; the process = the app, `android_main` = the game-loop thread):**

| state | loop period | fps | process CPU | android_main CPU | K2 free-run baseline (2026-10-16) |
|---|---|---|---|---|---|
| menu (SMain) | 5.27–5.34 ms | **187–190** | 56.8% | 54.1% | ~500 fps / 71% |
| in-game, light tutorial level (babysteps, active) | 5.34–5.54 ms | **181–187** | 69.3% | 61.8% | ~360 fps / 76% |
| in-game (examplelevel, active) | 5.54 ms | ~180 | — | — | — |
| death screen (examplelevel) | 5.25 ms | ~190 | 69.8% | 65.6% | — |

The cap is **enforced**: every state sits at ≈200 fps (the shortfall from the 5.0 ms pace
is ~0.3–0.5 ms of per-frame work + scheduling jitter), and no state approaches the
pre-fix free-run (menu ~500, in-game ~360 on this very light level). CPU **settled from
the K2 free-run's 76–71% to 69–57%** — but the drop is milder than the fps ratio suggests:
at ~188 fps the per-frame render work dominates (≈2.9 ms menu / ≈3.4 ms in-game of GL
command submission + driver work on the Adreno 610), and that is work the C++ pays per
frame too at 200 fps. "The C++'s paced lower burn" at EQUAL fps cannot be pinned without
the C++ APK on this device — that comparison remains open (noted in the K2 row); what the
measurement proves is the cap's enforcement (the divergence the K2 row was about) and a
real CPU reduction.

**ANR dropbox count (the 025i non-regression): 10 before, 10 after — FLAT.** The ~9-minute
session covered a boot, the main menu, three level starts, three deaths, two restarts and
one replay — zero new `data_app_anr` entries; the ten existing entries are the pre-session
records (the K3-era 2026-10-17 08:19:47 record is still the newest).

### Raw evidence

- `.agents/evidence/lane-051/lane051-session.logcat` — the session's logcat (boot, GL
  context lines, every loop-period line, the grid-dispatch lines, the `playLevelMusic`
  re-fires, the `death.ogg` decodes).
- `.agents/evidence/lane-051/oh051_d0.png` — the one image read (the game-over screen with
  the action row).
- `.agents/evidence/lane-051/` — the remaining captures (menu/level-select/death/replay
  pixel-diff references; not read, used only by the pixel oracles).

### Device state on return

Left at the **main menu** (Level Selection; the final capture is a 0.0% pixel diff vs the
known menu capture). App alive (pid 23270, the lane's install), no app uninstalled, the
on-device config untouched, the /sdcard temp screenshots removed, the old mDNS port 41611
is dead (the listener moved to 40119 — recorded for the next lane).
