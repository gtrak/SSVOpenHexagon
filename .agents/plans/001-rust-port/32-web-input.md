# Issue 32 — the wasm build has no input of any kind (the browser build cannot be driven)

Read `.agents/WORKER-HYGIENE.md` first. Context: the wasm branch (D-036, `origin/wasm`)
replaced winit on web with a canvas + `emscripten_webgl_*` + a `request_animation_frame`
loop, and **issue 026** covers integrating it and proving it *boots in a browser*. This
issue covers the gap that integration leaves: **no pointer, key, touch, wheel or resize
event ever reaches the engine.**

Found while scoping issue 31 (the operator wants touch controls on wasm too): the web
target cannot be driven at all, so there is nothing for an overlay to feed yet.

## 1. The evidence (verified at spec time)

- `crates/oh-web/src/web.rs` on `origin/wasm` is **127 lines** and contains only
  `status()`, `log()`, `seed_default_profile()`, `boot()` (plus module docs describing
  the boot sequence: `_initialize`, canvas + `emscripten_webgl_*`, the rAF loop).
- A branch-wide search over `crates/**` on `origin/wasm` for
  `emscripten_set_keyboard|touch|mouse|click`, `EventListener`, `addEventListener`,
  `onpointer`, `ontouch` finds **nothing**.

So the build boots, seeds a default profile and runs a frame loop, and renders a menu
that cannot be navigated. **Issue 26's acceptance (the port's own `[channel]` console
lines proving the engine ran) is necessary but cannot prove interactivity** — do not
treat 26 as covering this, and make sure 26's report does not imply it did.

## 2. Requirements

Feed **the same input sink/state the windowed path feeds** — the shared touch sink
(`crates/oh-app/src/windowed.rs:203`, `:249-286`) and the input state
(`crates/oh-render/src/input.rs:78`) — from DOM events on the canvas:

1. **Pointer**: `pointerdown` / `pointermove` / `pointerup` / `pointercancel`, plus
   `wheel` (the 29b2 wheel sink). Track by `pointerId` and use `setPointerCapture` so a
   drag off the element still delivers its release (a stuck-down input is the classic
   browser input bug).
2. **Keyboard**: `keydown` / `keyup` mapped onto the port's existing key codes (see
   `oh-render/src/input.rs` and the SFML key enum pinned in
   `.agents/plans/001-rust-port/mapping/oracle-vectors.md`) — the browser must produce
   the **same codes** the desktop path produces, or replays and menu binds diverge.
   `preventDefault` the keys the game consumes (so the page does not also scroll/zoom),
   without swallowing browser-level shortcuts.
3. **Resize / DPR**: the canvas' CSS size and `devicePixelRatio` → the *real* window
   size. This is the 025g lesson again (config size ≠ real size; the menu layout must
   follow the real size), and a wrong DPR mapping silently breaks every hit test.
4. **Focus / blur**: releasing all held inputs on `blur`/visibility change (otherwise a
   key held when the tab loses focus stays stuck), and no accumulated `dt` after a long
   frame gap (the 025i pacing lesson).
5. **The pointer-position path**: the engine rotates the player from the pointer
   position, so browser pointer coordinates must go through the **same view/camera
   transform** the windowed path uses. A mismatch here does not crash — it rotates the
   player wrongly, which is worse.

## 3. Verification

- **In-process (primary)**: the event → sink mapping (a synthetic DOM-shaped event
  yields the right sink entry / key code), and the release-on-blur behavior.
- **Browser (the real gate)**: with the `emscripten/emsdk` container + `agent-browser`,
  drive the build as a user would: boot → name entry → menu → start a level, using both
  keyboard and pointer. Evidence = `console` (the port's own `[channel]` lines),
  `errors` clean, and **at most one screenshot** (see §4 hygiene). If a step cannot be
  driven, report precisely what blocked it instead of inferring success.
- **Meet issue 31 here**: once 31's platform-neutral pointer seam exists, wire the web
  feed to it and show the on-screen controls working in a browser. 31 and 32 converge;
  neither is complete on the web side without the other.

## 4. Hygiene

`tools/gate.sh` 4/4 before each commit; `cargo fmt --all`; bounded processes (`timeout`);
commit per milestone with **explicit paths**; **at most one image read**; stop-and-report
on any wall.

## 5. Scope guard

Parity with the desktop input model — **no new input semantics**. No rebinding UI, no
gamepad-on-web, no gesture vocabulary beyond what the C++'s own touch path
(`HGUpdate.cpp:589`) and issue 31's overlay define. If wiring an event type turns out to
need an engine-side change (e.g. the sink is not platform-neutral in practice), stop and
report that as the finding rather than widening the engine's input model here.
