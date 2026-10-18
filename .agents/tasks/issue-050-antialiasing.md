# Task 050 — `antialiasing_level` does nothing on ANY target (the audit's one MISSING row)

Read `.agents/WORKER-HYGIENE.md` first (§11 matters: the pinned SFML/GLEW sources live in the oracle's
dep cache, `~/.cache/ohoracle-deps/`, NOT in the empty repo submodules).

## The finding, established and verified

Lane 047's config-application audit left exactly one **MISSING** row, and I re-verified it: the
`antialiasing_level` config has no effect on desktop, Android or wasm. The port's GL contexts request no
MSAA at all — `crates/oh-render/src/gl.rs:541` has `antialias: false`, and there is no `multisampling`
anywhere in `oh-render` — while the C++ applies SFML msaa: `main.cpp:307` passes the level into the
context settings before the window is created (audit citation: also `MenuGame.cpp:1447-1464`; read it in
the pinned SFML under `~/.cache/ohoracle-deps/sfml-fork/` and cite what you actually read).

Read `.agents/audits/config-application.md` (section A, last row, and "Filed but unfixed") for the
context — including the fact that the audit's verdict came from the same class of evidence as the
`limit_fps` row that later proved wrong, so treat the *fix shape* as open until you have measured it.

## STEP 1 — diagnose the SHAPE before writing any plumbing (this decides whether the task is small)

**Establish where the game actually renders.** If the scene is drawn to the default framebuffer,
requesting a multisampled context is the whole fix (SFML's own model). If the port draws through an
offscreen FBO / render target — check `GlDrawTarget`, the shader/bloom path, `render_states_for_stage`
and whatever `029n1`'s fragment-shader work touches — then **default-framebuffer MSAA is a no-op for
that scene**, and the fix becomes a multisampled FBO plus a resolve blit. That is a genuinely different
size, so:

- If it is the small shape: implement it (below).
- If it is the FBO shape: **report the finding with the size before starting**, and stop — a
  MEDIUM-LARGE rendering change is a lane of its own, and §3 forbids rabbit-holing. A precise "it needs
  a multisampled FBO + resolve, here is the evidence, here is the size" is a good outcome for this lane.

## If it is the small shape: what the fix must do

- Plumb the config's `antialiasing_level` into all three context creations: glutin's
  `ConfigTemplateBuilder` (`.with_multisampling(n)`), the Android EGL config chooser (request the sample
  count, and handle the real risk that no matching EGL config exists — fall back rather than fail, and
  report which happened), and the emscripten WebGL2 context (`antialias` is a BOOLEAN there — the
  browser picks the sample count — so the *level* cannot be honoured exactly; map it honestly and SAY so
  rather than pretending the level is applied).
- Mirror the C++'s own semantics for the level's meaning (0 = off; the C++ passes it straight into
  `sf::ContextSettings::antialiasing`), and follow the port's existing config plumbing conventions.

## Acceptance — measure the real thing, not the intent

`GL_SAMPLES` is the authoritative observable and is available on all three targets
(`glGetIntegerv(GL_SAMPLES)`): prove the context reports the requested sample count when the level is
nonzero (and 0 when it is zero), and report the ACTUAL value a driver/browser gives you rather than the
value you asked for — a downgrade is a finding, not a failure. If you have budget after that, the
visual half can be a pixel-level edge analysis (029n1's style: count the intermediate-colour pixels
along a known edge with the level on vs off), but `GL_SAMPLES` is the acceptance and a screenshot read
by eye is not.

Gates: host gate before each commit, `cargo check --target aarch64-linux-android` (§4 — the EGL path is
invisible to the host gate), and the wasm build in the emsdk container if you touch the web context.
Also update the audit doc's row and the tracker: either "APPLIED" with the level's real behaviour
(including the wasm boolean mapping) or the FBO finding with its size.

## Boundaries

`crates/oh-render/` (the three context paths + the config plumbing). Do NOT change the game's draw
semantics, do not change the level's default or the C++'s meaning of the value, and do not attempt the
multisampled-FBO work inside this lane if Step 1 shows that is what it needs. Commit with explicit
paths, bound everything with `timeout`, and append a `## Report`.

## Report

**Verdict: it was the SMALL shape — implemented, desktop half measured.** The scene renders to the
DEFAULT framebuffer, so requesting a multisampled context IS the whole fix (SFML's own model). No
multisampled-FBO + resolve work was needed or attempted.

### Step 1 — the shape diagnosis (evidence, read, not inferred)

- **Zero `glBindFramebuffer` calls anywhere in `oh-render`** (grep across the crate: no matches).
  The `GlDrawTarget`/`GlPasses`/`GlBatchUploader` draw passes set program/texture/VAO/uniform state
  and draw to the currently-bound framebuffer, which is the DEFAULT one (nothing ever binds another).
- `GlParts::read_framebuffer` / `save_screenshot` (gl.rs) read the default framebuffer via
  `glReadPixels` — the C++'s `saveScreenshot` reads the window's back buffer; the port reads the
  default framebuffer. Consistent: **the scene IS the default framebuffer**.
- `RenderTexture` (texture.rs:461+) is a CPU-side analog (its `gl_id` is never set by any live
  path — there is no FBO-creation code in the crate); `render_states_for_stage` (graphics.rs:102,
  the 029n1 area) selects only per-stage SHADER/BLEND state — it never touches a framebuffer target.
- The C++'s own model matches: `GameWindow::recreateWindow` (SSVStart `GameWindow.hpp:85-99`, read in
  the main checkout per hygiene §11) passes `antialiasingLevel` into
  `sf::ContextSettings{0, 0, antialiasingLevel, 0, 0}` (:91) before `renderWindow.create`;
  `setAntialiasingLevel` (:187-190) stores + marks `mustRecreate`; `main.cpp:307` sets the value
  before the window's first recreation. SFML draws the scene to the window back buffer — a
  multisampled context's back buffer, resolved at swap.

Conclusion: a multisampled context = the fix. The MEDIUM-LARGE FBO+resolve shape was NOT needed.

### What changed (files)

- `crates/oh-render/src/gl.rs` — `pick_msaa_config` (the new config pick, the shared desktop+android
  `finish_gl` tail): level 0 → the first config (the pre-050 behaviour, byte-identical); level > 0 →
  the smallest config with samples ≥ level; none → the largest sample count BELOW the level + a
  named warning (the available sample counts are printed). Deliberately NOT a hard
  `ConfigTemplateBuilder::with_multisampling(n)` filter (the spec's suggested mechanism): (a) a hard
  `EGL_SAMPLES = n` / `GLX_SAMPLES` / `WGL_SAMPLES_ARB` filter (glutin 0.32.3's per-backend
  `config.rs`) returns ZERO configs when the driver offers only HIGHER sample counts — the C++'s
  SFML would take those (its `evaluateFormat` scoring, pinned `GlContext.cpp:733-749`: the shortfall
  penalty ×100000 is monotonic, the excess penalty ×1), and the hard filter would silently lose MSAA
  entirely; (b) glutin's `with_multisampling` `debug_assert`s a power of two (glutin
  `config.rs:143-147`), and the C++ menu's cycle reaches level 1 (MenuGame.cpp:1450-1464), which no
  driver offers as 1 samples — the C++ resolves level 1 to the smallest ≥1 config; the port now does
  the same. `GlParts::gl_samples()` (new — `glGetIntegerv(GL_SAMPLES)`, the acceptance observable on
  all three targets). Both context-creation tails log the ACTUAL `GL_SAMPLES` after make-current;
  the emscripten path logs the boolean mapping (`antialias = level > 0` — WebGL2's `antialias` is a
  BOOLEAN, the browser picks the sample count; the level's exact value is NOT honoured on the web,
  stated at the create site, deviations-register row 050).
- `crates/oh-render/src/window.rs` — `Window::create` gains the `antialiasing_level` parameter
  (desktop + emscripten variants; recorded on the window in all arms, headless included);
  `ensure_android_gl` passes `self.antialiasing_level` into `create_android_parts` (the C++'s own
  shape: the level is read at window CREATION — this materialization IS the port's creation; the
  runtime-recreate half stays with 29w's queued lane, the port's standing no-window-recreation
  deviation); `set_antialiasing_level`'s doc now states the creation-time application + the
  no-runtime-recreate deviation. New headless test: the store-half round trip (C++ default 3 per
  GameWindow.hpp:38).
- `crates/oh-render/tests/backend.rs` — the two acceptance tests (below) + the three existing
  probe contexts (gpu_smoke / swap-interval / fullscreen) updated to the explicit level-0 form.
- `crates/oh-app/src/windowed.rs` — `create_window` (the shared desktop+wasm path) passes
  `config.get_antialiasing_level()` into `Window::create` (the C++ main.cpp:307 value; the post-create
  `set_antialiasing_level` call stays, the C++-order store half).
- `crates/oh-app/src/android.rs` — comment corrected: antialiasing is no longer in the
  "android-platform no-op half" (it is applied at GL materialization; fullscreen / fps-limit stay
  record-only; vsync applies live at the materialization).
- `crates/oh-render/tests/golden_frames.rs`, `crates/oh-app/examples/pack_shaders_gl.rs` — the probe
  contexts pinned at explicit level 0 (pre-050 first-config behaviour).
- `.agents/audits/config-application.md` — section A's last row: MISSING×3 → APPLIED×3 with the real
  behaviour per target (incl. the wasm boolean mapping); "Filed but unfixed" → the closed-record
  section with the shape diagnosis.
- `.agents/plans/STATUS.md` (row 50) + `.agents/plans/001-rust-port/deviations.md` (row 050 — the
  wasm mapping).

### Citations actually read (hygiene §11)

- C++: `cpp-original/src/SSVOpenHexagon/Core/main.cpp:307` (`window->setAntialiasingLevel(
  hg::Config::getAntialiasingLevel())` in the pre-`run` setter block :304-310);
  `cpp-original/src/SSVOpenHexagon/Global/Config.cpp:703-708` (the setter's change-gated store) +
  `:1177` (the getter); `cpp-original/src/SSVOpenHexagon/Core/MenuGame.cpp:1446-1464` (the menu
  "antialiasing" item: 0 → 1, else ×2 clamped to 16 / ÷2).
- SSVStart (main checkout, populated — hygiene §11's "not everything is empty"): `GameWindow.hpp:38`
  (`antialiasingLevel{3}`), `:85-99` (`recreateWindow` → `ContextSettings{0, 0, antialiasingLevel,
  0, 0}` at :91), `:187-190` (`setAntialiasingLevel` = store + `mustRecreate`).
- Pinned SFML fork `dacceddbd` (`~/.cache/ohoracle-deps/sfml-fork/` — the CPM pin; the repo's SFML
  submodule is EMPTY in the lane worktree): `GlContext.cpp:725-749` (`evaluateFormat` —
  `antialiasingDiff` ×100000 on shortfall, ×1 on excess), `:974-988` (the post-creation
  "does not fully meet the settings" warning — the downgrade-is-a-warning shape),
  `EglContext.cpp:300-352` (enumerate-ALL-configs + score, `multiSampling ? samples : 0`),
  `:365-396` (`updateSettings` reading back `EGL_SAMPLE_BUFFERS`/`EGL_SAMPLES`), `GlxContext.cpp:443`,
  `Win32/WglContext.cpp:489-500`, `OSX/SFContext.mm:182-204`.
- glutin 0.32.3 (cargo registry source): `api/egl/config.rs:122-124` (+ glx/wgl siblings) — the
  hard-filter mechanics the pick deliberately avoids; `config.rs:143-147` (`with_multisampling` +
  the power-of-two `debug_assert`); the `GlConfig::num_samples` read-backs per backend.

### The measured acceptance (the ACTUAL values, not the requested ones)

Desktop, Mesa GL 4.5 (llvmpipe software, Xvfb :99) via a throwaway main-thread probe binary
(`/tmp/lane050-probe` — NOT committed: winit 0.30 panics creating an `EventLoop` off the main thread
on X11, so the `backend.rs` windowed tests can only measure from a main-thread binary; they
skip-with-reason headless, D-011):

| requested level | available Mesa configs | measured `GL_SAMPLES` | pick |
|---|---|---|---|
| 0 | [0, 0, 4, 4, 8, 8, 0, 4, 8] | **0** | first config (pre-050 behaviour) |
| 2 | as above | **4** | smallest ≥ (no 2-sample config; the C++'s scoring answers identically) |
| 4 | as above | **4** | exact |
| 8 | as above | **8** | exact |
| 16 | as above | **8** | closest available + the named warning (SFML-faithful) |
| 32 | as above | **8** | closest available + the named warning (SFML-faithful) |

So: the context reports the requested count when the level is nonzero AND the driver carries it,
reports 0 when it is zero, and downgrades with a named log line when it can't — exactly the C++'s
observable behaviour on this driver.

Tests: `antialiasing_level_zero_reports_zero_gl_samples` (hard assert GL_SAMPLES == 0 at level 0)
and `antialiasing_level_four_reports_the_driver_samples` (level 4 → GL_SAMPLES ≥ 1, the measured
value printed — a driver with no MSAA config at all prints 0 + the pick's warning instead of
failing: reported, not asserted away) — both pass on this box with a live display and skip with a
printed reason headless; plus the headless store-half round trip (window.rs).

### Gates

- Host gate on the final tree: fmt / build / clippy `-D warnings` / `test --workspace` (998 passed,
  0 failed — 995 + the 3 new) / `tools/lua-parity.sh` — all green.
- `tools/check-android.sh` (`cargo check -p oh-app --target aarch64-linux-android`) green on the
  final tree — the EGL path is invisible to the host gate (hygiene §4; this check is the point).
- `cargo check -p oh-web --target wasm32-unknown-emscripten` green in the emsdk container
  (`docker.io/emscripten/emsdk`, rustup 1.97.1 pin + the wasm target — the recipe in
  `/tmp/lane050-wasm-check.sh`): the emscripten cfg paths compile (the full release build +
  wasm-bindgen link is the release workflow's job, as in 049's gate pattern).

### What I could NOT establish (honest negatives)

- **The Android device's `GL_SAMPLES`** (probe A — **VERIFIED 2026-10-18, 051, operator's device, ADB TCP `192.168.10.138:40119`):** the standing create-time log read as expected — logcat shows `[oh-render] context MSAA: requested level 4 — config with 4 samples` immediately followed by `[oh-render] context GL_SAMPLES = 4` (tag `RustStdoutStderr`, via the 039 log sink): the device context honors the full requested 4× MSAA, matching the desktop's readback. Raw: `.agents/evidence/lane-051/lane051-session.logcat`. **Both 050 device-side residuals are now closed.**
- **The browser's `GL_SAMPLES`** — no browser on this box. The standing probe is the emscripten
  create log (`[oh-render] WebGL2 antialias = … — GL_SAMPLES = N`, the browser console / the
  web-boot trace channel); the level's exact value was never claimable there (the boolean mapping).
- **The runtime level-change half** — the C++'s `mustRecreate` → `recreateWindow` (a level change in
  the options menu recreates the window with the new samples) has no port equivalent: the port's
  window is created once and never recreated at runtime (the standing documented deviation; 29w's
  queued lane owns the recreation mechanism). Level changes after boot record on the window but do
  not re-apply. The menu's "antialiasing" item itself is unported (the C++'s MenuGame.cpp:1446 item
  has no port counterpart yet), so in practice the level only ever reaches the context from the
  config file at boot — which IS the C++'s main flow (main.cpp:307).
- A screenshot read by eye was NOT used as acceptance (explicitly excluded by the spec); the
  `GL_SAMPLES` read-back above is the acceptance.

### Decisions taken (spec deviations, stated)

1. **Enumerate-and-score instead of `with_multisampling(n)`** for the glutin/android pick — reasons
   above (the zero-configs-on-higher-only-sample-counts failure mode + the level-1 power-of-two
   `debug_assert`). The observable behaviour matches the C++ (the pinned SFML's own scoring); the
   spec's parenthetical named the MECHANISM, and the measurement-driven shape won, per the spec's
   "treat the fix SHAPE as open until you have measured it".
2. **The fallback picks the CLOSEST available config (largest samples below the level), not the
   0-sample first config** — the first draft did the latter and was corrected against the pinned
   `GlContext.cpp:733-749` monotonic-shortfall scoring (level 16 over {0,4,8} must land on 8, as the
   C++ would; 0 would have been a C++-divergence on exactly the Android risk case the spec named).
