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
## Report

Lane 038, branch `lane/038` (cut from master `99661e72`). Four commits:
`56be2f19` (the move), `fa36135b` (tools), `15027a84` (the new root README),
`154da3dd` (DECISIONS D-038 + mapping/hygiene lines). Explicit paths staged
everywhere; no `git add -A`; no C++ source content modified (the move commit is
100% renames + 3 symlinks + `.gitmodules` + a `.gitignore` entry; verified by
`git log --stat` and by the deinstrument round-trip below). Mid-run scope
extension from the operator (restate of the supervisor ruling): also move the
remaining non-Rust root entries — including the upstream `README.md` — into
`cpp-original/`, and write a new port `README.md` at the root (its own commit).

### 1. Per-entry move decisions

**Moved into `cpp-original/`** (the move commit, 626 files, all 100% renames):

| Entry | Decision | Why |
|---|---|---|
| `src/`, `include/`, `extlibs/` | move | spec's explicit move list; the citation/oracle heart. `extlibs`'s 5 gitlinks moved with the tree; `.gitmodules` paths updated in the SAME commit (`git mv` did 5 of 6 itself; the orphan `extlibs/SFML` entry — no gitlink, the build uses the CPM SFML fork — was updated by hand to follow its siblings). |
| `CMakeLists.txt` | move | spec's list; the oracle's `-S` target. Its content was NOT edited (move, not edit) — including its `${CMAKE_SOURCE_DIR}/_RELEASE/*.so` link inputs, which is what forces the internal pointer below. |
| `build.sh`, `buildlx/`, `buildrel/`, `clangformat_everthing.sh`, `prepare_release.sh`, `prepare_release_linux.sh`, `run_prepared_release_test.sh` | move | spec's list (upstream C++ dev/release scripts). |
| `test/` | move | CMake `add_subdirectory(test)` — must sit next to the moved CMakeLists for the build to work; holds `test/fixtures/cube_1_apeirogon.ohr.z` (the `tools/package.sh` smoke fixture — the script updated to the new path). |
| `public/` | move | CMake source glob (`public/discord/*`, `public/sqlite/*.c`) + include dirs — build input. |
| `art/` | move | CMake icon `.res` inputs (WIN32) + the upstream art (incl. the game's credits textures). |
| `old/`, `misc/`, `webpage/`, `vbox/`, `server/`, `release_checklist.md` | move | upstream C++ machinery/docs (spec default: "upstream/C++ machinery moves"). |
| `build/` | move | upstream C++ dev-build scripts; holds `copyArtifacts.bash` (the reference file lost once by an `add -A` — it moved intact, 100% rename). Operator's scope extension explicitly listed it; the orchestrator's "leave it at root" alternative was available and NOT taken. |
| `README.md` (upstream) | move to `cpp-original/README.md` | operator's ask; kept as archival reference, not deleted. |
| `.clang-format` | move | the C++ clang-format config (used by the moved `clangformat_everthing.sh`); no port-side reference. |

**Moved to `.agents/`**: `ISSUE-019-REPORT.md` → `.agents/ISSUE-019-REPORT.md`
(port-authored record, not upstream; zero citations churned — verified by grep
before moving; the orchestrator offered root as the alternative, not taken).

**Stayed at the root**: `crates/`, `tools/`, `Cargo.toml`, `Cargo.lock`,
`rust-toolchain.toml`, `.github/` (lane 037's territory — untouched), `.agents/`,
`.cargo/`, `target/` (gitignored), `LICENSE`, `CODEOWNERS`, `.gitignore`
(repo-level machinery; a root `.gitignore` is what keeps port-side ignores like
`target/` and `crates/oh-parity/oh030b-corpus/` working — moving it into
`cpp-original/` would have orphaned them), and **`_RELEASE/`** — the shared game
data, a real directory, untouched (gate green is the proof the Rust tests and
`tools/package.sh` still read it where they always did).

### 2. The symlink decision and its verification

**Root citation symlinks — exactly two: `src -> cpp-original/src` and
`include -> cpp-original/include`** (committed as mode-120000 entries; targets
verified byte-for-byte via `git cat-file`). The load is the citation corpus:
at this HEAD, 344 live citation-form references in the four corpora (plans 133,
crates 117, audits 52, tasks 42 — the spec's "325" re-measured), including the 52
audit citations that are IMMUTABLE by hygiene §7. The orchestrator ruled the
set minimal and declined a third root symlink for the legacy C++ CI; that
ruling is recorded in D-038.

**`cpp-original/_RELEASE -> ../_RELEASE` — C++-side BUILD GLUE, not a citation
symlink.** Approve-and-document (supervisor Q1 ruling): the moved CMakeLists
links `${CMAKE_SOURCE_DIR}/_RELEASE/{libsteam_api,libdiscord_game_sdk,
libsdkencryptedappticket}.so` (the tracked loader stubs) on every non-Android
build and its content may not be edited, so without the pointer the oracle
BUILD fails at link. Requirements, each executed:
1. *Traverses from inside the container mount*: `docker run -v $REPO:/src:ro`
   → `readlink -f /src/cpp-original/_RELEASE` = `/src/_RELEASE` (target inside
   the mount), ELF bytes read through it, and
   `cmp /src/_RELEASE/config.json /src/cpp-original/_RELEASE/config.json`
   byte-identical. (Check: the oracle image `oh-cpp-oracle:1`, a 300s-bounded
   `sh -lc` script.)
2. *Post-build copy lands in the REAL root `_RELEASE/`*: proven by write-
   through — a file written to `cpp-original/_RELEASE/` appeared in the root
   `_RELEASE/` (and was removed again). And `.gitignore` still covers the
   C++-installed binary set: `git check-ignore` positive for
   `_RELEASE/SSVOpenHexagon`, `SSVOpenHexagonLinux`, `OHWorkshopUploader`,
   `OHWorkshopUploaderLinux`, `OHServerControl`, `OHServerControlLinux` — a C++
   build cannot leave commit-able artifacts in the data dir.
3. *Recorded as build glue*: D-038 states it explicitly, in its own bullet,
   distinct from the two citation symlinks and "not a second source of truth
   for the game data".

**Verification of the root symlinks (operator's ask):**
- *Plain read*: `head src/SSVOpenHexagon/Core/main.cpp` / `head
  include/SSVOpenHexagon/Core/HexagonGame.hpp` — host and inside the container
  mount.
- *`grep -r`*: `grep -rln "namespace hg" /src/include` inside the mount = 79
  files = the real path's count (a command-line symlink argument is followed
  by `grep -r`; from the mount root, recursion does NOT follow the `src`
  symlink — the real tree is found via `cpp-original/`, so nothing is missed
  and nothing is double-counted).
- *git tooling boundary (found, not worked around silently)*: `git apply`
  REFUSES paths that traverse the root symlinks ("affected file ... is beyond
  a symbolic link" — verified on the moved tree). The fingerprint patch is
  therefore applied with an explicit prefix: `git apply
  --directory=cpp-original` in `run.sh instrument/deinstrument`.
  `fingerprint.patch` itself is 030b's untouched artifact (hunk content
  unchanged, zero path edits in the patch file); `git apply --check
  --directory=cpp-original` passes against the pristine moved tree, and so did
  the baseline `git apply --check` pre-move.

### 3. Tool updates (commit `fa36135b`)

- `tools/cpp-oracle/run.sh` — `init`: submodules now
  `cpp-original/extlibs/*`; `configure`/`build`: `cmake -S /src/cpp-original`;
  `instrument`/`deinstrument`: `git apply [--directory=cpp-original]` (the
  symlink refusal above); header + Dockerfile comments state the new source
  dir and the `_RELEASE` pointer. `seed`/`run` unchanged in substance (`run`
  reads `/src/_RELEASE` — the root, which stayed put).
- `tools/oracle/run.sh` + `tools/parity.sh` — FLAGS (`-I…/include`,
  `-I…/cpp-original`, the three extlibs include dirs) and every real-source
  path now canonical `cpp-original/…` (tools use the real tree; the symlinks
  are for the citation corpus, not tool mechanics).
- `tools/package.sh` — the committed replay fixture path
  (`cpp-original/test/fixtures/cube_1_apeirogon.ohr.z`) + the
  `release/README.md` pointer (the docs dir moved).
- `tools/oracle/db_probe.cpp` — the manual build-recipe comment updated to
  post-reorg paths (comment only; the probe itself is untouched).
- `tools/k3-luajit-hang/build.sh` — CHECKED, NO CHANGE NEEDED: it references
  only the CMakeLists.txt CPM *pin* (a citation, not a path dependency); no
  C++ tree paths. Same for `tools/lua-parity.sh`, `tools/gate.sh`,
  `tools/package-android.sh`, `tools/build-windows.sh` (grep sweep for
  `src/SSVOpenHexagon`, `include/SSVOpenHexagon`, `extlibs`, `CMakeLists`,
  `test/fixtures`, `buildlx/buildrel`, `prepare_release`, `/src`).
- `.gitignore` — one entry added (with a comment): `cpp-original/build/*`
  (the upstream `build.sh`'s `BUILD_DIR` after the move; mirrors what
  `/build/*` did; tracked `.sh` files unaffected). The pre-existing
  `extlibs/luajit-src*` entries are dead (D-037 removed the dir) and were left
  alone; the pre-existing NUL-byte-corrupted entry at line 234 (`*.ao<NUL>h…`)
  is upstream damage, left alone and noted here.

### 4. Submodules

Moved with the tree; `.gitmodules` updated in the same commit as the gitlinks
(same-commit requirement met). `git submodule status` before: 5 entries `-`
(uninitialized); after `tools/cpp-oracle/run.sh init` (the updated script): 5
entries checked out at the pinned SHAs under `cpp-original/extlibs/`, no
errors. The worktree-local `.git/config` `submodule.extlibs/*` sections were
removed before `init` so only the new-path registrations exist. The orphan
`extlibs/SFML` `.gitmodules` entry (no gitlink — the build uses the CPM
`vittorioromeo/SFML` fork, per run.sh's WALL notes) follows its siblings'
paths; it has no gitlink, so it affects neither `git submodule status` nor
`init`.

### 5. The oracle, end to end, at the new path

All of it ran — nothing was substituted by a lighter proof:
1. `init` — 5 submodules at `cpp-original/extlibs/*` (network clones, bounded).
2. `seed` — all 6 CPM pins by SHA into the lane's own build dir
   (`/home/gary/dev/oh-lanes/oh-cpp-build` — note: a lane worktree's computed
   `BUILD` is `…/oh-lanes/oh-cpp-build`, NOT the old main-checkout's
   `…/oh-cpp-build`; the first configure attempt failed on that, the reseed
   is what the run below used).
3. `configure` — `cmake -S /src/cpp-original -B /build` (container, bounded
   900s): GREEN; all extlibs found under `/src/cpp-original/extlibs/…`;
   `CMAKE_HOME_DIRECTORY:INTERNAL=/src/cpp-original`; the `_RELEASE/*.so`
   link inputs resolved through the committed pointer.
4. `instrument` — `git apply --directory=cpp-original` (the 7 files, exactly
   the patch's set).
5. `build` — full instrumented build (SFML + LuaJIT + the game + the two
   tools; ~3 min on 24 cores; 326/326; bounded 5400s background process).
6. **Fingerprint run**: all six corpus replays through the instrumented
   binary (`run -- -headless /build/corpus/<n>.ohr.z -fingerprint
   /build/fp/<n>.fp.txt`): **3627/3627 frames MATCH the committed C++ column**
   (`cpp_fingerprints.json`: apeirogon_spin439 439, autotest0_spin1180 1180,
   arcadia_spin456 456, bipolarity_spin358 358, apeirogon_spin439_dm25 408,
   autotest0_mixed 786).
7. `deinstrument` — reverse-applied; `git status` shows the `cpp-original/`
   tree byte-identical to the committed state (pristine, as required for the
   citation reference).

### 6. Gate, citations, mechanical checks

- `bash tools/gate.sh` GREEN (5/5 stages) — run after the move commit AND
  again on the final tree (both `GATE_RC=0`).
- `tools/parity.sh` GREEN at the new paths (probe compiles the real moved
  sources; the three C++-derived corpora byte-identical; `oh-parity` green).
- Citation sample, method stated: `grep -rhoE` the citation form
  `(src|include)/(SSVOpenHexagon|OHServerControl|OHWorkshopUploader)/….(cpp|hpp)`
  across the four corpora → 116 distinct files → `shuf -n 20` → `test -f` from
  the repo root (exercising the symlinks): **19/20**. The one miss is
  `src/SSVOpenHexagon/Utils/Config.cpp` — a PRE-EXISTING dead citation in
  `.agents/tasks/issue-029m-batch-mechanical-config.md`: the real file is
  `src/SSVOpenHexagon/Global/Config.cpp` (it exists and resolves through the
  symlink); `Utils/Config.cpp` never existed, so the move did not break it.
  (A re-sample of the surviving corpus is 20/20 — the first pre-final-commit
  sample was 20/20.)
- `git status` clean; `git submodule status` error-free; object count went
  5443 → 5530 (the symlinks/docs are the only new blobs) and the pack is
  unchanged at 86.53 MiB — no accidental copies; the working tree grew only
  by the checked-out submodules (expected, `.gitignore`d in substance as
  before).
- Image reads: **zero** (the lane's one-read budget was not spent; all art
  questions were answered by text).

### 7. What I could NOT verify (stated, not reasoned away)

- **The upstream C++ CI** (`.github/workflows/ci_linux.yml`,
  `ci_windows.yml`) — the C++ project's own workflows (lane 037 owns
  `.github/`; the operator: "actions should work out i can fix it"). They
  assume a root `CMakeLists.txt` and `cd build && cmake ..`; after the move
  `cd build` resolves via the real `cpp-original/build/` dir (no symlink
  there, per the orchestrator) and `cmake ..` points at `cpp-original/`, which
  HAS the CMakeLists — so the configure step MIGHT keep working, but I did
  not run the workflows, and `build/` now holds the tracked dev scripts the
  CI would pollute. Recorded as a known consequence (D-038), not fixed.
- **Windows-runner symlink materialisation** for the two root symlinks:
  UNVERIFIED (no Windows checkout exercised; a plain Windows `git` checkout
  may materialise `src`/`include` as text files unless developer mode /
  `core.symlinks` cooperate).
- **A `cmake --install` into `_RELEASE/` from a real C++ build**: proven by
  write-through + gitignore coverage + the link-input resolution, not by
  running the install (the oracle mount is read-only by design; the host has
  no SFML/X11 deps to host-configure).
- **The `db_probe` schema recipe** — comment-updated only; the probe itself
  was not re-run (it needs the vendored `public/sqlite*` + `-lsqlite3`;
  `tools/oracle/run.sh`'s probes, which DO compile the real moved sources,
  were exercised via `tools/parity.sh`).
- **`prepare_release*.sh` / `build.sh` (the moved upstream scripts)**: moved
  intact but not executed (they target the original developer machine
  layout); their CWD-relative `_RELEASE` assumptions now resolve only when
  run from the repo root (upstream scripts; reference status, not tooling).
