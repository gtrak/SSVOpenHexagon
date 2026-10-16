# Task spec — issue 015c-tail: real `save_screenshot` (glReadPixels→PNG), SIGINT, window icon

You are a worker. Implement exactly this. Do not redesign or widen scope.
This closes issue 15's last three TODOs (small lane).

## Context

Repo `/home/gary/dev/SSVOpenHexagon`, base = master tip. `crates/oh-render/src/window.rs:573`
`save_screenshot` is a `_path` stub; `create_window`
(`crates/oh-app/src/windowed.rs`) has `TODO(issue-15c-icon)`; the C++ SIGINT
handler (`main.cpp:340-349`) is unported (a `TODO` marks it).

## C++ oracle (cite what you read)

1. `extlibs/SSVStart/include/SSVStart/GameSystem/GameWindow.hpp:167-174` —
   `saveScreenshot` = `sf::Texture::create(size)`, `t.update(renderWindow)`,
   `t.copyToImage()`, `img.saveToFile(path)`. SFML's `Texture::update(Window)`
   copies the back buffer with `glCopyTexSubImage2D` into a BOTTOM-UP-flipped
   texture (`m_pixelsFlipped = true`; SFML fork `src/SFML/Graphics/Texture.cpp:628-656`,
   local checkout `/tmp/sfml-fork`) and `copyToImage` reverses the rows on the
   way back (`Texture.cpp:413-424`) — net effect: the framebuffer (GL origin
   bottom-left, rows BOTTOM-UP) becomes a TOP-DOWN image (the port reads the
   default framebuffer directly with `glReadPixels` and does the same row
   flip); `Image::saveToFile` writes PNG (SFML fork
   `src/SFML/Graphics/ImageLoader.cpp:262` `stbi_write_png` — stb_image_write
   v1.16, `extlibs/headers/stb_image/stb_image_write.h`).
2. `main.cpp:340-349` (block `334-350`) — the SIGINT handler: log the
   signal, log stopping, `globalWindow.stop()`, log done. Note the C++
   `TODO (P2): UB` comment (main.cpp:339) —
   the port must do the SAME actions via SAFE means (an
   `Arc<AtomicBool>`/atomic flag the loop checks, not signal-handler
   mutation of non-atomic state; document the equivalence).
3. `main.cpp:312-328` — the icon: load `Assets/icon.png` via `sf::Image`,
   `window->setIcon`, AND re-run on window recreation (`onRecreation`).

## Scope

1. **`save_screenshot`**: real GL read (the port's GL stack is glutin+glow —
   `crates/oh-render/src/gl.rs`; read pixels from the bound framebuffer AFTER
   the frame's draws, rows bottom-up) + PNG encode. Use the smallest
   sound dependency (e.g. the `png` crate or `image` with default features
   off — justify in a D-004-style comment) rather than hand-rolling. Add a
   `[[allow]]`-style unit test only if an offscreen context is reachable;
   otherwise assert the flip/RGBA layout on a hand-built pixel buffer and
   mark the GL read as skip-with-reason (D-011 precedent) + a MANUAL verify
   note. Keep the signature/semantics the C++ has (path in; the C++ ignores
   the result — keep that and mirror `mustTakeScreenshot` consumption which
   already works via `draw_frame`).
2. **SIGINT**: the atomic-flag stop the loop already honours (check how
   `WindowedClient::run` exits — extend it), logging the same four C++ lines
   at the same points. Test: set the flag from a closure, assert the loop
   exits and the logs appear (a headless-testable seam — e.g. inject the
   signal closure).
3. **Icon**: load `_RELEASE/Assets/icon.png` (the PNG decode from (1)'s
   dependency does both), set it on the window (`winit`
   `set_window_icon` — check `Window`'s surface; extend `oh-render` minimally
   if the setter is missing), and document the recreation-hook equivalent
   (winit 0.30 recreations — does the port's `Window` recreate? If not
   applicable, a comment saying so).
4. Tests + gate: whatever is headless-testable per above; full
   `bash tools/gate.sh` (4 stages, print `GATE_EXIT=<rc>`, never pipe to
   tail), `cargo test --workspace`,
   `cargo test --workspace --features oh-game/produce-lua-metadata`;
   `.agents/plans/STATUS.md` row 15 note (tail closed) in your final commit.

## Process rules

- Budget: 90 minutes (expect ~30). Commit after every green deliverable:
  (a) PNG/screenshot; (b) SIGINT; (c) icon; (d) gate + STATUS. clippy
  (`-D warnings`) fixed immediately per commit; `cargo test -p <crate>`
  after each file; verify every citation (D-014).
- HARD EXCLUSIONS: do NOT touch `crates/oh-net/**` (parallel 018 lane),
  `crates/oh-menu/**`, `crates/oh-game/src/hexagon_game.rs`, the simulation
  order, the D-019 fix, or the determinism harness.
