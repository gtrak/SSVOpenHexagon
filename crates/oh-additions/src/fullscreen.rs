//! The web's fullscreen button's (`#fs`) click / change-sink INSTALL —
//! the D-034-class web-only addition's RUST HALF (lane 057, step C, the
//! F33 row). The C++ had no such control (its fullscreen toggles were
//! the LAlt+Enter bind + the options menu items, which applied the
//! config at the window's creation / recreation —
//! `Config::setFullscreen`'s `recreateWindow`, GameWindow.hpp:90; the
//! register row's FINDING: the port never applied the `fullscreen`
//! config anywhere — this install is the FIRST path that does, on the
//! web target, through the browser Fullscreen API).
//!
//! What lives here: the two sink CALLBACKS' install — the click sink's
//! the browser Fullscreen API's apply + the `fullscreenchange`'s
//! config re-sync (issue 055's desync close), the 041-class
//! `[oh-web-fs]` wiring-evidence lines through the `oh-game` log sink's
//! browser-console route. What stays: the `#fs` element's get-or-create
//! (the `oh-render` emscripten window layer — the platform path, not a
//! patch of ported logic), the per-frame `ohstatechange` transition
//! signal (the rAF loop body — the page-side visibility gate's edge
//! feed), and the shell's `#fs` button itself (a named shell, not
//! moved).

#[cfg(target_os = "emscripten")]
use std::sync::{Arc, RwLock};

#[cfg(target_os = "emscripten")]
use oh_config::Config;
#[cfg(target_os = "emscripten")]
use oh_render::window::Window;

/// issue 046 (F33) — the fullscreen button's (`#fs`) click sink
/// (the `Window::set_fullscreen_click_sink` docs — the `oh-render`
/// emscripten half attached the listener; this installs the callback):
/// the browser Fullscreen API's apply. The `config` here is the SAME
/// shared `Arc<RwLock<Config>>` the `MenuConfig` seam wraps (the
/// LAlt+Enter bind's `cfg.set_fullscreen(!cfg.fullscreen())` —
/// `register_menu_binds`'s) — the button and the menu's own toggle
/// cannot disagree (the task's "single source of truth"). The C++
/// applied this config at the window's creation / recreation
/// (`Config::setFullscreen`'s `recreateWindow` —
/// `sf::Style::Fullscreen`, GameWindow.hpp:90; the port's FINDING —
/// the register row — never applied it anywhere: the winit window
/// is created windowed and `Window::set_fullscreen` is
/// bookkeeping-only); this callback is the web's applier (the
/// browser Fullscreen API — the C++'s `recreateWindow`'s
/// fullscreen half, with the `documentElement` as the fullscreened
/// node so the `#fs` stays visible IN the fullscreen).
#[cfg(target_os = "emscripten")]
pub fn install_sinks(window: &mut Window, config: &Arc<RwLock<Config>>) {
    let cfg = config.clone();
    window.set_fullscreen_click_sink(std::sync::Arc::new(move || {
        // issue 055 — the direction is the BROWSER's ACTUAL state
        // (`document.fullscreenElement`), NOT the config's value
        // (the pre-055's desync — its own comment's record of the
        // hole): an Esc / the Android back leaves fullscreen
        // without the game asking, and a config-value toggle then
        // set the config `false` + called `exitFullscreen()` — a
        // no-op: the FIRST tap after an external exit did nothing
        // visible (the operator's "i can no longer reach the
        // button"). The browser's state makes the tap's direction
        // correct in EVERY case (a fast double-tap included: the
        // second tap re-reads the browser, not a half-written
        // config). The config write happens in the
        // `fullscreenchange` re-sync (below) — the browser's own
        // authoritative signal for the transition — so a REJECTED
        // request (no transient user activation — the headless
        // CDP's honest limit, the operator's real tap always
        // grants it) fires no event, writes nothing: the config,
        // the button's appearance (the `html:fullscreen` CSS flip)
        // and the browser all stay on the browser's actual state —
        // they cannot disagree.
        let in_fullscreen = web_sys::window()
            .and_then(|w| w.document())
            .is_some_and(|d| d.fullscreen_element().is_some());
        let next = !in_fullscreen;
        // The browser Fullscreen API's apply (the FINDING's
        // applier half — the port never applied the config
        // anywhere; this is the first path that does, on the
        // web target). The `request_fullscreen` binding resolves
        // the call's SYNCHRONOUS result (the `Ok` = the Promise
        // was created); the browser's ASYNC verdict (the
        // transient user activation's grant or deny — the
        // headless CDP's honest limit: a CDP click cannot grant
        // the activation, the task's "the transition is the
        // operator's real tap") surfaces as the Promise's
        // rejection, which the shell's `unhandledrejection`
        // listener routes to the `[oh-web-fs] promise rejected`
        // console line (the wiring's async evidence).
        let browser = web_sys::window()
            .and_then(|w| w.document())
            .map(|doc| {
                if next {
                    doc
                        .document_element()
                        .map(|el| match el.request_fullscreen() {
                            Ok(()) => "requestFullscreen() called (the Promise's async verdict: the [oh-web-fs] console line)".to_string(),
                            Err(e) => format!("rejected (sync): {e:?}"),
                        })
                        .unwrap_or_else(|| "no documentElement".to_string())
                } else {
                    doc.exit_fullscreen();
                    format!(
                        "exitFullscreen() called (fullscreen_enabled={})",
                        doc.fullscreen_enabled()
                    )
                }
            })
            .unwrap_or_else(|| "no DOM (the headless fallback)".to_string());
        // The wiring-evidence line (the 041's `[oh-audio]`'s
        // console-visible diagnostic's pattern — the Rust
        // stdio's black hole, the `oh-game` log sink's
        // browser-console route).
        oh_game::log_sink::emit_log(
            "web",
            &format!("[oh-web-fs] fullscreen -> {next} (browser: {browser})"),
        );
    }));
    // issue 055 — the `fullscreenchange` re-sync (the
    // `Window::set_fullscreen_change_sink` docs): the ported
    // `fullscreen` config's WRITER for the browser's transitions
    // — ONCE, from the browser's own signal (the task's
    // "reconcile, do not abandon"): every transition (our API's
    // grant/exit AND the external exits — Esc / the Android back —
    // the game did not ask for) fires it, and the config tracks
    // the browser's ACTUAL state there: the operator's desync (the
    // external exit's stale `true`) is closed, the config stays
    // the SETTING's single source of truth (the options menu's +
    // the LAlt+Enter bind's writes ride the SAME shared
    // `Arc<RwLock<Config>>` unchanged — F33's rule), and a
    // rejected request (no state change, no event) writes nothing
    // — the config, the button's appearance (the `html:fullscreen`
    // CSS flip) and the browser cannot disagree.
    window.set_fullscreen_change_sink(std::sync::Arc::new(move || {
        let in_fullscreen = web_sys::window()
            .and_then(|w| w.document())
            .is_some_and(|d| d.fullscreen_element().is_some());
        let prev = { cfg.read().expect("config poisoned").get_fullscreen() };
        if prev != in_fullscreen {
            cfg.write()
                .expect("config poisoned")
                .set_fullscreen(in_fullscreen);
        }
        // The wiring-evidence line (the 041's `[oh-audio]`'s
        // console-visible diagnostic's pattern — the Rust stdio's
        // black hole, the `oh-game` log sink's browser-console
        // route): the transition's own account (the probe's grep
        // target) — the `no write` half marks the event that
        // found the config already in sync (an LAlt+Enter /
        // options-menu write ahead of the transition).
        oh_game::log_sink::emit_log(
            "web",
            &format!(
                "[oh-web-fs] fullscreenchange -> {in_fullscreen} (config {prev} -> {in_fullsync}{no_write})",
                in_fullsync = in_fullscreen,
                no_write = if prev == in_fullscreen { " (no write — already in sync)" } else { "" },
            ),
        );
    }));
}
