# Issue 27 — Plan-to-issue coverage audit (the "missed subsystem" guard)

## Why

Issue 25e exists because a **planned crate never became an issue**:
PLAN.md's crate table lists `oh-audio` (mirroring
`src/SSVOpenHexagon/Core/Audio.cpp` + `Data/MusicData.cpp`), but the issue
table (01–24) jumps 09 (game loop) → 10 (render backend) — no audio issue
was ever written, so no lane was ever dispatched. The seams, Lua bindings,
and the `oh-render` device surface all existed, which made the port *look*
complete in review while nothing ever opened an audio device. (Compounding
factor: a stale comment attributed the gap to "issue 17 owns the audio
port"; 17 was the net client.)

## The guard (do this once, mechanically)

1. **Every PLAN crate → exactly one landed issue row**, or an explicit
   D-decision that says why it is folded/deferred. Write a checker script
   (`tools/check-coverage.sh` or inline in the gate) that fails when a
   crate in the workspace is referenced by no tracker row and no decision.
2. **Every C++ translation unit in the PLAN's mapping scope → a row in a
   mapping doc with a LANDED marker.** The mapping docs exist
   (`gameplay.md`, `data-json.md`, `rendering-menu.md`, `networking.md`,
   `lua-api.md`, `oracle-vectors.md`); an audio section is missing from all
   of them, which is the same hole one level down.
3. **`TODO(...)` / `not ported` / `Noop*` sweep**: grep the crates for the
   markers and require each to name a live issue or a D-decision. Today's
   sweep found `TODO(issue-009-wiring)` (12 sites, D-020 tracks them), the
   `NoopAudio` seam (now 25e), and the placeholder texture path in
   `gl_passes.rs` (documented, 1×1 placeholders — issue-10 remainder,
   partially addressed by 025c).
4. **The misleading-comment rule**: any comment that attributes missing
   work to an issue must name an issue whose scope actually covers it, or
   it is deleted. (The "issue 17 owns the audio port" comment is the
   exemplar of why.)

## Status

OPEN — small mechanical lanes. Not started: the audit above is
orchestrator-scoped reconnaissance; the checker script + the mapping-doc
rows are worker-sized.

## Known holes found by the 2026-10-16 audit (all now tracked)

| Hole | Tracking |
|---|---|
| `oh-audio` never issued (music/sound/segments/DM sync) | issue 025e (in flight) |
| Music-driven gameplay sync (`getMusicDMSyncFactor` → pulse/beat) | 025e step 4 (same lane) |
| Mapping docs have no audio section | 025e (write `mapping/audio.md`) + this issue's guard |
| `TODO(issue-009-wiring)` × 12 (game↔render composition) | tracked by D-020 (issue 15 lineage) |
| 1×1 placeholder textures (options menu icons, key icons) | 025c partial; residual documented |
| Live Steam/Discord/on-display checks | MANUAL only (no client/display in CI) |
| LuaJIT 16KB page-size on Android 15 devices | documented watch-item (25b/25e report) |
