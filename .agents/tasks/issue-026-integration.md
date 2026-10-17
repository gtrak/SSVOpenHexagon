# Task 026-integration — bring the `wasm` lane onto master, verified

Read `.agents/WORKER-HYGIENE.md` first — it applies to this lane.

## Where things stand

`origin/wasm` is **10 commits on top of master `3610f3fb`**; master is now **11
commits ahead** of that base (the audit-fix batches: 29b1–29i1, the `029misc`
items, the hygiene doc). So this is a merge job with known collision points, not
a fast-forward.

The branch's own record is **D-036** (`DECISIONS.md`) and
`.agents/tasks/issue-026-web.md`. What it landed (W0–W6):

- **Target = `wasm32-unknown-emscripten` for the whole app; `winit` dropped on
  web** (`winit` 0.30's web backend is `target_os = "unknown"`-only and cannot
  target emscripten). The web build uses a canvas + `emscripten_webgl_*` context
  + `glow::Context::from_loader_function_cstr`, isolated in `oh-render`; and
  `winit`/`egui`/`egui-winit`/`egui_glow`/`glow 0.17`/`signal-hook` are gated to
  `not(emscripten)`.
- **Content via MEMFS preload, not a VFS refactor**:
  `--preload-file=_RELEASE@/RELEASE` + `-sFORCE_FILESYSTEM=1`, so master's
  existing `std::fs` content path works on web unchanged (the 26a `oh-vfs` trait
  is dropped).
- **Lua backend selectable per target**: `oh-lua` uses mlua `luajit` on desktop
  and `luau` on emscripten (Luau/C++ is the only Lua that builds there);
  `mlua::Integer` width fixed for the Luau build; `oh-lua/src/convert.rs` carries
  the per-target conversion differences. **Global-Luau is explicitly a follow-up
  experiment, not this lane.** Do not attempt it here.
- **`crates/oh-web`** (new entry crate) + `.cargo/config.toml` target recipe +
  `wasm-bindgen`-as-a-bin (`-Clink-arg=-sWASM_BINDGEN`).

## What to do

### 1. Bring the branch onto current master
Use rebase or cherry-pick of the 10-commit range so history stays linear (this
repo's landings are linear and per-milestone; a merge commit is a last resort —
if you take one, say why). **Preserve the branch's commit structure and its
docs** (`DECISIONS.md` D-036, `issue-026-web.md`, the `oh-web` crate, the site
assets) — they are the lane's record.

**Named collisions — resolve each deliberately, and say what you did:**

- `crates/oh-render/src/window.rs` (+516 in the branch) — **master changed this
  file too** (29b2's mouse-wheel sink). The wheel sink must survive.
- `crates/oh-app/src/windowed.rs` (+196) — master changed it for 29b9 (F3/F4),
  29b4 (`-p/-l` boot), 29u1 (cursor), 29r1 (desktop mode + config logging).
  Master's newer behaviour wins; the branch's platform gating is layered on top.
- `crates/oh-lua/Cargo.toml` — master has **029k1's** vendored LuaJIT
  (`extlibs/luajit-src` + `[patch.crates-io]`, `LUAJIT_DISABLE_FFI`) and the
  corrected comment; the branch adds the per-target backend table. Both must
  survive: desktop keeps the vendored FFI-off LuaJIT, emscripten gets Luau.
- `.cargo/config.toml` — master has the Android linkers; the branch adds the
  emscripten target recipe. Keep both.
- `crates/oh-render/src/{types.rs,gl.rs}`, `crates/oh-app/{Cargo.toml,src/client.rs,src/lib.rs,src/main.rs}`,
  the root `Cargo.toml`/`Cargo.lock` — check each against master's newer state.

### 2. Re-evaluate the profile-seed workaround (it predates a fix)
D-036 records an `oh-web` prototype workaround that **seeds a default profile
into MEMFS**, justified by "master's profile/save system is a known no-op
(tracker 29b6/29c1)". **29c1 has since landed**: profiles, scores, favorites and
the exit save are real now, and 29b6 (the created-profile menu item) is in too.
Re-check whether the seed is still needed (a genuinely fresh web user may have no
profile, and the C++'s boot flow has a first-run path — find it and prefer it over
a seed). Either keep it as a documented deviation or remove it; do not leave it
unexamined.

### 3. Fix the decision-number collision
The branch's **D-036** (web target) is the first D-036, so the Lua-runtime
decision becomes **D-037**. Correct `.agents/plans/001-rust-port/30-lua-runtime.md`,
which says "next free is D-036".

### 4. Verify — the browser boot is REQUIRED, not optional

**The wasm build was never actually run in a browser.** The branch claims
boot-to-menu (its W5/W6), but that claim is unverified upstream — so verifying it
here is part of the integration, not a nice-to-have.

**Host gate must stay green**: `bash tools/gate.sh` — 4/4, master's 908 tests
plus whatever the branch adds. Non-negotiable.

**The web target** — no emscripten toolchain is installed on this machine (no
`emsdk`, no `emcc`), but **docker works** (verified; no sudo needed) and the
`emscripten/emsdk` image is **not yet pulled** (expect one ~1 GB pull, bounded).
Mount the checkout and run the branch's documented recipe **once**; if the pull or
the recipe fails, **stop and report the exact command + error** rather than
burning the lane on toolchain archaeology.

**Then test it in a real browser — this is the milestone.** The tooling is here
(`agent-browser` 0.27.0, installed globally):

```
AB="node $(npm root -g)/agent-browser/bin/agent-browser.js"   # no PATH shim
python3 -m http.server <port> --directory <build-dir> &        # serve (bounded)
$AB open http://localhost:<port>/ ; $AB wait <ms>
$AB console        # console logs — the wasm/WebGL boot trace
$AB errors         # page errors — the decisive failure signal
$AB eval '...'     # canvas size, WebGL context, module/state probes
$AB screenshot /tmp/web-boot.png
$AB close --all
```

Evidence to report, in this order: `errors` (must be empty of fatal ones),
`console` (the port's own `[channel]` log lines — the engine's boot trace should
appear, which proves the *engine* ran and not just JS), the `eval` probes, and a
screenshot analysed with **PIL** (dimensions + non-black pixel count) — the
hygiene rule applies: **at most ONE image read**, bounded commands, and if the
page white-screens, read `console`/`errors` before spending an image.

WebGL in headless Chromium may need software GL; if the context fails, say so and
report whether it is a toolchain limitation or a port bug — do not report "works"
without the console/errors evidence shown above.

### 5. Report
What you integrated, the per-file conflict resolutions, the profile-seed decision,
the D-036/D-037 fix, the host gate result, and **exactly how far the web
verification got** (with the commands and their output). Be explicit about what
is unverified.

## Non-goals
- No global-Luau migration (issue 30's remaining half).
- No VFS refactor (dropped by D-036).
- No new web features beyond what the branch already contains.

## Hygiene
- **No worktree** (D-035). Work in the main checkout, but note: this lane rewrites
  history for a moment — before you start, confirm the tree is clean and record
  `git rev-parse HEAD`, so a botched rebase can be undone (`git reset --hard
  <sha>` / the reflog) without loss.
- One commit per logical step; **stage explicit paths, never `git add -A`**.
- `cargo fmt --all` before committing; gate before each commit.
- If the rebase gets messy and you are not converging, **stop, restore the
  recorded HEAD, and report** — a clean "not integrated, here is the blocker" beats
  a half-merged tree.
