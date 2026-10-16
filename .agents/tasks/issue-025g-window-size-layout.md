# Task 025g — the menu's layout space must follow the REAL window size

Lane 025g. Master green (gate 4/4, 737 tests) at `a14bd715`. **No worktree**
(D-035): work in the main checkout, commit early and often.

## The defect (device-verified by the orchestrator, with a probe)

The 9-zone touch grid (landed in 025f, `a14bd715`) dispatches on
`window_size()` = the menu's `s.w`/`s.h`, which `refresh_camera`
(`crates/oh-menu/src/menu_game.rs:1195`) derives from
**`config.size_x()/size_y()`** — the CONFIG window size.

The C++ (`MenuGame::refreshCamera`, `MenuGame.cpp:3129-3136`) derives them
from **`getWindowWidth()` / `getWindowHeight()`** — the *actual current
window* (the SFML render-target size):

```cpp
const float fw{1024.f / getWindowWidth()};
const float fh{768.f / getWindowHeight()};
const float fmax{std::max(fw, fh)};
w = getWindowWidth() * fmax;
h = getWindowHeight() * fmax;
```

On the phone the window is **1530x678** (the emulator/screen is 1600x720,
minus system bars) while the config is 1024x768-ish, so the port's menu
space (probe read `window=(1024,1024)`) does **not** match the space the
touch coordinates live in. The grid's thirds therefore land in the wrong
places on a real device — a center tap can land in the "right" band.

This is the SAME class of bug as the missing grid: a config-vs-actual
divergence that is invisible on desktop (where config size == window size
in practice) and wrong on mobile/any resized window.

## What to do

1. **Find how the port learns the real window size** and thread it to the
   menu layout the way the C++ does (`getWindowWidth()`/`getWindowHeight()`
   = the render target's size). Candidates to check: `MenuConfig`'s
   size getters (probably config-backed), the window/`GlParts` width/height,
   and `WindowedClient`'s per-frame `set_size`/`Resized` handling. Cite
   what you find; if a seam is missing, add one (the C++ reads the window
   object directly — `MenuGame` holds a `window` reference).
2. **Change `refresh_camera`** (and everything that consumes `s.w`/`s.h`)
   to use the real window size, exactly as the C++ computes it — same
   `fw`/`fh`/`fmax` math, applied to the actual width/height. Keep the
   C++'s `backgroundCamera`/`overlayCamera` derivation (`:3138-3159`)
   unchanged in form.
3. **Re-verify `refresh_view`** (the port's helper in `oh-app/src/menu_draw.rs`
   around the `refresh_camera` call site, `:430-460`) — its inputs and the
   stale-detection tuple must now carry the window size, not the config
   size.
4. **Desktop must not regress**: when the window matches the config (the
   normal desktop case) the computed `w`/`h` must be unchanged — prove it
   with a unit test that pins the C++ formula for both a matching case and
   a mismatched (mobile-like 1530x678 vs 1024x768) case.
5. **Device acceptance** (this is the point of the lane): on the phone
   (adb `192.168.10.138:38721`; **wake with `KEYCODE_WAKEUP` first**;
   `pm clear` for a fresh install) — type a name via the IME, then tap the
   screen center and the profile must be created and the main menu must
   appear showing `CURRENT PROFILE: <name>`.
   - The screen is 1600x720; the TRUE center is physical (800,360) — but
     **derive the band from the code you just fixed, not from a guess**:
     the middle band is logical-height/3 .. 2/3, so compute the physical y
     that lands inside it given your new window-size source and say in the
     report exactly which physical coordinate you tapped and why.
   - Also verify the other zones still dispatch (a left-band tap in level
     selection changes the pack/level, a bottom-band tap moves the
     selection down).
   - Evidence: screencaps (pixel histograms are fine; ONE image read at the
     very end), plus the state transitions. Do NOT rely on adb keyevents
     for the submit — the whole point is a touch-only submit.
6. **Report**: the window-size source you chose (with citations), the
   `refresh_camera` diff rationale, the unit-test names, the device evidence
   (which physical coords you tapped + what happened), and any residual
   risk. Keep it short. Also add the register row (issue 28's rule) — this
   is a deviation you are FIXING.

## Hygiene

- **Commit every 3-5 tool calls.** No worktree: your uncommitted work sits
  in the shared tree, and a timeout loses it (this has happened three times).
- Do not touch unrelated crates. Do not run the emulator. Do not touch
  non-phone processes.
- If the IME/dumpsys path gets flaky, prefer reading state from the app's
  own logging or a temporary probe you REVERT before your final commit.
