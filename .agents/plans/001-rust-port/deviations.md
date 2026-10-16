# Deviations register — raw inventory (lane 28a)

**VERDICTS COME LATER (28b).** This file is the mechanical inventory only: every marker hit below is a raw row — no verdicts, no triage, no fixes. The category column is a provisional guess from the `28-deviations-audit.md` table (honest `unknown` where unsure); 28b assigns the real category + verdict.

## Sweep parameters

- **HEAD:** `01cdbd51aeb8a1ee78744e51c172618c6f810f41` (worktree `pi-worktree-d75260e1…s0-0`), tree clean at sweep time.
- **Swept:** `crates/`, `.rs` files only, excluding `tests/` and `target/`.
- **Markers (case-insensitive, in this order):** `not ported`, `unported`, `not implemented`, `documented no-op`, `no-op`, `placeholder`, `TODO(`, `FIXME`, `divergenc`, `approximat`, `simplif`, `out of scope`, `stub`, `the port's analogue`, `no-op sink`, `Noop`, `NoAudio`, `NoSteam`, `NoDiscord`, `NoJoystick`.
- **Reproducible command (per marker, in the order above):**
  ```sh
  grep -rniF --include='*.rs' '<marker>' crates/ | grep -vE '(^|/)(tests|target)/'
  ```
  Dedup rule: a line matching several markers is recorded **once**, under the **first** marker in the list above (e.g. a `documented no-op` line is not also a `no-op` row). 260 unique hit lines (raw per-marker counts: 280).
- **Row order:** grouped by crate (alphabetical); within a crate, file path (byte order) then line number — the sweep's reproducible order.
- **C++ citation verification (D-014):** every `C++:` citation below was grepped against the C++ tree (`src/`, `include/`, `public/`) at the HEAD above. `extlibs/SSVStart` (and the other `extlibs/*` submodules) are **not checked out** at this HEAD (`git submodule status`: all `-`/empty), so citations into SSVStart headers (`GameEngine.hpp`, `GameWindow.hpp`, `GameTimer.hpp`, `GameState.hpp`, `Manager.hpp`, `TimerBase`, `msUpdate`, `runEvents`, `GameEngine`, `ssvs::TimerStatic`, `ssvs::getGlobalBottom`, `LuaScripting::doc`) are marked **UNVERIFIABLE** — neither confirmed nor refuted; 28b should check out `extlibs/SSVStart` before ruling on them.
- **No row was invented:** every row is a real marker hit; `tests/` is excluded (test doubles are not deviations) but doc-comment module headers are included.

## Categories (from `28-deviations-audit.md`)

| code | category |
|---|---|
| PS | platform seam |
| DC | dead C++ |
| NP | not-yet-ported |
| DP | draw-path remainder |
| LB | load-bearing |
| DR | determinism-risk |
| LT | test-double-or-legit-no-op |
| UN | unknown |

## oh-android (1 row)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 1 | `crates/oh-android/build.rs:9` | no-op | // shim anyway: a plain C file with a no-op-on-host builtin. | fn main | — | test-double-or-legit-no-op |

## oh-app (89 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 2 | `crates/oh-app/src/android.rs:19` | NoSteam | //! * **Steam** — the default (no-`steam`-feature) `NoSteam` backend, | module docs (crates/oh-app/src/android.rs) | C++: src/SSVOpenHexagon/Core/Steam.cpp (no-SDK `#else` block :961-1062 — verified), Discord.cpp (:187-219 — verified) | platform seam |
| 3 | `crates/oh-app/src/android.rs:25` | NoDiscord | //!   port's `NoDiscordCore` manager, passed to `WindowedClient::build` | module docs (crates/oh-app/src/android.rs) | C++: src/SSVOpenHexagon/Core/main.cpp:514-515 (gotoMenu after Discord init — verified) | platform seam |
| 4 | `crates/oh-app/src/android.rs:46` | no-op | //! * **Audio** — the no-op `NoopAudio` menu sink + the engine's `None` | module docs (crates/oh-app/src/android.rs) | — | platform seam |
| 5 | `crates/oh-app/src/android.rs:95` | no-op | /// first run, then write the marker; SKIP (the Ok no-op) when the marker | fn extract_content | — | test-double-or-legit-no-op |
| 6 | `crates/oh-app/src/android.rs:208` | FIXME | // attributes are ignored by the platform — winit FIXME noted in | fn create_android_window | — (winit platform FIXME, not a C++ site) | unknown |
| 7 | `crates/oh-app/src/android.rs:229` | no-op | // android-platform no-op half (fullscreen / antialiasing / vsync / | fn create_android_window | — | platform seam |
| 8 | `crates/oh-app/src/android.rs:241` | no-op | // window the set is a no-op — the launcher icon is the APK manifest's | fn create_android_window | — | platform seam |
| 9 | `crates/oh-app/src/android.rs:356` | no-op | // `NoDiscordCore` manager is that arm: every method a logged no-op | fn run | — | platform seam |
| 10 | `crates/oh-app/src/android.rs:359` | NoDiscord | Arc::new(oh_discord::DiscordManager::new(oh_discord::NoDiscordCore)) | fn run | — | platform seam |
| 11 | `crates/oh-app/src/bridge.rs:51` | out of scope | //! debug console — issue 14 step (b) territory, out of scope here.) | module docs (crates/oh-app/src/bridge.rs) | — | not-yet-ported |
| 12 | `crates/oh-app/src/bridge.rs:92` | placeholder | /// color / origin — the texture is the composition's placeholder; the C++ | fn sprite_from_state | C++: HexagonGame.cpp:324-325 (verified) | draw-path remainder |
| 13 | `crates/oh-app/src/bridge.rs:94` | placeholder | /// `HexagonGame.cpp:324-325` — step (c) replaces the placeholders with the | fn sprite_from_state | C++: HexagonGame.cpp:324-325 (verified) | draw-path remainder |
| 14 | `crates/oh-app/src/bridge.rs:130` | placeholder | // `HexagonGame.cpp:324-325`) — placeholders in step (a); step (c) | struct AppDraw | C++: HexagonGame.cpp:324-325 (verified) | draw-path remainder |
| 15 | `crates/oh-app/src/bridge.rs:185` | placeholder | // C++ `initKeyIcons` (`HexagonGame.cpp:87-107`) — the placeholder | fn new | C++: HexagonGame.cpp:87-107 (initKeyIcons def @87 — verified) | draw-path remainder |
| 16 | `crates/oh-app/src/client.rs:117` | unported | // mutation API is unported; headless replay runs never create replays | fn main_client | — | not-yet-ported |
| 17 | `crates/oh-app/src/client.rs:121` | no-op | // takes the None/no-op equivalents its seams already accept.) | fn main_client | C++: src/SSVOpenHexagon/Core/main.cpp:468-470 (verified) | platform seam |
| 18 | `crates/oh-app/src/compose.rs:68` | NoJoystick | Audio, NoJoystick, NullReplayStore, PcgCosmeticRng, Profiles, ScriptError, ScriptHost, | module-level | — | platform seam |
| 19 | `crates/oh-app/src/compose.rs:87` | no-op | /// The `set_shader_uniform_*` methods are no-ops: C++ routes them to | struct RealAssetsApi | — (routes to SFML `setUniformUnsafe` on the GL surface; issue 10) | platform seam |
| 20 | `crates/oh-app/src/compose.rs:456` | not ported | /// not ported yet, issue 17; the step-(a) tests pass their | fn compose_with | — | not-yet-ported |
| 21 | `crates/oh-app/src/compose.rs:528` | NoJoystick | Arc::new(NoJoystick), | fn compose_with_config | — | platform seam |
| 22 | `crates/oh-app/src/dispatch.rs:118` | NoSteam | // (the NoSteam/feature-split backend; the server path never | fn main_server | — | platform seam |
| 23 | `crates/oh-app/src/dispatch.rs:263` | not ported | /// 2. (libsodium init `:615-621` — issue 16, not ported yet: no sodium use | fn run | C++: main.cpp:616 (sodium_init — verified; the comment's `:615-621` is main.cpp. The adjacent HexagonServer.cpp:1386-1398 citation is the server's SIGINT block — verified, different subject) | not-yet-ported |
| 24 | `crates/oh-app/src/lib.rs:26` | stub | //! not-yet-ported `mainServer` stub). | module docs (crates/oh-app/src/lib.rs) | C++: main.cpp:197 (mainServer — verified); GameEngine (SSVStart — UNVERIFIABLE) | not-yet-ported |
| 25 | `crates/oh-app/src/lifecycle.rs:11` | placeholder | /// C++ `sf::VideoMode::getDesktopMode()` placeholder (main.cpp:283 — | module-level | C++: main.cpp:283 (verified — `Config::reapplyResolution` call site; `sf::VideoMode::getDesktopMode` itself: Config.cpp:480/487) | not-yet-ported |
| 26 | `crates/oh-app/src/lifecycle.rs:148` | no-op | /// is a silent no-op when present (the C++ `isFolder()` early return — | fn folder_created_once_idempotent_after | C++: main.cpp:69 (`path.isFolder()` early return — verified; also Assets.cpp:318/493) | test-double-or-legit-no-op |
| 27 | `crates/oh-app/src/lua_console_ui.rs:587` | stub | // &ilcTextEditCallbackStub, ...)` (HGUpdate.cpp:1419-1549). | fn console_contents | C++: HGUpdate.cpp:1419-1549 (verified — contains `ilcTextEditCallbackStub` use @1426; def @1118) | test-double-or-legit-no-op |
| 28 | `crates/oh-app/src/menu_draw.rs:11` | no-op | //! explicit no-op; this module ports the *draw* half. It lives in `oh-app` | module docs (crates/oh-app/src/menu_draw.rs) | C++: src/SSVOpenHexagon/Core/MenuGame.cpp (file verified) | draw-path remainder |
| 29 | `crates/oh-app/src/menu_draw.rs:78` | documented no-op | //! `oh_menu` update side carries documented no-ops | module docs (crates/oh-app/src/menu_draw.rs) | C++: MenuGame.cpp:2349 (`resetNamesScrolls` call site — verified) | draw-path remainder |
| 30 | `crates/oh-app/src/menu_draw.rs:89` | placeholder | //! **The sprite textures are 1×1 placeholders.** The C++ textures | module docs (crates/oh-app/src/menu_draw.rs) | — | draw-path remainder |
| 31 | `crates/oh-app/src/menu_draw.rs:94` | placeholder | //! renders them (the 1×1 placeholder texture uploads as-is — the | module docs (crates/oh-app/src/menu_draw.rs) | — | draw-path remainder |
| 32 | `crates/oh-app/src/menu_draw.rs:95` | placeholder | //! placeholder *size* still shifts the `getGlobalBottom`-relative | module docs (crates/oh-app/src/menu_draw.rs) | C++: `ssvs::getGlobalBottom` (SSVStart — def UNVERIFIABLE; C++ use sites verified, e.g. MenuGame.cpp:3160/5814) | draw-path remainder |
| 33 | `crates/oh-app/src/menu_draw.rs:226` | placeholder | /// both are the 1×1 placeholder, see the module docs). | struct MenuDraw | — | draw-path remainder |
| 34 | `crates/oh-app/src/menu_draw.rs:325` | placeholder | let placeholder = Texture::create(1, 1); | fn new | — | draw-path remainder |
| 35 | `crates/oh-app/src/menu_draw.rs:383` | placeholder | title_bar: Sprite::new(placeholder.clone()), | fn new | — | draw-path remainder |
| 36 | `crates/oh-app/src/menu_draw.rs:384` | placeholder | credits_bar1: Sprite::new(placeholder.clone()), | fn new | — | draw-path remainder |
| 37 | `crates/oh-app/src/menu_draw.rs:385` | placeholder | credits_bar2: Sprite::new(placeholder.clone()), | fn new | — | draw-path remainder |
| 38 | `crates/oh-app/src/menu_draw.rs:386` | placeholder | s_online: Sprite::new(placeholder.clone()), | fn new | — | draw-path remainder |
| 39 | `crates/oh-app/src/menu_draw.rs:387` | placeholder | epilepsy_warning: Sprite::new(placeholder), | fn new | — | draw-path remainder |
| 40 | `crates/oh-app/src/menu_draw.rs:867` | placeholder | // C++ `{20.f, getGlobalBottom(titleBar) + 8}` (the placeholder | fn draw | C++: MenuGame.cpp:5814 (`sf::Vector2f{20.f, ssvs::getGlobalBottom(titleBar) + 8}` — verified, exact match) | draw-path remainder |
| 41 | `crates/oh-app/src/menu_draw.rs:1687` | placeholder | // (creditsBar2) + 10.f)` (the placeholder texture shifts the value by | fn draw_options_submenus | C++: MenuGame.cpp:3160 (`ssvs::getGlobalBottom(creditsBar1)` — verified) | draw-path remainder |
| 42 | `crates/oh-app/src/menu_draw.rs:3767` | placeholder | // swap — both textures are placeholders in the port, module docs; the | fn draw_online_status | — | draw-path remainder |
| 43 | `crates/oh-app/src/menu_draw.rs:3983` | Noop | use oh_menu::seams::{FakeAssets, FakeConfig, LoadResults, MenuConfig, NoopNetwork}; | mod tests | — | test-double-or-legit-no-op |
| 44 | `crates/oh-app/src/menu_draw.rs:4035` | Noop | let network = Rc::new(NoopNetwork); | fn menu_and_app | — | test-double-or-legit-no-op |
| 45 | `crates/oh-app/src/menu_draw.rs:4149` | Noop | let network = Rc::new(NoopNetwork); | fn menu_with_config | — | test-double-or-legit-no-op |
| 46 | `crates/oh-app/src/menu_draw.rs:4248` | Noop | // favorites, an offline `NoopNetwork` → no RANKED badges, the | fn level_selection_draws_both_panels_in_cpp_order | — | test-double-or-legit-no-op |
| 47 | `crates/oh-app/src/print_lua_docs.rs:40` | unported | ///   mutation API is unported — issue 17); the printDocs path never reads | fn print_lua_docs | — | not-yet-ported |
| 48 | `crates/oh-app/src/steam.rs:18` | stub | //! against the loader stub cannot even start without the Steam client) | module docs (crates/oh-app/src/steam.rs) | C++: Steam.cpp:961-1062 (no-SDK `#else` block — verified); HexagonGame.hpp:90-92 (`steamAttempt`/`steamHung` fields — verified) | platform seam |
| 49 | `crates/oh-app/src/steam.rs:19` | NoSteam | //! the manager runs on the `NoSteam` backend — the C++'s | module docs (crates/oh-app/src/steam.rs) | C++: Steam.cpp:961-1062 (no-SDK `#else` block — verified); HexagonGame.hpp:90-92 (`steamAttempt`/`steamHung` fields — verified) | platform seam |
| 50 | `crates/oh-app/src/steam.rs:21` | no-op | //! every method is a logged no-op, and the game is fully playable | module docs (crates/oh-app/src/steam.rs) | C++: HexagonGame.hpp:90-92 (verified) | platform seam |
| 51 | `crates/oh-app/src/steam.rs:41` | NoSteam | Box::new(oh_steam::NoSteam) | fn backend | — | platform seam |
| 52 | `crates/oh-app/src/windowed.rs:16` | not ported | //!     [mustRecreate → recreateWindow]     // not ported: created once | module docs (crates/oh-app/src/windowed.rs) | C++: GameWindow.hpp:126 / GameTimer.hpp:26-37 / GameEngine.hpp:96-101 (all SSVStart — UNVERIFIABLE) | not-yet-ported |
| 53 | `crates/oh-app/src/windowed.rs:19` | no-op | //!     gameEngine->refreshTimer()          // GameTimer::refresh no-op (GameTimer.hpp:26-37) | module docs (crates/oh-app/src/windowed.rs) | C++: GameTimer.hpp:26-37 (`GameTimer::refresh` — SSVStart — UNVERIFIABLE) | not-yet-ported |
| 54 | `crates/oh-app/src/windowed.rs:67` | not ported | //!   the states' actions. The SSVStart input manager is not ported, so the | module docs (crates/oh-app/src/windowed.rs) | C++: GameWindow.hpp:79 + GameEngine.hpp:58-62 (SSVStart — UNVERIFIABLE); HexagonGame.cpp:335 (`game.onAnyEvent += Imgui::processEvent` — verified) | not-yet-ported |
| 55 | `crates/oh-app/src/windowed.rs:73` | no-op | //!   is a no-op for both states here: the C++ `HexagonGame`'s `onAnyEvent` | module docs (crates/oh-app/src/windowed.rs) | C++: HexagonGame.cpp:335 + main.cpp:436-440/451-454 (verified) | not-yet-ported |
| 56 | `crates/oh-app/src/windowed.rs:75` | unported | //!   unported) and `MenuGame` registers none. | module docs (crates/oh-app/src/windowed.rs) | C++: main.cpp:436-440/451-454 (verified) | not-yet-ported |
| 57 | `crates/oh-app/src/windowed.rs:78` | unported | //!   (SSVStart input manager) is unported with the input management above; | module docs (crates/oh-app/src/windowed.rs) | C++: MenuGame.cpp:259 (`game.onDraw += draw` — verified); main.cpp:451-454 (verified) | not-yet-ported |
| 58 | `crates/oh-app/src/windowed.rs:79` | no-op | //!   the wired closure documents the no-op. | module docs (crates/oh-app/src/windowed.rs) | C++: MenuGame.cpp:259 (`game.onDraw += draw` — verified); main.cpp:451-454 (verified) | not-yet-ported |
| 59 | `crates/oh-app/src/windowed.rs:141` | Noop | use oh_menu::{AudioSink, MenuConfig, MenuGame, NoopNetwork, RealMenuAssets}; | module-level | — | platform seam |
| 60 | `crates/oh-app/src/windowed.rs:418` | unported | // `Config::loadAllJoystickBinds()` tail is the unported joystick | fn refresh_binds | C++: HexagonGame.cpp:448 / MenuGame.cpp:3387 (`Config::loadAllJoystickBinds` — verified) | not-yet-ported |
| 61 | `crates/oh-app/src/windowed.rs:419` | documented no-op | // (NoJoystick seam) — documented no-op. | fn refresh_binds | — | platform seam |
| 62 | `crates/oh-app/src/windowed.rs:452` | documented no-op | // seam does not hold — documented no-op. | fn set_resolution_auto | — | platform seam |
| 63 | `crates/oh-app/src/windowed.rs:458` | unported | // equivalent is unported, so (as with `FakeConfig`) the list is | fn resolution_modes | — | not-yet-ported |
| 64 | `crates/oh-app/src/windowed.rs:898` | no-op | // no-op (the cache is per-process, so a new run clears it). | fn clear_lua_file_cache | — | test-double-or-legit-no-op |
| 65 | `crates/oh-app/src/windowed.rs:1076` | no-op | /// A no-op target (headless windows / GL-less fallback): the C++'s | struct NullDrawTarget | C++: `window == nullptr` guard (e.g. HGGraphics.cpp:307 — verified) | platform seam |
| 66 | `crates/oh-app/src/windowed.rs:1078` | no-op | /// `window_present()` guard is what actually no-ops the game draw; this | struct NullDrawTarget | — (`window_present()` is the port's own bridge guard; C++ equivalent = the `window == nullptr` guards) | test-double-or-legit-no-op |
| 67 | `crates/oh-app/src/windowed.rs:1476` | unported | // lane (the `mouseHovering` state is unported — no menu item tracking | fn register_menu_binds | C++: MenuGame.cpp:925/934 (`mouseHovering` — verified) | draw-path remainder |
| 68 | `crates/oh-app/src/windowed.rs:1595` | unported | // consumer (MenuGame.cpp:2852) is the unported `MenuGame::draw` lane | fn register_menu_binds | C++: MenuGame.cpp:2852 (verified) | draw-path remainder |
| 69 | `crates/oh-app/src/windowed.rs:1640` | unported | // (issue 13b, unported). | fn register_menu_binds | — | draw-path remainder |
| 70 | `crates/oh-app/src/windowed.rs:1644` | TODO( | // TODO(010b-menu-screenshot): C++ `mustTakeScreenshot = true` | fn register_menu_binds | C++: MenuGame.cpp:987 (`mustTakeScreenshot = true` — verified; the cited :1000 is 13 lines off) | load-bearing |
| 71 | `crates/oh-app/src/windowed.rs:1646` | unported | // unported `MenuGame::draw` (issue 13b). | fn register_menu_binds | — | draw-path remainder |
| 72 | `crates/oh-app/src/windowed.rs:1688` | unported | // the favorites-drawer half is the unported 13b detail. | fn register_menu_binds | C++: MenuGame.cpp:1004/4835 (`addRemoveFavoriteLevel` — verified; the cited :1020 is off) | load-bearing |
| 73 | `crates/oh-app/src/windowed.rs:1692` | TODO( | // TODO(010b-menu-favorite): C++ `addRemoveFavoriteLevel()` | fn register_menu_binds | C++: MenuGame.cpp:1004 (F1 binding) / :4835 (def) — verified; cited :1020 off | load-bearing |
| 74 | `crates/oh-app/src/windowed.rs:1693` | unported | // (MenuGame.cpp:1020) — the favorites toggle is the unported | fn register_menu_binds | C++: MenuGame.cpp:1004 (F1 binding) / :4835 (def) — verified; cited :1020 off | load-bearing |
| 75 | `crates/oh-app/src/windowed.rs:1705` | TODO( | // TODO(010b-menu-favorites-toggle): C++ | fn register_menu_binds | C++: MenuGame.cpp:1008-1009 (F2 binding `switchToFromFavoriteLevels` — verified; the cited :1024 is off) | load-bearing |
| 76 | `crates/oh-app/src/windowed.rs:1707` | unported | // favorites view switch is the unported issue 13b detail. | fn register_menu_binds | — | load-bearing |
| 77 | `crates/oh-app/src/windowed.rs:1898` | Noop | // engine-audio / `NoopAudio` menu-sink (the missed subsystem). | fn build | — | platform seam |
| 78 | `crates/oh-app/src/windowed.rs:1937` | Noop | // 20/21/17; `NoopNetwork` / `NoopAudio` / the real `Assets` via | fn build | — | platform seam |
| 79 | `crates/oh-app/src/windowed.rs:2028` | Noop | Rc::new(NoopNetwork), | fn build | — | platform seam |
| 80 | `crates/oh-app/src/windowed.rs:2055` | unported | // `Config::loadAllJoystickBinds()` tail is the unported joystick | fn build | C++: HexagonGame.cpp:448 / MenuGame.cpp:3387 (verified) | not-yet-ported |
| 81 | `crates/oh-app/src/windowed.rs:2056` | documented no-op | // — documented no-op). | fn build | — | platform seam |
| 82 | `crates/oh-app/src/windowed.rs:2412` | no-op | // the next-impl swap is a no-op — `mainClient` installs the timer | fn tick_fields | C++: main.cpp:330-332 (`window->setTimer` — verified); GameWindow.hpp:42-83/133 + GameEngine.hpp:80-86 (SSVStart — UNVERIFIABLE) | test-double-or-legit-no-op |
| 83 | `crates/oh-app/src/windowed.rs:2417` | documented no-op | // `gameEngine->handleEvent` is the documented no-op, module docs). | fn tick_fields | C++: GameEngine.hpp:80-86/40-50 + GameState.hpp:49-57 + GameWindow.hpp:131-137 (SSVStart — UNVERIFIABLE) | not-yet-ported |
| 84 | `crates/oh-app/src/windowed.rs:2564` | no-op | /// 1. `clear` (the pre-update framebuffer clear — headless: no-op); | fn frame | C++: GameEngine.hpp:80-86/89-94/96-101 (SSVStart — UNVERIFIABLE) | test-double-or-legit-no-op |
| 85 | `crates/oh-app/src/windowed.rs:2565` | no-op | /// 2. `runEvents` (the caller's event loop / the test's no-op — the | fn frame | C++: GameEngine.hpp:80-86/89-94/96-101 (SSVStart — UNVERIFIABLE) | test-double-or-legit-no-op |
| 86 | `crates/oh-app/src/windowed.rs:2605` | Noop | use oh_menu::seams::{FakeAssets, FakeConfig, NoopNetwork}; | mod tests | — | test-double-or-legit-no-op |
| 87 | `crates/oh-app/src/windowed.rs:2623` | Noop | Rc::new(NoopNetwork), | fn release_edge_advances_the_boot_screens | — | test-double-or-legit-no-op |
| 88 | `crates/oh-app/src/windowed.rs:2712` | Noop | Rc::new(NoopNetwork), | fn device_event_chain_advances_the_boot_screens | — | test-double-or-legit-no-op |
| 89 | `crates/oh-app/src/windowed.rs:2789` | no-op | /// `TextEntered` fold arm was a no-op, so `enteredStr` could never | fn entered_text_chain_accumulates_the_entered_str | C++: MenuGame.cpp:491-498 (`TextEntered` handler) + :2862-2878 (`enteredChars` accumulation) — verified (regression-test doc) | test-double-or-legit-no-op |
| 90 | `crates/oh-app/src/windowed.rs:2799` | Noop | Rc::new(NoopNetwork), | fn entered_text_chain_accumulates_the_entered_str | — | test-double-or-legit-no-op |

## oh-assets (5 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 91 | `crates/oh-assets/src/asset_storage.rs:15` | no-op | //!   - a second load of the same id is a no-op and returns `false` (first | module docs (crates/oh-assets/src/asset_storage.rs) | — | test-double-or-legit-no-op |
| 92 | `crates/oh-assets/src/asset_storage.rs:70` | no-op | // Second load is a no-op: `inserted` is false, C++ returns | fn try_emplace_and_load | — | test-double-or-legit-no-op |
| 93 | `crates/oh-assets/src/asset_storage.rs:93` | no-op | // `emplace`-then-load — a second load of the same id is a no-op). | fn load_sound_buffer | — | test-double-or-legit-no-op |
| 94 | `crates/oh-assets/src/asset_storage.rs:167` | Noop | fn failed_load_keeps_a_slot_and_second_load_is_noop() { | mod tests | — | test-double-or-legit-no-op |
| 95 | `crates/oh-assets/src/asset_storage.rs:177` | no-op | // second load of the same id is a no-op (first load wins). | fn failed_load_keeps_a_slot_and_second_load_is_noop | — | test-double-or-legit-no-op |

## oh-audio (3 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 96 | `crates/oh-audio/src/lib.rs:7` | Noop | //! `NoopAudio` (menu) / `None` (engine) stand-ins. It implements the | module docs (crates/oh-audio/src/lib.rs) | C++: src/SSVOpenHexagon/Core/Audio.cpp:25 (`Audio::AudioImpl` — verified) | platform seam |
| 97 | `crates/oh-audio/src/lib.rs:52` | no-op | //! no-ops while the object state (volume / pitch / offset / playing) is | module docs (crates/oh-audio/src/lib.rs) | — | platform seam |
| 98 | `crates/oh-audio/src/lib.rs:56` | no-op | //! logcat, not silently no-op'd). | module docs (crates/oh-audio/src/lib.rs) | — | platform seam |

## oh-config (2 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 99 | `crates/oh-config/src/config.rs:2039` | documented no-op | /// error in C++). The port keeps the name as a documented no-op. | fn joystick_binds_sanity_check | C++: Config.hpp:219 (declaration — verified; no definition in Config.cpp — verified dead) | dead C++ |
| 100 | `crates/oh-config/src/config.rs:2041` | no-op | // no-op: C++ has no definition. | fn joystick_binds_sanity_check | C++: Config.hpp:219 (declaration — verified; no definition in Config.cpp — verified dead) | dead C++ |

## oh-discord (18 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 101 | `crates/oh-discord/src/core.rs:59` | NoDiscord | /// `crates/oh-discord/tests/manager.rs`). [`crate::no_discord::NoDiscordCore`] | trait DiscordCore | C++: Discord.cpp:187-219 (no-SDK `#else` block — verified) | platform seam |
| 102 | `crates/oh-discord/src/core.rs:60` | NoSteam | /// is the no-SDK default (mirrors `oh-workshop-uploader`'s `NoSteam`): | trait DiscordCore | C++: Discord.cpp:187-219 (no-SDK `#else` block — verified) | platform seam |
| 103 | `crates/oh-discord/src/core.rs:74` | documented no-op | /// meaningful on the Game-SDK route; the IPC impl is a documented no-op | fn set_log_hook | — | platform seam |
| 104 | `crates/oh-discord/src/ipc.rs:14` | documented no-op | //!   equivalent — documented no-ops (`register_*` return `true`, the C++'s | module docs (crates/oh-discord/src/ipc.rs) | C++: Discord.cpp:79 (`_core->RunCallbacks` — verified) | platform seam |
| 105 | `crates/oh-discord/src/ipc.rs:67` | documented no-op | /// crate logs via the `log` crate. Documented no-op. | fn set_log_hook | — | platform seam |
| 106 | `crates/oh-discord/src/ipc.rs:71` | documented no-op | /// `activity_manager.h:11` — the party-join command). Documented no-op; | fn register_command | C++: public/discord/activity_manager.h:11 (`RegisterCommand` — verified) | platform seam |
| 107 | `crates/oh-discord/src/ipc.rs:78` | documented no-op | /// `activity_manager.h:12` — Steam-linking). Documented no-op; returns | fn register_steam | C++: public/discord/activity_manager.h:12 (`RegisterSteam` — verified) | platform seam |
| 108 | `crates/oh-discord/src/lib.rs:23` | NoDiscord | //!   default is [`NoDiscordCore`], and tests use a fake. | module docs (crates/oh-discord/src/lib.rs) | C++: src/SSVOpenHexagon/Core/main.cpp:264-269 (`discord_manager` — verified) | platform seam |
| 109 | `crates/oh-discord/src/lib.rs:56` | NoDiscord | pub use no_discord::NoDiscordCore; | mod no_discord | — | platform seam |
| 110 | `crates/oh-discord/src/no_discord.rs:2` | NoSteam | //! `oh-workshop-uploader`'s `NoSteam`: `init` fails, so a | module docs (crates/oh-discord/src/no_discord.rs) | C++: Discord.cpp:187-219 (verified) | platform seam |
| 111 | `crates/oh-discord/src/no_discord.rs:12` | NoSteam | //! loud, as the `NoSteam` pattern intends. | module docs (crates/oh-discord/src/no_discord.rs) | — | platform seam |
| 112 | `crates/oh-discord/src/no_discord.rs:17` | NoDiscord | pub struct NoDiscordCore; | module-level | — | platform seam |
| 113 | `crates/oh-discord/src/no_discord.rs:19` | NoDiscord | impl DiscordCore for NoDiscordCore { | struct NoDiscordCore | C++: Discord.cpp:25-28 (verified) | platform seam |
| 114 | `crates/oh-discord/src/no_discord.rs:27` | NoDiscord | unimplemented!("NoDiscordCore: never initialized") | fn set_log_hook | — | platform seam |
| 115 | `crates/oh-discord/src/no_discord.rs:31` | NoDiscord | unimplemented!("NoDiscordCore: never initialized") | fn register_command | — | platform seam |
| 116 | `crates/oh-discord/src/no_discord.rs:35` | NoDiscord | unimplemented!("NoDiscordCore: never initialized") | fn register_steam | — | platform seam |
| 117 | `crates/oh-discord/src/no_discord.rs:39` | NoDiscord | unimplemented!("NoDiscordCore: never initialized") | fn run_callbacks | — | platform seam |
| 118 | `crates/oh-discord/src/no_discord.rs:43` | NoDiscord | unimplemented!("NoDiscordCore: never initialized") | fn update_activity | — | platform seam |

## oh-game (27 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 119 | `crates/oh-game/src/custom_timeline.rs:44` | no-op | /// Lua `ct_*` API handle validation (issue 007 logs + no-ops invalid | fn is_handle_valid | C++: HGScripting.cpp:471 (`checkHandle` — verified) | test-double-or-legit-no-op |
| 120 | `crates/oh-game/src/custom_wall_manager.rs:7` | not ported | //! has no definition (dead/unused) and is therefore NOT ported. | module docs (crates/oh-game/src/custom_wall_manager.rs) | C++: CCustomWallManager.hpp:50 (`destroyAllOutOfBounds` declaration — verified; no definition in CCustomWallManager.cpp — verified dead) | dead C++ |
| 121 | `crates/oh-game/src/custom_wall_manager.rs:126` | no-op | /// `destroy(handle)` — no-ops (with a log) if the handle is not live. | fn destroy | — | test-double-or-legit-no-op |
| 122 | `crates/oh-game/src/lua_console.rs:59` | simplif | "## Utility Functions (u_)\n\nBelow are the utility functions, which can be identified with the \"u_\" prefix. These are overall functions that eithe… | module-level | — (C++ Lua doc text ported verbatim; no C++ site named) | test-double-or-legit-no-op |
| 123 | `crates/oh-game/src/lua_console.rs:294` | no-op | // to `int` yields `-1` (no-op browse); the `i32` here does the | fn history | — | test-double-or-legit-no-op |
| 124 | `crates/oh-game/src/lua_console.rs:360` | divergenc | /// Documented divergence from libstdc++ (reachable only via exotic console | fn parse_stod | — | determinism-risk |
| 125 | `crates/oh-game/src/lua_console.rs:1767` | no-op | // At 0, Up is a no-op (C++ `else if (ilcHistoryPos > 0)`). | fn history_up_walks_and_stops_at_zero | C++: HGUpdate.cpp:1248-1251 (`ilcHistoryPos` — verified) | test-double-or-legit-no-op |
| 126 | `crates/oh-game/src/lua_console.rs:1895` | Noop | fn tracked_results_noop_when_nothing_tracked() { | fn tracked_results_unknown_error_string | — | test-double-or-legit-no-op |
| 127 | `crates/oh-game/src/lua_console.rs:1926` | divergenc | // Documented divergence: C++ hex floats (`stod("0x1p3")` → 8) are | fn parse_stod_pins_libstdcxx_behaviors | — | determinism-risk |
| 128 | `crates/oh-game/src/lua_console.rs:1938` | NoJoystick | use crate::seams::{NoJoystick, NoneSteamSink, NullProfiles, NullReplayStore, PcgCosmeticRng}; | fn parse_stod_pins_libstdcxx_behaviors | — | test-double-or-legit-no-op |
| 129 | `crates/oh-game/src/lua_console.rs:1958` | NoJoystick | Arc::new(NoJoystick), | fn bare_engine | — | test-double-or-legit-no-op |
| 130 | `crates/oh-game/src/lua_console.rs:2067` | no-op | game.set_ilc_show_console_next(); // no-op (debug=false) | fn post_update_runs_the_gate_and_the_toggle_swap | — | test-double-or-legit-no-op |
| 131 | `crates/oh-game/src/lua_state.rs:167` | no-op | /// C++ `backgroundCamera->setRotation(value)` (no-op when no camera; the | fn set_background_camera_rotation | C++: HGScripting.cpp:665-667 (`backgroundCamera->setRotation` — verified) | test-double-or-legit-no-op |
| 132 | `crates/oh-game/src/lua_state.rs:174` | no-op | /// C++ `steamManager->unlock_achievement(id)` (no-op when there is no | fn unlock_achievement | C++: Steam.cpp:145/328 (`unlock_achievement` — verified) | test-double-or-legit-no-op |
| 133 | `crates/oh-game/src/renderer.rs:41` | no-op | /// A no-op renderer that records every flash batch it is shown, so tests can | struct RecordingRenderer | — | test-double-or-legit-no-op |
| 134 | `crates/oh-game/src/seams.rs:68` | NoAudio | pub struct NoAudio; | fn set_current_music_pitch | — | platform seam |
| 135 | `crates/oh-game/src/seams.rs:70` | NoAudio | impl Audio for NoAudio { | struct NoAudio | — | platform seam |
| 136 | `crates/oh-game/src/seams.rs:131` | no-op | /// In-memory no-op store (always "succeeds"; nothing is written). | struct NullReplayStore | — | platform seam |
| 137 | `crates/oh-game/src/seams.rs:352` | NoJoystick | pub struct NoJoystick; | fn rising_edge_id | — | platform seam |
| 138 | `crates/oh-game/src/seams.rs:354` | NoJoystick | impl Joystick for NoJoystick { | struct NoJoystick | — | platform seam |
| 139 | `crates/oh-game/src/seams.rs:432` | NoSteam | /// (or without a Steam client) the manager's `NoSteam` backend makes every | trait SteamSink | C++: HexagonGame.hpp:90-92 (verified) + HGUpdate.cpp:148 (`set_rich_presence_in_game` — verified) | platform seam |
| 140 | `crates/oh-game/src/seams.rs:433` | no-op | /// call a logged no-op — the C++'s graceful-degradation model | trait SteamSink | C++: HexagonGame.hpp:90-92 (verified) + HGUpdate.cpp:148 (`set_rich_presence_in_game` — verified) | platform seam |
| 141 | `crates/oh-game/src/seams.rs:467` | no-op | /// (every call site is a no-op; the C++ gates them with | struct NoneSteamSink | — | platform seam |
| 142 | `crates/oh-game/src/speed_data.rs:51` | no-op | /// When `accel == 0` the update is a no-op. | fn update | — | test-double-or-legit-no-op |
| 143 | `crates/oh-game/src/update.rs:682` | NoJoystick | /// half — `NoJoystick` makes every query false until issue 10 feeds it). | fn update_input_update_joystick_controls | — | platform seam |
| 144 | `crates/oh-game/src/update.rs:1316` | no-op | /// positions/scales for the issue-11 draw pass); a no-op here, exactly | fn update_key_icons | — | draw-path remainder |
| 145 | `crates/oh-game/src/vertex_batch.rs:57` | no-op | /// `reserve(n)` — C++ no-ops when `capacity >= n`, else reallocates to `n`. | fn reserve | — | test-double-or-legit-no-op |

## oh-lua (20 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 146 | `crates/oh-lua/src/api/custom_timelines.rs:7` | no-op | //! no-ops — no Lua error (mapping §8.9). | module docs (crates/oh-lua/src/api/custom_timelines.rs) | — | test-double-or-legit-no-op |
| 147 | `crates/oh-lua/src/api/level_utils.rs:63` | no-op | // C++: `if (window == nullptr) return;` — headless no-op. | fn register | — | test-double-or-legit-no-op |
| 148 | `crates/oh-lua/src/api/menu.rs:8` | no-op | //! * ~70 "unused" level functions are replaced with no-ops. The list is the | module docs (crates/oh-lua/src/api/menu.rs) | C++: MenuGame.cpp:1089-1105+ (unused-function no-op list — verified) | test-double-or-legit-no-op |
| 149 | `crates/oh-lua/src/api/menu.rs:20` | no-op | /// The C++ no-op list, verbatim (including the historical `u_timelineWait` | module-level | C++: MenuGame.cpp:1092 (`u_timelineWait` in the C++ no-op list — verified) | test-double-or-legit-no-op |
| 150 | `crates/oh-lua/src/api/menu.rs:134` | no-op | // Unused functions -> zero-argument no-ops (C++ `lua.writeVariable(un, | fn register | C++: MenuGame.cpp:1085/1089 (`lua.writeVariable(un, ...)` — verified; also HGScripting.cpp:48) | test-double-or-legit-no-op |
| 151 | `crates/oh-lua/src/api/mod.rs:3` | stub | //! `HexagonGame::initLua` per-game groups, the menu-context stubs, and the | module docs (crates/oh-lua/src/api/mod.rs) | — | test-double-or-legit-no-op |
| 152 | `crates/oh-lua/src/api/mod.rs:14` | stub | //!   menu stubs); | module docs (crates/oh-lua/src/api/mod.rs) | — | test-double-or-legit-no-op |
| 153 | `crates/oh-lua/src/api/mod.rs:37` | no-op | //! no-ops the unused list). | module docs (crates/oh-lua/src/api/mod.rs) | — | test-double-or-legit-no-op |
| 154 | `crates/oh-lua/src/api/mod.rs:135` | no-op | /// getters, and the no-op "unused functions" list (mapping §8.10). | fn init_menu_api | — | test-double-or-legit-no-op |
| 155 | `crates/oh-lua/src/api/shaders.rs:5` | no-op | //! `withValidShaderId` (headless → no-op; invalid id → log + no-op), and | module docs (crates/oh-lua/src/api/shaders.rs) | C++: HGStatus.hpp:26 (`RenderStage` — verified) | test-double-or-legit-no-op |
| 156 | `crates/oh-lua/src/api/shaders.rs:7` | no-op | //! `RenderStage::Count == 9` (out of range → log + no-op; headless → no-op). | module docs (crates/oh-lua/src/api/shaders.rs) | C++: HGStatus.hpp:26 (`RenderStage` — verified) | test-double-or-legit-no-op |
| 157 | `crates/oh-lua/src/api/steam.rs:22` | no-op | // C++: no-op while replaying; only fires with a live steam | fn register | — | test-double-or-legit-no-op |
| 158 | `crates/oh-lua/src/convert.rs:302` | not implemented | /// table `_read` is `throw std::logic_error{"Not implemented"}`; array reads | impl | C++: LuaWrapper.hpp:1033 (`throw std::logic_error{"Not implemented"}` — verified) | test-double-or-legit-no-op |
| 159 | `crates/oh-lua/src/docs.rs:169` | placeholder | // around the placeholder survive (matches the C++ parser). | fn feature_on_mode_populates_entries | — | test-double-or-legit-no-op |
| 160 | `crates/oh-lua/src/metadata.rs:7` | placeholder | //! drop the C++ destructor resolves the `$N` placeholders in the docs and | module docs (crates/oh-lua/src/metadata.rs) | — | test-double-or-legit-no-op |
| 161 | `crates/oh-lua/src/metadata.rs:31` | no-op | //! proxy exactly as in C++, where their bodies are no-ops but the proxy | module docs (crates/oh-lua/src/metadata.rs) | — | test-double-or-legit-no-op |
| 162 | `crates/oh-lua/src/metadata.rs:47` | placeholder | /// Docs with `$N` placeholders resolved to the Nth argument name. | fn _docs | — | test-double-or-legit-no-op |
| 163 | `crates/oh-lua/src/metadata.rs:73` | simplif | "## Utility Functions (u_)\n\nBelow are the utility functions, which can be identified with the \"u_\" prefix. These are overall functions that eithe… | fn _docs | — | test-double-or-legit-no-op |
| 164 | `crates/oh-lua/src/metadata.rs:191` | placeholder | /// with the `$N` placeholders in the docs resolved to the Nth argument name. | struct LuaMetadataProxy | — | test-double-or-legit-no-op |
| 165 | `crates/oh-lua/src/metadata.rs:227` | placeholder | /// C++ `doc(docs)`: the doc text; `$0`, `$1`, ... placeholders are | fn doc | C++: `.doc()` call sites (HGScripting.cpp:65+ — verified); the method itself is SSVStart `LuaScripting` — UNVERIFIABLE | test-double-or-legit-no-op |

## oh-math (2 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 166 | `crates/oh-math/src/hg/color.rs:88` | approximat | /// The C++ literal is the deliberate approximation `3.14f` (Color.hpp:33, | fn transform_hue | C++: Color.hpp:33 (`std::cos(H * 3.14f / 180.f)` — verified; the port keeps the C++ literal) | test-double-or-legit-no-op |
| 167 | `crates/oh-math/src/ssvu/math.rs:4` | not ported | //! arithmetic, …) is NOT ported; see the issue report "known gaps". | module docs (crates/oh-math/src/ssvu/math.rs) | — | not-yet-ported |

## oh-menu (16 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 168 | `crates/oh-menu/src/dialog_box.rs:11` | out of scope | //! coordinates; a display/golden check is out of scope (D-011). | module docs (crates/oh-menu/src/dialog_box.rs) | — (test-scope statement, no C++ site) | unknown |
| 169 | `crates/oh-menu/src/lib.rs:45` | Noop | LevelInfo, MenuAssets, MenuConfig, NetEvent, NetworkClient, NewGameCallback, NoopNetwork, | mod states | — | platform seam |
| 170 | `crates/oh-menu/src/menu_game.rs:280` | unported | // unported joystick (NoJoystick seam). | fn set_ignore_all_inputs | — | platform seam |
| 171 | `crates/oh-menu/src/menu_game.rs:1135` | unported | /// unported `MenuGame::draw` lane (issue 13b) — the timer accumulates. | fn add_exit_timer | C++: MenuGame::draw lane (issue 13b; `mouseHovering` verified @MenuGame.cpp:925/934) | draw-path remainder |
| 172 | `crates/oh-menu/src/menu_game.rs:1213` | unported | /// `Online::setForceLeaderboardRefresh(true)` stays unported (dead in | fn init | C++: MenuGame.cpp:810 (`// Online::setForceLeaderboardRefresh(true)` — commented out — verified dead) | dead C++ |
| 173 | `crates/oh-menu/src/menu_game.rs:1675` | no-op | // the erase here is a no-op unless a bind control reports it. | fn erase_action | — | test-double-or-legit-no-op |
| 174 | `crates/oh-menu/src/real.rs:7` | documented no-op | //! profile *editor* API, issue 005); those are documented no-op seams here. | module docs (crates/oh-menu/src/real.rs) | — | not-yet-ported |
| 175 | `crates/oh-menu/src/real.rs:152` | documented no-op | // these are documented no-op seams (matching the C++ `p*` deferral). | fn profile_count | C++: Assets.cpp:1500-1511 (`getLocalProfileNames` def @1503 — verified) | not-yet-ported |
| 176 | `crates/oh-menu/src/seams.rs:17` | Noop | //!   [`NoopNetwork`] only ever reports `Disconnected` and no events. | module docs (crates/oh-menu/src/seams.rs) | — | platform seam |
| 177 | `crates/oh-menu/src/seams.rs:20` | Noop | //! The `fakes` submodule provides `FakeAssets` / `FakeConfig` / `NoopNetwork` | module docs (crates/oh-menu/src/seams.rs) | — | platform seam |
| 178 | `crates/oh-menu/src/seams.rs:21` | Noop | //! / `NoopAudio` so the crate's own tests (and issue 013's state-machine | module docs (crates/oh-menu/src/seams.rs) | — | platform seam |
| 179 | `crates/oh-menu/src/seams.rs:47` | unported | /// unported joystick (NoJoystick seam). | type IgnoreInputsCallback | — | platform seam |
| 180 | `crates/oh-menu/src/seams.rs:276` | no-op | /// `Config::setCurrentResolutionAuto(window)` (no-op seam default). | fn set_resolution_auto | C++: MenuGame.cpp:1322 / Config.cpp:669 (`setCurrentResolutionAuto` — verified) | platform seam |
| 181 | `crates/oh-menu/src/seams.rs:472` | no-op | /// request kick (MenuGame.cpp:5571-5575); a no-op seam offline. | fn request_scores | C++: UNCITED — names "request kick (MenuGame.cpp:5571-5575)" but those lines are local-profile score text (verified); the kick flow exists at HexagonClient.hpp:53 (`EKicked`) / HexagonServer.hpp:132 (`sendKick`) — verified | platform seam |
| 182 | `crates/oh-menu/src/seams.rs:1180` | Noop | pub struct NoopNetwork; | fn set_disable_game_rendering | — | platform seam |
| 183 | `crates/oh-menu/src/seams.rs:1182` | Noop | impl NetworkClient for NoopNetwork { | struct NoopNetwork | — | platform seam |

## oh-menu-system (1 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 184 | `crates/oh-menu-system/src/lib.rs:25` | out of scope | //! Menu **rendering** is out of scope (issue 013, MenuGame port). | module docs (crates/oh-menu-system/src/lib.rs) | — | not-yet-ported |

## oh-net (11 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 185 | `crates/oh-net/src/client.rs:967` | no-op | // C++ `setBlocking(true)` (`:485`) — a no-op on the port's | fn disconnect | C++: HexagonClient.cpp:485 (`_socket.setBlocking(true)` — verified) | test-double-or-legit-no-op |
| 186 | `crates/oh-net/src/client.rs:1265` | no-op | /// no-op (the replay is simply not sent — NOT a failure). | fn try_send_compressed_replay | — | test-double-or-legit-no-op |
| 187 | `crates/oh-net/src/client.rs:1729` | Noop | fn update_is_noop_without_socket() { | fn connect_failure_yields_connection_error_event | — | test-double-or-legit-no-op |
| 188 | `crates/oh-net/src/client.rs:1740` | no-op | client.update(); // must be a no-op (HexagonClient.cpp:779-782) | fn update_is_noop_without_socket | C++: HexagonClient.cpp:779-782 (`if(!_socketConnected)` — verified) | test-double-or-legit-no-op |
| 189 | `crates/oh-net/src/client.rs:2245` | Noop | fn compressed_replay_noop_when_level_unsupported() { | fn ready_gated_requests_refused_before_login | — | test-double-or-legit-no-op |
| 190 | `crates/oh-net/src/client.rs:2251` | no-op | // "lv3" is not in {lv1, lv2} → a `true` no-op | fn compressed_replay_noop_when_level_unsupported | C++: HexagonClient.cpp:893-899 (compressed-replay no-op — verified) | test-double-or-legit-no-op |
| 191 | `crates/oh-net/src/database.rs:354` | no-op | /// value `>= value` → **no-op** (`:310-313`); otherwise update the | fn add_score | C++: Database.cpp:310-313 (verified) | test-double-or-legit-no-op |
| 192 | `crates/oh-net/src/server.rs:2984` | no-op | // — the executor runs again, and the C++'s `addScore` is a no-op | fn replay | C++: Database.cpp:310-313 (`addScore` — verified) | test-double-or-legit-no-op |
| 193 | `crates/oh-net/src/server.rs:3008` | no-op | "the C++'s `addScore` no-op on the non-greater value" | fn replay | — | test-double-or-legit-no-op |
| 194 | `crates/oh-net/src/server.rs:3038` | stub | fn the_skeleton_dispatches_heartbeat_and_stubs_the_rest() { | fn construction_binds_and_computes_the_supported_validators | — | test-double-or-legit-no-op |
| 195 | `crates/oh-net/src/sodium.rs:191` | no-op | /// (see the module docs for why the zero key is not a no-op). | fn sodium_hash | — | test-double-or-legit-no-op |

## oh-render (37 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 196 | `crates/oh-render/src/audio.rs:11` | no-op | //! audio device) simply have no sink and the `play` paths no-op while the | module docs (crates/oh-render/src/audio.rs) | — | platform seam |
| 197 | `crates/oh-render/src/audio.rs:413` | no-op | /// failed Android/AAudio sink open is diagnosable (not silently no-op'd). | fn try_open_default | — | platform seam |
| 198 | `crates/oh-render/src/audio.rs:549` | Noop | fn headless_device_is_not_ready_and_play_noops() { | fn music_state_and_offset | — | test-double-or-legit-no-op |
| 199 | `crates/oh-render/src/batching.rs:27` | TODO( | //! `TODO(issue-009-wiring)` NOTE (issue-025c): the full-frame pipeline has | module docs (crates/oh-render/src/batching.rs) | — | draw-path remainder |
| 200 | `crates/oh-render/src/camera.rs:161` | no-op | /// No-op while dead (`status.hasDied`). | fn draw_set_camera_centers | C++: HGStatus.hpp:71 (`status.hasDied` — verified) | test-double-or-legit-no-op |
| 201 | `crates/oh-render/src/gl.rs:63` | FIXME | //!   FIXME); `GlParts.width/height` come from the native window, so the | module docs (crates/oh-render/src/gl.rs) | — (winit platform note, no C++ site) | unknown |
| 202 | `crates/oh-render/src/gl.rs:124` | documented no-op | /// is sticky for the window's life — the documented no-op batch). | struct BatchSlot | — | platform seam |
| 203 | `crates/oh-render/src/gl_passes.rs:46` | placeholder | //! game's PARTICLE sprites clone their (placeholder) texture every frame | module docs (crates/oh-render/src/gl_passes.rs) | — | draw-path remainder |
| 204 | `crates/oh-render/src/gl_passes.rs:63` | placeholder | //! off state — the menu's placeholder textures' default). The font atlas | module docs (crates/oh-render/src/gl_passes.rs) | — | draw-path remainder |
| 205 | `crates/oh-render/src/gl_passes.rs:121` | placeholder | /// particle sprites clone their placeholder texture per frame (a fresh | impl Eq | — | draw-path remainder |
| 206 | `crates/oh-render/src/gl_passes.rs:181` | no-op | /// compile/link failure documents a no-op (the `GlBatchUploader`'s | struct GlPasses | — | test-double-or-legit-no-op |
| 207 | `crates/oh-render/src/gl_passes.rs:508` | no-op | // no-op). | fn draw_sprite | — | test-double-or-legit-no-op |
| 208 | `crates/oh-render/src/glyphs.rs:128` | the port's analogue | /// it grows on demand, the port's analogue of SFML's in-font texture | fn new | — | test-double-or-legit-no-op |
| 209 | `crates/oh-render/src/glyphs.rs:649` | no-op | // raster, owns the no-op). | fn text_quads | — | test-double-or-legit-no-op |
| 210 | `crates/oh-render/src/graphics.rs:29` | TODO( | //! `TODO(issue-009-wiring)` markers below name the `HexagonGame` call sites | module docs (crates/oh-render/src/graphics.rs) | C++: HexagonGame call sites (file verified) | draw-path remainder |
| 211 | `crates/oh-render/src/graphics.rs:159` | no-op | /// Default no-op: the non-GL targets (headless/Recording) have nothing | fn present | — | platform seam |
| 212 | `crates/oh-render/src/graphics.rs:267` | TODO( | /// `TODO(issue-009-wiring)`: `HexagonGame::draw()` fills these from | struct FrameInput | C++: `HexagonGame::draw()` (file verified) | draw-path remainder |
| 213 | `crates/oh-render/src/graphics.rs:393` | TODO( | /// out of scope for the draw layer; `TODO(issue-009-wiring)`.) | fn draw_frame | — | draw-path remainder |
| 214 | `crates/oh-render/src/graphics.rs:395` | TODO( | /// `TODO(issue-009-wiring)`: called from `HexagonGame`'s `onDraw` | fn draw_frame | C++: `HexagonGame` `onDraw` (file verified) | draw-path remainder |
| 215 | `crates/oh-render/src/graphics.rs:710` | TODO( | // (mapping §3.4); out of scope here. `TODO(issue-009-wiring)`. | fn draw_frame | — | draw-path remainder |
| 216 | `crates/oh-render/src/graphics.rs:813` | TODO( | // `TODO(issue-009-wiring)`: called from `HexagonGame::update()` — | fn draw_swap_particles | — | draw-path remainder |
| 217 | `crates/oh-render/src/graphics.rs:1072` | TODO( | /// `TODO(issue-009-wiring)`: called on window creation/recreation | fn init_key_icons | — | draw-path remainder |
| 218 | `crates/oh-render/src/graphics.rs:1117` | TODO( | /// `TODO(issue-009-wiring)`: called from `HexagonGame::update()` when | fn update_key_icons | — | draw-path remainder |
| 219 | `crates/oh-render/src/graphics.rs:1254` | TODO( | /// `TODO(issue-009-wiring)`: called from `HexagonGame::update()`; the level | fn update_level_info | — | draw-path remainder |
| 220 | `crates/oh-render/src/graphics.rs:1383` | TODO( | // `TODO(issue-009-wiring)`: `HexagonGame::draw()` calls | fn draw_level_info | C++: `HexagonGame::draw()` (file verified) | draw-path remainder |
| 221 | `crates/oh-render/src/shader.rs:203` | no-op | ///   (already GLES — the idempotent no-op); | fn translate_for_gles | — | test-double-or-legit-no-op |
| 222 | `crates/oh-render/src/shader.rs:436` | Noop | fn translate_is_idempotent_noop_on_already_es_source() { | fn no_version_fragment_gets_version_and_precision_rest_verbatim | — | test-double-or-legit-no-op |
| 223 | `crates/oh-render/src/timer.rs:30` | not ported | //!   NOT ported: `mainClient` installs the timer exactly once before | module docs (crates/oh-render/src/timer.rs) | C++: main.cpp:330-331 (`window->setTimer<ssvs::TimerStatic>` — verified); GameEngine.hpp:52-56 + `TimerBase` (SSVStart — UNVERIFIABLE) | not-yet-ported |
| 224 | `crates/oh-render/src/view.rs:419` | no-op | // update() with no pending change is a no-op | fn camera_update_applies_and_clears_pending | — | test-double-or-legit-no-op |
| 225 | `crates/oh-render/src/window.rs:483` | no-op | /// pauses are no-ops, the C++'s no-GL `window == nullptr` equivalent); | fn ensure_android_gl | — | platform seam |
| 226 | `crates/oh-render/src/window.rs:549` | no-op | /// The sink's write half (the touch-event updates — `None` = no-op). | fn with_touch_sink | — | platform seam |
| 227 | `crates/oh-render/src/window.rs:557` | no-op | /// `None` = no-op; the `apply_event` release arms call it). | fn with_released_sink | — | platform seam |
| 228 | `crates/oh-render/src/window.rs:584` | no-op | /// `None` = no-op; the `apply_event` `TextEntered` arm calls it). | fn with_entered_text_sink | — | platform seam |
| 229 | `crates/oh-render/src/window.rs:868` | no-op | /// guard (HGGraphics.cpp:307) is the port's `NullDrawTarget` no-op. | fn save_screenshot | C++: HGGraphics.cpp:307 (`if(window != nullptr)` — verified) | platform seam |
| 230 | `crates/oh-render/src/window.rs:875` | no-op | /// context is attached. Headless (`gl() == None`) is a no-op (the C++'s | fn swap_buffers | — | test-double-or-legit-no-op |
| 231 | `crates/oh-render/src/window.rs:1057` | no-op | // Desktop: the statement is android-gated (no-op). | fn resumed | — | platform seam |
| 232 | `crates/oh-render/src/window.rs:1283` | no-op | // no-op (the `with_entered_text_sink` `None` arm) — no panic. | fn text_entered_fold_appends_to_the_entered_text_sink | — | test-double-or-legit-no-op |

## oh-server-control (1 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 233 | `crates/oh-server-control/src/lib.rs:220` | no-op | // empty controlMsg as a no-op, HexagonServer.cpp:344-346). | fn encode_is_u32be_length_plus_raw_bytes | C++: HexagonServer.cpp:344-346 (`controlMsg.empty()` — verified) | test-double-or-legit-no-op |

## oh-steam (11 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 234 | `crates/oh-steam/src/lib.rs:18` | NoSteam | //! * [`NoSteam`] — the no-SDK backend (default build = the C++'s | module docs (crates/oh-steam/src/lib.rs) | C++: Steam.cpp:961-1062 (no-SDK `#else` block — verified) | platform seam |
| 235 | `crates/oh-steam/src/lib.rs:55` | NoSteam | pub use no_steam::NoSteam; | mod steamworks_sdk | — | platform seam |
| 236 | `crates/oh-steam/src/manager.rs:12` | no-op | //! API"), EVERY method short-circuits to `false`/`nullopt`/no-op — no | module docs (crates/oh-steam/src/manager.rs) | C++: Steam.cpp:54-80 (`initialize_steamworks` — verified); HexagonGame.hpp:90-92 + HexagonGame.cpp:480/487 (`steamHung` — verified) | platform seam |
| 237 | `crates/oh-steam/src/no_steam.rs:5` | no-op | //! method returns `false` / `nullopt` / no-ops. The C++'s branch stores a | module docs (crates/oh-steam/src/no_steam.rs) | — | platform seam |
| 238 | `crates/oh-steam/src/no_steam.rs:15` | NoSteam | pub struct NoSteam; | module-level | — | platform seam |
| 239 | `crates/oh-steam/src/no_steam.rs:17` | NoSteam | impl SteamSdk for NoSteam { | struct NoSteam | — | platform seam |
| 240 | `crates/oh-steam/src/sdk.rs:31` | NoSteam | /// Implementors: [`crate::NoSteam`] (the no-SDK default — the C++'s | trait SteamSdk | C++: Steam.cpp:961-1062 (verified) | platform seam |
| 241 | `crates/oh-steam/src/sdk.rs:33` | no-op | /// everything is false/nullopt/no-op) and, behind the `steam` feature, | trait SteamSdk | — | platform seam |
| 242 | `crates/oh-steam/src/steamworks_sdk.rs:5` | stub | //! crate bundles deprecated it, and its loader stub no longer exports the | module docs (crates/oh-steam/src/steamworks_sdk.rs) | C++: public/steam/steam_api_flat.h:432 (external Steam SDK header — file in repo, line in range) | platform seam |
| 243 | `crates/oh-steam/src/steamworks_sdk.rs:81` | stub | // loader stub no longer exports the flat symbol — a plain link against | struct Events | — | platform seam |
| 244 | `crates/oh-steam/src/steamworks_sdk.rs:101` | stub | // the `steamworks` link has already loaded the official loader stub. | fn request_current_stats | — | platform seam |

## oh-workshop-uploader (16 rows)

| # | path:line | marker | comment (verbatim, trimmed) | enclosing item | C++ counterpart (verified at HEAD) | category (provisional) |
|---|---|---|---|---|---|---|
| 245 | `crates/oh-workshop-uploader/src/lib.rs:22` | NoSteam | //! is testable without Steam. This issue ships the trait + [`steam::NoSteam`] | module docs (crates/oh-workshop-uploader/src/lib.rs) | C++: src/OHWorkshopUploader/main.cpp:716-719 (`steam_helper` init — verified) | platform seam |
| 246 | `crates/oh-workshop-uploader/src/lib.rs:23` | TODO( | //! (a `TODO(issue-20)` no-SDK impl — `init` fails, so the binary takes the | module docs (crates/oh-workshop-uploader/src/lib.rs) | C++: src/OHWorkshopUploader/main.cpp:716-719 (`steam_helper` init — verified) | not-yet-ported |
| 247 | `crates/oh-workshop-uploader/src/main.rs:5` | NoSteam | //! [`oh_workshop_uploader::steam::SteamUgc`] impl is `NoSteam` (the | module docs (crates/oh-workshop-uploader/src/main.rs) | C++: src/OHWorkshopUploader/main.cpp:716-719 (`steam_helper` init — verified) | platform seam |
| 248 | `crates/oh-workshop-uploader/src/main.rs:6` | TODO( | //! `TODO(issue-20)` no-SDK seam), so without the Steam runtime the binary | module docs (crates/oh-workshop-uploader/src/main.rs) | — | not-yet-ported |
| 249 | `crates/oh-workshop-uploader/src/main.rs:14` | NoSteam | let mut steam = oh_workshop_uploader::steam::NoSteam; | fn main | — | platform seam |
| 250 | `crates/oh-workshop-uploader/src/readers.rs:37` | no-op | // EOF: the C++ clears the flags, ignores (a no-op at EOF) and | fn read_integer | — | test-double-or-legit-no-op |
| 251 | `crates/oh-workshop-uploader/src/steam.rs:20` | NoSteam | //! [`NoSteam`] is the no-SDK default shipped by this issue: `init` fails, so | module docs (crates/oh-workshop-uploader/src/steam.rs) | C++: src/OHWorkshopUploader/main.cpp:716-719 + public/steam/steam_api.h (verified) | platform seam |
| 252 | `crates/oh-workshop-uploader/src/steam.rs:23` | TODO( | //! the Steam runtime. TODO(issue-20): replace with the real `SteamAPI_*` / | module docs (crates/oh-workshop-uploader/src/steam.rs) | C++: public/steam/steam_api.h (external SDK — in repo); OHWorkshopUploader/main.cpp:4 | not-yet-ported |
| 253 | `crates/oh-workshop-uploader/src/steam.rs:462` | TODO( | /// TODO(issue-20): replace this with the real Steam UGC wiring (the C++ | struct NoSteam | C++: src/OHWorkshopUploader/main.cpp:126-500 (UGC wiring — in range; :126 = `class steam_helper`, verified) | not-yet-ported |
| 254 | `crates/oh-workshop-uploader/src/steam.rs:466` | NoSteam | pub struct NoSteam; | fn result_to_string | — | platform seam |
| 255 | `crates/oh-workshop-uploader/src/steam.rs:468` | NoSteam | impl SteamUgc for NoSteam { | struct NoSteam | — | platform seam |
| 256 | `crates/oh-workshop-uploader/src/steam.rs:475` | TODO( | unimplemented!("TODO(issue-20): wire the real Steam UGC (CreateItem)") | fn create_item | — | not-yet-ported |
| 257 | `crates/oh-workshop-uploader/src/steam.rs:478` | TODO( | unimplemented!("TODO(issue-20): wire the real Steam UGC (StartItemUpdate)") | fn start_item_update | — | not-yet-ported |
| 258 | `crates/oh-workshop-uploader/src/steam.rs:481` | TODO( | unimplemented!("TODO(issue-20): wire the real Steam UGC (SetItemContent)") | fn set_content | — | not-yet-ported |
| 259 | `crates/oh-workshop-uploader/src/steam.rs:484` | TODO( | unimplemented!("TODO(issue-20): wire the real Steam UGC (SetItemPreview)") | fn set_preview | — | not-yet-ported |
| 260 | `crates/oh-workshop-uploader/src/steam.rs:487` | TODO( | unimplemented!("TODO(issue-20): wire the real Steam UGC (SubmitItemUpdate)") | fn submit_update | — | not-yet-ported |

## Appendix A — UNCITED / drift findings (citations that failed verification)

Highest-value 28b input. “Verified” = file + line/symbol exists in this tree at HEAD.

1. **oh-menu/src/seams.rs:472 — `C++: UNCITED`.** Names “request kick (MenuGame.cpp:5571-5575)” but those lines are local-profile score text (verified); no `kick` in MenuGame.cpp. The kick flow exists at `include/SSVOpenHexagon/Core/HexagonClient.hpp:53` (`EKicked`) and `HexagonServer.hpp:132` (`sendKick`) (verified).
2. **Systematic line drift, oh-app/src/windowed.rs hardcoded-bind block.** The cited MenuGame.cpp lines are uniformly ~13–16 lines off (symbols all verified at the true lines): `mustTakeScreenshot` cited :1000 → actual :987; `addRemoveFavoriteLevel` cited :1020 → actual F1 binding :1004 / def :4835; `switchToFromFavoriteLevels` cited :1024 → actual F2 binding :1008-1009 / call :2753. Register rows 70, 72, 73/74, 75/76 (windowed.rs:1644, 1688, 1692/1693, 1705/1707) carry the corrected citations.
3. **oh-app/src/dispatch.rs:263.** “libsodium init `:615-621`” resolves to `src/SSVOpenHexagon/Core/main.cpp:616` (`sodium_init` — verified), not to the adjacent `HexagonServer.cpp:1386-1398` citation (that range is the server's SIGINT block — verified, different subject).
4. **oh-app/src/lifecycle.rs:11 (minor).** `main.cpp:283` is the `Config::reapplyResolution` call site; `sf::VideoMode::getDesktopMode` itself is `src/SSVOpenHexagon/Global/Config.cpp:480/487`.
5. **SSVStart UNVERIFIABLE (systematic).** All citations into `GameEngine.hpp`/`GameWindow.hpp`/`GameTimer.hpp`/`GameState.hpp`/`Manager.hpp` (windowed.rs:16/19/67/2412/2417/2564/2565, timer.rs:30, lib.rs:26) cannot be checked at this HEAD — `extlibs/SSVStart` is not checked out. Also the symbols `TimerBase`, `msUpdate`, `runEvents`, `GameEngine`, `ssvs::TimerStatic`, `ssvs::getGlobalBottom`, `LuaScripting::doc` (defs).

## Appendix B — load-bearing candidates (flagged for 28b triage)

Rows in category `LB` — dropped/stubbed behaviors a user would notice; 28b must decide (accepted / FIX / decision):

- `crates/oh-app/src/windowed.rs:1644` — // TODO(010b-menu-screenshot): C++ `mustTakeScreenshot = true`
- `crates/oh-app/src/windowed.rs:1688` — // the favorites-drawer half is the unported 13b detail.
- `crates/oh-app/src/windowed.rs:1692` — // TODO(010b-menu-favorite): C++ `addRemoveFavoriteLevel()`
- `crates/oh-app/src/windowed.rs:1693` — // (MenuGame.cpp:1020) — the favorites toggle is the unported
- `crates/oh-app/src/windowed.rs:1705` — // TODO(010b-menu-favorites-toggle): C++
- `crates/oh-app/src/windowed.rs:1707` — // favorites view switch is the unported issue 13b detail.

Also worth 28b's eye (not categorised LB, but user-visible omissions stated by the comments): `oh-app/src/lib.rs:26` (mainServer stub), `oh-game/src/update.rs:682` (joystick unwired until issue 10), `oh-menu/src/real.rs:7/152` (profile editor no-op seams), `oh-workshop-uploader/src/steam.rs:475-487` (Steam UGC `unimplemented!`), and the `menu_draw` 1×1 placeholder textures (rows 13–25).

## Appendix C — per-crate row counts

- oh-android: 1
- oh-app: 89
- oh-assets: 5
- oh-audio: 3
- oh-config: 2
- oh-discord: 18
- oh-game: 27
- oh-lua: 20
- oh-math: 2
- oh-menu: 16
- oh-menu-system: 1
- oh-net: 11
- oh-render: 37
- oh-server-control: 1
- oh-steam: 11
- oh-workshop-uploader: 16
- **total: 260 rows**

## Fixed deviations (the FIX lanes' rows — the standing rule: a fixed
deviation gets its register row in the same commit)

| # | site | deviation | C++ counterpart (verified) | fix |
|---|---|---|---|---|
| F1 | `crates/oh-menu/src/menu_game.rs` (`refresh_camera`, `window_size`, the 9-zone grid's thirds) + `crates/oh-app/src/menu_draw.rs` (`draw_online_status`) | the menu's layout space ran on the CONFIG resolution (`MenuConfig::size_x()`/`size_y()`) where the C++ runs on `getWindowWidth()`/`getWindowHeight()` = `window.getRenderWindow().getSize()` — the ACTUAL current render-target size. Invisible on the desktop (window == config resolution × pixelMult; the `refresh_view` math is scale-invariant under a uniform factor) but wrong on the phone (window 1530×678 vs the 1024×768-ish config — the orchestrator probe read the port's menu space as `window=(1024,1024)` while the touch coordinates live in the 1530×678 space, so the 9-zone grid's thirds landed in the wrong place: a center tap dispatched to the "right" band instead of the center `ok` band). | C++: MenuGame.cpp:3129-3136 (`refreshCamera`'s `fw`/`fh`/`fmax` on `getWindowWidth()`/`getWindowHeight()`), 3350-3357 (`getWindowWidth()`/`getWindowHeight()` = `window.getRenderWindow().getSize()`), 2804-2807 (the grid's thirds), 6001-6002 (`drawOnlineStatus`'s identity view) — all verified | **025g** (commits `3fa6fcab` + `fb0ea4a5`): the `MenuWindow` seam (oh-menu, the C++ ctor's `window` reference's size half) + the `RenderWindowSize` slot refreshed per frame from the window's current size (oh-app `tick_fields`) + the `refresh_if_stale` stale-detection tuple carrying the window size; host tests: `oh-menu` `layout`'s `refresh_view_matching_window_desktop_unchanged` / `refresh_view_mismatched_mobile_window` + `tests/window_size.rs` (5 tests, incl. the device-finding host repro); device acceptance: the phone's center tap creates the profile (the lane's device evidence) |
