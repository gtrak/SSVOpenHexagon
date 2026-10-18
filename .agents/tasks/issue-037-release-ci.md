# Task 037 — release CI: tag-triggered releases, a Windows cross-build, the Android APK, and gh-pages with the wasm build inline

Read `.agents/WORKER-HYGIENE.md` first. Operator request, verbatim: *"queue up github ci for
tagger releases, a windows build, the android apk, and gh-pages with the wasm build inline.
you can use cargo-xwin for windows."*

Repository: `https://github.com/gtrak/SSVOpenHexagon` (so Pages will serve at
`gtrak.github.io/SSVOpenHexagon`). Workspace version: `Cargo.toml`'s `0.1.0`.

## What already exists (build on it, do not duplicate)

- `.github/workflows/rust.yml` — the port's own gate (`tools/gate.sh` on
  ubuntu+windows, plus a `tools/build-windows.sh --check` cross-check job). **The release
  workflow's first job is this gate, not a re-implementation.**
- `.github/workflows/ci_linux.yml` / `ci_windows.yml` — the UPSTREAM C++ project's CI
  (ninja + msys2). Leave them alone.
- `tools/package.sh` — the Linux release packager (matches the C++ `_RELEASE` layout
  exactly, from a pristine copy of the tracked `_RELEASE`; has `--windows` and
  `--with-steam`).
- `tools/build-windows.sh` — the **x86_64-pc-windows-gnu** cross-build (mingw-w64 + a
  libsodium prebuild workaround documented in its header; its comment says MSVC "is NOT
  cross-buildable from Linux", which is what cargo-xwin changes).
- `tools/package-android.sh` — the APK (cargo-apk 0.10, `tools/android-env.sh` for the
  NDK, a self-distribution keystore, `RUSTFLAGS=-C link-arg=-Wl,-z,max-page-size=16384`).
- The wasm build recipe: `cargo build --release -p oh-web --target
  wasm32-unknown-emscripten` in the `emscripten/emsdk` image with
  `CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions` (the root `.cargo/config.toml`
  carries the rest: MODULARIZE/EXPORT_ES6, the WebGL2 minimums, `--preload-file`, and the
  memory flags). The serve dir is `crates/oh-web/site/index.html` + `oh-web.js` +
  `oh_web.wasm` + `oh_web.data`.

## Deliverable: `.github/workflows/release.yml`

Triggered on **tags** (`v*`) AND on `workflow_dispatch` (so each job can be dry-run without
tagging — important, because none of this can be executed here). Jobs:

1. **gate** — reuse `bash tools/gate.sh` (ubuntu), so a release can only be cut from a green
   tree.
2. **linux** — `bash tools/package.sh` → upload the staged directory as a tar.gz.
3. **windows** — the operator asked for **cargo-xwin**: `cargo install cargo-xwin`, then
   `cargo xwin build --release --target x86_64-pc-windows-msvc`, then zip the `.exe`
   alongside the `_RELEASE` tree. **Known risk you must resolve rather than paper over**:
   the C dependencies cross-compile through `cc` — `luau0-src` (C++), `rusqlite` (bundled),
   and especially **`libsodium-sys` 0.2.7**, whose vendored autoconf `configure` is the
   reason `build-windows.sh` exists (it prebuilds for the canonical MinGW triple). If
   libsodium (or another C dep) cannot build for MSVC-on-Linux under xwin, **fall back to
   the already-scripted mingw-gnu release build** (`bash tools/package.sh --windows`) and
   say exactly which dep blocked MSVC in the workflow comments and your report. A working
   mingw artifact plus a documented reason beats a broken MSVC job. Try xwin FIRST (it is
   what was asked for) and report what you find either way.
4. **android** — `bash tools/package-android.sh` with the SDK/NDK set up
   (`android-actions/setup-android` + the NDK 27.2.12479018 pin), producing the
   `arm64-v8a` APK. Decide and document the signing story: the script generates a
   self-distribution keystore (fine for a non-store release) — if you prefer a real
   keystore, name the secrets and document that they must be created; **never** commit a
   keystore.
5. **wasm** — install emsdk (`mymindstorm/setup-emsdk`, or the `emscripten/emsdk`
   container), run the build command above, assemble the serve directory, and upload it as
   a zip.
6. **release** — the tagger release: create the GitHub Release for the pushed tag with all
   four artifacts attached (Linux tar.gz, Windows zip, Android APK, wasm zip) and generated
   notes. Name artifacts with the tag, e.g.
   `open-hexagon-<tag>-linux-x86_64.tar.gz`, `-windows-x86_64.zip`,
   `-android-arm64-v8a.apk`, `-web.zip`.
7. **gh-pages** — deploy the wasm build **inline**: the page and its artifacts in the SAME
   directory, so the game is playable straight from `gtrak.github.io/SSVOpenHexagon` with no
   external CDN. Use `peaceiris/actions-gh-pages` or `actions/deploy-pages`; state the
   required repo setting (Pages source = the deployed branch / GitHub Actions) in the
   workflow comments AND in your report, since that is an operator-side toggle.

## Verification (be honest about the boundary)

You cannot push a tag or run a GitHub runner here, so:

- **YAML must be valid and linted**: `actionlint` if you can install it (`go install` /
  a release binary), otherwise at minimum parse the YAML and shell-check every `run:`
  block (`bash -n` on the extracted scripts).
- **Every build command must be executed locally at least once** to prove the *command*
  works, even though its GitHub wiring cannot be: the Linux packaging (`tools/package.sh`),
  the Android APK (`tools/package-android.sh`), and the wasm build (the emsdk container
  recipe). For cargo-xwin, install it if the ~1 GB MSVC SDK download is acceptable on this
  box; if not, say so and mark the MSVC path as UNVERIFIED-BUT-SPECCED rather than implying
  it works.
- **State explicitly in the report** what could not be established: the GitHub-side runtime
  (runner images, action versions, the Pages deploy, the release API) and anything whose
  correctness depends on a repo setting the operator controls.
- Do not weaken the existing gate workflow, do not remove the upstream C++ workflows, and do
  not add a workflow that runs on every push if it would burn runner time for nothing — the
  release workflow is tag/dispatch-triggered.

## Hygiene

Commit with explicit paths, one commit per milestone (workflow → per-job validation → the
report); `cargo fmt` if you touch Rust; bound every process with `timeout`; at most ONE
image read (you probably need none); append a `## Report` to this file with the workflow
structure, the per-job evidence, the signing/Pages settings the operator must set, and the
unverified list.

## Addendum (operator, same session): REDO CI FROM SCRATCH

Operator, verbatim: *"redo CI from scratch, the stuff that's there is probably old c++."*
Their read is correct: `.github/workflows/ci_linux.yml` and `ci_windows.yml` are the **upstream
C++ project's** workflows (ninja + msys2 + ccache, pointed at the root `CMakeLists.txt` and
`build/`). They are obsolete for the port, and about to be doubly so because lane 038 is
archiving the whole C++ tree into `cpp-original/` in parallel.

Lane 037 therefore owns `.github/` entirely for this session (lane 038 is explicitly barred
from it), and its scope is now one coherent set:

1. **DELETE** `ci_linux.yml` and `ci_windows.yml`. Do not port the C++ build forward — the C++
   is the archived reference, verified locally through the container oracle, not a shipped
   artifact.
2. **REPLACE `rust.yml` with a fresh `ci.yml`**, as parallel jobs: `gate` (matrix ubuntu +
   windows → `tools/gate.sh`, with libudev for gilrs on Linux), `windows-cross`
   (`tools/build-windows.sh --check` — **the job that just proved its worth** by being the only
   thing that could see the `sigint.rs`/`libc::write` regression that left master Windows-red
   for several landings), `android` (a fast `cargo check` for `aarch64-linux-android` with the
   NDK pin), `wasm` (the emscripten build), and optionally a `workflow_dispatch`-only `oracle`
   job if it can be bounded (if not, omit it and say why — a job that times out is worse than
   no job).
3. The release workflow stays as specced, plus the approved `sigint.rs` fix.

**One source of truth for every command.** The CI jobs and the release workflow must invoke
the same `tools/` scripts, and any command that exists only inline in a workflow gets extracted
into a script. Command drift between a local run and CI is precisely how the Windows hole
happened: a check that only ever ran on GitHub.
## Report

Lane 037, branch `lane/037` (cut from master `a932988c`). Three commits:
`3d23490a` (the release workflow), `b5726c67` (the sigint.rs Windows
fix — approved by the orchestrator as this lane's own commit),
`7d7002f4` (the CI revamp — operator scope extension: `.github/` is this
lane's for the session). No keystore was committed; `cargo fmt --all`
clean; the tree is left at the report commit.

### 1. What landed

**`.github/workflows/release.yml`** (new; triggers: push tags `v*` +
`workflow_dispatch` only — never on every push). Jobs:

- **gate** — `bash tools/gate.sh` on ubuntu (REUSE of the port's own
  gate — the same script ci.yml's gate job runs; a release is only cut
  from a green tree).
- **linux** — `bash tools/package.sh` (release build + stage + the
  headless smoke of the packaged binary, pinned score `Final time:
  1.725`) → `open-hexagon-<tag>-linux-x86_64.tar.gz` (a tar of the
  staged `_RELEASE`, binary alongside the data, the C++ layout).
- **windows** — `bash tools/package.sh --windows` (the scripted
  x86_64-pc-windows-gnu cross-build + libsodium prebuild) →
  `open-hexagon-<tag>-windows-x86_64.zip`. **Not cargo-xwin MSVC** —
  see §3 for the exact, locally-verified reason.
- **android** — setup-java@v5 + setup-android@v4 (NDK 27.2.12479018
  pin + build-tools 34.0.0) + `cargo install cargo-apk --version
  0.10.0` + `bash tools/package-android.sh` →
  `open-hexagon-<tag>-android-arm64-v8a.apk`.
- **wasm** — setup-emsdk + the lockfile's wasm-bindgen CLI +
  `bash tools/package-web.sh` (the web recipe, §4) →
  `open-hexagon-<tag>-web.zip` (page + `oh-web.js` + `oh_web.wasm` +
  `oh_web.data` at the zip root — the inline-deploy layout).
- **release** (tag pushes only) — all four artifacts attached to the
  GitHub Release + `generate_release_notes: true`.
- **gh-pages** — the wasm artifact unpacked and deployed to the
  `gh-pages` branch root via peaceiris/actions-gh-pages@v4, deploy step
  gated to tag pushes (a dispatch dry-run must not overwrite the live
  page). Page + artifacts land in the SAME directory →
  `https://gtrak.github.io/SSVOpenHexagon/` serves the game with no
  external CDN.

Artifact naming uses `<tag>` = `github.ref_name` (on a
`workflow_dispatch` dry-run that is the branch name; no Release is
created on dispatch, no Pages deploy on dispatch).

**`.github/workflows/ci.yml`** (new; on push + pull_request; replaces
`rust.yml`, which was deleted with the stale C++ workflows
`ci_linux.yml`/`ci_windows.yml` — operator scope extension, "redo CI
from scratch, the stuff that's there is probably old c++"). Jobs:

- **gate** — `tools/gate.sh` on an ubuntu+windows matrix (the libudev
  step kept for gilrs on Linux; the pinned 1.97.1 toolchain mirrors
  `rust-toolchain.toml`).
- **windows-cross** — `tools/build-windows.sh --check` on ubuntu.
  **Never drop this job** — see §3 for the regression it is the only
  thing that catches.
- **android** — `tools/check-android.sh` (NEW: `cargo check -p oh-app
  --target aarch64-linux-android` under the NDK r27c pin) — the fast
  per-push signal; the slow signing-dependent APK stays in the release
  workflow.
- **wasm** — `tools/package-web.sh` under emscripten-core/setup-emsdk
  (compile proof per push; the release workflow uploads the artifact).
- **oracle** — CONSIDERED AND LEFT OUT (documented in the ci.yml
  header): `tools/cpp-oracle/run.sh` is not bounded on a 4-core runner
  (dependency-image build + CPM seeding + submodule init + a long
  SFML/LuaJIT/game build, several independent failure points stacked
  in one job) and its paths are in motion while lane 038 moves the C++
  tree into `cpp-original/`. Add it back as a workflow_dispatch-only
  job once the move settles and the build can be cached; until then
  the C++ column stays a local (dev-box) verification, by design.

**One source of truth** (the orchestrator's rule): every CI job and
every release job invokes a `tools/` script — `tools/gate.sh`,
`tools/build-windows.sh`, `tools/package.sh`, `tools/package-android.sh`,
`tools/check-android.sh` (NEW), `tools/package-web.sh` (NEW). No build
command exists inline in a workflow. `tools/package-web.sh` is the web
recipe's single home (the `CXXFLAGS_wasm32_unknown_emscripten=
-fwasm-exceptions` env, the release build from the repo root, the
serve-dir staging — including the `release/deps/oh_web.data` location
the lane-036 serve recipe cites — the non-empty-file checks, and the
zip with files at the zip root); it also version-checks the wasm-bindgen
CLI against Cargo.lock and fails loudly on drift.

### 2. Per-job evidence (executed locally, pinned toolchain 1.97.1,
lane worktree's own target dir, every process bounded by `timeout`)

| job command | local result |
|---|---|
| `bash tools/gate.sh` (gate) | ALL 5 STAGES `GATE_EXIT=0` (fmt / build / clippy -D warnings / test — **1003 tests passed** / lua-parity); log: `target/gate.log` |
| `bash tools/build-windows.sh --check` (windows-cross) | BEFORE the sigint fix: `error[E0308] ... sigint.rs:69 expected 'u32', found 'usize'` (reproduced twice, incl. the full build). AFTER: `== cargo check --workspace --target x86_64-pc-windows-gnu` / `Checking oh-app v0.1.0 ...` / `Finished 'dev' profile [unoptimized] target(s) in 0.89s`, **rc=0** |
| `bash tools/package.sh` (linux) | full workspace release build (263 crates) + staging + smoke **PASS** (`exit 0`, `Final time: 1.725`, Finished + config-save guard, runtime folders created); staged `_RELEASE` tarred locally (7.9 MB, 248 entries) |
| `bash tools/package.sh --windows` (windows) | full cross release build + link + staging; **`PACKAGED: target/package/windows-x86_64-pc-windows-gnu/_RELEASE`**; `file` on the staged binary: `PE32+ executable for MS Windows 5.02 (console), x86-64`, 27.9 MB; smoke skipped-with-reason (no .exe runtime on a Linux host — the documented behavior); zip step proven with the equivalent archive (248 entries, `_RELEASE/` root, 9.9 MB) |
| `bash tools/package-android.sh` (android) | **`APK OK — target/release/apk/open-hexagon.apk`** (13 M, 229 files, 223 asset entries); `ELF LOAD alignment: 0x4000` (16KB-page-ready); `android_main` exported; `apksigner verify` — **signature: verified** (the self-distribution keystore) |
| `bash tools/check-android.sh` (ci android) | `Finished 'dev' profile [unoptimized] target(s) in 19.85s`, **rc=0** |
| `bash tools/package-web.sh` (wasm; ci + release) | in the `emscripten/emsdk` container (the `/tmp/em-build2.sh` recipe pattern, mounted at this worktree): build `Finished 'release' profile [optimized]` (only the known `WASM_BIGINT is deprecated` linker warning), serve dir staged (`index.html` 5.5 KB, `oh-web.js` 167 KB, `oh_web.wasm` 2.6 MB, `oh_web.data` 22.7 MB), **`WEB PACKAGED` + `SCRIPT_RC=0`**, zip 8.8 MB with the 4 files at the zip root. The serve dir was also served locally (`python3 -m http.server`): `/`, `/oh-web.js`, `/oh_web.wasm`, `/oh_web.data` all **200** with full sizes — the inline, no-CDN layout works |

YAML/lint: **actionlint v1.7.12 clean (rc=0)** on `ci.yml` +
`release.yml` (shellcheck integration disabled — not installed here);
**every `run:` block `bash -n`-passed** (ci.yml 11, release.yml 20);
the two new tool scripts `bash -n`-clean. Action pins were verified
against the live GitHub API the day of this work: tag existence +
input specs from each action's `action.yml` — checkout@v4,
upload-artifact@v4 + download-artifact@v4 (a matched pair),
setup-java@v5, setup-android@v4 (the `packages` input confirmed
space-separated sdkmanager specs), setup-emsdk@v16 (the action moved
`mymindstorm/` → `emscripten-core/`; v16's action.yml verified),
softprops/action-gh-release@v2 (`files`, `generate_release_notes`
verified), peaceiris/actions-gh-pages@v4 (`publish_branch`,
`publish_dir` verified).

### 3. The windows job: why mingw-gnu, not the cargo-xwin MSVC that
was asked for

`cargo install cargo-xwin` (0.23.1) + `cargo xwin build --release
--target x86_64-pc-windows-msvc` was TRIED FIRST, locally. The MSVC
SDK download succeeded (CRT in 18 s) and the build started, then
**FAILED on three C dependencies**:

1. **`libsodium-sys` 0.2.7** (oh-net, the D-004 sodiumoxide pin) — the
   hard wall. Its `build.rs` runs the vendored libsodium 1.0.18 autoconf
   `configure` with `--host=x86_64-pc-windows-msvc`, and the crate's
   `build-aux/config.sub` rejects the rust msvc triple outright:
   `Invalid configuration 'x86_64-pc-windows-msvc': OS 'msvc' not
   recognized` → `configure: error` → build-script panic at
   `build.rs:257`. The autoconf build cannot target EITHER rust windows
   triple — the same wall `tools/build-windows.sh`'s header documents
   for `x86_64-pc-windows-gnu`, and the reason that script prebuilds
   libsodium for the canonical MinGW triple `x86_64-w64-mingw32`.
2. **`libsqlite3-sys` 0.28.0** (rusqlite, bundled) and **`libz-sys`
   1.1.30** — cc-rs selects `AR=llvm-lib` for the msvc target, but
   cargo-xwin does not provide `llvm-lib` on the build PATH: `error
   occurred in cc-rs: failed to find tool "llvm-lib"`. Their archive
   steps cannot run even past libsodium.

(`luau0-src`'s C++ was queued in the same build; the failure set above
is what the run produced — the job is the scripted mingw-gnu path
regardless.)

Hence the windows job uses `bash tools/package.sh --windows`
(x86_64-pc-windows-gnu, the documented prebuild workaround; the .exe is
MinGW-based, no MSVC CRT at runtime). The full reason is in the
`release.yml` header. The MSVC path is therefore not merely
"unverified" — it is **verified-failed** on this tree; if a future
`libsodium-sys` ships an msvc target (or a prebuilt msvc libsodium
appears), the one-line swap is documented in the header.

**The master-red regression the cross-check caught (the
"host gate is not a Windows gate" lesson):** master was
*x86_64-pc-windows-gnu*-red since `52a470d8` (29o1):
`crates/oh-app/src/sigint.rs::write_stdout` passed `bytes.len()`
(usize) to `libc::write`, whose count argument is `c_uint` (u32) on
the Windows targets — E0308. `tools/gate.sh` never compiles the
Windows target, so EVERY local gate stayed green while the shipped
Windows target did not compile; only `rust.yml`'s cross-check job
(GitHub) could see it — the sibling of the standing rule "the host
gate is not a web gate". Fix (`b5726c67`, orchestrator-approved):
platform-split the count — on Windows `c_uint::MAX`-clamped then cast
(no panic/`try_into().unwrap()` in a signal handler, no partial-write
loop — the C++'s single `write` mirrored), on Unix the usize count
unchanged. Proved by BOTH gates: full `tools/gate.sh` (5/5 stages,
1003 tests) and `tools/build-windows.sh --check` (the quoted output
above).

### 4. GitHub-side forensics (public-repo API, day of this work)

- **Actions IS enabled** on `gtrak/SSVOpenHexagon` (39 workflow runs)
  — nothing to turn on there.
- **`rust.yml` has NEVER been green on master**: 8/8 push runs failed
  since the first (10-16T11:47), and the latest run (master
  `ef5b655b`, which includes the 031 landing that postdates this
  lane's branch point `a932988c`) failed ALL THREE jobs —
  Cross-check, Gate (windows-latest) AND Gate (ubuntu-latest). The
  cross-check leg of that is the sigint E0308 (reproduced + fixed in
  this lane); the host-gate legs on `ef5b655b` are NOT identifiable
  from here (job logs need admin rights, and this branch point is
  locally green on both host and cross, so anything else red is newer
  than `a932988c` — flag for the orchestrator to confirm the post-merge
  master gate before tagging).
- The stale C++ `CI Linux` job sat stuck **`queued` on every push**
  (never started) and `CI Windows` was red — consistent with deleting
  both.
- **`has_pages: false`** — Pages has never been enabled; the operator
  toggle below is mandatory.
- No branch protection is implied anywhere: landings continued across
  the red streak, so nothing was gating.

### 5. Operator-side settings (not settable from the workflows)

1. **Pages source** (mandatory for the gh-pages job to serve):
   Settings → Pages → Build and deployment → **Source: "Deploy a
   branch"**, branch `gh-pages`, folder `/ (root)`. Then
   `https://gtrak.github.io/SSVOpenHexagon/` serves the game inline
   (page + wasm + data in the same directory, no CDN). `has_pages`
   is `false` today — the toggle has never been made.
2. **Android signing choice** (decided: the self-distribution
   keystore). `tools/package-android.sh` signs with
   `tools/android/openhexagon-release.keystore` (generated on first
   use, fixed password documented in the script's header). Fine for a
   non-store release. For store distribution the operator must create
   repo secrets **`ANDROID_RELEASE_KEYSTORE`** (base64 .keystore),
   **`ANDROID_RELEASE_KEYSTORE_PASSWORD`**,
   **`ANDROID_RELEASE_KEYSTORE_ALIAS`** and have a follow-up lane wire
   them into `tools/package-android.sh` (env override with the
   self-key fallback) — a keystore must NEVER be committed.
   Note for the record: the self-distribution keystore file is ALREADY
   tracked in git (landed with 25b, committed deliberately as the
   self-distribution key). This lane neither added nor committed any
   keystore; if the house rule is "no keystores in git at all", that
   tracked file is the operator's call to remove (it would change the
   signing identity of future self-distributed APKs — a product
   decision, not made here).
3. **No secrets are required** for the shipped workflows: the release
   job and the Pages deploy both use the generated `GITHUB_TOKEN`
   (the `permissions:` block grants `contents: write` + `pages: write`).
4. **Optional but recommended**: require the `CI` workflow (at least
   its `gate` + `windows-cross` jobs) to pass before merging — the
   forensics above show master landed repeatedly across a fully-red
   `rust.yml`. That is a branch-protection setting the operator owns.

### 6. What could NOT be established (the honest boundary)

No tag can be pushed and no GitHub runner can be run from here, so:

- **The GitHub-side runtime, end to end**: runner images (that
  `zip`/`unzip`/`pkg-config` are present on `ubuntu-latest` as
  assumed — standard but not proven here), each action's actual
  runtime behavior (setup-emsdk@v16 installing emcc on the runner,
  setup-android@v4's `packages` input landing the NDK at
  `$ANDROID_HOME/ndk/27.2.12479018`, `cargo install` timings on
  4-core runners), the **Pages deploy** (the branch push + the
  `gtrak.github.io/SSVOpenHexagon/` URL), and the **release API**
  (Release creation + generated notes + the four attached artifacts).
- **The `cargo xwin`/MSVC path** beyond the verified failure (§3): the
  failure is established locally at the pinned versions; the same
  failure is *expected* on a runner (same crates, same toolchain pin),
  which is why the job does not use it — but the runner-side behavior
  itself was not observed.
- **Tag-triggered behavior** (`push: tags: ["v*"]` firing, the
  `workflow_dispatch` dry-run path, the `if: github.event_name ==
  'push'` gates on `release`/the gh-pages deploy step) — the triggers
  and conditions are lint-validated, not executed.
- **The host-gate red on the LATEST master** (`ef5b655b`, postdating
  this branch point) — cause not identifiable without admin log
  access (§4); this lane's branch point is locally green on host +
  cross + the fix is in, so this lane does not add host-gate red.

The local execution of every build command (§2) is the strongest
substitute available: the commands the workflows run are the commands
that produced the artifacts above, in the pinned toolchain.
