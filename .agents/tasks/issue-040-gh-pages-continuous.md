# Task 040 — gh-pages hosts the wasm build AUTOMATICALLY (continuous), not only on tags

Read `.agents/WORKER-HYGIENE.md` first. Operator directive, verbatim: *"that's not quite right. I
want gh-pages to host a wasm build automatically."* — correcting issue 037's design, where the
gh-pages deploy is tag-only (`.github/workflows/release.yml`: the job is gated
`if: github.event_name == 'push'` and `on:` is `tags: ["v*"]` + `workflow_dispatch`).

## What to build

A **continuous** deploy: every push to the default branch publishes the wasm build to `gh-pages`,
so `https://gtrak.github.io/SSVOpenHexagon/` always reflects master. Two workflows own Pages, so
they must not race each other.

1. **`ci.yml`** — add the packaging + deploy to the per-push workflow:
   - the existing `wasm` job builds the compile proof; on a **default-branch push** have it also
     produce the publishable package through the SAME script everything else uses,
     `bash tools/package-web.sh` (one source of truth — do not invent an inline build here).
     The script already emits the inline serve directory and the zip whose root layout is the
     gh-pages contract; the page's asset references are RELATIVE (`site/index.html` imports
     `"./oh-web.js"`; the loader looks for `oh_web.wasm` beside it and the `.data` is fetched
     relative to the page), which is what makes it work from a PROJECT subpath — verified, so do
     not "fix" it into absolute paths.
   - a `pages` job that `needs` it, uploads nothing new, and deploys with
     `peaceiris/actions-gh-pages@v4` (`publish_branch: gh-pages`, `publish_dir:` the staged
     `_WEB` directory), gated to **default-branch pushes only**:
     `if: github.event_name == 'push' && github.ref == 'refs/heads/master'`. Never on PRs, never
     on forks (a fork's push must not be able to publish), and never on tag pushes (a tag push
     has `github.ref` = `refs/tags/...`, so the same predicate excludes it).
   - `permissions:` at the job (or workflow) level: `contents: write` (Pages publishing writes a
     branch) and `pages: write` if you use the Pages API.
2. **Concurrency is mandatory**: give BOTH this job and the release workflow's gh-pages job
   `concurrency: { group: gh-pages, cancel-in-progress: false }` so the two publishers serialize
   instead of racing (they write the same branch).
3. **Decide and state who wins on a tag**: the release workflow's deploy publishes the RELEASED
   bytes; `ci.yml` publishes master's. Keeping both is fine (the tag's content is normally
   identical, and a tag on a non-master commit still gets its own page) — but say which you chose
   and why. If you drop one, say so explicitly; do not leave two silent publishers.

## Operator-side settings (name them precisely in the report)

With the branch publisher the live page needs **Settings → Pages → Build and deployment →
Source: "Deploy a branch"**, branch `gh-pages`, folder `/ (root)`. GitHub reports
`has_pages: false` today, so nothing serves until that is set — the workflow cannot set it. If
you instead switch to the Actions-based Pages flow (`actions/configure-pages` +
`upload-pages-artifact` + `deploy-pages`), the required setting is "GitHub Actions" — pick ONE
and say which, because the wrong pairing produces a silent 404.

## A robustness fix in the same commit range (found while verifying)

`tools/package-web.sh` stages from a hardcoded `target/...`, so running it with
`CARGO_TARGET_DIR` set (which every lane and the orchestrator do) fails at the staging step with
`cp: cannot stat 'target/wasm32-unknown-emscripten/release/oh-web.js'` — I hit exactly this. Make
it honour `${CARGO_TARGET_DIR:-target}` the way `tools/gate.sh` already does (`LOG_DIR`), and
prove it by running it BOTH ways. This matters because the container recipe (no host `emcc`) is
how anyone builds this locally, and the container always sets `CARGO_TARGET_DIR`.

## Verification

You cannot run a GitHub runner, so:
- **lint the YAML** (`actionlint` if available, else parse + `bash -n` every `run:` block) and
  quote the result;
- **execute the build+package locally** (`bash tools/package-web.sh` in the emsdk container —
  the exact wrapper is worth recording in the report as the local recipe, including the
  `CARGO_TARGET_DIR=/work/target` subtlety) and confirm the staged `_WEB` directory contains the
  page + `oh-web.js` + `oh_web.wasm` + `oh_web.data` **in one directory**;
- **prove the page works from a subpath**: serve the staged directory and load it at
  `http://localhost:<port>/oy/`-style nested path (or at least assert every asset reference in
  `index.html` is relative), because `gtrak.github.io/SSVOpenHexagon/` IS a subpath;
- state plainly what remains unverified (the runner side, the Pages deploy, the repo setting).

## Hygiene

One commit per milestone (the CI change, the script fix, the report), explicit paths,
`cargo fmt` if Rust is touched, bound every process with `timeout`, at most ONE image read, and
append a `## Report` here with the workflow structure, the operator-side settings, the local
recipe, and the unverified list. Do not weaken the existing gate jobs and do not turn the
tag-release flow off by accident.

## Report` here with the workflow structure, the operator-side settings, the local
recipe, and the unverified list. Do not weaken the existing gate jobs and do not turn the
tag-release flow off by accident.

## Report

Lane 040, branch `lane/040` (cut from master `bfbf1bd9`). Three commits: `e6c3c228` (the
script fix), `eecf82c0` (the CI change), this report. No Rust touched (no `cargo fmt` needed);
host gate green (all 5 stages, 1003 tests) before BOTH of the first two commits; every process
bounded by `timeout`; zero image reads.

### 1. What landed

**`tools/package-web.sh`** (`e6c3c228`) — honours `${CARGO_TARGET_DIR:-target}` exactly the way
`tools/gate.sh` does for `LOG_DIR`: the three staging `cp`'s now read the build artifacts from
`${OUT}/${TARGET}/release/…` where `OUT="${CARGO_TARGET_DIR:-target}"`. Before: hardcoded
`target/…`, so any run with `CARGO_TARGET_DIR` set (every lane, the orchestrator, the emsdk
container recipe) died at staging with `cp: cannot stat 'target/wasm32-unknown-emscripten/
release/oh-web.js'` — while the build itself had succeeded into the custom dir (the negative
control below shows cargo finishing clean and only the staging failing). The STAGING/zip output
(`target/package/web/_WEB`, `open-hexagon-web.zip`) deliberately stays at the repo-root
`target/` (gitignored) regardless of `CARGO_TARGET_DIR`: it is this script's own output, not
cargo's, and a fixed path keeps the CI upload path (`ci.yml`'s pages job) and this contract
independent of wherever the build output is pointed. A relative `CARGO_TARGET_DIR` resolves
against the repo root — the same cwd cargo builds from, the same semantics `gate.sh` relies on.

**`.github/workflows/ci.yml`** (`eecf82c0`) — the continuous deploy:

- `wasm` job (the per-push compile proof — unchanged in what it builds): after `bash
  tools/package-web.sh` (one source of truth, no inline build), it now uploads the staged
  serve directory `target/package/web/_WEB` as artifact `web-publish` — the step is gated
  `if: github.event_name == 'push' && github.ref == 'refs/heads/master' && github.repository ==
  'gtrak/SSVOpenHexagon'` (no artifact on PRs, non-default branches, tags, or fork runs).
- **NEW `pages` job**: `needs: wasm`, the SAME gate, `concurrency: {group: gh-pages,
  cancel-in-progress: false}`, `permissions: contents: write` (job-level — least privilege;
  no `pages: write`, the Pages source is "Deploy a branch", not the Actions flow). Steps:
  `actions/download-artifact@v4` (name `web-publish`) → `peaceiris/actions-gh-pages@v4`
  (`publish_branch: gh-pages`, `publish_dir: web-publish`) with the generated `GITHUB_TOKEN`.
  The staged contents (page + `oh-web.js` + `oh_web.wasm` + `oh_web.data` in ONE directory,
  relative references intact) land at the gh-pages branch root →
  `https://gtrak.github.io/SSVOpenHexagon/` reflects master on every master push.
- **The gate, exactly**: never PRs (event = `pull_request`), never tag pushes
  (`github.ref` = `refs/tags/…`), never forks (see the deviation note below), never
  non-default branches (`github.ref` ≠ `refs/heads/master`). If the `wasm` job is red,
  `needs: wasm` skips `pages` — no incomplete page is ever published.

**`.github/workflows/release.yml`** (`eecf82c0`) — the `gh-pages` job gains the same
`concurrency: {group: gh-pages, cancel-in-progress: false}` (job-level, so only the deploy
serializes; the rest of the release graph is untouched). Everything else — the tag trigger
(`v*` + `workflow_dispatch`), the deploy step's `if: github.event_name == 'push'` (dispatch
dry-runs still must not overwrite the live page), the workflow-level permissions — is
unchanged. The tag-release flow is intact (verified: triggers, gates, and the deploy step all
as 037 wrote them).

### 2. Concurrency + who wins on a tag

After this change TWO workflows publish the `gh-pages` branch: `ci.yml`'s `pages` job (master
pushes) and `release.yml`'s `gh-pages` job (tag pushes). Both carry the IDENTICAL concurrency
spec (`group: gh-pages`, `cancel-in-progress: false`), so a master push and a tag push
SERIALIZE: the second publisher queues until the first's git push completes. A race that
half-writes the branch is exactly the kind of thing nobody diagnoses later — and
`cancel-in-progress: false` matters: cancelling the in-flight publisher would be the half-write
maker. (Skipped jobs — e.g. `pages` on a PR run — never enter the group, so PRs cost nothing.)

**Who wins on a tag: both are kept** (the spec says keeping both is acceptable; leaving two
silent publishers is not — so here is the behavior, stated):

- A tag cut on master's HEAD (the normal case): both publishers write IDENTICAL bytes (tag
  content == master content) — no observable winner, serialization just makes the double
  write atomic.
- A tag on an older or non-master commit: `release.yml` deploys the RELEASED bytes (that
  tag's content); if a newer master push is in flight or follows, `ci.yml` re-deploys master's
  bytes — last complete writer wins, and every write is a COMPLETE state (never half-written)
  because the group serializes. Re-pinning to master on the next master push is the operator's
  stated intent ("the live page always reflects master"); the released bytes are additionally
  captured in the release's `open-hexagon-<tag>-web.zip` artifact either way, so nothing a tag
  published is lost.

### 3. Operator-side Pages setting (name it exactly — the wrong pairing is a silent 404)

This design uses the BRANCH publisher (peaceiris pushes the `gh-pages` branch; no Pages API,
no `actions/configure-pages`/`deploy-pages` flow). Therefore the required setting is:

> **Settings → Pages → Build and deployment → Source: “Deploy a branch” → Branch: `gh-pages` →
> Folder: `/ (root)`**

(If someone instead switched a publisher to the Actions flow — `configure-pages` +
`upload-pages-artifact` + `deploy-pages` — the required setting would be Source: “GitHub
Actions”; mixing the two is the silent 404. Only ONE pairing exists today: branch publisher,
branch setting.) GitHub reports **`has_pages: false` today (037's forensics) — nothing serves
until the operator flips this toggle; the workflows cannot set it.** Also note: the default
branch is hardcoded as `master` in the gates (the spec's exact predicate) — if the operator
renames the default branch, the two `if:` predicates and the `refs/heads/master` comments need
the rename with it.

### 4. Local recipe (no host `emcc` — the build runs in the emsdk container)

The verified recipe (mounted at the lane worktree, mirrors `/tmp/em-build-037.sh`):

```bash
podman run --rm \
  -v /home/gary/dev/oh-lanes/040:/work \
  -v /home/gary/.rustup:/home/gary/.rustup \
  -v /home/gary/.cargo:/home/gary/.cargo \
  -e RUSTUP_HOME=/home/gary/.rustup -e CARGO_HOME=/home/gary/.cargo \
  -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 -e CARGO_PROFILE_DEV_INCREMENTAL=false \
  -w /work emscripten/emsdk:latest \
  bash -c 'export PATH=/home/gary/.cargo/bin:$PATH; bash tools/package-web.sh'
```

- The root `.cargo/config.toml` carries the rest of the recipe: MODULARIZE/EXPORT_ES6, the
  WebGL2 minimums, `--preload-file=_RELEASE@/RELEASE` (+ `-sFORCE_FILESYSTEM=1`), the memory
  flags (growth on, 64 MiB initial / 2 GiB max), `-sWASM_BINDGEN`; the script sets
  `CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions` (mandatory for the Luau link) and
  version-checks the wasm-bindgen CLI (0.2.129, lockfile-matched) against `Cargo.lock`.
- **The subtlety**: with `CARGO_TARGET_DIR` pointed OUTSIDE the repo, the pre-fix script's
  hardcoded staging paths broke (`cp: cannot stat 'target/…'`) while cargo happily built into
  the custom dir — so the working local run pointed `CARGO_TARGET_DIR` at the repo's own
  `target/` inside the mount (`/work/target`). The fix makes other values work too: `OUT=
  "${CARGO_TARGET_DIR:-target}"` (a relative value resolves against the repo root, the same
  cwd cargo builds from). Note the container's `git` cannot resolve the worktree's main-checkout
  metadata (`.git` is a pointer file into the unmounted main checkout) — irrelevant to the
  build, but keep in mind for any in-container git work.

### 5. Verification (executed locally; every process `timeout`-bounded)

| check | result |
|---|---|
| `actionlint v1.7.12` on `ci.yml` + `release.yml` (baseline AND after the change) | **rc=0 clean, both** (shellcheck integration unavailable — not installed) |
| `bash -n` on every `run:` block (python-yaml extraction) | ci.yml 11/11, release.yml 20/20 — the new steps are all `uses:` (upload/download/deploy), no new shell |
| structural check (python): the gate, `needs: [wasm]`, the concurrency on BOTH publishers, `permissions: contents: write`, the deploy step's `publish_branch`/`publish_dir`, the release triggers + deploy-step `if` untouched | all assertions passed |
| **run A — DEFAULT** (`CARGO_TARGET_DIR` unset, emsdk container): full COLD build (114 crates compiled, 0 Fresh; the known `WASM_BIGINT is deprecated` linker warning only) → `WEB PACKAGED: /work/target/package/web/_WEB`, **rc=0** | `index.html` 5515 B + `oh-web.js` 167130 B + `oh_web.wasm` 2645921 B + `oh_web.data` 22765538 B **in ONE `_WEB` directory** (the inline serve-dir contract); zip 8.8 MB |
| zip-root layout (the gh-pages deploy contract) | `python3 zipfile` listing: exactly `index.html`, `oh-web.js`, `oh_web.wasm`, `oh_web.data` at the zip ROOT — no nested dir |
| **run B — CUSTOM, airtight** (`CARGO_TARGET_DIR=/tmp/oh040-ctd`, outside the repo; the in-repo `target/wasm32-unknown-emscripten` HIDDEN for the run so the staging cp's could ONLY be satisfied from the custom dir) → `WEB PACKAGED`, **rc=0** (in-repo subtree restored afterwards) | proves the staging resolves through `${CARGO_TARGET_DIR:-target}` |
| **run C — the operator's recipe** (`CARGO_TARGET_DIR=/work/target`, explicit absolute inside the mount) | fresh (0.15 s), `WEB PACKAGED`, rc=0 |
| **negative control** (the PRE-FIX script from git HEAD, `CARGO_TARGET_DIR=/tmp/oh040-ctd`, in-repo artifacts hidden) | cargo `Finished 'release' profile` (the build succeeds into the custom dir) then **`cp: cannot stat 'target/wasm32-unknown-emscripten/release/oh-web.js': No such file or directory`, rc=1** — the operator-reported failure, reproduced exactly |
| **nested-subpath load** (the PROJECT subpath proof) | served the staged `_WEB` contents at `http://127.0.0.1:8471/SSVOpenHexagon/` (python http.server under a nested path, bounded) — all four assets **200 at exact staged sizes**; `index.html`'s single asset reference is `import Module from "./oh-web.js"` (relative, as served); `oh-web.js` resolves `oh_web.wasm`/`oh_web.data` beside itself via `locateFile` (`REMOTE_PACKAGE_BASE="oh_web.data"`, `scriptDirectory` = the loader's own dir) — NO absolute URL anywhere in the page's asset graph, so `gtrak.github.io/SSVOpenHexagon/` resolves. (The absolute `/work/target/…` strings inside `oh-web.js` are MEMFS preload-entry names — the packed `_RELEASE` content tree — not fetch URLs.) |
| host gate (`bash tools/gate.sh`, before each of the two code commits) | all 5 stages `GATE_EXIT=0` (fmt / build / clippy -D warnings / **1003 tests passed** / lua-parity) |

### 6. What remains UNVERIFIED (the honest boundary — no runner here)

- **The runner side, end to end**: `emscripten-core/setup-emsdk@v16` installing emcc on the
  runner, `actions/upload-artifact@v4`/`download-artifact@v4` across the `wasm`→`pages` job
  boundary (both were used in this shape by 037's release graph, but the artifact itself has
  never crossed that specific boundary), and every trigger/gate predicate as EXECUTED by
  GitHub (they are lint- and structure-validated, not fired).
- **The Pages deploy**: the actual `peaceiris/actions-gh-pages@v4` push of the `gh-pages`
  branch (it was exercised by 037 only as spec, never run), the `contents: write` token scope
  on a first-party push, and the live URL `https://gtrak.github.io/SSVOpenHexagon/` serving
  the game.
- **The repo setting**: `has_pages` is `false` today — the operator must flip
  Settings → Pages → “Deploy a branch” / `gh-pages` / `/ (root)` (§3) before ANY of this
  serves.
- **The concurrency behavior under real simultaneous master-push + tag-push** (the group is
  the documented GitHub mechanism; the race it prevents is real, but I could not produce the
  two simultaneous runs here).

### 7. Deviations from the spec's shape (stated, per the task)

1. **`github.repository == 'gtrak/SSVOpenHexagon'` added to the gate** (both the `pages` job
   and the `wasm` job's upload step). The spec's exact predicate
   (`event == 'push' && ref == 'refs/heads/master'`) does NOT exclude a push to a FORK's own
   master — fork runs match it. Without the guard that fork run would still only be able to
   write the fork's own `gh-pages` (its `GITHUB_TOKEN` is scoped to the fork, never the
   source), so the source page is safe either way; the guard makes the spec's own clause
   ("a fork's push must not be able to publish") literally true AND keeps forks from burning
   a full wasm build. Zero behavior change for pushes in this repo.
2. **The `wasm` job uploads the staged `_WEB` directory as an artifact** (`web-publish`). The
   spec's shape is a `pages` job that "needs it, uploads nothing new, and deploys the staged
   `_WEB` directory" — but the two jobs run on separate runners, so the staged directory
   must cross the runner boundary and the artifact is the only channel. Reading: the PAGES
   job uploads nothing new (it deploys the staged bytes as-is — no re-zip, no rename, no new
   packaging), which is what the spec's sentence says; the handoff upload is the `wasm` job's
   (it already runs `tools/package-web.sh` — one source of truth — and the upload is gated
   identically to the pages job, so it is free on every non-publishing run). An alternative
   (a single job that builds AND deploys) would have violated "a pages job that needs it".
3. **`permissions` is job-level (`contents: write`) on the `pages` job**, not workflow-level —
   the spec allows either; job-level keeps the gate/cross-check/android jobs on default
   token scopes and matches "`permissions` for branch writing" on the one job that writes
   the branch. (`pages: write` deliberately omitted — no Pages-API usage; see §3.)
