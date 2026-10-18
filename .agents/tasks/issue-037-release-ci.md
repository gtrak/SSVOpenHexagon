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
