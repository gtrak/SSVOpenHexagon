# Task 038 — move the C++ tree into `cpp-original/` without breaking the oracle or the citation corpus

Read `.agents/WORKER-HYGIENE.md` first. Operator request, verbatim: *"move all the c++ folder
structure into a cpp-original subfolder."*

**This is a repository reorganisation whose risk is concentrated in ONE place: the C++
tree is the project's citation reference and the oracle's source.** Break either and the
port loses the evidence that justifies every fidelity claim. Read this whole spec before
moving anything.

---

## 1. What moves, what stays (classified by owner)

**Move into `cpp-original/`:**
`src/`, `include/`, `extlibs/` (the C++ submodules), `CMakeLists.txt`, `build.sh`, `buildlx`,
`buildrel`, `clangformat_everthing.sh`, `prepare_release.sh`, `prepare_release_linux.sh`,
`run_prepared_release_test.sh`.

**Stay at the root (the Rust port):** `crates/`, `tools/`, `Cargo.toml`, `Cargo.lock`,
`rust-toolchain.toml`, `.github/`, `.agents/`, `target/`.

**MUST STAY — shared by both, and moving it breaks the port:** `_RELEASE/`. It is the game
data the Rust tests, the packaging (`tools/package.sh`) and the release layout all read. Do
not move it, do not symlink it.

**Classify and decide, reporting each decision:** `art/`, `build/` (it holds the C++
`copyArtifacts.bash` — a reference file that must not be lost), `CODEOWNERS`, `LICENSE`,
`README.md`, `ISSUE-019-REPORT.md`, `misc/`, `old/`, `public/`, `release/`,
`release_checklist.md`, `server/`, `test/`, `vbox/`, `webpage/`. The default is: upstream/C++
machinery moves, repo-level files (`LICENSE`, `README.md`, `CODEOWNERS`) stay. Say what you
did per entry; if a directory turns out to be used by the Rust build or the packaging, it
stays.

## 2. THE LANDMINE — the citation corpus (325 citations)

Measured now: `.agents/plans` **123**, `crates/` comments **108**, `.agents/audits` **52**,
`.agents/tasks` **42** — all of the form `src/SSVOpenHexagon/…` or `include/SSVOpenHexagon/…`.

- **`.agents/audits/*.md` are IMMUTABLE by project rule** (hygiene §7: "do not rewrite
  `.agents/audits/*.md`"). So those 52 citations cannot be updated, and neither can the ~300
  others be cheaply rewritten without touching files that are supposed to be a record.
- **Therefore: after moving, leave ROOT SYMLINKS `src -> cpp-original/src` and
  `include -> cpp-original/include`.** Every existing citation then still resolves — which is
  the whole point of a citation — and the tree is still reorganised as the operator asked.
  Verify a symlink traverses correctly for a plain read, for `grep -r`, and from inside the
  oracle container's mount (a symlink whose target is inside the same mounted tree is fine; a
  symlink pointing outside it is not).
- Record the decision (a `DECISIONS.md` entry: the reorg, the symlinks, and WHY — the
  immutable-citation rule) and add a line to the mapping docs / hygiene so a future reader
  knows the C++ root is `cpp-original/`.
- If the symlink approach breaks any tool in a way you cannot fix, **stop and report** with
  the specific breakage rather than silently abandoning the citations.

## 3. Every tool that points at the C++ tree must be updated

`tools/cpp-oracle/run.sh` (it mounts the repo and configures CMake with `-S /src`),
`tools/oracle/run.sh` (compiles the REAL C++ sources against shims — the oracle probes),
`tools/k3-luajit-hang/build.sh`, `tools/parity.sh`, and anything else the grep above found.
Also: **`tools/cpp-oracle/fingerprint.patch`** — the instrumentation patch is written against
`src/SSVOpenHexagon/…` paths, so either regenerate it against the new tree or make the
application step explicit about the prefix; it MUST still apply to a pristine checkout after
the move (`git apply --check` is the test). Do not touch the patch's *content*; only its
paths, and say exactly what you changed.

**Submodules:** `extlibs/` are six `.gitmodules` entries. Move them with the tree AND update
`.gitmodules`' paths in the same commit, or — if that proves fiddly — leave `extlibs/` where
it is and report precisely why (a partially-moved tree with a stated reason beats a broken
submodule state; a tree where `git submodule status` errors is unacceptable). The C++ oracle
container's `init` mode checks these out, so it must still work either way.

## 4. Verification (all of it — this is the bar)

1. **The oracle still works end to end**: the container configure (`tools/cpp-oracle/run.sh
   seed/init/configure`) at the NEW source path, the fingerprint patch applying to the
   pristine moved tree, and a fingerprint run producing vectors. If the container work is too
   slow, at minimum prove the patch applies and the CMake source path resolves — and say
   which parts you did not run.
2. **The citations still resolve**: mechanically check a SAMPLE (say 20 randomly chosen
   citations across the four corpora) resolve to real files after the move, and state the
   method.
3. **The Rust gate is unaffected**: `bash tools/gate.sh` green (it should not care, but prove
   it — the move touches the repo root, and `_RELEASE` staying put is exactly the kind of
   thing that silently breaks a test).
4. `git status` clean, `git submodule status` not erroring, and the repo size not doubled by
   accidental copies.
5. State explicitly what you could NOT verify.

## 5. Hygiene

One commit for the move (a pure `git mv` commit is ideal and keeps the history legible),
then separate commits for the tool updates, the docs/DECISIONS entry, and the report —
**explicit paths, never `git add -A`** (a root-level move is exactly where a stray `add -A`
would sweep something it should not). `cargo fmt` if you touch Rust. Bound every process with
`timeout`. At most ONE image read. Append a `## Report` to this file: the per-entry move
decisions, the symlink decision and its verification, the tool updates, the oracle result,
and the honest gaps.

## Addendum (operator, same session): move the README out, and write OUR OWN

Operator, verbatim: *"also move out the top level readme and other non rust folders and add our
own readme with port description, method and features, and credit to the original."*

- **The top-level `README.md` is the C++ project's** — move it to `cpp-original/README.md`
  (preserved, never deleted) and move every other remaining upstream directory at the root
  (`art/`, `misc/`, `old/`, `public/`, `release/`, `server/`, `test/`, `vbox/`, `webpage/`,
  `build/`, …). `ISSUE-019-REPORT.md` is PORT-authored: it goes under `.agents/`, not into the
  C++ archive.
- **Stays at the root:** `LICENSE` (it governs the whole repository, port included),
  `CODEOWNERS`, the port's entries, `_RELEASE/`, and the NEW `README.md`.
- **The new root `README.md`** carries the operator's four sections — **port description**
  (a literal 1:1 Rust port, replay-determinism and on-disk/wire compatible, with the C++
  reference in `cpp-original/`), **method** (file-by-file mirroring rather than a rewrite; the
  frame-level parity proof against the instrumented C++ — 3627 frames across 6 cases plus the
  43-case Lua A/B corpus; the gate; the F1..F30 deviations register; the `file:line` citation
  rule and the root symlinks that keep citations resolving), **features** (the game's modes,
  packs, Lua levels, replays, the online stack, Steam/Discord, the console; Linux/Windows/
  Android/wasm; one Lua VM since D-037; build/test pointers at `tools/gate.sh`,
  `tools/package.sh`, `tools/package-android.sh` rather than invented instructions) and
  **credit** (SSVOpenHexagon by **Vittorio Romeo**; based on Super Hexagon by **Terry
  Cavanagh**; music by **FantomenK**; the pack authors/artists; the inherited license) — the
  boot credits screen the port renders is the source of truth for those names.
- **No overclaiming**: the README must state the platform/verification distinctions it cannot
  honestly blur, and the "1:1" claim must point at the oracle that backs it.
