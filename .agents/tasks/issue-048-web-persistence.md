# Task 048 — the wasm build must PERSIST: today every profile, score and replay dies on reload

Read `.agents/WORKER-HYGIENE.md` first. Operator question, verbatim: *"is data persisted in localstorage
on the wasm build?"* — the honest answer, verified before writing this, is **no, nothing persists at
all**, and this task fixes it.

## The verified state (do not re-derive; confirm and go)

- The web's base dir is `const BASE_DIR: &str = "/RELEASE"` (`crates/oh-web/src/web.rs:44`) — the
  `_RELEASE` tree preloaded into emscripten **MEMFS** by the root `.cargo/config.toml`'s
  `--preload-file=_RELEASE@/RELEASE`.
- The port creates `Profiles/` and `Replays/` under that base dir and writes `config.json` there
  (`crates/oh-app/src/client.rs`), all through `std::fs` → MEMFS → **gone when the page reloads**.
- There is no `IDBFS`, `localStorage`, `indexedDB` or `syncfs` anywhere in `crates/` or `tools/`
  (grepped), and issue 026 recorded the limit: "Session-only persistence (MEMFS): profiles/config
  created in a session do not survive". Player-visible: the name they type at `ETLPNewBoot` is lost
  every reload, and so are scores, favourites and replays.

## localStorage is the WRONG tool — say why in the report and use IndexedDB

`localStorage` is ~5 MB, synchronous, and string-only; it is also not what anything here uses. The
right mechanism is emscripten's **IDBFS**, an IndexedDB-backed filesystem, which keeps the port's
existing `std::fs` code path intact (D-036's "MEMFS preload, NOT a VFS refactor" decision still holds).

## The design, and the one thing NOT to do

**Do NOT mount IDBFS over `/RELEASE` as a whole.** That would pull the 22 MB read-only content into
IndexedDB, and — worse — the persisted copy would then be *stale after the next deployment*, so a
returning player would run an old `_RELEASE` against a new `oh_web.wasm`. That is a real hazard, not a
theoretical one.

Instead: mount IDBFS at a small separate user-data directory, and make the WRITABLE paths land in it —
the clean way is a symlink per writable path (emscripten supports `FS.symlink`, for directories and
files both): `config.json`, `Profiles/`, `Replays/` → the IDBFS mount. The read-only content stays in
MEMFS exactly as today. Enumerate the writable paths from the code rather than assuming this list —
find EVERY write under the base dir (the favourites and any other save path included; the online
SQLite stack is desktop-only and not a concern, but confirm that rather than assuming it).

Three constraints that decide whether this works at all:

1. **Ordering at boot.** IndexedDB is asynchronous. The persisted files must be in place BEFORE the
   engine reads `config.json` / the profiles, so the web entry must await `FS.syncfs(true, cb)` (the
   load direction) before booting the composition — and the port's boot must not read those paths
   before that resolves.
2. **Sync after writes, and on the way out.** Persist after writes, and additionally on
   `visibilitychange`/`pagehide`: the operator tests on **Android Chrome**, which discards backgrounded
   tabs, so a "sync at the end" design would silently lose the data on a phone. This is the single most
   likely way to ship something that looks right in a desktop test and fails for the operator.
3. **The link flag.** `-lidbfs.js` has to reach the emscripten link (the root `.cargo/config.toml`'s
   wasm rustflags, or wherever this build sets its emcc flags) — verify the flag is actually applied by
   checking that `FS.syncfs` exists at runtime, not by assuming.

## Acceptance (this one IS verifiable here, unlike audibility or fullscreen)

The persistence claim can be proven end-to-end in a headless browser, because IndexedDB survives
navigation within a browser profile: create a profile (drive to `ETLPNewBoot`, commit a name with
CDP's `Input.insertText` — the IME path from 046 if it has landed, a keystroke path otherwise), then
**reload the page** with `tools/web-probe.mjs` (or your own CDP driver) and show that (a) the profile
is still there, (b) the boot no longer asks for a name, and (c) a changed setting (e.g. a volume or a
key bind) survives the same way. Quote the probe output for both the before and after reload.

Also: prove the NOT-mounting-`/RELEASE` property — state what the IndexedDB content actually holds
(e.g. from the CDP console: the object stores / the persisted size), so the 22 MB content staying out
is a fact and not a promise.

## Record and boundaries

- This is a platform-integration DECISION, not a transcription of the C++ (the C++'s emscripten build
  had no persistence either): add the register row (**F34**) and a short DECISIONS entry, stating what
  persists, where, and that the read-only content deliberately does not.
- Web only: `crates/oh-web/` + the emscripten flags + the symlink/sync glue. Do NOT change the desktop
  or Android data paths, do NOT change the on-disk FORMAT of any file (the `.ohr`/profile/config
  formats are pinned by parity — persistence must not touch their bytes), and do NOT edit
  `crates/oh-web/site/index.html` beyond what the mount genuinely needs if lane 046 is still editing it
  (it is adding an IME element and a fullscreen button there — coordinate by keeping your change
  additive and minimal; if you must touch the same lines, say so in the report).
- Gates: the host gate before each commit, the wasm build in the emsdk container (§4), and
  `cargo check --target aarch64-linux-android` if you touch shared code. Commit with explicit paths,
  bound everything with `timeout`, and append a `## Report`.

## Report (lane 048, 2026-10-18)

**Answer to the operator's question, now:** yes — and it is IndexedDB, not
`localStorage`. The wasm build persists its user data (profiles, config,
replays) in **IndexedDB** across reloads, through emscripten's **IDBFS**
(the IndexedDB-backed filesystem). `localStorage` is the wrong tool and was
not used: it is ~5 MB, synchronous, and string-only — and nothing in the
port uses it. IDBFS instead keeps the port's existing `std::fs` code path
intact (D-036's "MEMFS preload, NOT a VFS refactor" decision still holds):
the engine writes exactly where it always did (`/RELEASE/config.json`,
`/RELEASE/Profiles/`, `/RELEASE/Replays/`); the page re-points those three
paths at the IDBFS mount with symlinks, and emscripten's own IDBFS layer
persists them to IndexedDB.

**What persists — the writable-path census (from the code, not assumed):**
under the web base dir (`/RELEASE`) exactly THREE paths are ever written:
`config.json` (the `ConfigSaveGuard`, `crates/oh-app/src/client.rs:128` —
`save_config` is a direct `std::fs::write`, no tmp+rename), `Profiles/`
(`Assets::profile_create` / `profile_save_current` / `profile_save_all`,
`crates/oh-assets/src/assets.rs:790-1007` — direct `std::fs::write` through
the `root`-joined `Profiles/<name>.json`), and `Replays/`
(`RealReplayStore`). Confirmed desktop-only (absent on web):
`workshopCache.json` (oh-steam's `load_workshop_data` cache) and
`ohdb.sqlite` (oh-net's online stack). `scores.json` / `users.json` are C++
leftovers no Rust code writes. Each of the three is symlinked into the
`/userdata` IDBFS mount; first run seeds `/userdata/config.json` from the
shipped `_RELEASE` copy (the C++'s user-config-shadows-shipped shape).

**What deliberately does NOT persist: the 22 MB read-only `_RELEASE`
content.** IDBFS is never mounted over `/RELEASE` — a persisted copy would
be STALE after the next deployment (a returning player running an old
`_RELEASE` against a new `oh_web.wasm`). The probe's IDB inventory proves
this as a fact: after a full session the durable store holds EXACTLY 4
entries — `/userdata/config.json` (7952 B), `/userdata/Profiles/probe.json`
(123 B), and the `Profiles` + `Replays` directory entries. Nothing else.
(IndexedDB database `/userdata` — IDBFS names the DB after the mountpoint —
store `FILE_DATA`, version 21; emscripten 6.0.12 `src/lib/libidbfs.js`.)

**The design, per the three constraints:**
1. *Ordering at boot* — IndexedDB is asynchronous, so `main` no longer runs
   the composition synchronously. It calls the page's `ohWebPersistBoot`
   glue (`crates/oh-web/site/index.html`, defined in the page's module
   script before the factory is called), which mounts the IDBFS at
   `/userdata` (`autoPersist: true` — emscripten's own
   persist-after-each-write), runs the `FS.syncfs(true)` LOAD (the
   persisted user data in place BEFORE the composition reads
   `config.json` / `Profiles`), seeds the symlinks, and then invokes the
   callback. The callback is a `dyn Fn` `Closure` (wasm-bindgen has no
   `FnOnce` JS closures) with a `BOOT_GATE_FIRED` one-shot latch, its
   ownership transferred to the JS GC (`into_js_value`), so the composition
   boots exactly once, when the load settles. A load FAILURE (missing glue,
   no IndexedDB) degrades to the session-only MEMFS — the game still boots
   (the C++ has no durable storage either; this is a platform addition,
   not a transcription).
2. *Sync after writes + on the way out* — two layers: the mount's
   `autoPersist` (every write under `/userdata` persists to IndexedDB at
   the end of the frame — the profile reaches the durable store the moment
   it is created) AND the page's `pagehide` / `visibilitychange` glue: it
   calls the new `save_session` wasm export — the C++ `mainClient`
   exit-save (the `pSaveAll` + `saveConfig` pair, main.cpp:285-291 /
   :382-388) RE-FIRED in its LIFO guard order through
   `oh_app::windowed::web_session_save` (the stow at `build`'s composition
   point) — then flushes the mount with `FS.syncfs(false)`. The re-fire is
   the D-029 gap-closer: on web the rAF loop returns, so the C++'s exit
   guards drop at composition (with the boot-time values) and an in-session
   config change (the menu's options) would never have reached
   `config.json`. The operator tests on **Android Chrome, which discards
   backgrounded tabs** — `visibilitychange(hidden)` fires before the
   discard, so a "sync at the end of main" design would have silently lost
   the data on a phone.
3. *The link flag* — `-lidbfs.js` + `-sEXPORTED_RUNTIME_METHODS=FS,IDBFS`
   in the root `.cargo/config.toml`'s wasm rustflags. Verified applied:
   `Module["IDBFS"]` is present in the generated `oh-web.js` and
   `FS.syncfs` / `FS.mount` run at runtime (the probe's boot lines:
   `oh-web: user data restored from IndexedDB (IDBFS /userdata, autoPersist
   on)`).

**The acceptance — `tools/web-persist-probe.mjs`** (a headless-Chrome CDP
driver, the same recipe as `tools/web-probe.mjs`: SwiftShader GL, a fresh
`--user-data-dir` = a fresh user; exit 0 only when every checkpoint
passes). Probe output, verbatim (final run):

```
PHASE 1 (fresh IndexedDB):
  [PASS] boot to LoadingScreen (fresh profile) — 3s
  [PASS] advance to EpilepsyWarning
  [PASS] fresh user → profile creation (ETLPNewBoot)
  [PASS] profile "probe" created (→ SMain)
  [PASS] open MOpts (options category)
  [PASS] dispatch pagehide (the way-out glue) — synthetic pagehide dispatched
  [PASS] IndexedDB durable store reachable — 4 entries: /userdata/Profiles, /userdata/Profiles/probe.json, /userdata/Replays, /userdata/config.json
  [PASS] profile persisted to IndexedDB — key=/userdata/Profiles/probe.json, bytes=123
  [PASS] config persisted with the changed setting (auto_restart: true) — config.json 7952B in the durable store

PHASE 2 (Page.reload — MEMFS re-preloaded, only IndexedDB survives):
  [PASS] reboot: module up
  [PASS] reboot to LoadingScreen — 3s (the 22 MB _RELEASE re-preload + the FS.syncfs load)
  [PASS] boot SKIPS name entry (→ SLPSelectBoot, not ETLPNewBoot) — landed on menu:SLPSelectBoot — a persisted profile must put the boot on the profile list
  [PASS] changed setting survived the reload (auto_restart: true via the engine path) — config.json 7952B
  [PASS] profile still in IndexedDB after the reload — key=/userdata/Profiles/probe.json

persistence gate log : oh-web: user data restored from IndexedDB (IDBFS /userdata, autoPersist on) | oh-web: persisted user data ready (IDBFS /userdata) - booting the composition | …(×2, once per load)…
console errors        : none (4 known [oh-audio] volume diagnostics excluded)

web-persist-probe: 14/14 checkpoints passed
```

Phase 2's screenshot (`/tmp/persist-reload-selectboot.png`) shows the boot
on `SELECT LOCAL PROFILE / PRESS ESC TO CREATE A NEW PROFILE` with the
persisted `PROBE` profile listed — the boot skipped name entry. Phase 1's
(`/tmp/persist-mopts-gameplay.png`) shows `CURRENT PROFILE: probe` on the
MOpts GAMEPLAY page (captured before the `autorestart` toggle; the IDB
read proves the post-toggle `"auto_restart": true` reached IndexedDB). The
changed setting is read back through the ENGINE'S OWN PATH in Phase 2
(`ohWebModule.FS.readFile('/RELEASE/config.json')` — the module's FS, the
page's symlink into IDBFS — not a side channel).

**Changed files:** `crates/oh-web/src/web.rs` (the boot's persistence gate
+ the `save_session` export), `crates/oh-web/site/index.html` (the page
glue — ADDITIVE inside the existing module script: `ohWebPersistBoot`,
the `cfg`/`globalThis.ohWebModule` hook, the way-out listeners; also the
`Module()` call now passes `cfg` — that line is the factory-call 046's
work will touch too, noted below), `crates/oh-app/src/windowed.rs`
(emscripten-only `web_session_stow` / `web_session_save` + the stow call
in `build`), the root `.cargo/config.toml` (the two wasm rustflags),
`tools/web-persist-probe.mjs` (new), `.agents/plans/001-rust-port/`
(F34 + D-039). Web-only in effect: the `oh-app` additions are
`#[cfg(target_os = "emscripten")]` (zero desktop/Android effect —
`tools/check-android.sh` green); no on-disk format touched.

**Lane 046 coordination (per the task's boundary):** 046's IME element +
fullscreen button are NOT in the working tree yet (its commits are
spec-only at this HEAD; the last `index.html` commit is 032's). This
lane's page change is additive inside the module script and keeps the
canvas/status lines untouched; the ONE shared line is the factory call
(`const mod = await Module(cfg)` — was `Module()`), which 046's glue may
also want to carry its config object through. If 046 lands first, its
module-script additions and this lane's `ohWebPersistBoot` block must both
be kept (a merge of the module script, not a last-writer-wins).

**Gates:** host gate `tools/gate.sh` 5/5 green (fmt / build / clippy
`-D warnings` / test / lua-parity); the wasm build in the emsdk container
(`tools/package-web.sh` — the single source of truth for the recipe, incl.
`CXXFLAGS=-fwasm-exceptions` for the Luau link) + staging green;
`tools/check-android.sh` green; the probe 14/14, exit 0 (run twice).

**Probe input notes (recorded in the probe's comments so the next driver
doesn't re-learn them):** the CDP taps are HELD across frames (120 ms) —
the C++'s menu binds are the SSVStart Manager's per-frame trigger POLLING
(`game.addInput`, MenuGame.cpp:912-1018), so a key tapped entirely inside
one frame leaves no trace in the per-frame input snapshot — the port's 025c
documented finding, faithful to the C++. And the boot-screen-close key is
`Space`, never a letter: the C++'s `playLocally()` clears only
`enteredStr` (MenuGame.cpp:1783-1789), so a `w` close types a `w` into the
fresh profile name (`"WPROBE"` — observed in the first probe run; the C++
leaks it too, the `isEnteringText`-gated accumulation at MenuGame.cpp:
2862-2878); `Space` is dropped by the accumulation's
alphanumeric/punctuation filter and has no menu bind in the boot states.
