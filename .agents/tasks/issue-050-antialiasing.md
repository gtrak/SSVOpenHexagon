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
