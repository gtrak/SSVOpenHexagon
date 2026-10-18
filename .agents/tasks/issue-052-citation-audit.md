# Task 052 — re-derive the register's C++ CITATIONS (two rows were wrong about the original this week)

Read `.agents/WORKER-HYGIENE.md` first — **§11 is the whole method here**: the pinned C++ is in the MAIN
checkout for `cpp-original/src` and `cpp-original/include`, the extlibs are SUBMODULES that are empty in
your worktree (and not all of them: some are populated in the main checkout), and the authoritative copy
of anything CPM-pinned is the oracle's dep cache `~/.cache/ohoracle-deps/<name>-fork/`.

## Why this exists (a pattern, not a hunch)

Two register rows were found wrong about the C++ IN ONE WEEK, and both by lanes that happened to read the
source themselves:

- **K2** cited SFML pin `2f11710` at `Window.cpp:372-384` and `:440`. The real pin is `dacceddbd` and the
  limiter is at `:207-219`. The described *logic* matched; the citation did not. (049 fixed it.)
- **K7** claimed "the C++ has ONE global pcg32 for gameplay AND cosmetics". The C++ actually has two
  streams — the replay-seeded `HexagonGame::rng` that the level random API is bound to, and a SSVUtils
  `thread_local` `getRndEngine()` for cosmetics — and the port already mirrored both. The row's PREMISE
  was wrong, and a lane following it would have BROKEN parity. (K7 closed as non-existent.)

042's sweep verified the rows' DISPOSITIONS (still true / now fixed / stale) and did it well. Nothing has
ever re-derived their **citations**. That is this task, and it is bounded deliberately.

## Mission

For each register row that makes a claim ABOUT THE C++ (as opposed to a claim about the port's own state),
open the cited source and verify three things: (a) the file and line exist; (b) the construct is what the
row says it is; (c) the claim's DIRECTION is right — "the C++ reads X here", "the C++ never calls Y" — is
what the code actually does. A citation that resolves but says the opposite of the claim is the worst
kind, so check what the code DOES, not that the line number is plausible.

Scope, in priority order (a complete pass over 295 rows is not the goal; the load-bearing claims are):
1. **The K-section in full** (6 rows) — it is short, load-bearing, and 2 of 6 have already proven wrong.
2. **The F-section (31 rows)** — every F row asserts something the C++ does or does not do; these are the
   register's "sanctioned deviations" and they steer future lanes.
3. **Numbered rows whose text asserts C++ behaviour** — find them by grepping the register for "the C++"
   and similar phrasings rather than reading all 260. State how many you found and how many you checked.

## Output and acceptance

- A table in the report: row · the citation · verdict (VERIFIED / WRONG-citation / WRONG-claim / cannot
  check) · the correction where needed. Counts, so the reader knows the coverage: N verified, M corrected,
  K unverifiable and why.
- **Correct the register in place** where a citation is wrong (it is a living document — 042 established
  that) and say in the row what changed and when. Do NOT change code: if a wrong citation turns out to
  hide a real PORT bug, that is a FINDING to report, not a fix to make in this lane.
- Do not re-litigate dispositions (042's ground) and do not renumber rows.

Boundaries: `.agents/plans/001-rust-port/deviations.md` + a report in this spec. No code changes. Bound
every command with `timeout`, and where a source is genuinely unreadable (an uninitialised submodule at a
different ref) say so instead of guessing — "cannot check, and here is why" is a valid and useful verdict.
