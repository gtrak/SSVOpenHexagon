# Issue 36 — the web build's wasm memory is FIXED, so an allocation spike ABORTS the page

**Found by the orchestrator re-verifying lane 032's browser acceptance** (the lane's own
run passed; mine aborted, which is the point: it is intermittent, not a fluke of one
machine state).

## The finding

Re-driving the landed web build in headless Chrome 148 + SwiftShader produced, in the
browser console:

```
[error] memory allocation of 8388608 bytes failed
[error] note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
[error] Aborted()
```

— a Rust abort, mid-drive, with the state probe then frozen (`state=menu:ETLPNewBoot`).
That is a CRASH, not a graceful degradation.

## Root cause

`.cargo/config.toml`'s `[target.wasm32-unknown-emscripten]` rustflags carried **no memory
settings at all**, so the link inherited emcc's defaults — where **`ALLOW_MEMORY_GROWTH` is
OFF** and the heap is fixed. The engine plus Luau *fit*, but only barely, so an allocation
spike that exceeds the remaining fixed heap is fatal. The spike here was reachable through
ordinary use (a fresh page — no prior warm state — plus a name-entry/input sequence).

This is the same class as the "the host gate is not a web gate" lesson: nothing on the host
compiles or runs this, so the only way to see it is to drive the built page.

## The fix (applied here)

Three link flags in the same table:

```
-Clink-arg=-sALLOW_MEMORY_GROWTH=1
-Clink-arg=-sINITIAL_MEMORY=67108864      # 64 MiB
-Clink-arg=-sMAXIMUM_MEMORY=2147483648    # 2 GiB
```

Growth turns a fatal abort into a grow-and-continue; the explicit initial size keeps the
common case from having to grow at all, and the ceiling keeps a runaway from taking the tab
down silently.

## Verification required

1. Rebuild `-p oh-web --target wasm32-unknown-emscripten` (the emsdk container: the recipe
   in the 026 notes / `build-em.sh`).
2. Re-drive the same sequence that aborted: `errors`/`console` must show NO alloc failure,
   and the state probe must keep advancing (`menu:LoadingScreen` → `EpilepsyWarning` →
   `ETLPNewBoot` → … → `game`).
3. Keep the negative honest: if the abort recurs at a higher ceiling, the real problem is a
   leak or a pathological allocation pattern, not the budget — report it rather than raising
   the ceiling again.
