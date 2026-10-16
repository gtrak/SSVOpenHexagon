# Task spec — issue 013b-deferred: the options + online submenu draws

You are a worker. Implement exactly this. Do not redesign or widen scope.
Small lane (~40 min expected).

## Why

The 013b lane landed the playable-path menu draws but deferred the two
submenu arms with explicit markers: `TODO(issue-13b-options)`
(`drawSubmenusSmall`, `MenuGame.cpp:3606-3700`) and `TODO(issue-13b-online)`
(`drawOptionsSubmenus`, `MenuGame.cpp:3784-3880`). Without them the options
and online screens render nothing (navigation works, visuals don't).

## Scope

1. Port `drawSubmenusSmall` (`MenuGame.cpp:3606-3700`, cited 1:1) and
   `drawOptionsSubmenus` (`:3784-3880`) into the existing menu-draw bridge
   (`crates/oh-app/src/menu_draw.rs` — read how milestones (a)-(c) landed;
   follow the same pattern), replacing the two TODO markers. Walk the
   ssvs menu items exactly as the C++ does (the `oh-menu-system` port's
   item iteration), same textures/colours/offsets.
2. Tests: per-arm draw-call sequence over the `Recording` target (the
   established pattern), plus a same-seed full-recording determinism check.
3. Full `bash tools/gate.sh` (4 stages, print `GATE_EXIT=<rc>`, never pipe
   to tail), `cargo test --workspace`,
   `cargo test --workspace --features oh-game/produce-lua-metadata`,
   STATUS row 13 note (deferred arms closed) in your final commit.

## Process rules

Budget 90 min (expect ~40). Commit after every green milestone — (a) options
arm + test; (b) online arm + test; (c) gate + STATUS. clippy `-D warnings`
fixed immediately; verify every C++ citation (D-014).

## Out of scope

`crates/oh-net/**`, `crates/oh-discord/**`/`crates/oh-steam/**` (parallel
Discord lane + landed Steam), the egui console (14b), simulation changes,
the D-019 fix, the determinism harness.
