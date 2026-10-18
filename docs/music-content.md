# Music content — decision brief (lane 056, 2026-10-18)

**Status: deferred by the operator (2026-10-18). No music has been added to this repo.**
This is the write-up the deferral asked for: the facts, the official-source research,
the options, and a recommendation. Read it, then decide.

## The situation

The game plays level music whenever the pack's `Music/*.ogg` exists next to its
`Music/*.json` metadata (the port's path is verified correct — lane 039 proved it on
device: the volume chain is healthy, the port selects the segment and honestly logs
`No path for music id …`). The shipped `_RELEASE` carries the metadata for 24 tracks
but **zero music OGGs** — exactly like upstream, which never shipped the tracks from
source either. That is why the repo has no music: it is the upstream distribution
model, not an oversight. The operator said "i hear no music" three times, then
deferred the question with this brief.

## What the tree says (all verified 2026-10-18 in this worktree)

- **The code** is the **Academic Free License v3.0**, © 2013-2016 Vittorio Romeo
  (`LICENSE` at the root).
- **The sound effects ship with their own licence**:
  `_RELEASE/Assets/Open Hexagon Assets License.txt` — © 2020 LukeMans
  (Luuk Zellenrath). It names exactly seven files (`beep.ogg`, `death.ogg`,
  `error.ogg`, `increment.ogg`, `restart.ogg`, `select.ogg`, `swap.ogg`), grants them
  "for the game Open Hexagon" only, forbids commercial use, and states
  "**Reuploads of the sound effects are prohibited**" — i.e. they travel as part of
  the game, under terms that travel alongside. (The other shipped SFX OGGs —
  `gameOver`, `go`, `levelUp`, `openHexagon`, `personalBest`, `difficultyMult*`,
  `blip`, `swapBlip` — are not even named in that file; their terms are unstated too.)
- **The music has no stated terms anywhere in the tree.** The 24 `Music/*.json` files
  are metadata only (`id`, `file_name`, `name`, `album`, `author`, `segments`); the
  artists are BOSSFIGHT (×13), Fantomenk (×7) and Dunderpatrullen (×4). No licence
  field, no licence file, no terms.
- **Upstream's own documentation confirms the model**:
  `cpp-original/old/_DOCUMENTATION/README_MUSIC.html` is a *modder's guide* —
  "Simply place your music file in this folder" — it tells the player where to put
  *their* tracks and supplies none.

## Where the official music actually lives (research, 2026-10-18)

- **The official source found: [`vittorioromeo/SSVOpenHexagonAssets`](https://github.com/vittorioromeo/SSVOpenHexagonAssets)** —
  the project author's own GitHub repo, "Assets for SSVOpenHexagon". It carries the
  full `_RELEASE` tree **with every music OGG beside its `.json`**. State on
  2026-10-18: default branch `master`, last push **2022-08-23**, 243 files.
  - **21 OGGs, 62,079,210 bytes total (62.08 MB / 59.2 MiB)** — this corrects the
    earlier 10–25 MB estimate: cube 8 OGGs / 13.72 MB, hypercube 10 / 29.35 MB,
    orthoplex 2 / 15.00 MB, tutorial 1 / 4.01 MB.
  - One quirk: the `artwork`, `experimental` and `workshopexample` packs each carry a
    `jackRussel.json`, but the repo ships a single `jackRussel.ogg` (under `cube/`).
    A complete install copies that one file into those three `Music/` dirs too.
  - **The terms: none.** GitHub's licence detection reports no licence; the README is
    two lines; the only licence file in the whole tree is the SFX one above.
    The official source of the music **carries no licence text at all**.
- **The project's official wiki** (SuperV1234/SSVOpenHexagon, linked from the
  upstream README; `Building-Instructions`) gives the other official route, verbatim:
  *"If you want music, copy assets from the Steam version of the game"*
  (store.steampowered.com/app/1358090). The Steam store page was live on 2026-10-18;
  the EULA text ships with the installed game and is not retrievable from this
  environment — and a Steam EULA licenses *playing the game*, not redistributing its
  assets.
- **Vittorio's former site channel is gone from reach**: `vittorioromeo.it` does not
  resolve (no DNS here, and the Wayback Machine holds **zero** snapshots of the
  domain); `vittorioromeo.info` now hosts an unrelated C++ course site; the
  `openhexagon.org` the current README links to is an unrelated WordPress blog.
  The "official OHRM distribution, vittorioromeo.it" named in lane 039 could not be
  reached, and the expansion "Open Hexagon Resurrection Music" could not be
  independently confirmed anywhere on the web.
- **The upstream model is still intact**: re-verified against the newest public
  upstream ref (branch `2.1.4`, 917 files, 2026-10-18) — 14 SFX OGGs + 23
  `Music/*.json`, still zero music OGGs.
- **Could not establish**: the Steam EULA text; whether BOSSFIGHT / Fantomenk /
  Dunderpatrullen have ever published their own terms for these tracks (their
  streamable playlists exist on YouTube/Spotify, which is listening, not licensing).

## The options, with their real trade-offs

- **(A) Commit the 21 OGGs + a licence notice beside them.** Self-contained and best
  for players; but it requires terms that permit redistribution, and it makes this
  repo a *distributor* of the tracks — a deliberate content divergence from upstream
  (the port's code stays 1:1; its shipped content would not be), which is why the
  provenance has to be recorded wherever the audio lives. **+62 MB on every clone.**
  **Not available today: the official source carries no terms.**
- **(B) Do not commit; document how players add music** (one pointer to
  `vittorioromeo/SSVOpenHexagonAssets` or the Steam install's `Packs/`, copy the OGGs
  into `_RELEASE/Packs/<pack>/Music/` beside the `.json`s, plus the `jackRussel.ogg`
  three-way copy). This is *literally upstream's model*, so it is faithful rather
  than a cop-out; zero licensing risk; the cost is one copy-step for players who
  want music.
- **(C) Fetch at install / first run.** Does **not** avoid the issue: the release
  artifacts (or the fetcher's target) still distribute the same tracks under the
  same unknown terms — plus runtime complexity and a network dependency for zero
  licensing gain.
- **(D) Ask the artists** (BOSSFIGHT, Fantomenk, Dunderpatrullen — the names are in
  the shipped metadata) for explicit permission to redistribute. The only path that
  *creates* permission; afterwards (A) becomes clean. Timeline unknown.

## Recommendation

**(B) for now** — the official source of the music carries no terms that permit
redistribution, and an unclear licence is not a permission. Revisit with **(A)** the
moment terms that clearly permit redistribution are in hand (the official assets
repo adding a licence, or the artists' grant via (D)); the fetch-at-boot variant (C)
is not recommended under any terms, since it adds nothing (A) does not already have.

What a future decision needs in hand: (1) the licence text (from the official assets
repo or the artists); (2) a statement that it covers redistribution in a public game
repo + release artifacts; (3) the credits — already in the shipped metadata
(BOSSFIGHT ×13, Fantomenk ×7, Dunderpatrullen ×4, with track/album names); and
(4) the official channel's status — `SSVOpenHexagonAssets` was up on 2026-10-18 but
its last push was 2022-08-23, so pin what is fetched (commit or dated snapshot) and
record the URL + date beside the audio, as the SFX licence file already does for the
seven named sound effects.

## Option E — added 2026-10-18 on the operator's suggestion: **synthesize our own music**

The operator's new direction for `oh-additions` named "synth music" as one of the things they might add.
That is a genuinely different answer to this whole brief: a soundtrack WE generate is **our own original
work**, so no third-party terms apply at all — the licensing question above simply does not arise, and
neither does the 62 MB download or the Steam-EULA ambiguity.

It is also a DIFFERENT soundtrack: it would not be the original's music, so a player who wants *Open
Hexagon's* tracks still needs option A or D. What it does buy is a game that has music by default, with a
provenance we own.

Mechanically it is likely to fit an existing seam — the audio path is a sink, exactly like the Steam and
network sinks — which would make a synthesizer "our code attached at a seam" rather than a change to the
ported audio logic. **That is an illustration and not a design**: the operator has said these are not being
built yet, so nothing here should be read as a plan. Whatever shape it eventually takes belongs in
`oh-additions` under that crate's contract (see `.agents/tasks/issue-057-oh-additions.md`).
