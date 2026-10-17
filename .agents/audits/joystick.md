# 1:1 Port Audit — `Core/Joystick.cpp` → `crates/oh-config/src/joystick.rs`

Repo: `/home/gary/dev/SSVOpenHexagon`
C++ module (audited completely): `src/SSVOpenHexagon/Core/Joystick.cpp` (233 lines)
Header: `include/SSVOpenHexagon/Core/Joystick.hpp` (60 lines)
Rust counterpart (audited completely): `crates/oh-config/src/joystick.rs` (236 lines)

Audit focus: the state machine is ported — **does anything FEED it (a poller), and are
the calling sites dispatched?** Answer up front: **nothing in production feeds it, and
only the game-side call sites dispatch at all — against a *different* object
(`NoJoystick`), not the ported state machine.** Details below.

> Scope note: `Joystick.cpp` is a pure state machine (no files/network/sound/window ops);
> its side effect is the SFML poll inside `update`. The interesting 1:1 surface is therefore
> (a) the state machine itself, (b) the platform feed into it, and (c) the call sites that
> read from it (`HGUpdate.cpp`, `MenuGame.cpp`, `HexagonGame.cpp`, `Config.cpp`). (b) and (c)
> are explicitly in scope per the audit brief. The bind-capture UI and the menu event handlers
> belong to the `BindControl` / `MenuGame` pairs; only the `Joystick::setJoystickBind` /
> `ignoreAllPresses` / `loadAllJoystickBinds` call sites in them are flagged here, marked as adjacent.

## BEHAVIOR INVENTORY (C++ `Joystick.cpp` + `Joystick.hpp`)

All in namespace `hg::Joystick`. Single global: function-local static `JoystickState`
(`getJoystickState`, `Joystick.cpp:68-71`); the struct at `:56-66`:
`ignoreAllPresses` (:58), `dirWasPressed[4]` (:60), `dirPressed[4]` (:61),
`wasPressed[12]` (:62), `pressed[12]` (:63), `joystickInputs[12]` (:65, all value-initialized to 0/false).

Member functions:

| # | Behavior | C++ location |
|---|----------|--------------|
| 1 | `getJoystickState()` — the one global | `Joystick.cpp:68-71` |
| 2 | `ignoreAllPresses(bool)` — latches every `dirWasPressed`/`wasPressed` to the flag and sets `ignoreAllPresses`; comment: "If xxxWasPressed values are true all new presses return false. Placed here to not be reiterated every update() cycle." (`:75-85`) | `Joystick.cpp:74-92` |
| 3 | `setJoystickBind(unsigned button, int buttonID)` — `joystickInputs[buttonID] = button` | `Joystick.cpp:94-97` |
| 4 | `AxisDir` enum + `operator-` | `Joystick.cpp:99-109` |
| 5 | `axisPressed(deadzone, joyId, axis)` — **reads `sf::Joystick::getAxisPosition`** (:114); `pos < -deadzone → Left`, `pos > deadzone → Right`, else `Dead` | `Joystick.cpp:111-127` |
| 6 | `update(deadzone)` — `joyId = 0` hardcoded (:131); early-return when `ignoreAllPresses` (:135-138); polls 4 axes via `axisPressed`: PovX (:140-142), PovY (:145-147), X (:150-151), Y (:153-154); `xIs = dpadX \|\| stickX` (:156-157); `yIs = dpadY \|\| stickY(-dir)` — stick Y inverted (:159-160); `doDir` exchange-into `dirWasPressed/dirPressed` for Left/Right/Up/Down (:162-171); `doButton` for all 12 `Jid`s reading **`sf::Joystick::isButtonPressed(0, joystickInputs[jid])`** (:173-191) | `Joystick.cpp:129-192` |
| 7 | `pressed(Jdir)` / `risingEdge(Jdir)` (`pressed && !wasPressed`) | `Joystick.cpp:194-203` |
| 8 | `pressed(Jid)` / `risingEdge(Jid)` | `Joystick.cpp:205-214` |

Enums (hpp): `Jdir {Unknown=-1, Left, Right, Up, Down, JoystickDirectionsCount}`
(`Joystick.hpp:9-17`); `Jid {Unknown=-1, Select=0, Exit, Focus, Swap, ForceRestart,
Restart, Replay, Screenshot, NextPack, PreviousPack, AddToFavorites, FavoritesMenu,
JoystickBindsCount}` (`Joystick.hpp:19-36`).

Event handlers/subscriptions: none (no `onEvent` inside the module).
Side effects: SFML controller-0 poll only. State transitions: `dirPressed`/`pressed`
updated per `update()` via `std::exchange` into the `*WasPressed` slots (:162-171,
:173-179); `ignoreAllPresses` freezes all rising edges (:135-138).

### C++ call sites (the dispatch surface)

| Call site | Location | What it does |
|-----------|----------|--------------|
| `HexagonGame::updateInput_UpdateJoystickControls` | `HGUpdate.cpp:560-585` | `window == nullptr` gate (:561-564); `Joystick::update(getJoystickDeadzone())` (:567); `pressed(Jdir::Left/Right)` → `setInputImplIfFalse` CCW/CW (:569-570); `pressed(Jid::Swap/Focus)` (:571-572); `risingEdge(Jid::Exit)` → `goToMenu()` (:574-576); `risingEdge(Jid::ForceRestart) \|\| (hasDied && risingEdge(Jid::Restart))` → MustRestart (:578-582); `hasDied && risingEdge(Jid::Replay)` → MustReplay (:583-585) |
| `MenuGame::update` | `MenuGame.cpp:2643-2784` | `Joystick::update` (:2643); `focusHeld = pressed(Jid::Focus)` gated on `LevelSelection && !isFavoriteLevels() && PackChange::Rest` (:2650-2653); quick pack switch `focusHeld && !wasFocusHeld` → `setIndex(0)`/`quickPackFoldStretch()` (:2656-2669); `risingEdge(Jid::NextPack)` → `changePackAction(1)` (:2737-2740); `risingEdge(Jid::PreviousPack)` → `changePackAction(-1)` (:2741-2744); `risingEdge(Jid::AddToFavorites)` → `addRemoveFavoriteLevel()` (:2746-2749); `risingEdge(Jid::FavoritesMenu)` → `switchToFromFavoriteLevels()` (:2751-2754); `risingEdge(Jdir::Left/Right/Up/Down)` → `leftAction/rightAction/upAction/downAction` (:2756-2771); `risingEdge(Jid::Select)` → `okAction()` (:2773-2776); `risingEdge(Jid::Exit)` → `exitAction()` (:2777-2780); `risingEdge(Jid::Screenshot)` → `mustTakeScreenshot = true` (:2782-2784) |
| `MenuGame::setIgnoreAllInputs` | `MenuGame.cpp:3390-3403` | `ignoreInputs = presses`; on 0: `game.ignoreAllInputs(false)` + `Joystick::ignoreAllPresses(false)` (:3396-3397); else `game.ignoreAllInputs(true)` + `Joystick::ignoreAllPresses(true)` (:3401-3402) |
| `MenuGame::refreshBinds` | `MenuGame.cpp:3373-3388` | keyboard trigger refresh loop, then `Config::loadAllJoystickBinds()` (:3387) |
| `HexagonGame` ctor | `HexagonGame.cpp:448` | `Config::loadAllJoystickBinds()` ("Joystick binds" section :445-448) |
| `Config::loadAllJoystickBinds` | `Config.cpp:1609-1615` | for each of 12 `joystickTriggerGetters`: `Joystick::setJoystickBind(getter(), i)` (:1613) |
| `createJoystickBindControl` callback | `MenuGame.cpp:1288-1314` | `joystickCallBack = [](button, buttonID) { Joystick::setJoystickBind(button, buttonID); }` (:1290-1292), passed to the 12 `JoystickBindControl` items; capture fires from the `JoystickButtonReleased` `onEvent` handler: `bc->newJoystickBind(mEvent.joystickButton.button)` (`MenuGame.cpp:673-727`, esp. :712-726) |

## IMPLEMENTED (Rust `oh-config/src/joystick.rs` + production wiring)

State machine (all present, logic-matched to C++):

| # | Rust | Location | C++ counterpart |
|---|------|----------|-----------------|
| 1 | `Jdir` / `Jid` enums, same values/order | `joystick.rs:12-19` / `:26-44` | `Joystick.hpp:9-17` / `:19-36` |
| 2 | `JOY_COUNT=4` / `BIND_COUNT=12`, `JIDS` table | `joystick.rs:48-49`, `:54-67` | (implicit in C++ arrays/loop) |
| 3 | `JoystickState` struct (same 6 fields) | `joystick.rs:69-76` | `Joystick.cpp:56-66` |
| 4 | `do_dir` / `do_button` (`mem::replace` = `std::exchange`) | `joystick.rs:80-83` / `:85-88` | `Joystick.cpp:162-171` / `:173-179` |
| 5 | Global `static JOYSTICK_STATE: Mutex<JoystickState>` (all-false/zero init) + poison-tolerant `state()` | `joystick.rs:91-103` | `Joystick.cpp:68-71` |
| 6 | `ignore_all_presses(ignore)` — identical latch loops + flag | `joystick.rs:107-117` | `Joystick.cpp:74-92` |
| 7 | `set_joystick_bind(button, button_id)` | `joystick.rs:120-122` | `Joystick.cpp:94-97` |
| 8 | `JoystickPoll` snapshot (`pov_x, pov_y, x, y, buttons[8]`) + `button_pressed` (out-of-range → false) | `joystick.rs:129-141` | the SFML reads inside `update` (`Joystick.cpp:114, 140-154, 176-177`) |
| 9 | `AxisDir` + `negate` + `PartialEq` | `joystick.rs:144-166` | `Joystick.cpp:99-109` |
| 10 | `axis_pressed(deadzone, pos)` — same deadzone comparisons | `joystick.rs:168-175` | `Joystick.cpp:111-127` |
| 11 | `update(deadzone, poll)` — early-return on `ignore_all_presses`; dpad/stick OR with inverted stick-Y; 4 `do_dir`; 12 `do_button` from `poll.button_pressed(joystick_inputs[i])` | `joystick.rs:183-211` | `Joystick.cpp:129-192` |
| 12 | `pressed_dir` / `rising_edge_dir` / `pressed` / `rising_edge` | `joystick.rs:213-216` / `:219-223` / `:226-229` / `:232-236` | `Joystick.cpp:194-214` |

Explicit seams/stubs in the file (markers quoted):
- Module doc `joystick.rs:1-9`: "the port takes an explicit `JoystickPoll` snapshot as
  the input seam (**issue 10 wires the window backend into it**). Controller id is
  hardcoded to 0 in the C++ as well."
- `set_joystick_bind` doc `joystick.rs:118-119`: "fed by `Config::load_all_joystick_binds`."
- No TODO/FIXME markers in the file.

Present in Rust that the C++ lacks (all benign): `Mutex` + poison recovery
(`joystick.rs:91-103`), explicit `JoystickPoll` type with an OOB button guard
(`joystick.rs:139-141`), `must_use` attributes.

### Production wiring of the state machine (outside the module)

- **Feed (poller): ABSENT in production.** `JoystickPoll` appears only in
  `oh-config/src/{lib.rs:31,joystick.rs}` and `oh-config/tests/config_release.rs`
  (the `joystick_state_machine` test, :610-655). Nothing in `oh-app`, `oh-game`,
  `oh-menu`, `oh-render` constructs one. `oh-render`'s window does expose
  `is_joystick_connected` / `get_joystick_axis` / `is_joystick_button_pressed`
  (`oh-render/src/window.rs:864-881`) but every one is a hardcoded headless no-op
  ("always `false` headless", "`0` headless") and nothing consumes them.
- **Game-side call sites: PRESENT but dispatched at a different object.**
  `oh-game` does NOT use `oh_config::joystick`. It defines its own `trait Joystick`
  (`oh-game/src/seams.rs:340-349`: `update`, `pressed_dir`, `pressed_id`,
  `rising_edge_id`) and `pub struct NoJoystick` (`:352`), whose impl (`:354-363`) is
  a no-op `update` + `false` for every query. The production composition wires
  `Arc::new(NoJoystick)` (`oh-app/src/compose.rs:544`). The verbatim game call-site
  port exists at `oh-game/src/update.rs:681-714` (`update_input_update_joystick_controls`:
  `!window` gate :684-686 = C++ `window == nullptr` gate `HGUpdate.cpp:561-564`;
  `update(deadzone)` :688-690; `pressed_dir(Left/Right)` :693-701; `pressed_id(Swap/Focus)`
  :702-703; `rising_edge_id(Exit/ForceRestart/Restart/Replay)` :706-714) — but every query
  returns `false` until "issue 10 feeds it" (comment `update.rs:681-682`).
- **Menu-side call sites: ABSENT.** `oh-menu`'s per-frame loop
  (`oh-menu/src/menu_game.rs:1824-1918`, `MenuGame::update`) contains no joystick
  `update`, no `Jid`/`Jdir` query, no `JoystickPoll`. `focus_held` is driven only by
  the app-layer keyboard trigger (`windowed.rs:1750-1763` → `set_focus_held`,
  `menu_game.rs:1922-1924`; prior-frame mirror at `menu_game.rs:1841-1842`); the C++
  joystick OR-term `focusHeld = Joystick::pressed(Jid::Focus)` (`MenuGame.cpp:2650-2653`)
  has no counterpart.
- **`ignore_all_presses` production call: ABSENT.**
  `oh-menu/src/menu_game.rs:304-313` (`set_ignore_all_inputs`) fires only the
  keyboard-half `ignore_inputs_cb` callback.
- **`load_all_joystick_binds` production call: ABSENT.** The function exists
  (`oh-config/src/config.rs:2015-2033`) and does call `joystick::set_joystick_bind`
  (`config.rs:2033`), but the only caller is the test
  (`oh-config/tests/config_release.rs:613`). Neither the `HexagonGame`-constructor
  equivalent (`oh-game/src/hexagon_game.rs` ctor, :503-640) nor the windowed
  `refresh_binds` calls it.

## MISSING

Ordered highest risk first. "Risk" per the audit rubric.

| # | item | C++ evidence (file:line) | absence proof (command + result) | risk |
|---|------|--------------------------|----------------------------------|------|
| 1 | **Platform poller — no production feed of controller 0 into the state machine.** C++ `update` polls SFML every frame (`getAxisPosition` ×4, `isButtonPressed` ×12, hardcoded joyId 0). Rust `update` requires a `JoystickPoll` that only the unit test ever builds; the windowed app never constructs one, so the ported state machine is dead code in production and all controller input is dead in the shipped game. Marker exists ("issue 10 wires the window backend", `joystick.rs:4-8`) but the behavior is still absent. | `Joystick.cpp:114` (axis read), `:140-154` (4 axes), `:176-177` (12 button reads), `:131` (joyId 0); call sites `HGUpdate.cpp:567`, `MenuGame.cpp:2643` | `grep -rn "JoystickPoll" crates --include='*.rs' \| grep -v tests` → only `oh-config/src/lib.rs:31`, `oh-config/src/joystick.rs:6,129,137,183` (definition + re-export). Zero hits in `oh-app`, `oh-game`, `oh-menu`, `oh-render` src. `oh-render/src/window.rs:864-881` — the only window joystick surface — is `false`/`0`/`false` hardcoded headless. | uservisible-gameplay |
| 2 | **Menu-side joystick dispatch has no Rust counterpart at all.** The C++ menu polls the controller each frame and dispatches: focus (quick pack switch), NextPack, PreviousPack, AddToFavorites, FavoritesMenu, stick/dpad Left/Right/Up/Down menu navigation, Select, Exit, Screenshot. The Rust menu's per-frame `update` does none of this; menu navigation is keyboard-trigger-driven only (`windowed.rs:1654-1929` `register_menu_binds` registers `Tid` keyboard triggers), and `focus_held` is keyboard-sourced only. No seam marker in `oh-menu` declares this omission (only the `ignoreAllPresses` half is marked). | `MenuGame.cpp:2643` (update), `:2650-2653` (Focus), `:2656-2669` (quick pack), `:2737-2744` (Next/PreviousPack), `:2746-2749` (AddToFavorites), `:2751-2754` (FavoritesMenu), `:2756-2771` (Jdir ×4), `:2773-2776` (Select), `:2777-2780` (Exit), `:2782-2784` (Screenshot) | `grep -rn "Jid::\|Jdir::\|rising_edge\|joystick" crates/oh-menu/src` → no `Jid::`/`Jdir::` usage anywhere in `oh-menu/src` (only deadzone config getters, `set_ignore_all_inputs`, and options-menu items that fire `trigger_refresh`); `MenuGame::update` body `menu_game.rs:1824-1918` contains no joystick. | uservisible-gameplay |
| 3 | **`ignoreAllPresses` is never called in production.** C++ latches the state machine on every `setIgnoreAllInputs` (boot screens, dialogs, `returnToLevelSelection`, etc. — `MenuGame.cpp` calls `setIgnoreAllInputs` ~30×, e.g. :3369, :2554). Rust `set_ignore_all_inputs` fires only the keyboard `ignore_inputs_cb`; `oh_config::joystick::ignore_all_presses` has no production caller. Consequence once a poller lands: a controller button held during a dialog/boot screen fires a spurious rising edge on release (the latch exists precisely to prevent this, `Joystick.cpp:75-85`). | `MenuGame.cpp:3397` (`ignoreAllPresses(false)`), `:3402` (`ignoreAllPresses(true)`); latch rationale `Joystick.cpp:74-92` | `grep -rn "ignore_all_presses" crates` → only `oh-config/src/joystick.rs:107` (definition) and `oh-config/tests/config_release.rs:635,639` (test). No caller in `oh-menu`/`oh-app`/`oh-game`. (Marker: `oh-menu/src/menu_game.rs:308-310` + `oh-menu/src/seams.rs:48-51` — "the C++ `Joystick::ignoreAllPresses` half is the unported joystick (NoJoystick seam)".) | uservisible-gameplay |
| 4 | **`loadAllJoystickBinds()` → `setJoystickBind` has no production call site.** C++ loads the 12 `j_*` config values into the state machine at engine construction (`HexagonGame.cpp:448`) and after every rebind (`MenuGame.cpp:3387` → `Config.cpp:1613`). Rust's `load_all_joystick_binds` exists (`config.rs:2015-2033`, calls `set_joystick_bind` at :2033) but is test-only; the windowed `refresh_binds` explicitly skips the tail and the `HexagonGame`-ctor equivalent never calls it. Consequence: even when the poller lands, `joystick_inputs` stays `[0; 12]` — every bind maps to SFML button 0 (and config rebinds never reach the machine). | `HexagonGame.cpp:448`; `MenuGame.cpp:3387`; `Config.cpp:1609-1615` (esp. :1613) | `grep -rn "load_all_joystick_binds" crates` → only `oh-config/src/config.rs:2015` (definition) and `oh-config/tests/config_release.rs:613` (test). (Partial marker: `oh-app/src/windowed.rs:519-520` and `:2282` — "the `Config::loadAllJoystickBinds()` tail is the unported joystick (NoJoystick seam) — documented no-op"; the `HexagonGame.cpp:448` half has no marker.) | uservisible-gameplay |
| 5 | **`setJoystickBind` from the bind-capture callback is unwired (adjacent: `BindControl`/menu lane).** C++: capturing a new controller bind in the options menu runs `joystickCallBack` → `Joystick::setJoystickBind` (`MenuGame.cpp:1290-1292`, fired by `bc->newJoystickBind(mEvent.joystickButton.button)` in the `JoystickButtonReleased` handler, `MenuGame.cpp:673-727` esp. :712-726; `BindControl.cpp:147+`). Rust: `JoystickBindControl::new_joystick_bind` takes the callback (`oh-config/src/binds.rs:205-211`) but no production code instantiates or drives a `JoystickBindControl` — the options-menu "joystick" category creates plain `Single` items that only fire `trigger_refresh` (`oh-menu/src/menu.rs:267-287`) — and no production callback calls `joystick::set_joystick_bind`. | `MenuGame.cpp:1290-1292` (callback), `:723` (capture), `:673-727` (handler) | `grep -rn "new_joystick_bind\|JoystickBindControl" crates` → only `oh-config/src/{binds.rs,lib.rs}` and `oh-config/tests/config_release.rs` (tests pass `|_, _| {}` / recording closures, :591, :596). Zero hits in `oh-menu`, `oh-app`. | uservisible-gameplay |
| 6 | **`JoystickButtonReleased` window event has no feed at all (adjacent: menu-event lane).** C++ registers the handler (`MenuGame.cpp:673`): on any joypad release it (a) advances boot screens via `checkCloseBootScreens` (:679-683), (b) runs the dialog/`ignoreInputs` release tail (:685-707), (c) captures the bind in MOpts (:708-726). Rust's once-per-frame release drain (`oh-app/src/windowed.rs:1416-1446`) counts/drains only `edges.keys + edges.buttons + edges.fingers` (:1433); `ReleasedEdges` has no joystick field. Controller-only users: cannot close the loading/epilepsy boot screens and joystick releases never decrement `ignoreInputs`. | `MenuGame.cpp:673-727` (handler), :679-683 (boot screens) | `sed -n '320,330p' crates/oh-render/src/window.rs` → `pub struct ReleasedEdges { keys, buttons, fingers }` (:323-327); `grep -rn "JoystickButton" crates/oh-app crates/oh-render/src` → only the doc comment `window.rs:314` (quoting the C++ handler) and the headless no-op `is_joystick_button_pressed` (:877-880). | uservisible-gameplay |

Not counted as missing (verified present): the `window` gate on the game-side call
sites (C++ `HGUpdate.cpp:561-564` ↔ Rust `oh-game/src/update.rs:684-686`), the
`setInputImplIfFalse` folding (C++ :569-572 ↔ Rust :693-703), the MustRestart/
MustReplay state changes (C++ :578-585 ↔ Rust :706-714), the deadzone config value
(`Config::getJoystickDeadzone` ↔ `config.joystick_deadzone`, `oh-menu/src/seams.rs:244`),
and the 12 `j_*` config keys + `joystick_bind(Jid)` getter (oh-config, test-verified in
`config_release.rs:102-114`).

## SEAMS

Accepted/deliberate deviations, each with its declaring marker:

| seam | marker (quoted) | location |
|------|-----------------|----------|
| `JoystickPoll` input snapshot replaces inline SFML polling in `update` | "the port takes an explicit `JoystickPoll` snapshot as the input seam (issue 10 wires the window backend into it). Controller id is hardcoded to 0 in the C++ as well." | `oh-config/src/joystick.rs:1-9` |
| `NoJoystick` test double — the only production `Joystick` impl (always-idle) | "C++ `Joystick` global state (`Core/Joystick.hpp` — fed by the platform window in a later issue; `updateInput_UpdateJoystickControls` only runs when a window is present)" + "The always-idle joystick (no platform backend attached yet)" | `oh-game/src/seams.rs:337-352`; wired at `oh-app/src/compose.rs:544` (+ test doubles `oh-game/tests/common/mod.rs:490`, `oh-game/tests/discord_retry.rs:97`, `oh-parity/tests/common/mod.rs:503`, `oh-game/src/lua_console.rs:1958`) |
| `Joystick::ignoreAllPresses` half of `setIgnoreAllInputs` unported | "The C++ `Joystick::ignoreAllPresses` half is the unported joystick (NoJoystick seam)." | `oh-menu/src/menu_game.rs:308-310`, `oh-menu/src/seams.rs:48-51` |
| `refreshBinds`' joystick tail (`loadAllJoystickBinds`) no-op'd in the windowed composition | "the `Config::loadAllJoystickBinds()` tail is the unported joystick (NoJoystick seam) — documented no-op." | `oh-app/src/windowed.rs:519-520` (also `:2282`) |
| `oh-render` window joystick surface as headless no-ops | "`sf::Joystick::isConnected(id)` — always `false` headless." / "`0` headless." / "always `false` headless." (test: "joystick surface is a clean no-op headless.") | `oh-render/src/window.rs:864-881`; `oh-render/tests/backend.rs:498-500` |
| Game-side call sites run but query the no-op double | "`NoJoystick` makes every query false until issue 10 feeds it" | `oh-game/src/update.rs:681-682` |

Per the audit rubric these markers make the omissions deliberate, but under the
operator's 1:1 standard the underlying C++ behavior is still absent — see MISSING
rows 1-4, which are exactly these seams' missing behaviors.

## DIFFERENT

Present but renamed or behaviorally different:

| # | item | C++ | Rust | note |
|---|------|-----|------|------|
| 1 | `update` signature / feed location | `void update(float deadzone)` polls SFML internally (`Joystick.cpp:129`) | `pub fn update(deadzone: f32, poll: &JoystickPoll)` (`joystick.rs:183`) | SFML poll moved out of the module to (currently nonexistent) production feeders; SFML's implicit `sf::Joystick::update()`-on-read is now the poller's responsibility. Documented seam. |
| 2 | Query names | `pressed(Jdir)` / `risingEdge(Jdir)` overloads (`Joystick.cpp:194-203`) | `pressed_dir` / `rising_edge_dir` (`joystick.rs:213-223`); Jid overloads keep `pressed` / `rising_edge` (:226-236) | Rename only (Rust has no overloading); logic identical. |
| 3 | Global state container | plain function-local static, no synchronization (`Joystick.cpp:68-71`) | `static Mutex<JoystickState>` with poison recovery `unwrap_or_else(|e| e.into_inner())` (`joystick.rs:91-103`) | Adds locking semantics the C++ lacks; behavior identical single-threaded, poison-tolerant where C++ would just reuse the static. |
| 4 | Bind-slot index handling | `setJoystickBind(unsigned, int)` writes `joystickInputs[buttonID]` with signed `int` index — UB if negative (`Joystick.cpp:94-97`) | `set_joystick_bind(u32, i32)` indexes `button_id as usize` — panics if negative (`joystick.rs:120-122`) | Identical for valid inputs; panic replaces UB. |
| 5 | `Unknown = -1` query behavior | `pressed(Jdir::Unknown)` / `pressed(Jid::Unknown)` → out-of-bounds read (UB) | `pressed_dir(Jdir::Unknown)` / `pressed(Jid::Unknown)` → `usize` index panic (`-1 as usize`) | No call site uses `Unknown`; panic replaces UB. |
| 6 | **Two parallel "joystick" abstractions** | one global state machine + free functions | `oh-config::joystick` state machine (test-only) **and** `oh-game::seams::Joystick` trait + `NoJoystick` (`seams.rs:340-363`) | The game call sites dispatch at the trait object, not the ported machine. If issue 10 lands a poller, someone must bridge `oh_config::joystick` into the `oh-game` `Joystick` trait (or rewire) — as written the machine stays dead. |
| 7 | Menu focus source | `focusHeld` = keyboard trigger (`add2StateInput`, `MenuGame.cpp:956-957`) **OR** `Joystick::pressed(Jid::Focus)` (`MenuGame.cpp:2650-2653`) | `focus_held` set only by the app-layer keyboard trigger (`windowed.rs:1750-1763` → `menu_game.rs:1922-1924`) | The joystick OR-term is missing (subset of MISSING #2). |
| 8 | Axis value types at the window surface | `sf::Joystick::getAxisPosition` returns `float` | `oh-render` `get_joystick_axis` returns `i32` (`window.rs:871-875`); `JoystickPoll` fields are `f32` (`joystick.rs:129-136`) | Type mismatch at the (currently dead) headless seam; `JoystickPoll` docs claim POV range `[-127,127]` / stick `[-100,100]` (`joystick.rs:124-128`), matching SFML. |
| 9 | SFML button count assumption | `isButtonPressed` on 8-button controller (implicit) | `JoystickPoll::buttons: [bool; 8]` + explicit OOB→false guard (`joystick.rs:139-141`) | Parity preserved; guard is additive. |

Citation check: no wrong line citations were found in comments for this pair — the
Rust comments cite `MenuGame.cpp:3393-3400` for `setIgnoreAllInputs`
(`menu_game.rs:308`), which matches the actual :3390-3403; `MenuGame.cpp:3373-3387`
for `refreshBinds` (`windowed.rs:331,516`) matches :3373-3388; `HGUpdate`'s gate and
dispatch lines match as cited in `update.rs:681-682`. The `JoystickBindControl`
port cites C++ `Core/BindControl` behavior consistent with `BindControl.cpp:147+`.

## VERDICT

**FAIL 1:1.** The state machine itself is a faithful, test-covered port, but the
production joystick is a double no-op: nothing feeds controller 0 into the
`oh-config` state machine (no poller — "issue 10"), the game-side call sites
dispatch at a separate always-false `NoJoystick` test double instead of the ported
machine, the 12 config binds are never loaded into the machine in production,
`ignoreAllPresses` has no production caller, and the entire menu-side dispatch
(`MenuGame.cpp:2643-2784`) has no Rust counterpart. Single most important gap:
**the missing platform poller — in the Rust app, controller input is completely
dead in both game and menu** (MISSING #1, compounded by #2-#4).
