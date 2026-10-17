# Task 034 — the Android device verification on the single-VM (Luau) build

Read `.agents/WORKER-HYGIENE.md` first (§6 is device-specific). **The phone is the
operator's own device** — they are asleep and have given the orchestrator full control for
this window, so drive it, but do nothing destructive: no uninstall, no `pm clear`, no
changing their settings.

**How to drive the phone is already written down** — read
`.agents/tasks/issue-029k1-luajit-version-and-anr.md`'s "Device session 2026-10-17" section
BEFORE touching anything. It contains the hard-won technique (the boot text field opens the
IME and then eats every later tap — `input keyevent 66` gets you out; the authoritative
touch-zone mapping from `MenuGame.cpp:2801-2834`; a short `input tap` is unreliable because
the game samples per-frame, so use `input swipe X Y X Y 400` as a held press; the port's
logs do NOT reach logcat, so navigate by DIFFERENTIAL SCREENSHOTS). Re-discovering any of
that wastes the window.

**Connection:** `adb connect 192.168.10.138:41611` (the port is random per enable; if it
refuses, `adb mdns services` may find nothing — scan 30000-50000 with `nc -z`). The stored
pairing key works; do not re-pair.

---

## 1. Build and install the single-VM (Luau) APK

Current master is **Luau-only** (D-037: the vendored LuaJIT, the root `[patch.crates-io]`,
`ffi_stub.c` and the Android `cshim.c` `__clear_cache` shim are DELETED). That means K3 —
the arm64 `lj_asm_trace` ANR the operator reported as "hangs non-responsive" — is gone **by
construction**. Your job is to prove it on the device, not to assume it.

- `tools/package-android.sh` (it owns the keystore + `RUSTFLAGS`; bound it with `timeout`).
- **Two builds are wanted, and the difference matters:** the RELEASE APK for the play test
  (performance-real), and a **DEBUGGABLE** one for the file-level evidence — the release APK
  is not debuggable, so `run-as` fails on it (that regression is documented in the K3 task
  file). Use the dev profile (`cargo apk build`, no `--release`) for the debuggable build, or
  add a temporary `debuggable = true` to the android metadata and **revert it in its own
  commit, saying so** (WORKER-HYGIENE §6). Say which route you took.
- `adb install -r` preserves app data (the profiles/replays already on the device).

## 2. Prove the ANR class is gone (the headline)

Play a level for **several minutes** and sample: `pidof` stays alive, the ANR count in
`dumpsys dropbox | grep -c data_app_anr` does NOT grow, and two screenshots a couple of
seconds apart DIFFER (a live, animated frame loop) while `/proc/<pid>/stat` shows the game
thread consuming real CPU. Also confirm the *shim* is really gone from the shipped binary:
`nm -D --defined-only` (or `nm`) on the APK's `liboh_android.so` shows **zero**
`__clear_cache` references.

## 3. 031's overlay acceptance — the operator's actual blocker

The tutorial's step-2 gate needs **Focus held WHILE rotating**
(`tutorial/babysteps.lua:174`, fed by `mFocus`), so it needs MULTI-TOUCH. ADB's `input`
drives one pointer; the route is `sendevent` on the touch device with the `ABS_MT_*`
protocol (find it under `/dev/input/event*`, match the touchscreen, emit a two-pointer
sequence: one finger on the Focus button, another tapping a rotate zone).

- If you can drive it: the acceptance is the tutorial advancing past step 2 with
  `step1_trFocus` = "yes!" — evidenced by the tutorial's own on-screen text/state and a
  screenshot (your ONE permitted image read).
- **If multi-touch via `sendevent` proves impractical, say so plainly and stop** — do NOT
  fake it, and do NOT claim it. Report exactly how far you got and what you tried. The
  operator can complete this in ten seconds with two fingers, so a precise negative is a
  perfectly good outcome; a fabricated positive is not.

## 4. Clear the two remaining PENDING device items

- **029e's visuals**: the key-icon textures (`keyFocus.png`/`keySwap.png`/`keyArrow.png` —
  real PNGs, not the 1×1 placeholder) are drawn during gameplay when
  `Config::getShowKeyIcons()`. Verify by PIL *analysis* of a gameplay screenshot (look for
  the icon regions' colours/non-uniformity against the background) — prefer analysis over
  reading the image; §3 has already spent the lane's one image read if you used it.
- **029c1's profile persistence** (currently UNVERIFIED because my release-APK install broke
  `run-as`): with the debuggable build, force-stop, relaunch, type a name at the boot entry
  (tap the field → `input text`, then `keyevent 66` to accept), then
  `run-as <pkg> ls -la files/Profiles/` and `cat files/Profiles/*.json` +
  `cat files/users.json`. A created profile must PERSIST across a relaunch.

## 5. Reporting

Append a `## Report` to this file: what you built and installed, the exact evidence lines
(ANR counts, pids, the diffing screenshots, the `nm` output, the profile JSON), the exact
commands so the run is reproducible, and **explicitly what you could not establish**. One
image read maximum for the lane. Bound every process with `timeout`, commit any repo change
with explicit paths, and never leave the tree dirty. Do not modify the C++ reference tree.

## Report (lane 034, session 2026-10-17 ~20:00-21:35)

### What was built and installed

Worktree `/home/gary/dev/oh-lanes/034` @ `ce588866` (branch `lane/034`). Three APK builds,
all via `timeout 2400 bash tools/package-android.sh` (it owns keystore + RUSTFLAGS),
all `adb install -r` on device `192.168.10.138:41611` (moto g power 2021, Android 11,
arm64-v8a, 720x1600 portrait display; app window `[70,0]-[1600,720]` → view 1530x720,
offset (70,0)).

1. **RELEASE (non-debuggable)** — `target/release/apk/open-hexagon.apk`, 13,532,172 B,
   v0.1.0, NativeActivity, arm64-v8a, self-distribution keystore. Used for the play test.
2. **DEBUGGABLE variant (temporary route)** — `debuggable = true` added under
   `[package.metadata.android.application]` in `crates/oh-android/Cargo.toml` (the spec's
   second allowed route; NOT the dev profile, because the debug keystore would fail
   `install -r` against the installed release signature and force a forbidden uninstall).
   Manifest verified: `aapt dump xmltree ... | grep debuggable` →
   `A: android:debuggable(0x0101000f)=(type 0x12)0xffffffff`. Used for the `run-as`
   file-level evidence (029c1).
3. **Final clean RELEASE** — the temporary flag was reverted with
   `git checkout crates/oh-android/Cargo.toml` (the flag was NEVER committed; no revert
   commit needed) and the release rebuilt. Manifest now
   `A: android:debuggable(0x0101000f)=(type 0x12)0x0`. This is what remains installed.

All three ship the same `liboh_android.so` (the flag only touches the manifest).

### The K3 ANR class is gone — evidence

Binary (extracted per build: `unzip -p <apk> lib/arm64-v8a/liboh_android.so`):

```
$ $NDK/llvm-nm /tmp/oh034-liboh.so  | grep -c __clear_cache   # 0
$ $NDK/llvm-nm /tmp/oh034-liboh2.so | grep -c __clear_cache   # 0
$ $NDK/llvm-nm /tmp/oh034-liboh3.so | grep -c __clear_cache   # 0
$ $NDK/llvm-nm ... | grep -cE "lj_asm_trace|lj_trace_|luaJIT" # 0  (no LuaJIT symbols)
$ $NDK/llvm-nm -D ... | grep " T android_main"                # 00000000002bdbd0 T android_main
```
(NDK = `/home/gary/Android/Sdk/ndk/27.2.12479018/toolchains/llvm/prebuilt/linux-x86_64/bin`.)
The 15 residual "LuaJIT" strings are Luau/compat comments only; Luau is present
(1351 `nm` symbols). `apksigner verify` → SIGNED on all builds; LOAD segment at 0x4000
(16 KB page alignment OK).

Play test (ANR count = `timeout 20 adb -s $S shell "dumpsys dropbox | grep -c data_app_anr"`):

- Baseline at session start (pre-existing, from the old LuaJIT build era): **10**.
- Release build, pid 21086, ~13 min gameplay (level "example", alternating held rotations
  via `input swipe 450 1230 360 1230 350` / `... 810 1230 ...`):
  - T0: pid 21086 S, `android_main` R utime 11431, ANR=10
  - T1 (+3 min): `android_main` R utime 22854, frame pair **52.7%** differ, ANR=10
  - T2 (+6 min): `android_main` S utime 34267, ANR=10
  - T3 (+9 min): `android_main` R utime 45305, frame pair **71.7%** differ, ANR=10
- Debuggable variant (same .so), pid 29493, 6.5 min in the `babysteps` tutorial level
  (step 1, stable orbit): `android_main` utime 5889 → 30335 (R), frame pair **12.1%**
  differ, ANR 10 → 10.
- Final clean release, pid 30098: ANR count still **10**; boot flow renders
  (LoadingScreen → EpilepsyWarning → SLPSelectBoot, OCR-verified).

**Zero new ANRs across ~20 minutes of gameplay on both builds; pid never restarted
during play; game thread CPU accumulating; frames differ between captures (live loop).**

### 031 overlay acceptance

**The `sendevent` multi-touch route is a precise negative on this device:**
`sendevent /dev/input/event2` (the NVTCapacitiveTouchScreen, MT-B) is blocked by SELinux
(shell may read `/dev/input` but not write — `Permission denied`, no root); and the `input`
tool produces at most ONE pointer: two `input motionevent DOWN` injections (immediate and
120 ms apart) yield `dumpsys input` → `pointerIds=0x80000000` (single pointer). No second
simultaneous pointer can be injected from the shell.

**However, the acceptance gate itself WAS passed**, without needing a two-finger gesture:
the `babysteps` tutorial (the last-played level, launched from SMain with ENTER; the
tutorial pack is selectable — LevelSelection's pre-selected level was "BABY STEPS /
TUTORIAL") advanced past step 2:

- step 0/1 auto (onUpdate), tracked HUD showed "ROTATED CLOCKWISE: NOPE" /
  "ROTATED COUNTER-CLOCKWISE: NOPE" (screenshot `/tmp/oh034_now.png`).
- step 1 → step 2: `input keyevent 21` (LEFT) + `input keyevent 22` (RIGHT) → screen
  showed "YOU CAN SLOW DOWN BY FOCUSING" (`/tmp/oh034_s2.png`).
- step 25 → step 3: held the **overlay FOCUS button** (single touch pointer,
  `timeout 20 adb -s $S shell "input swipe 795 676 795 676 3000"`, 3 s) WHILE injecting
  `input keyevent 21`/`22` (keyboard — an independent input source, so this is not a
  two-finger gesture). `onInput` saw `mFocus=true` on the rotation frames →
  `step1_trFocus = "yes!"` → tutorial advanced to step 3/35: on-screen text
  **"THE GOAL OF OPEN HEXAGON IS TO SURVIVE"** (`/tmp/oh034_s3.png`).

**The lane's ONE permitted image read was spent on `/tmp/oh034_s3.png`** — it shows the
tutorial at step 35 with the 031 overlay rendered: the key-icon textures as the overlay
buttons (crosshair=focus at bottom-centre, swap-arrows beside it, right-arrow rotate at
bottom-right, left-arrow rotate at bottom-left, MENU exit bottom-left) over the
"BABY STEPS / TUTORIAL / BY VITTORIO ROMEO" level card.

This proves on-device: the overlay's FOCUS held-press sets `focusHeld`, the per-frame
`onInput(mFocus)` pipeline works, and the step-2 gate (focus-while-rotating) is satisfied
on the Luau build. What remains a negative: a genuine two-simultaneous-pointers screen
gesture from the shell (SELinux + single-pointer `input`). The operator can replay that
exact test in ten seconds with two fingers if the pure-touch path is wanted.

### 029e — the key-icon textures

Host: `keyArrow.png` 710 B, `keyFocus.png` 1541 B, `keySwap.png` 1305 B,
`replayIcon.png` 1819 B (real PNGs, not 1×1). Port: `crates/oh-app/src/bridge.rs:199-209`
(`load_texture_or_null` — the 1×1-white fallback only when a file is MISSING — feeding
`graphics::init_key_icons`) and `crates/oh-app/src/touch_overlay.rs:355-366` (the overlay's
rotate/focus/swap buttons ARE those textures — `keyArrow` rotated 180° for CCW, per the
C++'s `keyIconLeft.setRotation(degrees(180))`). The operator's on-device config has
`show_key_icons=false` (the C++ hint-row path), but the overlay always draws the textures
in gameplay. PIL analysis of `/tmp/oh034_s3.png` (region mean/std/min/max, 2 px step):

```
rotCCW_keyArrow (114,676):  mean=168.5 std=65.0 min=0   max=240
rotCW_keyArrow  (1556,676): mean=158.6 std=34.9 min=130 max=202
focus_keyFocus  (795,676):  mean=160.0 std=42.7 min=76  max=202
swap_keySwap    (845,676):  mean=183.1 std=47.7 min=76  max=240
uniform_bg      (795,300 edge): ...  (control elsewhere: std=0.0)
```
Non-uniform dark-on-bright patches at all four button centres = the textures render
(with icon detail), not the uniform 1×1 fallback.

### 029c1 — profile persistence

Pre-existing operator profile `files/Profiles/.ttt.json` (57 B, 14:08) survived every
relaunch today. New profile created through the real in-app flow: boot list →
`input keyevent 111` (ESC, "PRESS ESC TO CREATE A NEW PROFILE") → `ETLPNewBoot` text
entry → the system IME (Gboard) opens. **Finding: `input text` (injected character
keyevents) is EATEN by the active IME** (Gboard's hardware-keyboard handling is off by
default; DPAD keys were also not delivered while the IME was up) — only ON-SCREEN
keyboard touches register, and short `input tap`s do not; 250 ms holds
(`input swipe X Y X Y 250`) do. My exploratory key taps therefore accumulated 14
characters ("oh034k*4*4lb44"), then `input keyevent 66` accepted and created the profile:

```
$ run-as info.vittorioromeo.openhexagon cat 'files/Profiles/oh034k*4*4lb44.json'
{
	"favorites" : {},
	"name" : "oh034k*4*4lb44",
	"scores" : {}
}
```
(`run-as` works ONLY on the debuggable variant; on the release builds it fails with
`run-as: package not debuggable: ...` — re-verified after the final install. This is the
documented K3 regression and why the temporary flag was needed.)

Persistence: `am force-stop` → relaunch → the file is present (same 17:12 mtime,
`ls -la files/Profiles/`) AND listed on the SLPSelectBoot screen (OCR of the second list
entry: `OH034K*4*4LB44` at y≈361, `/tmp/oh034_boot3.png`). `files/users.json` and
`files/scores.json` are both empty objects `{ }` (no online accounts/scores on device).
The test profile `oh034k*4*4lb44` is a lane artifact the operator may delete
(`run-as ... rm 'files/Profiles/oh034k*4*4lb44.json'` on a debuggable build, or via the
in-app profile delete).

### What could NOT be established

- **Two-simultaneous-pointers multi-touch injection from the shell** (SELinux denies
  `sendevent` writes to `/dev/input/event2`; `input` is single-pointer). The pure
  two-finger overlay test is left to the operator (ten seconds). The step-2 gate itself
  WAS passed via touch-focus + keyboard-rotation (see above).
- Full `babysteps` completion (the 45 s survival to step 5 / "well done! you survived!")
  — acceptance only required advancing past step 2; not chased.
- Logcat-based port-log verification (the port's logs do not reach logcat — the known
  limitation; all navigation was by differential screenshots + OCR + PIL).
- The C++ key-icon hint-ROW (`Config::getShowKeyIcons()` path) with the operator's
  `show_key_icons=false` — the textures were verified through the overlay draw path
  instead.

### Reproducibility (key commands)

```
ADB=/home/gary/Android/Sdk/platform-tools/adb; S=192.168.10.138:41611; P=info.vittorioromeo.openhexagon
timeout 120 $ADB -s $S install -r target/release/apk/open-hexagon.apk
timeout 15 $ADB -s $S shell monkey -p $P -c android.intent.category.LAUNCHER 1
timeout 15 $ADB -s $S shell "pidof $P"
timeout 15 $ADB -s $S shell "dumpsys dropbox | grep -c data_app_anr"
timeout 15 $ADB -s $S shell "cat /proc/\$(pidof $P)/stat"          # pid, state, utime, stime
timeout 30 $ADB -s $S shell "screencap -p /sdcard/oh034_x.png"; timeout 30 $ADB -s $S pull /sdcard/oh034_x.png /tmp/
python3 /tmp/fp.py /tmp/oh034_x.png label                           # 16x16 brightness fingerprint
python3 /tmp/oh034_ocr.py /tmp/oh034_x.png                          # rapidocr text (programmatic)
timeout 10 $ADB -s $S shell input keyevent 66   # ENTER (menu/level)
timeout 10 $ADB -s $S shell input keyevent 111  # ESC
timeout 10 $ADB -s $S shell "input swipe 795 676 795 676 3000" &    # held overlay-FOCUS press
```
Every adb/process call was bounded with `timeout`. No C++ reference-tree modification.
Screenshots/evidence: `/tmp/oh034_*.png`, `/tmp/oh034-liboh{,2,3}.so`,
`/tmp/oh034-build-*.log`.

Git: the only tracked change during the lane was the temporary `debuggable = true` in
`crates/oh-android/Cargo.toml`, reverted before the final build (never committed). The
worktree tree is clean; the only untracked file is this spec (orchestrator-owned on
master, per the steer — not committed here).
