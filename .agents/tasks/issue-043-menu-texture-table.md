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
