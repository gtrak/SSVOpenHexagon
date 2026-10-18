# Task 056 — the music content: the operator has now asked three times

Read `.agents/WORKER-HYGIENE.md` first, then 039's decision-A section in
`.agents/tasks/issue-039-android-input-music.md` (the supervisor decision this revisits) and the
register's F-row for it.

## Context, stated honestly

039 root-caused the silence and it is NOT a code bug: the port's music path is correct, the volume chain
is healthy, and the packaged `_RELEASE` contains ZERO music OGGs — exactly like upstream, which never
shipped them either (only the `Music/*.json` metadata). I decided then (option A) to mirror upstream and
add nothing, because committing 10-25 MB of third-party OHRM audio to a public repo is a licensing call
that is not the orchestrator's to make. **The operator has since said "i hear no music" three times.**
That is the answer to the licensing question: they want it. This lane acts on that.

## Mission

Add the real music content to `_RELEASE` the way a player would get it, with the provenance recorded:

1. **Find the official source first.** The OHRM (Open Hexagon Resurrection Music) packs come from the
   official distribution — Vittorio Romeo's site / the project's own release channels. Prefer the
   official source by a wide margin, and record EXACTLY what you fetched: the URL, the version or date,
   and the license or terms you could find. If the official source is gone or the license is unclear,
   STOP and report the options rather than substituting an unofficial mirror — that is a licensing
   decision that belongs to the operator, and a wrong call here is worse than silence.
2. **Place them where the port already looks** (`_RELEASE/Packs/<pack>/Music/` per `Music/*.json`), and
   verify the port actually plays them: the port's music path resolves the segment and logs the resolved
   path, so the log line that 039 quoted (`No path for music id …`) must be replaced by real playback on
   the surfaces you can reach — the web build via the probe (a decoded-buffer observable, as 041's work
   established), and on Android via logcat if the phone's ADB is available (it was, as of lane 051).
3. **Keep the repo honest**: state the total size added, and add a short note (README or a doc) naming
   the source and the license, so a future reader knows where the audio came from and under what terms.
   This is a content addition, NOT a code change — if the port needs a code change to play the files, that
   is a finding to report, not a licence to start refactoring the audio path.
4. **Update the register**: 039's row and the F-row it produced should point at this lane's outcome, so
   the "content gap, upstream-faithful" statement does not stand as the last word once the content is in.

## Acceptance

The files present and installable, the source + license documented with the exact provenance, playback
observed on at least one surface (with the observable named), the total size stated, and the register
updated. If you must stop at step 1 for a licensing reason, that is a valid and useful outcome — report
it with what you found and what the options are, and do NOT commit the audio.

Boundaries: `_RELEASE/` content plus docs/register. Do not change the audio code path, do not touch the
touch overlay or the web shell (lanes 054/055 own those), and keep every download bounded with `timeout`
and its size checked before committing.

## Report (2026-10-18 — lane 056, worktree `oh-lanes/056`, branch `lane/056` off master `f6dec6f1`)

**Scope changed mid-lane (operator decision, via the orchestrator): DEFER the music; document it.**
The original mission (find the official source, place the OGGs, verify playback, record
provenance) was superseded *before any audio was fetched*: the operator said "defer the
music and document the issue for me to read later." No .ogg, no archive, no pack content
was downloaded or committed by this lane; only research web pages were fetched (to a temp
dir, since deleted). The deliverable is the **decision brief at `docs/music-content.md`**
(plus the one-line README pointer and the register/tracker notes below).

### What the brief records (verified 2026-10-18 in this worktree)

Tree facts, each re-verified here rather than taken on faith:
- the code is **AFL-3.0** (root `LICENSE`, © 2013-2016 Vittorio Romeo);
- the SFX carry their own licence (`_RELEASE/Assets/Open Hexagon Assets License.txt`,
  © 2020 LukeMans / Luuk Zellenrath): it names exactly seven files, permits "the game
  Open Hexagon" only, forbids commercial use, and prohibits reuploads — redistribution
  travels *as part of the game, with the terms alongside*; the other shipped SFX OGGs
  are not even named in that file;
- the MUSIC has **no stated terms anywhere in the tree** (the 24 `Music/*.json` are
  metadata only; artists BOSSFIGHT ×13, Fantomenk ×7, Dunderpatrullen ×4; zero OGGs);
- upstream's own `cpp-original/old/_DOCUMENTATION/README_MUSIC.html` is a modder's
  guide ("Simply place your music file in this folder") — the original never
  redistributed the tracks, so their absence is the upstream distribution model.

The research value this lane added (the mission's step 1, completed read-only):
- **Official source found: `vittorioromeo/SSVOpenHexagonAssets`** (the project
  author's own GitHub repo, "Assets for SSVOpenHexagon"; default branch `master`,
  last push 2022-08-23, live on 2026-10-18 via the GitHub API). It carries the full
  `_RELEASE` tree **with all 21 music OGGs beside their `.json`s** — totalling
  **62,079,210 bytes (62.08 MB)**: cube 8 / 13,722,330 B, hypercube 10 / 29,349,893 B,
  orthoplex 2 / 15,001,902 B, tutorial 1 / 4,005,085 B. This **corrects the earlier
  10–25 MB estimate** (the brief states both). One quirk: `jackRussel.ogg` ships only
  under `cube/`, but `artwork/`, `experimental/` and `workshopexample/` each carry a
  `jackRussel.json` — a complete install copies that one file into those three `Music/`
  dirs.
- **Its terms: NONE.** GitHub licence detection reports no licence; the README is two
  lines; the only licence file in the whole tree is the SFX one. The brief records
  this as the load-bearing fact: the official source of the music carries no licence
  text, and an unclear licence is not a permission.
- Cross-checks recorded in the brief: the project's official wiki
  (SuperV1234/SSVOpenHexagon `Building-Instructions`, linked from the upstream README)
  says verbatim *"If you want music, copy assets from the Steam version of the game"*
  (store.steampowered.com/app/1358090 — store page live 2026-10-18; the EULA text is
  not retrievable from this environment and a Steam EULA licenses playing, not
  redistributing); the newest public upstream ref (branch `2.1.4`, 917 files) still
  carries 14 SFX OGGs + 23 `Music/*.json` and zero music OGGs (the model is intact);
  Vittorio's former site channel is unreachable (`vittorioromeo.it`: no DNS here and
  **zero** Wayback Machine snapshots; `vittorioromeo.info` now hosts an unrelated C++
  course site; the `openhexagon.org` the current upstream README links to is an
  unrelated WordPress blog), so the "official OHRM distribution, `vittorioromeo.it`"
  named in 039 could not be reached, and the expansion "Open Hexagon Resurrection
  Music" could not be independently confirmed.
- **Could not establish** (stated, not guessed past): the Steam EULA text; whether
  BOSSFIGHT / Fantomenk / Dunderpatrullen have ever published their own terms for
  these tracks.

Options + recommendation (in the brief): (A) commit the 21 OGGs + a licence notice —
blocked until terms permitting redistribution exist, and would make this repo a
distributor (+62 MB per clone, a deliberate content divergence from upstream);
(B) do not commit, document how players add the music — **upstream's own model**,
zero licensing risk; (C) fetch at install/first-run — does not avoid the issue (the
artifacts/target still distribute the tracks under the same unknown terms);
(D) ask the artists — the only path that creates permission. **Recommendation: (B)
for now; (A) only once redistribution-permitting terms are in hand.**

### Repo changes (this commit; explicit paths, docs-only)

- **NEW `docs/music-content.md`** — the decision brief (≈5-minute read).
- `README.md` — one line in the Credit section: the music-credit bullet no longer
  claims the tracks "ship in `_RELEASE/`" (they don't) and points at the brief.
- `.agents/plans/STATUS.md` — row 39: symptom 3 is now "Revisited 2026-10-18 (056,
  operator deferral): DEFERRED, not closed … the brief is at `docs/music-content.md`";
  new Issues row 056 recording the deferral, the brief's path and its findings.
- `.agents/plans/001-rust-port/deviations.md` — row 4 (the `android.rs:46` NoopAudio
  row, the register's 039 music pointer): the parenthetical now notes the deferral,
  the brief's path, and the open licensing call.
- `.agents/tasks/issue-056-music-content.md` — this report.

No audio, no code changes, no `_RELEASE/` content touched; host gate not required
(docs-only commit). The original spec's playback-verification half (web probe /
Android logcat) did not run — it is moot while the audio is deferred; the 039
evidence stands (the port logs the resolved segment and the `No path for music id …`
line, so the first observable after a future (A) is that line giving way to a
decoded-buffer line on web and a `playLevelMusic` line on Android).
