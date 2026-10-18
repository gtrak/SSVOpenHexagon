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
