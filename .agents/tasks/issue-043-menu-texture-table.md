# Task 043 — finish the menu texture table: the epilepsy warning (and titleBar, and the online icon) render as the 1×1 placeholder

Read `.agents/WORKER-HYGIENE.md` first. Operator report, verbatim: *"also the epilepsy warning
screen is missing on all builds, it's just a blank screen"* — **"all builds"** is the clue, and the
cause is already located in the code (below), so start from the diagnosis rather than reproducing
from scratch.

## The diagnosis (verified before writing this spec)

The C++'s arm does two things:

```cpp
MenuGame.cpp:5834  case States::EpilepsyWarning:
            :5835      render(epilepsyWarning);
            :5836      renderText("PRESS ANY KEY OR BUTTON TO CONTINUE", txtProf.font, …);
            :5837      return;
```

The PORT's arm and transform are both already correct —
`crates/oh-app/src/menu_draw.rs:1016-1027` draws the sprite AND the same prompt text, and
`:674-679` mirrors the C++'s origin/scaleFactor math (`MenuGame.cpp:3163-3165`). What is wrong is
upstream of the draw: **the sprite's texture is a 1×1 placeholder that is never replaced**, and at
scale 0.36 it is sub-pixel — invisible, which is exactly why the screen reads as "blank".

**Three sprites are still placeholders** in `crates/oh-app/src/menu_draw.rs`, and the file's own
comments say so (`:368` "…load here — `titleBar` / `epilepsyWarning` / the online icons…"):

| port site | the texture the C++ actually uses | C++ authority |
|---|---|---|
| `:434 title_bar` | `titleBar.png` | ctor `MenuGame.cpp:169`; `initAssets` setSmooth `:824` |
| `:439 s_online` | `onlineIconFail.png` (and `onlineIcon.png`, swapped at `:6078`/`:6082`) | ctor `:173`; setSmooth `:141-142` |
| `:440 epilepsy_warning` | `epilepsyWarning.png` | ctor `:172`; setSmooth `:824` |

All three are initialised from the same `let placeholder = Texture::create(1, 1);`
(`:365`) — this is the **third instance of the 29e1 class** (29e1 did the key/particle textures,
29w1 the credits; both added "is this the real PNG?" tests, e.g.
`credits_bar_textures_are_the_real_assets_pngs_not_placeholders` at `:4218`).

## VERIFIED SYMPTOM in a real browser (orchestrator, against the CI-deployed page)

The diagnosis above came from reading code. It is now confirmed at the *symptom* level against the
build GitHub Actions published, using the new durable probe `tools/web-probe.mjs`:

```
node tools/web-probe.mjs https://gtrak.github.io/SSVOpenHexagon/ 24 --keys=Enter
  requests      : <base>/ . oh-web.js . oh_web.data . oh_web.wasm   (all 200; the ONLY 404 is /favicon.ico)
  state timeline: menu:LoadingScreen -> [Enter] -> menu:EpilepsyWarning -> [Enter] -> menu:ETLPNewBoot
```

So the port DOES enter `States::EpilepsyWarning`, ticks there at ~60/s, and draws nothing visible.
Combined with the code read (the draw arm and the transform are already C++-faithful), the sprite's
1x1 placeholder texture is what makes the screen blank - a player sees black and has to guess that a
key advances it. That is the whole bug, so do not go looking for a missing state or a missing case.

ACCEPTANCE, in those terms: with a browser available, `node tools/web-probe.mjs <url> 30 --keys=Enter`
against either the deployed page or a local serve of `target/package/web`, and confirm (a) the
timeline reaches `menu:EpilepsyWarning`, and (b) the screenshot at that moment shows the real
epilepsyWarning.png AND the "PRESS ANY KEY OR BUTTON TO CONTINUE" prompt. Compare against the actual
PNG in the original tree rather than a memory of it. (The favicon 404 is not yours; the probe
tolerates it.) The probe is a manual tool - it needs a browser - so do NOT wire it into the gate.

## What to do

1. **Load the real PNGs through the same asset path the credits/key textures already use** (the
   `load_texture_or_null` bridge pattern 29w1 established, with the same missing-file fallback to
   the placeholder), for `titleBar.png`, `epilepsyWarning.png`, `onlineIcon.png` +
   `onlineIconFail.png`, and apply **`setSmooth(true)` to exactly the textures the C++ does** — the
   8-entry list at `MenuGame.cpp:819-828` (`titleBar`, `creditsBar1`, `creditsBar2`, `creditsBar2b`,
   `creditsBar2c`, `creditsBar2d`, `bottomBar`, `epilepsyWarning`) plus the two online icons at
   `:141-142`. **Check `bottomBar.png` too**: the C++ smooths it, so if the port has a `bottomBar`
   sprite drawn from a placeholder, it belongs in this task; if the port genuinely has no such
   sprite, say so rather than inventing one.
2. **SWEEP the class rather than patching these three** — that is the real value here. Enumerate
   every texture the C++ loads for the menu (`grep 'getTexture("' MenuGame.cpp` and the
   `initAssets` list) and every place the port constructs a 1×1 placeholder or a
   `Texture::create(1, 1)`, then reconcile the two: any C++ texture the port draws but never loads
   is the same bug and belongs in this fix. Report the reconciliation table; if you find a fourth
   one, that is a finding, not a scope violation.
3. **Add the guard tests** in the 29w1 pattern: each newly loaded texture asserts its real
   dimensions (non-1×1) and that a missing asset still falls back to the placeholder. The class is
   only closed when a regression cannot re-open it silently.

## Verification (the operator's report is CROSS-PLATFORM, so prove it on more than one)

- **Web (fastest, and the operator can see it)**: the build is served locally on
  `http://localhost:8081/` (and deployed at `https://gtrak.github.io/SSVOpenHexagon/`). Drive to
  `States::EpilepsyWarning` over CDP (it is reachable from boot: `keyevent`/Enter advances
  `LoadingScreen → EpilepsyWarning`) and **PIL-analyse the frame** for the sprite's region — before
  the fix it is absent, after it is real pixels. Prefer analysis over reading the image; the lane
  gets the usual ONE image read if a screenshot is the honest evidence.
- **Desktop**: the same draw path (it is platform-neutral), so a host-side check is cheap — and the
  port's menu-draw tests are the natural home for the guard.
- **Device**: if 039 releases the phone (`192.168.10.138:41611`, remembering it needs a
  *debuggable* build for `run-as`), a screenshot of the warning screen is the strongest evidence,
  and it is the platform the operator reported on. If the phone is busy, say PENDING rather than
  claiming it.

## Hygiene

One commit per milestone (the loads, the sweep findings, the tests, the report), explicit paths,
`cargo fmt` before committing, the gate (or targeted `cargo test -p oh-app`) green before each
commit, bound every process with `timeout`, and append a `## Report` with the reconciliation table,
the evidence, and what you could NOT verify. Do not touch `crates/oh-assets`' loader semantics in a
way that changes the other textures' behaviour, and do not "fix" this by disabling the placeholder
fallback — the fallback is correct; the missing load is the bug.

## Report (lane 043, 2026-10-18)

### The fix (one file: `crates/oh-app/src/menu_draw.rs`)

- `MenuDraw::new` now loads the remaining C++ ctor textures through the SAME
  `bridge::load_texture_or_null` path the 29w1 credits bars use (module docs + ctor updated):
  `titleBar.png`, `epilepsyWarning.png` (smooth, `initAssets` :822/:824) and
  `onlineIcon.png` / `onlineIconFail.png` (smooth, `initOnlineIcons` :141-142). The
  `let placeholder = Texture::create(1, 1)` line is gone — it was the single source of all
  three 1×1 sprites (spec :365/:434/:439/:440).
- The online-status pass now mirrors the C++'s per-frame `sOnline` icon swap
  (MenuGame.cpp:6078-6084): new fields `online_icon` / `online_icon_fail` (the canonical
  decodes) + `s_online_is_good`; the sprite's texture materializes only on a `stateGood`
  FLIP (issue 36 — the C++ `setTexture` is a zero-copy shared-pointer assign, the port's
  takes ownership; 64×64 = 16 KiB per copy, the same pattern as `credits_bar2_frame`).
  `s_online` still starts on `onlineIconFail.png` (ctor :173).
- No asset-pipeline changes: `oh-assets` is untouched (the direct `assets_dir` decode IS the
  established 29e1/29w1 pattern; the spec's "say so before doing it broadly" boundary held).

### The sweep (C++ menu textures vs port placeholders)

`grep 'getTexture("' MenuGame.cpp` → eight call sites, seven distinct files:

| C++ texture (MenuGame.cpp) | port status after this lane |
|---|---|
| `titleBar.png` (:169, smooth :822) | was 1×1 → **now the real 1024×279 PNG** |
| `creditsBar1.png` (:170) | real (29w1) |
| `creditsBar2*.png` (:171 + frame cycle) | real (29w1) |
| `epilepsyWarning.png` (:172, smooth :824) | was 1×1 (the operator's blank screen) → **now the real 1600×1600 PNG** |
| `onlineIconFail.png` (:173, smooth :142) | was 1×1 → **now the real 64×64 PNG** (ctor initial icon) |
| `onlineIcon.png` (:141, :6078) | was never loaded (the swap was a documented no-op) → **now the real 64×64 PNG + the swap** |
| `bottomBar.png` (smooth list :824 only) | **NOT loaded — deliberately**: the C++ never draws it (no `bottomBar` sprite anywhere in MenuGame; the smooth-list entry is the only reference). The port has no such sprite, so there is nothing to load. Recorded, not invented. |

Every other 1×1 in the port: `bridge.rs:173-177` (the `DrawState` key/replay icons start as
`Texture::new()` but `init_key_icons` overwrites them with the real `keyArrow/keyFocus/
keySwap/replayIcon.png` decodes immediately in the same ctor — the 29e1 work, not a defect) and
the `load_texture_or_null` fallback itself (the C++ `nullTexture` — correct by design). No
other menu placeholder exists. `Texture::create(1, 1)` no longer occurs anywhere in `crates/`.

### Tests (4 new, the `menu_draw` test module)

- `menu_bar_and_icon_textures_are_the_real_assets_pngs_not_placeholders` — the 29e1/29w1
  dimension guard, extended to the whole table: titleBar 1024×279, epilepsyWarning 1600×1600,
  onlineIcon/onlineIconFail 64×64, all `is_smooth` per the C++ smooth sets, the ctor's
  `s_online` pixel-identical to `online_icon_fail` (as the credits bar starts on
  `CREDITS_BAR_IDS[0]`), and the two icons provably distinct decodes.
- `menu_bar_and_icon_missing_assets_fall_back_to_the_placeholders` — the `nullTexture` half:
  a broken install still yields the 1×1 for all five.
- `online_status_stays_on_the_fail_icon_when_state_good_is_false` / `online_status_picks_the_
  good_icon_when_state_good_is_true` — the :6078-6084 swap through a REAL `draw` (a local
  `GoodNetwork` `NetworkClient` fake: `Connected` + rt keys → `stateGood`), asserting the
  sprite's pixels flip to `online_icon` and hold on the second draw (the issue-36 no-re-copy).

A future 1×1 regression of any of the five now fails the dimension test by construction.

### Browser observation (acceptance half 2 — my OWN build, not the deployed page)

- Built in the `emscripten/emsdk` container (the 040 recipe, mounted at this worktree, own
  `CARGO_TARGET_DIR=/home/gary/dev/oh-lanes/043-wasm-tgt`): release build clean (only the
  known `WASM_BIGINT` linker warning), `WEB PACKAGED` — `index.html` 5515 B, `oh-web.js`
  167202 B, `oh_web.wasm` 2646499 B, `oh_web.data` 22765538 B (the data size matches the
  orchestrator's probe of the deployed page → the four load assets did not regress).
- Served `target/package/web/_WEB` on `http://127.0.0.1:8493/` (8081 was occupied by
  another process — left untouched), then
  `node tools/web-probe.mjs http://127.0.0.1:8493/ 30 --keys=Enter --shot=/tmp/oh-043-epilepsy.png`:
  - **booted: yes** (15 status lines, ~2 s cadence), **no console errors**, the only non-200
    is the tolerated `/favicon.ico` 404 (python http.server artifact, same class as on the
    deployed page); the probe's single `ERR_ABORTED (Document)` is the page-navigation
    transition, not an asset.
  - **state timeline: `menu:LoadingScreen -> menu:EpilepsyWarning`** — the Enter at t≈15 s
    advanced into the warning; the last status line (t≈30 s): `status booted=true;exit_code=0;
    ticks=1799;state=menu:EpilepsyWarning` (1799 ticks ≈ 60/s × 30 s — the state machine ticks
    in that state, exactly as the orchestrator's deployed-page probe saw).
- **The screenshot (one deliberate image read)** shows, on a black clear: the white warning
  triangle with the exclamation mark, the `EPILEPSY WARNING` title, the two body paragraphs
  ("OPEN HEXAGON CONTAINS VISUALS WHICH MAY CAUSE SEIZURES IN INDIVIDUALS AFFECTED BY
  EPILEPSY" / "IF YOU ARE AFFECTED, PLEASE TALK TO YOUR DOCTOR BEFORE PLAYING!"), and the
  `PRESS ANY KEY OR BUTTON TO CONTINUE` prompt at bottom-left (x 9..306, y 559..567). The
  page's own `div#status` (top-right gray text) reads `state=menu:EpilepsyWarning` — the
  port's own account of itself in the frame.
- **Compared against the real PNG, not a memory of it**: `epilepsyWarning.png` is a
  1600×1600 RGBA (white content, transparent background) with exactly 7 alpha-content row
  segments (the triangle; the title; 5 body lines). PIL segmentation of the screenshot found
  the SAME 7 segments, in the same order, and every one sits where the C++ transform says it
  must: with the canvas at 1280×576 (the real window — the 577th page row is the `#111` body
  margin), layout w = 1280·max(1024/1280, 768/576) = 1706.7, h = 768, overlay-view →
  on-screen scale 0.75, sprite centre on-screen (853.3·0.75, 334·0.75) = (640.0, 250.5),
  on-screen art scale 0.36·0.75 = 0.27: all 7 predicted segment boxes (e.g. the triangle
  y 48.5..176.5 vs observed 49..176; the title x 441.3..841.2 vs observed 441..841) land
  within 1 px of the observed pixels. A 1×1 placeholder at that transform is 0.27 px — the
  blank screen the operator reported; this frame is the real art, sub-pixel-faithful to the C++.

### What I could NOT verify

- **Device (the phone)**: PENDING — the phone was not part of this lane's budget and 039's
  release of it was not confirmed; the draw path is platform-neutral (the same `MenuDraw`),
  the host gate covers the code, and the web build is the operator-visible one. A device
  screenshot of the warning remains the one unperformed confirmation.
- The `WASM_BIGINT is deprecated` linker warning is pre-existing (every earlier wasm lane
  logged it) — not introduced here.

### Remaining placeholders I chose NOT to fix (all by design, none drawn from a placeholder)

- The `load_texture_or_null` 1×1 fallback (the C++ `nullTexture` — the spec says the
  fallback is correct; it now only fires on a broken install).
- `bridge.rs` `DrawState`'s `Texture::new()` key/replay icons (overwritten with the real PNGs
  in the same constructor — 29e1).
- `bottomBar.png` (the C++ smooths it but never draws it — see the sweep table).
