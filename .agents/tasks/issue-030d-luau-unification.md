# Task 030d — unify on Luau: delete the LuaJIT machinery (D-037)

Read `.agents/WORKER-HYGIENE.md` first. Authority: **D-037** in
`.agents/plans/001-rust-port/DECISIONS.md` (the operator authorized this after asking
"will we still keep parity with c++ without rust luajit built in? if so, go ahead"), plus
`.agents/plans/001-rust-port/30-lua-runtime.md` (§12) and the three landed evidence lanes:
030a (the A/B harness), 030b (the C++ oracle), 030c (the three-way proof + the dialect
census + the 5.1 prelude).

**The goal:** the port ships ONE Lua VM — Luau — and the LuaJIT machinery is gone.

**What you must NOT lose:** the C++ parity guarantee and the ability to keep proving it.
The oracle is VM-agnostic (it fingerprints game STATE, not Lua internals), so it survives
untouched — and after this change the *shipping* configuration is the one compared against
the C++ (030c already proved 3627/3627 frames under Luau). If any step would weaken that,
**stop and report** rather than trading it away.

---

## 1. The flip and the one capability that must be preserved

- `crates/oh-lua/Cargo.toml`: `default = ["backend-luau"]`; keep the feature names only if
  they still buy something — otherwise collapse to mlua's `luau` feature and drop the
  `backend-*` indirection through `oh-app`/`oh-game`/`oh-parity`.
- **`loadstring` is the one 5.1-ism that is NOT Lua-emulatable** (Luau's base library has no
  chunk compiler at all — not even `load`). The C++ (LuaJIT) HAS it, so dropping it is a
  parity regression for third-party content. Add the **one-line Rust binding** 030c
  demonstrated (`mlua`'s `Lua::load` exposed as `load`), then let the landed prelude's
  `loadstring = load` line activate it, and pin it with a test
  (`loadstring('return 6*7')() == 42`). Ship the prelude's seven-ism compat set (030c's
  `lua51_prelude.lua`) — it is already written and tested.

## 2. Delete the machinery (this is the point of the change)

- `extlibs/luajit-src` (the vendored tree) and the root `Cargo.toml`'s `[patch.crates-io]`
  entry for it.
- `crates/oh-android/cshim.c`'s `__clear_cache` shim and its `build.rs` wiring (it exists
  only for LuaJIT's mcode); the Android Cargo/manifest references to it.
- `crates/oh-lua/build.rs`'s FFI/`ffi_stub.c` linkage (and the file itself).
- The `LUAJIT_DISABLE_FFI` references, the LuaJIT-specific docs/comments, and
  `.agents/skills/lib-luajit.md` if it is now purely historical — *point the skill at the
  Luau reality instead of deleting the knowledge*.
- **Deviation F29 and the K3 mitigation go with it**: remove `LuaContext::disable_jit` and
  the `cfg(all(target_os = "android", feature = "backend-luajit"))` call in
  `LuaContext::new` (there is no `jit` table to disable), then update `deviations.md` so
  **F29 is removed and K3/K4 are recorded as closed BY CONSTRUCTION** (cite D-037), with
  `.agents/plans/001-rust-port/33-k3-jit-off.md` and the K3 section of
  `issue-029k1-luajit-version-and-anr.md` pointing at D-037 rather than being silently
  rewritten.

## 3. The gate and the tests must follow (do not leave them red or vacuous)

- `tools/lua-parity.sh`: the LuaJIT arm disappears with the VM. **Stage 5 must remain
  meaningful**: the Luau column against the committed C++ vectors (`parity_cpp_fingerprints`,
  the 030c per-case table), the dialect census (which self-checks its discrimination), the
  VM probe pins, and the prelude tests. A stage that no longer compares anything must be
  DELETED, not left passing vacuously — a green gate that checks nothing is worse than no
  stage. Stage 7 (clippy under Luau) becomes redundant once the default IS Luau (gate stage
  3 covers it): remove it and say so, or keep it only if it still tests a non-default
  configuration.
- `crates/oh-parity/tests/lua_engine_parity.rs`: the A/B comparator loses its second
  backend. **Repurpose it as the regression pins** (the fingerprints it emits are still
  valuable as a fixed reference) or delete the arm — but do not leave a test that iterates
  one side and calls itself a comparison.
- `crates/oh-lua/tests/stdlib_surface.rs`: the per-backend arms collapse; the `jit` tests
  (`jit_library_is_reachable_and_toggleable`, `disable_jit_turns_the_engine_off`) are
  deleted with their subject, and the divergence notes they encode should move into the
  record (D-037's residual risks) rather than vanish.
- `tools/gate.sh` stage comments, `.github/workflows/*` (issue 22's CI), and any
  packaging/Android recipe that mentions `luajit`/`backend-luajit`.

## 4. Verification (all of it, on the final tree)

1. `tools/gate.sh` **green, 4-5 stages**, with the reworked parity stage actually comparing
   the Luau column against `cpp_fingerprints.json`.
2. **The C++ parity column still passes** — the headline evidence: the same 3627 frames,
   now from the shipping configuration.
3. **Android**: `cargo check -p oh-app --target aarch64-linux-android` (and, if cheap, an
   APK build) with **no C shim**; note that the shim's removal is exactly what K3's fix
   depends on being unnecessary.
4. **wasm/emscripten**: the target still builds (issue 26's `oh-web`). If the per-target
   selection is gone, the emscripten build must simply work with the single VM.
5. `cargo tree`/`Cargo.lock`: no `luajit-src`, no `lua-src`; `luau0-src` present.
6. Report the **deleted line/file counts** — the size of the win is part of the deliverable.

## 5. Sequencing and hygiene

- **Lane 026 is mid-flight** and is wiring the per-target Lua selection for emscripten.
  **This change supersedes that half of 026's work** (D-037), so do not coordinate
  file-by-file: the orchestrator lands 026 first and you rebase over it, and the resolution
  is to DELETE the split. If you must touch `crates/oh-app/src/windowed.rs` or
  `crates/oh-web/**`, keep your edits minimal and say so in the report.
- Commit per milestone with **explicit paths**; `cargo fmt --all`; run the gate (or targeted
  checks while other lanes run) before each commit; bound every process with `timeout`;
  **at most one image read** (probably none); **stop-and-report** on a wall.
- **Do not** re-litigate the decision, keep a "just in case" copy of the LuaJIT tree, or
  weaken the C++ parity check to make the tree green — if parity breaks, that is THE
  finding, and it stops the change.
