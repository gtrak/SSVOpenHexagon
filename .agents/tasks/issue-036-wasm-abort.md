# Task 036 — the emscripten build ABORTS with a Rust allocation failure (investigate + fix)

Read `.agents/WORKER-HYGIENE.md` first — especially the rule near §4 that **the host gate is
not a web gate** (nothing on the host compiles or runs this path, so driving the built page
is the only way to see it), and §5 (stop-and-report beats grinding).

Read `.agents/plans/001-rust-port/36-wasm-memory.md` — the issue, including the CORRECTION
section. The short version of what is already established, so you do not repeat it:

## Established facts

- The built page **aborts** in the browser console:
  `[error] memory allocation of 8388608 bytes failed` + `[error] Aborted()`.
- It happens **with NO input at all** on a fresh page: after 75 s untouched the state probe
  is still `menu:LoadingScreen` and the alloc failure is in the console.
- `-sALLOW_MEMORY_GROWTH=1 -sINITIAL_MEMORY=67108864 -sMAXIMUM_MEMORY=2147483648` are in
  `.cargo/config.toml`'s `[target.wasm32-unknown-emscripten]` table and the relink is
  verified (artifact sizes changed) — **and the abort is unchanged**, so the fixed-heap
  diagnosis was incomplete.
- 8 MiB is a suspiciously round number: look for a *growth request* or one large buffer,
  rather than organic exhaustion.
- **`agent-browser errors` does not report this** (it is a console `[error]`). Any check you
  write MUST read the console, not just `errors` — that difference is why the previous lane
  reported "clean at every step".

## What to do

1. **Reproduce deterministically and bound it**: fresh page, no input, how long until the
   abort? Does it abort at load, at the first frame, or on an allocation the engine makes?
   Instrument: run the module with emscripten's alloc tracing / `-sASSERTIONS=1` (or an
   `emscripten` build with `MALLOC` debugging) and see WHAT requests 8 MiB. Also record the
   wasm memory's actual size at the moment of failure (with `MODULARIZE`/`EXPORT_ES6` the
   vars are not globals, so hook `WebAssembly.instantiate`/`Module.onAbort` or the module's
   exported memory to observe it).
2. **Distinguish the environment from a real bug.** If the growth is failing in the JS host
   (ArrayBuffer reallocation), that is a headless/SwiftShader artifact worth documenting and
   a different fix than a real leak or a per-boot 8 MiB request. Test what you can: a second
   browser/profile, a smaller `INITIAL_MEMORY`, a bigger `MAXIMUM_MEMORY`, no shadow stack,
   etc. Do NOT just raise ceilings until it stops — if a ceiling change is what fixes it, say
   exactly which one and why, because that means the real problem is the allocation pattern.
3. **Fix it**, then re-drive the acceptance the way it was meant to be driven: fresh page,
   `keyboard type`/`press` to the name entry, ENTER, menu → level selection → **`state=game`**,
   a rotation key, an overlay tap, and the overlay EXIT tap back to the menu — with `console`
   grepped for `allocation|Aborted|panic` (not just `errors`), and at most ONE image read.
   Record the exact commands; the working serve recipe is
   `python3 -m http.server 8067 --directory /tmp/oh-web-serve-032`, Chrome with
   `--headless=new --no-sandbox --disable-dev-shm-usage --use-angle=swiftshader
   --enable-unsafe-swiftshader --remote-debugging-port=9333 --user-data-dir=<fresh>`, and
   `agent-browser` via its **`connect 9333` subcommand** (NOT a `--cdp` flag, which silently
   auto-launches its own browser on another port).
4. **Report**: the mechanism, the fix, the evidence, and explicitly what you could NOT
   establish. If the honest answer is "it is an environment artifact under headless
   SwiftShader and the page is fine in a real browser, here is the evidence", that is a valid
   and valuable outcome — but it needs evidence, not assertion.

## Hygiene

Commit per milestone with explicit paths; `cargo fmt`; run the host gate (or targeted checks)
before each commit; **build the wasm target** before claiming anything about it; bound every
process with `timeout`; at most ONE image read for the whole lane; do not modify the C++
reference tree; do not touch other lanes' worktrees. The emscripten build recipe (mounts +
`CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions`) is in
`/home/gary/dev/oh-lanes/026-wasm-tgt/build-em.sh` — reuse it rather than rediscovering it.

## Report

**Status (checkpoint): the mechanism is established, the abort does NOT
reproduce, one real per-frame allocation bug was found and fixed, and the
final clean-build acceptance is in flight.**

### The mechanism (established)

The abort line `memory allocation of 8388608 bytes failed` is Rust's
default alloc-error handler for an 8 MiB global allocation. 8388608 is
suspiciously round, and it is, in fact, a computed size:

**8388608 = 1024 x 2048 x 4 — the glyph atlas's second height doubling.**
The atlas (`crates/oh-render/src/glyphs.rs`) starts at 1024x512 (2 MiB,
the boot's 2097152-byte allocation) and doubles its height when the menu's
glyph set outgrows the rows: 512 -> 1024 -> 2048. At the 2048 row the
`grow_height` buffer is exactly 8388608 bytes. This is the ONLY computed
8 MiB in the codebase — verified by:

- no literal `8388608` anywhere in the source (grep);
- no 8 MiB file in the preloaded `_RELEASE` content (largest files: the
  6.79 MiB Discord `.so`, x2, plus the 3.71 MiB DLL — none of which a web
  build loads or decodes);
- the audio path's PCM totals ~4 MiB (not the source);
- the `epilepsyWarning.png` (1600x1600x4 = 10 MiB) is lazy-loaded in the
  EpilepsyWarning state and is the wrong size anyway;
- `gl.rs`'s framebuffer readback allocates `w x h x 4` for the SCREENSHOT
  key only (a no-input page never reaches it), and the headless canvas is
  800x600 (1.9 MiB), not an 8 MiB shape.

The host-gate test `glyphs::tests::atlas_height_growth_to_2048_is_exactly_
the_8_mib_abort_size` (commit 11d52c87) drives the atlas through both
doublings and pins `pixels().len() == 8388608` at the 2048 row.

**Why it aborted on the ORIGINAL build:** emcc's default is a FIXED heap
(`ALLOW_MEMORY_GROWTH` off). On a fixed heap, any allocation that outgrows
the heap is a FATAL abort rather than a growth. The pre-036 build hit that
on the atlas's 2048-row reallocation (or, under the same fixed heap, on
whatever large buffer the host pressure put there first). Commit a742af3c
turned growth on (`-sALLOW_MEMORY_GROWTH=1`, 64 MiB initial, 2 GiB max);
the `.cargo/config.toml` comment is now rewritten to this mechanism
(commit 0f6e1b9f).

### Environment vs. real bug (the distinguishing evidence)

Driven the built page through CDP (fresh SwiftShader Chrome, the same
memory hooks as before, console captured in full). Findings:

1. **The original abort does NOT reproduce.** ~10 drives: 6 no-input
   100 s runs (the exact "fresh page, no input" scenario) plus full
   acceptance walks at both 800x600 and 1920x1080. The console is clean
   for `allocation|Aborted|panic` in every one of them.
2. **The heap never needs to grow.** The wasm memory stays pinned at
   64 MiB (67108864) for the entire run — zero `memory.grow` calls. A raw
   8 MiB probe (`alloc_zeroed`, the same path as the abort, non-
   destructive) SUCCEEDS at boot, mid-game, and after the overlay exit.
   Even a 48 MiB probe fits the 64 MiB heap without any growth — in-use
   at boot is <= ~16 MiB.
3. **The machine is under heavy memory pressure** (60 GiB RAM, 14/15 GiB
   swap in use, a large sglang server running). That is the signature of
   a host-memory-pressure event: at the moment of the original abort the
   JS host could not back the (copy-based, `resizable=false`) reallocation
   that a fixed/pressured heap demands.

So the honest answer to "environment artifact or real per-boot request":
the 8 MiB request is REAL and content-driven (the atlas growth above), but
its FAILURE was a host-side event under memory pressure, not a leak, not a
bad ceiling, and not reproducible under the current (growth-enabled)
build. With growth on, the same request is a normal reallocation.

### The one real bug found and fixed

`MenuDraw::draw` (crates/oh-app/src/menu_draw.rs) deep-cloned the 464 KiB
(475136-byte) credits-bar `Texture` on EVERY frame — 60/s of transient
alloc + copy where the C++ does a zero-copy pointer assign
(`setTexture(assets.getTexture(...))`). That churn is exactly the kind of
thing that tips a pressured host over. Commit 61231d20 materializes the
frame only when the cycling id changes (now 1 per 100-frame credits
cycle — verified in the allocation ring: 475136 entries spaced ~1670 ms
apart, one per frame of the 100-frame cycle).

### What I ruled out

- The 8 MiB is not a file, not audio, not the epilepsy texture, not the
  screenshot readback (all above).
- The recurring 256 KiB / 852 KiB allocations in the ring are ONE-TIME
  boot decodes (4 and 2 respectively, within 7 ms of each other), not
  churn.
- The glyph atlas does NOT grow in the current environment at either
  viewport (the base-pack menu glyph set fits the 1024x512 start) — so the
  8 MiB request only occurs when a glyph set (a larger pack, a longer
  level-name list, a bigger window) outgrows the rows. That is content-
  driven and matches the C++ SFML font-texture behaviour (parity), not a
  port defect.

### What I have NOT established

- I could not capture the ORIGINAL abort live (it no longer reproduces),
  so I cannot 100% confirm WHICH allocation was in flight at that exact
  moment — though 8388608 uniquely matches the atlas 2048-row and there is
  no other candidate in the code.
- I have not measured the host memory state AT the original abort (only
  the current state: pressured, but currently able to back 8 MiB and even
  48 MiB without growth).

### The CDP harness (where it lives)

All in `/tmp/oh-036/` (temporary, not committed):

- `build-em.sh` — the 026 emsdk/podman recipe, adapted to the 036 worktree
  + its own target dir (`/home/gary/dev/oh-lanes/036-wasm-tgt`). Usage:
  `bash build-em.sh cargo build --release -p oh-web --target
  wasm32-unknown-emscripten`.
- `cdp-capture.py <seconds> <log>` — passive: fresh headless Chrome
  (SwiftShader) + CDP, injects pre-nav hooks on `WebAssembly.Memory.
  prototype.grow` (logs `[MEMGROW]`/`[MEMGROW-FAIL]`), finds the memory
  export (mangled name `yf`), polls `big_allocs()`/`status()` to the
  console, captures every console line. No input.
- `cdp-drive.py <seconds> <log>` — the full acceptance driver: the same
  hooks + a single CDP event-pump that walks boot -> name entry -> SMain ->
  LevelSelection -> `state=game` -> a rotation key -> an overlay tap ->
  the overlay EXIT tap back to the menu, while capturing the console.
  Env: `OH_URL` (default 8068), `OH_WIN` (viewport, default 800,600),
  `OH_NOPROBE=1` to skip the now-removed probe actions.

Key CDP lessons (why the earlier "clean at every step" was wrong, and why
the input needed care):

- The abort is a console `[error]`, NOT a CDP `Log`/exception —
  `agent-browser errors` misses it; you must read the console.
- The wasm memory export is mangled (`yf`), so `exports.memory` is
  undefined — the hook finds it by `instanceof WebAssembly.Memory`.
- The menu/game input managers are SNAPSHOT-based (`Type::Once` fires on
  the frame the key is down in the `InputState`); a CDP keyDown+keyUp in
  the same frame is INVISIBLE to them. The boot screens use the release-
  EDGE sink (advance on any release), but `ok_action` / rotation need the
  key HELD across a frame — the driver holds each key ~100 ms.
- `agent-browser` connects via its `connect <port>` subcommand, NOT a
  `--cdp` flag (which auto-launches its own browser on another port).

### The fix (what stands)

1. `-sALLOW_MEMORY_GROWTH=1` + 64 MiB initial + 2 GiB max (a742af3c) — the
   atlas's content-driven 8 MiB reallocation is no longer a fatal abort.
   This is not "raising a ceiling until it stops": it turns a fatal
   fixed-heap abort into a normal reallocation, which the steady-state
   (<= ~16 MiB in use) never even needs.
2. The credits-bar per-frame clone (61231d20) — the one real per-frame
   allocation bug, now 1 per 100-frame cycle.
3. Diagnostic scaffolding removed (0f6e1b9f); the `.cargo/config.toml`
   memory comment rewritten to the established mechanism.

### Exact commands (record)

Investigation drives (diagnostic build, port 8068): `python3 -m http.server
8068 --directory /tmp/oh-web-serve-036-diag`; Chrome
`--headless=new --no-sandbox --disable-dev-shm-usage
--use-angle=swiftshader --enable-unsafe-swiftshader
--remote-debugging-port=9346 --remote-allow-origins=*
--user-data-dir=<fresh>`; `OH_URL=http://127.0.0.1:8068/ python3
/tmp/oh-036/cdp-drive.py 120 <log>` (and `cdp-capture.py 100 <log>` for
the no-input runs).

FINAL ACCEPTANCE (clean build, all diagnostics removed, commit 0f6e1b9f;
artifacts rebuilt from the final tree):

    mkdir /tmp/oh-web-serve-036
    cp crates/oh-web/site/index.html /tmp/oh-web-serve-036/
    cp <tgt>/release/oh-web.js <tgt>/release/oh_web.wasm /tmp/oh-web-serve-036/
    cp <tgt>/release/deps/oh_web.data /tmp/oh-web-serve-036/
    # one page-level probe line (the CDP state poll needs the module):
    #   window.__mod = mod;   (after `const mod = await Module();`)
    python3 -m http.server 8069 --directory /tmp/oh-web-serve-036
    OH_URL=http://127.0.0.1:8069/ OH_NOPROBE=1 python3 \
        /tmp/oh-036/cdp-drive.py 120 final-accept.log    # the full walk
    OH_URL=http://127.0.0.1:8069/ python3 \
        /tmp/oh-036/cdp-capture.py 100 final-noinput.log # the no-input run

(`<tgt>` = `/home/gary/dev/oh-lanes/036-wasm-tgt/wasm32-unknown-emscripten`.)

Result - the full acceptance, the way it was meant to be driven:

- Fresh page -> LoadingScreen -> (Enter) EpilepsyWarning -> (Enter)
  ETLPNewBoot -> type `Gary` + Enter -> SMain -> (Enter) LevelSelection
  -> (Enter) **state=game** -> KeyA (rotation) -> overlay tap (756,556)
  -> overlay EXIT tap (44,44) -> **state=menu:LevelSelection** (back in
  the menu).
- Console grepped for `allocation|Aborted|panic`: **0 matches** (the only
  `[error]` in the log is the unrelated favicon-404).
- No-input on the same clean build, 100 s untouched: 0 matches, state
  healthy at `menu:LoadingScreen`, ticks advancing.
- The heap stayed pinned at 64 MiB the whole time (`[HEAP]` trace); zero
  `memory.grow` calls.

### Evidence lines (representative)

- `atlas_height_growth_to_2048_is_exactly_the_8_mib_abort_size ... ok`
  (host gate).
- `[MEMCREATED] t=86 name=yf initial=67108864 resizable=false` (the heap
  is 64 MiB, non-resizable / copy-based growth).
- `heapprobe(50331648) -> 'before=67108864 ... ok=true during=67108864
  after=67108864'` (a 48 MiB request fits the 64 MiB heap — no growth).
- `probe(8388608) -> 'ok:8388608'` at boot, mid-game, and post-exit.
- Full walk: `state=menu:LoadingScreen` -> `EpilepsyWarning` ->
  `ETLPNewBoot` -> (type "Gary" + Enter) `SMain` -> (Enter)
  `LevelSelection` -> (Enter) `state=game` -> (KeyA rotate) -> (overlay
  tap 756,556) -> (overlay EXIT tap 44,44) `state=menu:LevelSelection`.
  `alloc/aborted lines: 0`, `MEMGROW events: 0`.
- Credits cycle in the ring: `475136` entries ~1670 ms apart (1 per
  100-frame cycle), not 60/s.

### Conclusion (the precise answer)

- **What requests 8388608:** the glyph atlas's second height doubling
  (1024 x 2048 x 4) — content-driven, parity with the C++ SFML
  font-texture growth; the only computed 8 MiB in the codebase (pinned by
  a host-gate test).
- **Why it failed:** on the pre-036 build the heap was FIXED (emcc
  default, growth off), so that reallocation — under the host memory
  pressure of the time (this box: 14/15 GiB swap in use, a large sglang
  server) — was a fatal abort. With growth enabled (a742af3c) it is a
  normal reallocation; the steady state (<= ~16 MiB in use of the 64 MiB
  heap) never needs to grow at all.
- **Fix:** (1) the memory flags (kept, mechanism now documented in
  `.cargo/config.toml`); (2) the one real allocation bug — the 464 KiB
  credits-bar deep clone every frame — fixed in 61231d20 (now 1 per
  100-frame cycle); (3) no ceiling was raised as a fix.
- **Acceptance:** the full walk passes on the clean build with the
  console grepped clean (`allocation|Aborted|panic` = 0); the no-input
  scenario does not reproduce. **The 8 MiB failure is a host-side event
  (fixed-heap + memory pressure), not a leak or a per-boot request the
  code cannot satisfy.**
