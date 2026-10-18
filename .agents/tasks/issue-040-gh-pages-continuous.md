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
