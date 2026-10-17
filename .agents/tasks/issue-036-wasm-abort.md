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
