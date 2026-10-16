//! Issue 014b — the egui console overlay's frame wiring, headless-first
//! (spec §5): the C++ `window == nullptr` early returns
//! (`HGUpdate.cpp:1290-1293` / `HGGraphics.cpp:319-322`) as the port's
//! GL-gate — a headless `Window` (`gl() == None`) never creates the
//! overlay and never writes the engine's `ilc_capture` (the
//! `imguiLuaConsoleHasInput` gate stays headless-identical). The egui
//! pass itself (the UI tail + the GL paint) is the MANUAL windowed check
//! (no display in CI — D-011 skip-with-reason; the
//! `crate::lua_console_ui` module docs: MANUAL check).

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use oh_app::lua_console_ui::{frame_draw, frame_post_update};
use oh_config::Config;
use oh_render::window::Window;

/// A debug-on composition (the console's `!Config::getDebug()` force-hide
/// gate, HGUpdate.cpp:1295-1300, lifted — the 010b toggle goes through).
fn debug_composed() -> (oh_app::compose::Composed, Arc<RwLock<Config>>) {
    let assets = Arc::new(
        oh_assets::Assets::new(common::release_dir(), false, false).expect("_RELEASE loads"),
    );
    let config = Arc::new(RwLock::new({
        let mut c = Config::default();
        c.set_debug(true);
        c.set_official(false); // the C++'s force-hide gate is `!debug || official` (HGUpdate.cpp:1295-1300)
        c
    }));
    let composed = oh_app::compose::compose_with_config(
        assets,
        common::release_dir(),
        true, /* window */
        Arc::new(oh_game::seams::NullProfiles),
        config.clone(),
        Arc::new(oh_game::seams::NoneSteamSink),
        None, // no Discord (headless test)
        None, // no window touch sink (the headless test's `nullptr` renderer)
    );
    (composed, config)
}

/// The GL-gate half of the C++'s two early returns (`window == nullptr`,
/// HGUpdate.cpp:1290-1293 / HGGraphics.cpp:319-322): with no GL context
/// the overlay slot stays `None` (never created) and the engine's
/// `ilc_capture` keeps its headless default — the port's analogue of the
/// C++'s `window == nullptr` console being entirely dormant headless.
#[test]
fn gl_gate_keeps_the_overlay_dormant_and_the_capture_default_headless() {
    let (composed, config) = debug_composed();
    let game = &composed.game;
    let window = Window::headless(800, 600);
    assert!(window.gl().is_none()); // the test premise (no GL context)
    let ilc = Rc::new(RefCell::new(None));

    // debug-on: the 010b toggle goes through the landed 14a prefix
    // (HGUpdate.cpp:1295-1311).
    game.set_ilc_show_console_next();
    game.post_update();
    assert!(game.ilc_show_console());

    // The post-update half (HGUpdate.cpp:1314-1643's call site): the GL
    // gate stops before the lazy creation — the slot stays `None`.
    frame_post_update(&window, game, &config, &ilc);
    assert!(ilc.borrow().is_none());
    // The draw half (HGGraphics.cpp:318-336's call site): the same gate.
    frame_draw(&window, game, &ilc);
    assert!(ilc.borrow().is_none());

    // The toggle-off frame (the C++'s `if (!ilcShowConsole) return;`,
    // HGUpdate.cpp:1308-1311): the `note_hidden` half over the empty slot
    // is a no-op.
    game.set_ilc_show_console_next();
    game.post_update();
    assert!(!game.ilc_show_console());
    frame_post_update(&window, game, &config, &ilc);
    assert!(ilc.borrow().is_none());
}

/// The gate's headless default (commit (b)'s contract, re-asserted at the
/// composition layer): with the console SHOWN but the overlay dormant
/// (no GL), `imguiLuaConsoleHasInput` is `shown && capture` with the
/// capture default `true` — the C++'s `WantCapture*` conjunct is what
/// step (b) writes per frame; headless, the default keeps the gate's
/// headless behaviour byte-identical to 14a.
#[test]
fn the_shown_console_gate_holds_the_capture_default_headless() {
    let (composed, _config) = debug_composed();
    let game = &composed.game;

    game.set_ilc_show_console_next();
    game.post_update();
    assert!(game.ilc_show_console());
    // The headless default (`ilc_capture = true`, commit (b)) + shown ⇒
    // the gate is true (HexagonGame.cpp:270-274 — the C++ whole, via the
    // landed `lua_console_has_input`).
    assert!(game.lua_console_has_input());

    // The step (b) writer flips it (the egui context's `wantCapture*`
    // analogue in the windowed frame — here the fake value): false ⇒ the
    // game's input passes (the C++'s gate false branch).
    game.set_ilc_capture(false);
    assert!(!game.lua_console_has_input());
}
