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
