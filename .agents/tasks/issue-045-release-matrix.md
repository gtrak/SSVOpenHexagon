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
