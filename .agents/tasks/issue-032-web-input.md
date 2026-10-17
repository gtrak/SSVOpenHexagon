# Task 032 — the web input feed (spec: `.agents/plans/001-rust-port/32-web-input.md`)

The emscripten build boots and the engine runs (issue 26) but has NO input of
any kind. This lane feeds the SAME input sink/state the windowed path feeds
(shared touch sink + `crates/oh-render/src/input.rs` InputState) from DOM
events on the canvas, per spec §2, and meets spec §3 acceptance in a browser.

## Report (lane 032 — acceptance completed, 2026-10-17)

### What landed (commits on `lane/032`, cut from master `643d88d9`)

1. `c806c53c` — the web input feed (`oh-render` window.rs, gl.rs,
   `oh-app` windowed.rs, `oh-web` site/index.html, oh-render Cargo.toml):
   - **Keys**: window-level `keydown`/`keyup` → the port's SFML key codes
     (`web_key_to_sfml`, the 026 mapping, now ungated + extended with the
     `Numpad*`/`Digit*`/symbol rows) → `KeyPressed`/`KeyReleased`; single
     printable chars (no modifiers, not `repeat`, not IME-composing) also
     emit `TextEntered`. `preventDefault` ONLY on consumed keys (never
     ctrl/meta-combos, F5/F11/F12, IME composition, or unmappable codes).
   - **Pointers**: canvas `pointerdown/move/up/cancel` tracked by
     `pointerId` through the pure, host-tested `WebPointerFeed` (≤16 finger
     slots; mouse → Mouse* events, touch/pen → Touch* events on the SAME
     shared touch sink the menu 9-zone grid + the issue-31 overlay read).
     `setPointerCapture` on down (delivery guarantee; swallowed
     `NotSupportedError` for untrusted pointers). `wheel` on the canvas →
     the wheel sink (`preventDefault`).
   - **Positions + size**: canvas CSS box × `devicePixelRatio` = the real
     window size (025g lesson) — `Window::create` measures it, the
     `Resized` arm keeps GL backing store + viewport in step
     (`GlParts::resize`, emscripten-only). Everything (grid thirds,
     overlay hit-tests, half-splits) lives in that one space.
   - **Release-on-blur**: window `blur` + `visibilitychange(hidden)` →
     `LostFocus` (the whole InputState resets — the C++ focus-loss fold).
   - **dt**: a >100 ms frame gap caps `frameTime` and drains the timer
     backlog (`WEB_FRAME_GAP_CLAMP_FT`, emscripten-gated in `tick_fields`)
     — no accumulated-dt catch-up after a background tab.
   - **issue 31 seam**: the gameplay touch overlay is ENABLED on
     emscripten (`any(android, emscripten)`) and is fed by this same
     pointer feed — 31 and 32 meet at the shared touch sink.
   - 7 new in-process tests (feed mapping, 17th-finger overflow,
     release-once guard, key mapping, prevent-default rules, real-size
     math, dt clamp).
2. `75e15d4a` — the `status()` probe's state half (the rAF loop's
   `WEB_TICKS` twin: a per-frame store of the composition's current state
   — the menu's C++ `States` name or `"game"` — read by `oh-web`
   `status()` as `state=<...>`).
3. `72bc0865` — the two emscripten-only signatures the host gate cannot
   see (`GlParts::resize(&mut)`, `set_pointer_capture(i32)`).

### Verification

- **Host gate**: `tools/gate.sh` green at every commit (fmt, clippy
  -D warnings, 1002+7 tests, lua-parity).
- **Wasm gate**: `cargo build --release -p oh-web --target
  wasm32-unknown-emscripten` in the `emscripten/emsdk:latest` podman
  image (recipe: `/home/gary/dev/oh-lanes/032-wasm-tgt/build-em.sh`,
  target dir `/home/gary/dev/oh-lanes/032-wasm-tgt`) — the host gate
  does NOT see the gated arms (026's 33d1402f lesson; this lane hit it
  once too — the fix commit).

### Browser acceptance (spec §3) — driven, not inferred

Environment: `http.server` on :8067 serving
`/tmp/oh-web-serve-032` (the three artifacts + `site/index.html`);
headless Chrome 148 (`--use-angle=swiftshader
--enable-unsafe-swiftshader`, CDP :9333) driven through agent-browser
(`connect 9333`, then `eval`-dispatched DOM events + the page's own
`status` div / `status()` probe as the observable).

Drive (fresh user — no `Profiles/` in the preloaded `_RELEASE`):

| step | input | observable (port's own state) |
|---|---|---|
| boot screens | 2× Enter key release | `menu:LoadingScreen` → `menu:EpilepsyWarning` → `menu:ETLPNewBoot` |
| name entry | typed `vee032` (per-char keydowns → `TextEntered`) | `menu:ETLPNewBoot` (unchanged; chars in the entered-text sink) |
| create profile | Enter held 400 ms (rising edge, frame-spanning) | `menu:SMain` (+ `[::loadAssets] loading local profiles` console line) |
| level select | Enter held | `menu:LevelSelection` |
| start level | Enter held | **`game`** |
| in-game keys | `A`/`D` held 600 ms each (the default RotateCCW/CW) | `game`, `errors` clean |
| wheel | `WheelEvent` on canvas | `errors` clean |
| issue-31 overlay | touch `pointerdown/up` on the bottom-left + bottom-right button rects (64×64, 12 px pad) | `game`, `errors` clean |
| engine half-split | touch hold in the left screen half (100, 200) | `game`, `errors` clean |
| overlay exit | touch tap at top-left (44, 44) | **`game` → `menu:LevelSelection`** (the C++ `fnGoToMenu` path — proves the overlay hit-test is in the SAME space as the pointer feed) |
| re-enter | Enter held | `game` |
| release-on-blur | window `blur` event | `errors` clean, `game` (the LostFocus fold) |

Evidence bar met: the port's own console lines (the `[web]` boot lines,
the `[::loadAssets]` channel line, the `status()` probe with
`state=<...>`), `errors` clean at every step, and **one** screenshot
(analyzed with PIL, not read as an image): 1280×577 (the real
CSS×DPR window), 93 color buckets / 0.31 % near-black (SwiftShader is
actually drawing the level — NOT the 026 black `NullDrawTarget`), the
translucent overlay buttons present in all four bottom positions +
top-left Exit.

### What I could NOT establish (precise negatives)

- **Rotation angle / piece state**: no in-engine probe exposes the
  rotation value or the tutorial step, so "the piece rotated" is proven
  only by (a) the inputs being consumed without error, (b) the overlay
  exit hit-test proving the coordinate space, and (c) the rendered
  frame. A pixel-diff of two frames would close this, but the spec caps
  image reads at one and PIL on one frame shows presence, not change.
- **The wheel's menu-side effect** (LevelSelection list scroll) was not
  visually confirmed (same one-frame limit); the mapping itself is the
  pre-existing 29b2 `map_window_event` wheel arm, covered by in-process
  tests, and the browser dispatch exercised the live path without error.
- **Profile persistence across reloads**: MEMFS is session-only — a
  reload re-preloads `_RELEASE` and the created profile is gone
  (expected; the persistent-VFS follow-up is out of spec for 32).
- **Trusted-input subtleties**: synthetic `PointerEvent`s are
  untrusted, so `setPointerCapture` throws `NotSupportedError` for them
  (swallowed by design — the fold still runs); a real user's pointer
  captures normally. Everything else (positions, buttons, capture
  delivery for real pointers) is the browser's own event pipeline.

### Re-verify (commands)

```sh
bash /home/gary/dev/oh-lanes/032-wasm-tgt/build-em.sh \
  "cargo build --release -p oh-web --target wasm32-unknown-emscripten"
# serve: /tmp/oh-web-serve-032 (oh-web.js + oh_web.wasm + oh_web.data + index.html)
python3 -m http.server 8067 --directory /tmp/oh-web-serve-032
# chrome: --headless=new --use-angle=swiftshader --enable-unsafe-swiftshader
#         --remote-debugging-port=9333  →  agent-browser `connect 9333`
# drive:  Enter ×2 (boot) → type name → Enter (held) → Enter → Enter
#         (status div shows state=menu:... → game)
```
