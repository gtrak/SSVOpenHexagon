# Task K7 — one PCG stream, not two: third-party levels' `onRenderStage` must consume the SAME RNG the C++ does

Read `.agents/WORKER-HYGIENE.md` first (§11: the pinned C++ lives in `~/.cache/ohoracle-deps/`, the repo's
extlib submodules are empty).

## The deviation (register row K7 — read it in `.agents/plans/001-rust-port/deviations.md`)

**the C++ has ONE global `pcg32` for gameplay AND cosmetics; the port SPLITS `cosmetic_rng`.** A level
whose `onRenderStage` draws random numbers therefore consumes the port's COSMETIC stream, while in the C++
those draws advance the SAME stream the gameplay draws from — so such a level diverges from the original
(and a replay of it diverges too). The register notes this affects *third-party content only*: the
shipped corpus does not appear to draw random numbers from the render hook, which is exactly why the
3627-frame C++ fingerprint corpus still passes.

## Why this is worth fixing, and why it is bounded

Replay determinism is the port's first-class promise, and this is a place where content that the original
handles correctly would not replay identically. The fix is narrow: make the cosmetic draws consume the
SAME stream as the gameplay draws, as the C++'s single global `pcg32` does. The port's shape today is a
seam — `cosmetic_rng: Arc<dyn CosmeticRng>` in `crates/oh-game/src/hexagon_game.rs:433`, used at
`hexagon_game.rs:1946/2476` and throughout `crates/oh-game/src/update.rs` (~:1329-1349 and neighbours) —
so decide deliberately whether you remove the seam or re-point it at the game's one RNG, and say which and
why. Do NOT introduce a third stream; the point is that there is exactly one.

## Acceptance — the oracle is already in the repo, so this is verifiable, not hopeful

1. **The C++ fingerprint corpus must still pass unchanged**: `.agents/plans/001-rust-port/parity/
   cpp_fingerprints.json` is the C++-generated column (030b) and the parity test compares the port against
   it (see `crates/oh-parity/tests/parity_cpp_fingerprints.rs`). If your change perturbs the corpus, STOP
   and report — that would mean the corpus DOES consume cosmetics, and the correct conclusion changes.
2. **Prove the property with a test**, not by inspection: a level (or a direct test) whose `onRenderStage`
   draws a random number must advance the SAME stream the gameplay reads — e.g. draw in the render hook,
   then assert the next gameplay draw equals what the C++'s single-stream order produces. A test that
   simply re-asserts the old split is worthless here.
3. Update the K7 register row (CLOSED with your landing hash, or still open with the new evidence) — and if
   the corpus DOES turn out to consume cosmetics, the row needs the opposite verdict and I need to know
   first.

## Boundaries

`crates/oh-game/` (the RNG plumbing and its call sites) plus the register row and the tracker. Do NOT
change the RNG algorithm or its seeding (that is 030b's pinned territory), do not touch the shipped
corpus, and do not "fix" this by special-casing the render hook. Host gate before each commit (it includes
the parity stage), `cargo check --target aarch64-linux-android` if you touch shared code, explicit-path
commits, `timeout` everywhere, and append a `## Report` with the verdict, the property test, and the
corpus result.
