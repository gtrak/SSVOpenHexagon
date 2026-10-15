# Issue 13 — `oh-menu` MenuGame: all states, dialog box

## Objective

Port `MenuGame` (`Core/MenuGame.cpp`, 6k lines) — the entire front-end: all ten
states (`ETLPNewBoot`, `LoadingScreen`, `EpilepsyWarning`, `SLPSelectBoot`,
`SMain`, `LevelSelection`, `MOpts`, `MOnline`, `SLPSelect`, `ETLPNew`), every menu,
every item, all transitions, all layout constants and text, the cameras, the
scrollbar, online status bar, text-entry, pack/level browsing, and
`HexagonDialogBox`.

## Key decisions

- Mirror `MenuGame.hpp` + `MenuGame.cpp` 1:1. This is a large file; the port is
  mechanical but must not skip states or items.
- All user-visible strings must match verbatim.
- Layout math (positions, sizes, font sizes at the current resolution) must match
  the C++ constants.
- `fnHG*` callbacks (to `HexagonGame`) become explicit trait/closure seams.
- `LeaderboardCache` and online flows depend on issue 17; leave the seam and stub
  the network with the exact interface.

## Files

| File | Change |
|---|---|
| `crates/oh-menu/Cargo.toml` | new — deps `oh-render`, `oh-menu-system`, `oh-lua`, `oh-assets` |
| `crates/oh-menu/src/{menu_game,states,dialog_box,layout}.rs` | new |
| `crates/oh-menu/tests/state_machine.rs` | new |

## Steps

1. Port the `States` enum and the state-change machinery.
2. Port initialization (`initAssets`, `initInput`, `initLua`, `initMenus`).
3. Port every menu construction (welcome, main, options, online, profile selection)
   with every item and callback.
4. Port navigation actions (`leftAction`/`rightAction`/`upAction`/`downAction`/
   `okAction`/`eraseAction`/`exitAction`, pack changes, wheel/touch).
5. Port `update` (per-state), `refreshCamera`, `reloadAssets`, text-entry.
6. Port the drawing for every state with exact layout.
7. Port `HexagonDialogBox`.

## Verification

- State-machine test: drive a scripted input sequence and assert the state
  transitions and the currently-selected item match the C++ sequence.
- A screen-capture/`to_string`-style render test for at least the main menu (golden
  coarse frame) — note it is coarse.
- Load the real `_RELEASE` and assert the menu lists every pack/level the C++
  lists (counts + names).
- `tools/gate.sh` green.

## Notes / risks

- Largest single issue; consider splitting into 13a (states+menus+nav) and 13b
  (drawing) if a worker cannot finish in one pass. Record any split in
  `STATUS.md`.
- Text strings: copy verbatim, including punctuation and version strings.
