# Task spec — issue 013b: port `MenuGame::draw()` (the menu is logic-complete but renders nothing)

You are a worker. Implement exactly this. Do not redesign or widen scope.

## Why this exists

Issue 13 landed `MenuGame`'s states/logic (`crates/oh-menu`) with `update` but
NO draw: `crates/oh-menu/src/menu_game.rs` has no `draw`. The C++
`MenuGame::draw()` (`src/SSVOpenHexagon/Core/MenuGame.cpp:5751`) and its helper
renders (`:3568-6161`: drawScrollbar `:3568`, drawMainSubmenus `:3586`,
drawSubmenusSmall `:3606`, drawMainMenu `:3700`, drawOptionsSubmenus
`:3784`, drawProfileSelection `:3880`, drawProfileSelectionBoot `:4007`,
drawEnteringText `:4091`, drawEnteringTextBoot `:4147`, drawLoadResults
`:4170`, drawLevelSelectionRightSide `:4957`, drawLevelSelectionLeftSide
`:5306`, draw `:5751`, drawGraphics `:5991`, drawOnlineStatus `:5999`) are
unported. The 15c wiring calls a `TODO(issue-13-menu-draw)` no-op — the menu
screen currently shows only the background. This lane makes the game
playable-navigable.

## What exists (verified — reuse, do not duplicate)

- `crates/oh-render/src/graphics.rs` — `draw_background_menu`
  (`HGGraphics.cpp:122-141` menu-hexagon background), `draw_key_icons`,
  `draw_level_info`, `FrameInput`, `DrawTarget` (+ the `Recording` test
  target at `:1565`), `render_states_for_stage`, `init_text`/`Text`.
- `crates/oh-render/src/text_draw.rs` — the text render path.
- `crates/oh-menu/src/dialog_box.rs:236` — the dialog box `draw` (already
  ported).
- The 15a bridge pattern (`crates/oh-app/src/bridge.rs`) — snapshot engine
  state, drive `oh_render` free functions. MenuGame drawing follows the SAME
  pattern: the menu state lives in `oh-menu`, which cannot depend on
  `oh-render` (check the actual dependency direction first and mirror
  whichever bridge shape the codebase uses — if `oh-menu` already depends on
  something render-shaped via seams, follow that).
- `crates/oh-render/src/window.rs` `Window` — a `DrawTarget` in the windowed
  path.

## C++ oracle

`src/SSVOpenHexagon/Core/MenuGame.cpp:3568-6161` (the draw methods above),
plus whatever they call (assets textures, `window->draw`, `sf::Text`,
`ssvs::Menu` item iteration via the issue-12 `oh-menu-system` port — read how
`drawSubmenusSmall` etc. walk `menu->getItems()` and mirror the ported menu
system's API). Cite line numbers for every method you port.

## Scope

1. A `draw()` on the MenuGame port (or a bridge function over it — mirror the
   15a/oh-app bridge shape and document the choice) reproducing the C++
   methods in the same call order, per state (main menu, level selection,
   options, profile selection/boot, entering-text, load-results, the
   scrollbar, online status). Literal 1:1: same textures, same colours, same
   offsets — cite each.
2. The pieces that depend on infrastructure you must add: e.g. if the C++
   uses `sf::RectangleShape` fills or per-item highlighting, use the
   `DrawTarget::draw_rectangle`/`draw_vertices` primitives that exist. If a
   needed primitive is missing from `oh-render`, add it there (minimal,
   SFML-fork-cited) rather than faking it.
3. Wire the `TODO(issue-13-menu-draw)` in `crates/oh-app` to the new draw.
4. Tests (headless, `Recording` target — the 15a test pattern): per state,
   assert the draw call sequence (text/vertices/sprites in C++ order), the
   menu-hexagon background enabled per `state >= SMain`, the scrollbar math
   pinned against the C++ (`:3568-3586`), and a full-recording same-seed
   determinism check.
5. Final: full `bash tools/gate.sh` (4 stages, print `GATE_EXIT=<rc>`, never
   pipe to tail), `cargo test --workspace`,
   `cargo test --workspace --features oh-game/produce-lua-metadata`, and the
   `.agents/plans/STATUS.md` row 13 note ("draw landed") in your final
   commit.

## Process rules (mandatory)

- Budget: 90 minutes. Commit after EVERY green milestone — suggest: (a) the
  main-menu draw + test; (b) level-selection draws; (c) profile/entering-text/
  load-results/online; (d) wiring + full gate. `cargo test -p <crate>` after
  each file; clippy `-D warnings` fixed immediately per commit.
- Verify every C++ citation against the real source (D-014); fix this spec in
  your commit if wrong and say so.
- If the full surface will not fit the budget, land the MAIN MENU + LEVEL
  SELECTION draws (the playable path) and leave the rest as explicit
  `TODO(issue-13b-<name>)` markers with a tracker note — a playable menu beats
  an uncommitted complete one.

## Out of scope

- The egui console overlay (14b), screenshot/SIGINT (15c tail), the online
  panels' live data (17 wires real data later — render the same layout with
  empty/placeholder-safe data paths exactly as C++ does when offline),
  simulation changes, `crates/oh-net/**` (parallel lane in flight).
