# Task spec — issue 010: `oh-render` backend (window, input, vertices, shaders, fonts)

You are a worker. Implement exactly this issue. Do not redesign or widen scope.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port. Issues 001–006 and
008 are LANDED; `bash tools/gate.sh` passes 4/4.

This issue builds the SFML analogue: the platform backend the game draws and reads
input through. **It must build on Linux and Windows.**

Read first:
1. `.agents/plans/001-rust-port/PLAN.md`, `DECISIONS.md`
2. `.agents/plans/001-rust-port/10-render-backend.md` (the contract)
3. `.agents/plans/001-rust-port/mapping/rendering-menu.md` §1 (RenderStage),
   §2 (`ssvs::Camera`), §10 (SSVStart geometry helpers)
4. `.agents/plans/001-rust-port/mapping/oracle-vectors.md` — the **SFML key/button
   enum values** (KeyCount 101, ButtonCount 5) and the `sf::Packet` encoding rules
5. The SFML usage inventory in
   `.agents/plans/001-rust-port/mapping/thirdparty-cpp.md` (if present; otherwise
   derive it with `grep -rho 'sf::[A-Za-z:]*' src/ include/ | sort -u`)
6. `.agents/skills/README.md`

## Working agreement

- **Settle the backend choice and RECORD it in `DECISIONS.md`** (D-004's SFML
  entry) with evidence from the candidate crate's real source. Assess against the
  actual usage inventory before committing. `rsfml` is the primary candidate;
  `winit`+`glow`+`rodio` is the fallback. Reading the crate's real source under
  `~/.cargo/registry/src/**` is required — do not fetch from the web.
- **Write first**, compile early, keep the crate buildable headless (no display
  needed for `cargo test`).
- **Enum values are a compatibility surface**: `KKey`/`MBtn` must map to the exact
  SFML integers (the oracle doc has the full table). `oh-config` already defines
  `Key`/`Button` with these values — reuse them if it is a clean dependency.
- **Commit at the first green build.**

## What to build

1. Window + event loop surface: create, poll events, close, vsync, framerate limit,
   icon, mouse cursor visibility, joystick.
2. Input state: per-key/button pressed maps over the exact enum values, matching
   how SSVStart's `InputState` works (see `extlibs/SSVStart/include/SSVStart/Input/`).
3. Vertex drawing: `VertexArray`/`Vertex`/`PrimitiveType`/`Color`/`RenderStates`
   with SFML's semantics, and the draw-entry surface `oh-game`'s vertex batch feeds.
4. Textures, `Font`/`Text` (origin/scale/rotation/color/outline/bounds), `View`
   (camera transform), `RenderTexture` (for screenshots/shaders).
5. Fragment shaders: load from file, set uniforms (the game's names — check
   `StyleData`/`HGGraphics` usage and the `.frag` files under
   `_RELEASE/Packs/*/Shaders/`).
6. Audio device surface: play/stop/pitch/volume/playing-offset for music and sound.
   (The game-level volume curves are `oh-audio`'s job, not this crate's.)

## Verification

1. `bash tools/gate.sh` — paste all `GATE_EXIT=` lines.
2. Headless test: create an offscreen render target, draw a triangle, read back
   pixels and assert color/position.
3. Enum table test: assert the exact integer values from the oracle doc.
4. Text layout test: measure a known string; assert it is non-degenerate and stable
   across runs.
5. `cargo build --workspace --target x86_64-pc-windows-gnu` if the target is
   installed (report the result either way — it is a real cross-platform signal).

## Report format

Module map, the backend decision + evidence, gate outputs, skill corrections,
deviations, known gaps (especially any feature the chosen backend lacks).
