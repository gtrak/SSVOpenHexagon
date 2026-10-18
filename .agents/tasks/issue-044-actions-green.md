# Task 044 — make the GitHub Actions gate actually PASS (the runner lacks the C-dep prerequisites)

Read `.agents/WORKER-HYGIENE.md` first. Operator: *"go for it re gh actions"* — after the first
push in weeks exposed why the CI has never been green.

## The measured state (from the real runs, via `gh`; do not re-derive)

The new `.github/workflows/ci.yml` runs five jobs on a push. From run `39085323362` (master
`18ea81f6`):

```
✓ Web (wasm compile proof)        3m47s
✓ Android (aarch64 check, NDK)     1m52s
✗ Gate (ubuntu-latest)   build=101  clippy=101  test=101   lua-parity=0 ✓
✗ Gate (windows-latest)  build=101  clippy=101  test=101   lua-parity=1
```

So **both gate legs fail at `cargo build --workspace`** while the wasm and Android legs build the
same workspace fine, and the parity stage passes on ubuntu. That is a **runner-environment** gap,
not a code failure — and it is **pre-existing**: the old `rust.yml` gate was red on 8/8 runs.

## Why, from our own manifest (deliberately chosen — do NOT "fix" these)

- `Cargo.toml:42` — `flate2 = { version = "1", default-features = false, features = ["zlib"] }`
  links the **system zlib**. It is pinned that way so the port's `.ohr.z` compression stays
  byte-identical to the C++'s zlib output (030b's compression pin). **Switching flate2 to its
  pure-Rust backend would break parity** — do not do it; provide the library instead.
- `Cargo.toml:51` — `sodiumoxide = "0.2.7"` builds **libsodium from source**, which needs
  `autoconf`/`automake`/`libtool` + `make` + a C compiler.
- `rusqlite` (bundled) and `luau0-src` (C++) need a C/C++ compiler, which the runners have.

Today the workflows install exactly one apt package (`libudev-dev`).

## Step 1 — MAKE THE FAILURE VISIBLE BEFORE FIXING IT

`tools/gate.sh:31` redirects every stage to `$LOG_DIR/gate.log`, so the CI log contains only
`GATE_EXIT=101` and **the compiler error dies with the runner**. That is why 8 red runs taught
nobody anything. Fix the diagnosability first — a workflow step running `if: failure()` that tails
`target/gate.log` (and/or have `gate.sh` print the failing stage's tail) — then re-run and **read
the actual error**. Do not guess the fix before you have seen it; the flate2/sodiumoxide reading
above is a strong hypothesis, and a hypothesis is not the error message.

## Step 2 — fix the legs against the evidence

- **Linux (`ubuntu-latest`)**: add the packages the C dependencies need (expected:
  `zlib1g-dev`, `autoconf`, `automake`, `libtool` — corrected by whatever Step 1 prints).
- **Windows (`windows-latest`)**: this is the hard leg — a system zlib **and** an
  autoconf-built libsodium on the MSVC runner. Two acceptable outcomes, in order of preference:
  **(a)** drop that matrix member and keep Windows covered by the ubuntu `windows-cross` job
  (which already carries the libsodium prebuild workaround and builds the whole workspace for
  `x86_64-pc-windows-gnu`) plus the release workflow's mingw build — *say so explicitly and cite
  the failure that justifies it*; **(b)** provide zlib/libsodium for MSVC (prebuilt/vcpkg) **if
  and only if that is genuinely small**. `windows-cross` PASSING is the evidence that (a) keeps
  real coverage: quote it. A leg that cannot pass is worse than no leg, but silently dropping
  Windows coverage is not acceptable.
- **Never** make a leg green by skipping tests, filtering the workspace, or dropping `-D warnings`.

## Method — iterate against the real runs (you can do this from your worktree)

`ci.yml` triggers on a push to **any** branch, so `git push -u origin HEAD` from your lane worktree
runs the whole CI **on your branch** (the `gh` CLI here is authenticated as `gtrak` with the repo
scope). Watch it with `gh run list -R gtrak/SSVOpenHexagon --limit 3` and
`gh run view <id>` / `gh run view --log-failed <id>`, iterate, and stop when **every job on your
branch is green**. Note the `pages` job is default-branch-only by design — it will not run on your
branch; do not "fix" that.

## Acceptance and the boundary

- **Acceptance**: a run on YOUR branch with **all jobs green**, quoting the run URL, the per-job
  conclusions, and the failure-log output that identified the cause.
- Do not touch `release.yml`'s triggers, jobs or artifacts (040 just landed the gh-pages
  concurrency group; leave it), keep every job calling the same `tools/` scripts (one source of
  truth), commit with explicit paths, bound every process with `timeout`, and **stop-and-report**
  if the Windows leg turns out to need real work — the fallback is (a), stated plainly.
- Append a `## Report` here: the error you actually saw, the packages added, the Windows decision
  and its evidence, the green run, and what remains unverified (e.g. a tag-triggered release is
  not exercised by a branch push).
