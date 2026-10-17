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
