# Task 029e-batch — the windowed render wiring batch (29e1, 29e2, 29e3, 29e4, 29n1)

Five audit findings, all in one coherent place: what the windowed build feeds the
renderer each frame. Gate green at **795 tests**. **No worktree** (D-035). One
commit per item.

The C++/Rust pair here is `HGGraphics.cpp` (frame pipeline) + `HexagonGame.cpp`'s
render setup, against `crates/oh-app/src/bridge.rs` + `crates/oh-render`. The
audit report with the full evidence tables is `.agents/audits/graphics.md`
(M1–M4 + the shader seam) — read it; the citations below were re-derived.

---

## 1. 29e1 — the HUD/particle textures are never loaded (M1)  [uservisible-cosmetic, most visible]

C++ loads and assigns the real textures: `initKeyIcons`'s texture loop,
`HexagonGame.cpp:94-105` (with `setSmooth(true)`), called from the ctor `:453`
and re-applied on `onRecreation` `:339`; and `:324-325` takes
`txStarParticle`/`txSmallCircle` from the loaded assets. The sprites covered:
`keyArrow.png` (×2 — check what the C++ really does), `keyFocus.png`,
`keySwap.png`, `replayIcon.png`, `starParticle.png`, `smallCircle.png`
(consumed by `HGGraphics.cpp:373-376`, `:383` icons and `:397-417` particles).

The windowed port instead renders from `Texture::new()` — a **1×1 white pixel**
(`oh-render/src/texture.rs:206-212`) — at `bridge.rs:125-127` (`tx_star_particle`,
`tx_small_circle`) and `bridge.rs:186-198` (`key_arrow`/`key_focus`/`key_swap`/
`replay` then `init_key_icons`). So all five HUD icons and every PB/trail/swap
particle draw as white squares. The PNGs exist in `_RELEASE/Assets/`.

Fix: load them through the same asset path the other sprites use (find how the
port already loads e.g. the player/hexagon textures, and follow it — including
whatever the C++ does for `setSmooth(true)`; if the port's `Texture` has no
smoothing concept, say so and record it), then wire them into `init_key_icons` and
the two particle sources. **Add a host test that the textures are real** — e.g.
after init, the key/particle texture dimensions are not 1×1, or they resolve to a
loaded image — so this cannot silently regress to placeholders again.

## 2. 29e2 — the per-frame text size/origin update is never applied (M2)  [uservisible-cosmetic]

C++ `updateText` sets `characterSize` and `origin` **every frame**
(`HGGraphics.cpp:552-593`): `timeText` 70, `text` 20 + origin, `fpsText` 20,
`messageText` 32 + origin `getGlobalWidth()/2`, `pbText`
`64 + sin(pbTextGrowth)*10` + centre origin, `replayText` 16.

The port HAS the function — `oh_render::text_draw::update_text` — but it has
**zero call sites** (grep: only the re-export `oh-render/src/lib.rs:93` and a doc
mention in `graphics.rs:249`). So the rendered `Text`s keep their ctor values
forever (`bridge.rs:172-179`: fps 25, time 70, text 25, replay 20, message 38,
pb 65 — those are the C++ *ctor* values at `HexagonGame.cpp:296-310`, which the
C++ then overwrites each frame). Consequences: wrong sizes everywhere, message
and PB text left-anchored instead of centred, and the PB size pulse lost
(`pb_text_growth` is copied into `DrawState` at `bridge.rs:353` and consumed by
nobody).

Fix: call `update_text` each frame with the C++ values, including the
`/zoomFactor` division wherever the C++ divides (read the ctor + updateText
together), and consume `pb_text_growth`. Test the pure part (sizes/origins/pulse
computed from the inputs) rather than trying to assert pixels.

## 3. 29e3 — the FPS counter is a constant `"0"` (M3)  [uservisible-cosmetic]

C++ streams `window->getFPS()` into `fpsText` (`HGGraphics.cpp:560-564`), gated on
`getShowFPS()`. The port reads the `Renderer` seam's `fps()`, whose default is `0`
and which `WindowTouchRenderer` never overrides (`oh-game/src/renderer.rs:44-47`,
`windowed.rs:172-185`), then formats a `u32` (`update.rs:1460-1464`).

The live value exists and is never read: `oh-render/src/timer.rs:168`
(`TimerStatic::get_fps`). Fix: feed the live FPS through to wherever the FPS text
is built, and match the C++'s string conversion — check `ssvu::toStr(float)`'s
spelling (the C++ formats a float, not an integer; the port's number-formatting
notes in `.agents/skills/lib-luajit.md` and the existing `snprintf` helpers are
the precedents) rather than guessing `u32`. Test the formatting.

## 4. 29e4 — `onRenderStage` always receives frameTime 1.0 (M4)  [uservisible-cosmetic, conditional]

C++ passes `60.f / window->getFPS()` to the `onRenderStage` Lua hook
(`HGGraphics.cpp:73-74`). The port's `app.fps` is initialised to `60.0`
(`bridge.rs:122`, `:209`), never updated in the windowed loop, so
`60.0 / input.fps` in `draw_frame` (`graphics.rs:405-411`) is always exactly
`1.0`. Levels that scale per-stage shader effects by the frame time therefore
behave as if the machine were locked at 60 fps.

Fix together with 29e3 (same plumbing: the live FPS from the timer into the frame
input). Same guard: the C++ reads the *window* FPS — make sure the value has the
same meaning (per-frame average, not instantaneous) as `GameWindow.hpp:335-339`.

## 5. 29n1 — do the pack fragment shaders ever execute? (verify, then implement or record)  [internal]

The audit's own largest "seam": per-stage **pack fragment shaders are bound in
`RenderStates` but never executed** on GL — `windowed.rs`'s module docs concede
"the documented issue-10 gap … the pack's fragment shaders do not execute", and
`GlDrawTarget::draw_vertices` never binds a pack shader.

**First verify this is true** (read the C++'s per-stage shader binding in
`HGGraphics.cpp` — what it binds, when, and from where) and then either implement
it or record precisely what is missing and why. This is the largest item here: if
implementing per-stage pack fragment shaders on GLES/GL is more than a
mechanical wire-up, **stop, record the precise gap with citations, and report it**
— do not half-build a shader pipeline. Custom levels' shader effects silently
doing nothing is a real fidelity gap, so a precise statement of it is valuable on
its own.

---

## Hygiene

- **The phone's ADB is DOWN** (`adb connect 192.168.10.138:38721` → connection
  refused; the host still pings it). Do NOT attempt device work. Do not add the
  temporary `debuggable` flag. Record any visual confirmation as **PENDING** in
  your report; the orchestrator batches device verification when the phone is
  re-armed.
- One commit per item, naming the row id. **Stage explicit paths — never
  `git add -A`** (a shared checkout/index).
- Keep the gate green after each item: `bash tools/gate.sh` (expect 795 + yours).
- Refresh `deviations.md` rows (issue 28's rule) for anything you fix that was a
  deviation; `.agents/audits/*.md` are committed reports — do not rewrite them.
- Append a `## Report`: per item — the fix, the citation you verified, the test,
  and explicitly what you could NOT establish (device-visual confirmations
  included).
