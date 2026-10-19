//! The overlay-side input injection (the boundary contract's item 1 —
//! "inject inputs through the existing per-`Tid` / input-state
//! seams"): the [`touch_overlay`]'s press edges' applies into the
//! ported code's OWN action bodies. This module is where the OURS
//! wiring lives (lane 057, step B): the ported files (`oh-app`'s slot
//! adapters, `oh-menu`'s `MenuGame`) keep only the C++'s logic — the
//! edges come from the overlay (this crate) and the bodies invoked are
//! the ported code's own (the C++'s `Tid` bodies — the injection CALLS
//! them, it does not re-implement them; that is what makes it
//! injection rather than a logic fork).
//!
//! Two arms:
//! * the F31 action row (issue 039 — the `Replay` / `Restart` /
//!   `Pause` trio + the game-side `Exit` → `trigger_go_to_menu`):
//!   [`apply_game_action_edges`].
//! * the F36 menu-side arm (issue 054 — the menu-state `Exit` → the
//!   `exit_action` body): [`apply_menu_exit`].
//!
//! Both arms take the overlay's pending press edges EVERY frame,
//! unconditionally (the drain is the stale-edge hazard's guard — see
//! each function's docs); the APPLY is gated (the action / menu
//! gates), never the take.

use oh_game::hexagon_game::HexagonGame;
use oh_game::lua_state::SharedStore;
use oh_menu::MenuGame;

use crate::touch_overlay::{Button, TouchOverlay};

/// F31 (issue 039) — the action row's press edges' apply into the
/// game's `Tid` bodies (the C++'s `Tid::Replay` / `Tid::Restart` /
/// `Tid::Pause` bodies, HexagonGame.cpp:389-397 / :399-407 / :421-441 —
/// the `death_input_ignore` + `has_died` gates, the `in_replay`
/// replay-skip, the debug-gated `debugPause` — and the game-side
/// `Tid::Exit` → `go_to_menu(true, false)`, the `Tid::Exit` bind's
/// contract, 029d1).
///
/// The edges are taken UNCONDITIONALLY (the pending edges drain every
/// frame — the C++'s `Type::Once` per-frame poll; an edge whose class
/// gate is off at the take is dropped, never deferred — the stale-edge
/// hazard: an untaken `Exit` edge would linger in the shared overlay
/// and fire `trigger_go_to_menu` on the next game entry). The apply is
/// under the action gate (`action_active` — the whole game state, incl.
/// the `has_died` screen and the replay playback: the keyboard binds'
/// reachability — issue 031's `enabled` + the F31 class split).
///
/// The console-gating follows the C++'s `notInConsole` wrappers —
/// `lua_console_has_input()` is consulted ONCE per apply (the C++'s
/// `updateInput`'s `imguiLuaConsoleHasInput()` early return's shape —
/// the game's own touch controls are gated the same way, issue 31 §2b)
/// — and the `Tid::Pause` bind is the ungated one
/// (HexagonGame.cpp:421-441).
pub fn apply_game_action_edges<S: SharedStore>(o: &mut TouchOverlay, game: &HexagonGame<S>) {
    let edges = o.take_press_edges();
    if o.action_active() && o.enabled() {
        let console = game.lua_console_has_input();
        if edges[Button::Exit.idx()] && !console {
            game.trigger_go_to_menu();
        }
        if edges[Button::Replay.idx()] && !console {
            game.trigger_replay();
        }
        if edges[Button::Restart.idx()] && !console {
            game.trigger_restart();
        }
        if edges[Button::Pause.idx()] {
            game.trigger_pause();
        }
    }
}

/// F36 (issue 054) — the menu state's half of the overlay's press
/// edges: the `Tid::Exit` bind's touch counterpart — the `exit_action`
/// body (MenuGame.cpp:2457-2502 — the `SMain` "main"-category no-op,
/// the `SLPSelectBoot` → `ETLPNewBoot` arm, the `LevelSelection` →
/// `SMain` arm, the `goBack` / `SMain` arms) under the bind's OWN
/// guard (the `is_entering_text()`, MenuGame.cpp:975-983: a menu `Exit`
/// press in the `ETLPNew` / `ETLPNewBoot` text-entry states must fire
/// nothing — the guard's mirror; the `SMain` / `LoadingScreen` /
/// `EpilepsyWarning` exclusion is the gate feed's — `menu_active` is
/// false there).
///
/// The edges are taken UNCONDITIONALLY — the drain is EVERY frame
/// (the stale-edge hazard: without it a menu-side `Exit` edge left
/// untaken would linger in the shared overlay and fire the game slot's
/// `trigger_go_to_menu` on the next game entry). In a menu state the
/// `Replay` / `Restart` / `Pause` buttons have no menu-side body — not
/// consumed (no capture / hold / fire, the F36 shape test) — so their
/// edges are drained, not applied.
pub fn apply_menu_exit(o: &mut TouchOverlay, menu: &MenuGame) {
    let edges = o.take_press_edges();
    if edges[Button::Exit.idx()] && o.menu_active() && !menu.is_entering_text() {
        menu.exit_action();
    }
}
