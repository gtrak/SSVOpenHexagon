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
