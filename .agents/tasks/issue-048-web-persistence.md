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
