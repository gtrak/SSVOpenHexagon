# Task 045 — the release workflow's build matrix has NEVER run; make it run (found by a dry-run)

Read `.agents/WORKER-HYGIENE.md` first. Found by the orchestrator tonight, not by a user: release.yml
(issue 037) is wired correctly in *intent* but its **gate job has never passed**, so the entire build
matrix — the mingw Windows zip, the Android APK, the Linux tarball, the wasm bundle — has never
executed even once.

## The evidence (a dry-run, because the workflow is designed for it)

release.yml's own header says: *"workflow_dispatch every job dry-runs without a tag (no Release is
[published])"* — the `release` job is `if: github.event_name == 'push'`, so a manual dispatch runs
every build and publishes nothing. I dispatched run **39097932008** and got:

```
X Gate (ubuntu) in 6m43s
- Linux x86_64 (tar.gz) · Windows x86_64 (zip, mingw-gnu cross) · Android arm64-v8a (APK) · Web (wasm)
- Release (tag pushes only) · gh-pages (the wasm build inline)     [all skipped: needs gate]
```

**The cause is workflow drift, visible by diffing the two files:** `ci.yml` gained the ALSA step in
044 (`pkg-config --exists alsa || sudo apt-get install -y libasound2-dev`, because rodio -> cpal ->
alsa-sys needs `alsa.pc`), and `release.yml` did **not** — it still installs only `libudev-dev`. Its
gate therefore dies exactly where the CI's did before 044. This is the same class of bug as 044's:
the workflow file, not the code.

## What to do

1. **Fix the drift, preferably at its root rather than by copy-paste.** Both workflows currently
   inline their apt prerequisites; that is why one got the ALSA step and the other did not. Prefer a
   single shared script (e.g. `tools/ci-deps.sh`, in the style of the existing `tools/check-android.sh`
   / `tools/package-web.sh` — "one source of truth, called by every job") that both workflows invoke,
   so the next prerequisite cannot drift again. If you keep them inline, say why, but the drift is the
   actual defect and should not survive your change.
2. **Then iterate the matrix to green** with `gh workflow run release.yml -R gtrak/SSVOpenHexagon
   --ref <your-branch>` (the `gh` CLI here is authenticated as `gtrak`; a dispatch on your own branch
   runs YOUR version of the file). Watch with `gh run list -R gtrak/SSVOpenHexagon` and
   `gh run view <id>` / `--log-failed <id>`. Expect real breakage in the builds themselves, because
   they have never run: the mingw `--build` path (not just the cross-check's `--check`), the APK
   packaging (`tools/package-android.sh`), the wasm packaging, the artifact uploads. Fix what breaks;
   each fix should be small and mechanical, and the *scripts* stay the source of truth — do not
   duplicate a command into YAML.
3. **Sanity-check the artifacts are real**, not just uploaded: their sizes and, where cheap, their
   contents (e.g. the APK carries the `_RELEASE` asset set, the zip contains the binary and its data
   dir) — a green upload of an empty file is not a pass. Keep it bounded.

## Acceptance, and what a dry-run cannot reach (state these plainly)

- **Acceptance**: a dispatch on YOUR branch with **gate, linux, windows, android and wasm all green**,
  quoting the run URL, the per-job conclusions, and the artifact names/sizes.
- **Out of reach by design — do NOT "fix" these**: the `Release` job and the `gh-pages (inline wasm)`
  job are push/tag-gated, so a dispatch cannot exercise them. That means the actual tag flow, the
  Release publication, and the tag-time gh-pages deploy remain **verified only by a real tag** —
  say so, and tell me in the report what the first real tag would additionally exercise.
- Do NOT push a tag, do NOT publish a release, do NOT weaken any gate or job in release.yml, and keep
  the artifact names/paths 037 chose (the operator's ask was tagger releases with a Windows build, an
  APK, and the wasm page inline).

Boundaries: `.github/workflows/` and `tools/` (plus a spec/Report). Commit with explicit paths, bound
every process with `timeout`, and append a `## Report`: the drift you found, the shape of the fix, the
galleries of failures the matrix produced, the green dispatch, the artifacts, and what only a real tag
can verify.

## Report

**Result: the release matrix now actually runs and goes green end-to-end on a dispatch.** Green run:
https://github.com/gtrak/SSVOpenHexagon/actions/runs/39106997812 (dispatch on `lane/045`, commit
78039df8). Gate, linux, windows, android and wasm all green; the `Release (tag pushes only)` job
skipped (correct); all four artifacts present and content-verified (below). The `ci.yml` side of the
same change went green on the lane branch first (run 39102726297, fully green).

### The drift found (why the gate had never passed)

`ci.yml` gained the ALSA apt prerequisite in 044 (`libasound2-dev` — rodio -> cpal -> alsa-sys needs
`alsa.pc`); `release.yml` did not — it still installed only `libudev-dev`, so its gate died in
`tools/gate.sh`'s `--build` exactly where CI's had died pre-044 (spec's run 39097932008). The deeper
drift: both workflows inlined their own apt prerequisite lists, so nothing kept them in lockstep. A
second, latent drift of the same class: the runner image's own NDK env vars vs the pinned NDK (gallery
item 2).

### The shape of the fix

- **`tools/ci-deps.sh` (new)** — one apt-prerequisites script, called by both workflows: `libudev-dev`
  (037), `libasound2-dev` (044), `mingw-w64` (cross job). Each package is guarded by a presence check
  (`dpkg -s` / `pkg-config --exists alsa`) so it is a no-op where the image already has it, and a
  no-op off-Linux. `ci.yml` (both gate legs), `release.yml`'s gate and the windows-cross job now call
  it; the inline apt commands are gone. The next prerequisite lands in one place and cannot drift.
- **ASSET_TAG normalization step** (all six release.yml jobs that use `ASSET_TAG`) — `github.ref_name`
  with `/` -> `-` via `GITHUB_ENV`. A no-op on tag pushes; on a dispatch whose branch name carries a
  `/` (e.g. `lane/045`) it yields a filesystem-safe tag (`lane-045`) for artifact names/paths. 037's
  tag-flow names are unchanged.
- **`tools/android-env.sh`** — after exporting the pinned NDK (r27c, via `OH_NDK` or the pinned
default) it now `unset`s `ANDROID_NDK_ROOT ANDROID_NDK_PATH NDK_HOME`. Why: ndk-build 0.10's
  `Ndk::from_env` resolves `ANDROID_NDK_ROOT` *before* `ANDROID_NDK_HOME`, and the ubuntu-latest
  image's `/etc/environment` sets both to the image NDK (27.3.13750724 = r28), silently defeating the
  pin. Negative control: presets pointed at nonexistent NDKs still took r27c.
- **`tools/package-android.sh`** — self-installs the rustup target it builds for (`rustup target add
  aarch64-linux-android`; `OH_APK_TARGET` override) instead of assuming it exists; and dropped the
  `| head -20` from the APK-contents listing (gallery item 3).
- **`release.yml`** — the windows zip step writes to the repo root (`../../../`; the stage dir is
  THREE levels deep — `target/package/windows-x86_64-pc-windows-gnu` — so `../../` had landed in
  `target/`), and all four upload steps set `if-no-files-found: error`, so a missing artifact is a
  FAILED job, not a green job with a warning.

No gate weakened, no job removed, no tag pushed, no Release published; 037's artifact names/paths
kept.

### Failure gallery (the matrix, iterated to green)

1. **39097932008** (spec's pre-fix dispatch): gate red — the ALSA drift above. Everything downstream
   never ran.
2. **39103410548** (after `ci-deps.sh` + ASSET_TAG normalization): **gate green for the first time**;
   linux/windows/wasm/gh-pages green. Android red with two latent defects at once:
   - **E0463, crate `android_log-sys` for `aarch64-linux-android`** — the rustup target was never
     installed. `check-android.sh` only `cargo check`s the host target and the gate never
     cross-checks, so neither lane nor CI had ever installed it; the script assumed it.
   - **Latent NDK drift** — the image's `ANDROID_NDK_ROOT` (= r28) resolved ahead of the pinned
     `ANDROID_NDK_HOME` (see shape of fix).
3. **39104852496** (after target self-install + NDK unset): compile, link and signing all succeeded
   (log proves r27c was used; zero occurrences of r28), then the job died in the script's OWN
   diagnostic: `unzip -l "$APK" | grep -E ... | head -20` — `head` exits after 20 lines and SIGPIPEs
   the grep mid-stream on a runner whose `unzip -l` still streams (page cache made local runs fast
   enough to dodge it). `set -o pipefail` turned `grep: write error: Broken pipe` (exit 2) into a red
   job. Fix: dropped the `| head -20` (reproduced the flake under a slow-stream simulation: old form
   rc=141, new form rc=0).
4. **39105852798** (after the SIGPIPE fix): **every job green — and the windows artifact did not
   exist.** The zip landed one level too deep (`target/`), the upload found nothing at the repo root,
   and `upload-artifact@v4`'s default `if-no-files-found: warn` merely warned: `##[warning]No files
   were found ... No artifacts will be uploaded` — a green job with zero artifact. The spec's "a
   green upload of an empty file is not a pass," caught by checking artifact contents instead of
   trusting the green.
5. **39106997812** (after zip depth + `if-no-files-found: error`): **fully green, all four artifacts
   present and verified.**

### Per-job conclusions (green run 39106997812, dispatch on `lane/045`)

- **Gate (ubuntu): green.** 037's `tools/gate.sh` (`--build --release` + `--check`) passes on
  ubuntu-latest with the shared prerequisites — the release gate's first passing run, ever.
- **Linux x86_64 (tar.gz): green.** `tools/package.sh`: native release build (smoke run, pinning
  1.725), `_RELEASE` staging, tar.gz.
- **Windows x86_64 (zip, mingw-gnu cross): green.** The mingw `--build --release` path running for
  the first time (CI had only cross-*checked* it): all four .exe cross-compile and link
  (`open-hexagon.exe` 27.8 MB PE32+, byte-identical size to the local cross build), zipped and
  uploaded.
- **Android arm64-v8a (APK): green.** cargo-apk 0.10 + pinned NDK r27c (the unset beat the image's
  r28 env) + self-installed rustup target. APK: 223 asset entries (the 039 `_RELEASE` asset set),
  16 KB ELF LOAD alignment (0x4000), `android_main` exported, signature verified.
- **Web (wasm, emscripten): green.** emscripten 3.1.64 build + 4-file bundle.
- **gh-pages: green** (builds inline, downloads and unpacks the web artifact); the deploy step was
  **skipped by design** (push-gated) in every dispatch — verified the `gh-pages` branch was untouched
  by any of these runs (its tip commit predates all four dispatches).
- **Release (tag pushes only): skipped** — correct.

### Artifacts (green run; a tag flow would name these `open-hexagon-<tag>-...`)

| artifact | size | contents verified |
|---|---|---|
| `open-hexagon-lane-045-linux-x86_64.tar.gz` | 8,172,929 B | 248 entries: `_RELEASE/open-hexagon` (the binary) + `config.json` + `Assets/` + `Packs/` + `ConfigOverrides/` |
| `open-hexagon-lane-045-windows-x86_64.zip` | 9,803,407 B | 248 entries: `_RELEASE/open-hexagon.exe` (27,787,390 B) + the same data tree |
| `open-hexagon-lane-045-android-arm64-v8a.apk` | 13,368,395 B | `lib/arm64-v8a/liboh_android.so` (14,972,408 B) + libc++_shared.so + AndroidManifest + 223 asset entries |
| `open-hexagon-lane-045-web.zip` | 9,545,116 B | `index.html` (5,515 B) + `oh-web.js` (202,320 B) + `oh_web.wasm` (3,528,846 B) + `oh_web.data` (22,765,538 B, the preloaded `_RELEASE` tree) |

### What only a real tag can verify (by design — not "fixed")

- **Release publication**: the `release` job (`upload-to-release@v4`) runs only on tag push. Its
  release-notes flow, attaching to the release, and the tagger-release UX have never executed.
- **The tag-time gh-pages deploy**: pushing the wasm bundle to the `gh-pages` branch (the
  `deploy: <sha>` commit that makes the page live) is push-gated; every dispatch skipped it. The
  first tag is the first real deploy of the continuous workflow (the page layout itself is already
  proven by today's one-off manual deploy).
- **The tag-trigger flow itself**: the `on: push: tags` trigger, `github.ref_name` as ASSET_TAG (the
  normalization step is a no-op on tags), and the whole matrix running off a tag checkout.

### Lane-hygiene notes

- Every commit here is shell/YAML-only — the full gate was therefore skipped per the lane-hygiene
  rule; instead: `bash -n` on every changed script, YAML parse of both workflows, local
  pre-verification of every matrix leg on the dev box (mingw cross-build, `package.sh` with smoke,
  `package-android.sh` with full verify), the NDK-pin negative control, and the SIGPIPE
  slow-stream reproduction.
- Every external process was bounded by `timeout`; every commit used explicit paths.
- No tag pushed, no Release published, no gate weakened; 037's artifact names/paths preserved.

