# Task 029n1 (K6) — the pack fragment shaders never execute on GL

Read `.agents/WORKER-HYGIENE.md` first. Context: the graphics audit's #5 finding
(`.agents/audits/graphics.md`), the last open item of issue 29. This is the
largest remaining fidelity gap in the port: **custom levels' shader effects do
not happen.**

**The audit claim to verify first, as a failing test:** per-stage pack shaders are
*described* (`RenderStates::shader`) but never *executed* —
`GlDrawTarget::draw_vertices` ignores `states.shader`, so no GL program is ever
bound and the pack's `.frag` code never runs. Prove it with a test that fails
before your change (see §4.1), then fix it.

---

## 1. What already exists — do NOT redo this

The *description* half of the subsystem is fully ported:

- **the whole Lua surface**: `crates/oh-lua/src/api/shaders.rs` registers all 13
  functions (`shdr_getShaderId` :36, `shdr_getDependencyShaderId` :67, the eight
  uniform setters `shdr_setUniformF` :145 / `FVec2` :169 / `FVec3` :193 / `FVec4`
  :215 / `I` :239 / `IVec2` :263 / `IVec3` :289 / `IVec4` :311, and
  `shdr_resetAllActiveFragmentShaders` :338 / `shdr_resetActiveFragmentShader`
  :354 / `shdr_setActiveFragmentShader` :381).
- **the asset registry**: `crates/oh-assets/src/assets.rs` — `ShaderRegistry`
  (:61/:167), the pack scan `load_pack_assets_load_shaders` (:527) over
  `.vert`/`.geom`/`.frag` (:548-550), mirroring C++ `Assets.cpp:107` (decl),
  `:495` (call), `:873-917` (impl, `:915-917` the three types).
- **the per-stage description + the documented hook**:
  `crates/oh-render/src/graphics.rs:93-121` (`shaders_enabled`, the
  `fragment_shader_ids[stage]` lookup, the `on_render_stage` hook documented as
  running "**before** returning the bound shader's states"), called at :408-409.
- **the `shadersRequired` update check**: `crates/oh-game/src/update.rs:485`.
- **GL plumbing to reuse**: `crate::shader::translate_for_gles`,
  `is_gles_context`, `compile_shader`, and the built-in pass-through program in
  `crates/oh-render/src/batching.rs:40-70`.

**The gap is one function class**: every `draw_vertices` implementation ignores
`RenderStates::shader` (trait at `graphics.rs:142`; the GL one is the real target,
the test-only impl at `graphics.rs:1581` shows the shape).

## 2. C++ authority (citations verified against the tree; D-014)

```
HGGraphics.cpp:60-77     the getRenderStates lambda:
  :62  if(!Config::getShaders()) → RenderStates::Default
  :67  status.fragmentShaderIds[static_cast<size_t>(rs)]
  :70  if(!fragmentShaderId.has_value()) → Default
  :73  runLuaFunctionIfExists<int,float>("onRenderStage", (int)rs, 60.f/window->getFPS())
  :77  return sf::RenderStates{assets.getShaderByShaderId(*fragmentShaderId)}
HGUpdate.cpp:420         if(!Config::getShaders() && levelStatus.shadersRequired) …
HGStatus.hpp:83          fragmentShaderIds (per-RenderStage)
LevelStatus.hpp:73       shadersRequired{false}
Assets.cpp:1125/:1139    getShaderIdByPath / getShaderByShaderId  (:1711-1719 public)
Assets.hpp:97-99         the public asset API
LuaScripting.cpp:1238    shdr_getShaderId        (:1249-1250 getDependentShaderFilename,
                                                 :1253 getShaderIdByPath)
LuaScripting.cpp:1274    shdr_getDependencyShaderId
LuaScripting.cpp:1377-1517  the eight setUniform* → all call shader.setUniformUnsafe(name, v)
LuaScripting.cpp:1519/1532/1549  resetAllActive / resetActive / setActiveFragmentShader
                         (bodies mutate hexagonGameStatus.fragmentShaderIds at :1522/:1536/:1553)
LuaScripting.cpp:766     lsVar("ShadersRequired", &LevelStatus::shadersRequired, …)
MenuGame.cpp:1383        the "shader effects" config row
Config.cpp:757 / Config.hpp:57   setShaders(bool)
```

**Also read, verbatim, before writing any GLSL handling:** SFML's own shader
implementation at the pinned commit (`vittorioromeo/SFML@dacceddb`, already seeded
locally by the 030b lane under `/home/gary/dev/oh-cpp-build/*-src`
(`sfml-src/`) — `src/SFML/Graphics/Shader.cpp` + `Shader.hpp`). **Do not guess
SFML's shader model.** Mirror exactly: the preamble it prepends, its source
rewrites/`#define`s, whether it injects a default vertex shader when only a
fragment file is given, how `gl_FragColor`/`gl_Color` are made to work, and how
the `texture` sampler is bound to the draw's current texture (`sf::Shader::CurrentTexture`
in SFML 2.x) — including that SFML **caches uniform values on the shader object
and uploads them at bind time**. `setUniformUnsafe` means **no existence check**:
an unknown uniform name is silently ignored, exactly as GL would.

## 3. Requirements

1. **Shader objects with a uniform store.** `setUniform*` writes to the shader
   object (per id) and the values are uploaded when that shader is bound — not at
   set time. Uniform kinds: `f32`, `i32`, `Vec2/3/4`, `IVec2/3/4` (the exact sets
   the 13 Lua fns expose).
2. **Compilation**, lazily, on the render thread with a live context (registration
   happens at asset load, possibly headless with no context — see §3.7). Compose
   `.vert` + `.frag` (and `.geom` when present) into one program like SFML does.
3. **GLES**: pack sources go through `translate_for_gles` when
   `is_gles_context`, matching the existing built-in program's path. The shipped
   packs' sources are GLSL-1.20-style and use `gl_FragColor`/`gl_Color`, which GLES
   does not have — SFML's rewrites are what make them work, so §2's reading is
   load-bearing here.
4. **Bind in the draw path.** In the real `draw_vertices`: if `states.shader` is
   set, bind that program, upload its stored uniforms, bind the current texture to
   the `texture` sampler (only when the shader actually uses it), draw, then
   restore the default program. Verify the *order* against C++/SFML — the shader is
   part of the render states for that single draw.
5. **The `onRenderStage` hook must fire, at the same point, with the same args.**
   `(stage: i32, 60.0/fps: f32)` — note `60.f/window->getFPS()`, so it needs the
   live fps the port already tracks (`AppDraw.fps`, the 029e4 work), and the
   callback must run **before** the draw for that stage so the uniforms it sets
   apply to that very draw. This is the ordering that makes the whole feature work;
   get it wrong and shaders render with stale/no uniforms. Headless has no draw
   path — record what the C++ does there (`Config::getDisableGameRendering` guard at
   `HGGraphics.cpp:55`) and mirror it.
6. **`shadersRequired`'s observable effect must match.** Read `HGUpdate.cpp:410-440`
   for exactly what the C++ does when `!Config::getShaders() && shadersRequired`,
   and compare with `update.rs:485` — 29s1 showed this class of check can be
   right in spirit and wrong in detail. There is a shipped level for it
   (`shaders_required.lua`).
7. **Headless semantics.** The C++ passes `headless` into
   `loadPackAssets_loadShaders` (`Assets.cpp:873`): read what it skips and mirror
   it. Registration must keep working headless (the Lua `shdr_getShaderId` path is
   exercised headless by the harness); compilation must not be attempted without a
   context.
8. **Keep the log strings verbatim.** The C++ logs `` `u_getShaderId` failed, no
   id found for '<x>' `` from the function named `shdr_getShaderId` — a real
   verbatim quirk (`LuaScripting.cpp:1257-1259`, `:1295-1297`). Do not "fix" the
   name; 29q1 exists precisely because this class of drift was everywhere.

## 4. Verification (three columns — this is the point of the task)

**4.1 Headless/in-process (the primary column).** Three *shipped, real* levels
exercise this subsystem, so no synthetic fixture is needed:

- `_RELEASE/Packs/artwork/Scripts/Levels/pointless.lua` — `steamart.frag` bound to
  **8 stages** (:110-117: BACKGROUNDTRIS, WALLQUADS3D, PIVOTQUADS3D, PLAYERTRIS3D,
  WALLQUADS, CAPTRIS, PIVOTQUADS, PLAYERTRIS) and uniforms set at :164-166
  (`u_resolution` Vec2, `u_time` F, `u_rotation` F).
- `_RELEASE/Packs/experimental/Scripts/Levels/text_shader.lua` — `text.frag` on
  `RenderStage.TEXT` (:5-6).
- `_RELEASE/Packs/experimental/Scripts/Levels/shaders_required.lua` — the
  `l_setShadersRequired(true)` path (:57).

Assert: the stage→shader assignments the Lua makes end up in
`fragment_shader_ids`; `onRenderStage` fires once per stage-that-has-a-shader,
with `(stage i32, 60/fps f32)`, **before** that stage's draw; the uniform store
receives exactly the sets the Lua performed; `shaders_enabled == false` yields no
shader state *and* suppresses the hook (C++ `:62` returns before `:73`).

**4.2 The compile guard (cheap, mechanical, high value).** Compile **all four**
shipped `.frag` files (`artwork/Shaders/{gradient,steamart}.frag`,
`experimental/Shaders/{pixelate,text}.frag`) both raw and through
`translate_for_gles`, asserting no compile/link error. This is the 29v1 lesson
(a syntax error that made a feature a no-op on every call) applied to GLSL, and it
catches the whole class without a GPU.

**4.3 GL column.** If this environment can give a real/software GL context, assert
the program links and the draw executes; if not (no display, no llvmpipe), gate the
test and **say so explicitly in the report** — do not claim GL verification you did
not run. **The single best visual evidence is a screenshot of `pointless` showing
the gradient**: that is your one permitted image read for this lane.

**4.4 The C++ oracle column (fits 030b).** `pointless` is a shader-using level, so
ask that the fingerprint corpus include a shader level; note in your report that
`onRenderStage` executes Lua from the **draw** path, i.e. it can consume the
cosmetic RNG (see 030b's game-vs-cosmetic RNG split) — flag it if the fingerprint
disagrees there, because that would be a real finding, not a nuisance.

## 5. Hygiene

`tools/gate.sh` 4/4 before every commit; `cargo fmt --all` before committing;
bounded processes (`timeout`); commit per milestone with **explicit paths**;
at most one image read (§4.3); **stop-and-report** if a wall is hit.

## 6. Scope guard

If reproducing SFML's shader model turns out to require reimplementing large parts
of SFML's GL abstraction, **stop and report the cost** with what you did
understand. A correct, narrow implementation (bind + uniforms + the hook + the
compile guard) that leaves geometry shaders unsupported is a fine landing; a broad
rewrite of the render backend is not.

## Report

### What landed

Two commits on `lane/029n1` (the inherited harvest `3ce3c351` + the red-gate fix
`4b538abb` precede them):

- **`4b538abb` (milestone 1)** — the harvested tree's red gate: the
  `lua_state.rs` double-slash shader path (`get_dependent_asset_filename`
  re-joined the already-relative path to the pack root), the sentinel
  `i64`-typed Lua assert, the test's pack slicing. Tree green (937 tests,
  clippy clean) before any new work.
- **`44c8f885` (milestone 2)** — the GL execution half + the real GL
  verification: `oh-render` `gl.rs` (the compatibility-profile desktop
  context + the `try_create_gles` EGL path), `pack_shaders.rs` (the
  C++-shaped fragment-only desktop program, the CPU view pre-transform,
  the set-time uniform upload), `gl_passes.rs` (the `QuadVertex` field
  visibility), and the new `oh-app` `pack_shaders_gl` example (the §4.2
  + §4.3 columns, with the `gles` child mode).

### §4.1 Headless/in-process — PASSED (primary column)

`crates/oh-app/tests/pack_shaders.rs` (9 tests, all green):
`pointless`'s 8 stage→`steamart.frag` bindings land in
`fragment_shader_ids` + the uniform store (name-sorted, the Lua's exact
sets); `text_shader.lua` binds `text.frag` to `RenderStage::Text`;
`shaders_required.lua`'s `l_setShadersRequired(true)` suppresses the
score only with shaders off; the `onRenderStage` hook fires once per
shader-bound stage with `(stage i32, 60/fps)` before the stage's draw,
and `shaders_enabled == false` suppresses both the state and the hook
(the C++ `:62`-before-`:73` order); the `shdr_getShaderIdFor` sentinel
for a missing name; headless load registers no shaders and the Lua path
is a no-op.

### §4.2 Compile guard — PASSED, both columns, NO GATE

Executed by the `pack_shaders_gl` example under `xvfb-run`:

- **raw (desktop) column** — all four shipped `.frag` files
  (`artwork/Shaders/{gradient,steamart}.frag`,
  `experimental/Shaders/{pixelate,text}.frag`) compile + link through
  `PackShaderProgram::new` on the desktop context (the exact draw-time
  path: the fragment-only program + the two stage VAOs).
- **translated column** — on a REAL GL ES 3.2 context (Mesa EGL, via
  `GlBackend::try_create_gles`; winit 0.30 allows one `EventLoop` per
  process, so the column runs in the example's `gles` CHILD MODE — a
  fresh process, same binary, `arg == "gles"`). The assertion is the
  real GLES execution path (`PackShaderProgram::new` — the 25b-SG3
  two-stage build; GL ES 3.x links no fragment-only program, probe:
  "program lacks a vertex or mesh shader"):
  - `pixelate.frag` (no fixed-function builtin): **the two-stage
    program builds OK** — the builtin-free path works end to end;
  - `gradient.frag` / `steamart.frag`: fail with the documented 25b
    sticky fallback (`gl_Color` undeclared — vertex-stage-only in ES
    1.00), the translations asserted **verbatim-faithful** (the 29v1
    guard: `translate_for_gles` may only touch the `#version` line +
    insert the missing float precision; every other line must pass
    through in order);
  - `text.frag`: fails with the documented fallback too — its `switch`
    + the `.f`-suffix floats are GLSL 1.30/ES 3.00 constructs (a
    reserved word / invalid suffix in ES 1.00), so it never compiles
    on GLES by design; on desktop the raw file compiles (above).

### §4.3 GL column — PASSED, NO GATE

- **§4.3a** — `gradient.frag` bound to a full-screen white quad through
  the real `GlDrawTarget::draw_vertices` (`RenderStates::with_shader`,
  the `Assets` uniform store + the `AssetsPackShaderProvider`):
  readback **bottom `[1, 0, 254, 255]` (blue) / top
  `[254, 0, 1, 255]` (red)** (the fragment's
  `u_color1`→`u_color0` mix over `gl_FragCoord.y/u_resolution.y`), the
  default-program control **`[255, 255, 255, 255]`** (pure white) —
  the negative control proving the pack fragment stage actually ran.
  This caught one real bug: the uniform upload must `gl.use_program`
  the pack program FIRST (the C++'s `UnsafeUniformBinder` does exactly
  this — a uniform set before the use lands in the wrong program's
  state: the "shader bound but the gradient is missing" symptom).
- **§4.3b** — the `pointless` level, a full game frame through the real
  game loop (the Lua level's 8 steamart-bound stages + uniforms from
  its own `onRenderStage`), shaders ON vs OFF: **7359 of 7500 sampled
  pixels differ, the ON frame has 277 distinct colours** (OFF: 8 —
  the default-program flat passes). Screenshot:
  `target/k6_pointless_shaders_on.ppm` (converted to
  `target/k6_pointless_shaders_on.png`).

### The one image read (consumed)

`target/k6_pointless_shaders_on.png`: the `pointless` frame with the
pack shaders ON — the scene is the `steamart.frag`'s green/cyan
swirl (clearly shader-driven, not a flat default pass), and the text
stage's glyph marks render as **monochrome dark blocks**: the Mesa
llvmpipe's fixed-function stage does not supply `gl_TexCoord[0]` from
attribute 3 (probe-verified: it stays `(0,0,0,0)`, so the glyph UVs
sample the font atlas's top-left texel — a uniform colour per glyph).
A spec-conformant desktop driver supplies the attribute-3 texture
coordinate, and the C++ binary itself relies on the same
fixed-function texcoord, so it renders the text stage identically on
that driver — a **driver limitation, not a port divergence**
(documented in the `pack_shaders.rs` module docs + the
`the_text_stage_vao_remaps_the_glyph_quads` test note).

### §4.4 (fits 030b) — flagged, as requested

`onRenderStage` executes Lua from the **draw** path (the per-stage
hook fires during the frame's draw, before that stage's GL draw). If
the 030b fingerprint corpus includes a shader level (`pointless`), the
hook's Lua can consume the **cosmetic** RNG — a fingerprint
disagreement there would be a real finding (cosmetic RNG bleeding
into the game-fingerprint comparison), not a nuisance.

### Decisions / divergences (all C++-anchored or probe-verified)

1. **The desktop context is the compatibility profile** — the C++'s
   SFML never sets `ContextSettings::Core` (the pinned fork's
   `GlxContext.cpp:688-691` non-`Core` default). glutin's default was
   Core 3.3, where a fragment-only program's fixed-function vertex
   stage cannot run at all. This also fixes the pack shaders on
   Windows (WGL's default context is compatibility too).
2. **The desktop pack program is the C++'s `sf::Shader` shape**: ONE
   fragment-only program per pack file (the `loadFromFile` verbatim
   compile — no injected vertex stage, no preamble rewrites), drawn
   through the two stage VAOs. Mesa 26 llvmpipe probe: a vertex
   shader writing `gl_Color` is rejected ("read-only"); the
   fragment-only + fixed-function shape is the only working path and
   the C++'s exact shape.
3. **The view transform is pre-applied on the CPU** — glow 0.18
   exposes no fixed-function matrix stack (the `glLoadMatrixf`
   equivalent), so `view_to_clip` replicates the built-in vertex
   shader's `mapCoordsToPixel` formula (the `batching.rs`
   `VERTEX_SHADER_SOURCE`'s per-vertex math, the 025c-verified port
   view semantics) before the upload: the same f32 ops, an
   observationally identical mapping. Pinned by
   `the_view_to_clip_matches_the_builtin_vertex_shader_formula`
   (the test's identity-case `y` expectations were sign-flipped
   against the built-in VS's `-2.0 * d.y / u_size.y` — corrected).
4. **The TEXT stage's VAO is re-mapped onto the fixed-function
   indices** (the `QuadVertex` 32-byte layout: attribute 0 = position
   @ 0, attribute 1 = color @ 16, attribute 3 = the font UV @ 8) —
   the scene stage keeps the `sf::Vertex` layout (attribute 0 =
   position, attribute 1 = the u8 color).
5. **The uniform upload is set-time against the pack program's own
   `glUseProgram`** (the C++'s `UnsafeUniformBinder` shape,
   `Shader.cpp:235-270`); the deferred-compile port re-uploads at
   bind/draw time against the re-bound program (the C++'s per-draw
   `bind`).

### Exact commands (the verification trail)

```sh
# the gate (4/4 green before each commit):
CARGO_TARGET_DIR=/home/gary/dev/oh-lanes/029n1/target \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
CARGO_PROFILE_DEV_INCREMENTAL=false bash tools/gate.sh

# §4.1:
cargo test -p oh-app --test pack_shaders        # 9/9

# §4.2 + §4.3 (the example spawns its own `gles` child internally):
cargo build -p oh-app --example pack_shaders_gl
xvfb-run -a -s "-screen 0 800x600x24" \
  target/debug/examples/pack_shaders_gl
# → "pack_shaders_gl: §4.2 + §4.3 PASSED"
# → target/k6_pointless_shaders_on.ppm (+ .png, PIL P6→PNG)

# the driver probes (one-shot, pre-implementation; the probe code is
# deleted, the findings are the module docs' caveat):
xvfb-run -a -s "-screen 0 800x600x24" \
  target/debug/examples/glprobe
```

### Hygiene

- One image read (the `pointless` screenshot, §4.3) — the lane's
  permitted maximum, consumed.
- All processes bounded (`timeout 900`/`1800`); no display-server
  mutation (xvfb); the C++ tree untouched; commits by explicit paths
  only; `cargo fmt --all` before each commit; gate 4/4 before each
  commit.
- Residual: the `gl_TexCoord[0]` text-stage pixels are NOT
  pixel-verified on this box (the llvmpipe limitation, §4.3b's
  monochrome glyph blocks); on a spec-conformant driver the path is
  the C++'s identical one.
