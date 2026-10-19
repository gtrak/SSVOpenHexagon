//! The soft-input (the system IME, the on-screen keyboard) controller —
//! the D-031 platform addition's RUST CONTROLLER HALF (lane 057, step C,
//! the F32 row). The seam the C++ never had: the C++'s Android text
//! input came from SFML's own Android layer (the IME wiring lived in
//! SFML, D-031), and the C++ has no web text path either (the F32
//! register row) — this is the deliberate platform addition, home in
//! the additions crate.
//!
//! What lives here (the controller): the [`SoftInput`] seam + the
//! desktop / headless no-op ([`NoOpSoftInput`]) + the web's
//! hidden-but-focusable `#ime` input's focus / blur ([`WebIme`],
//! emscripten-only). The ANDROID arm (`ImeController`, the
//! android-activity's `show_soft_input` / `hide_soft_input`) stays in
//! the oh-app's android glue (a separate artifact — it implements this
//! trait), and the `#ime` ELEMENT itself (its get-or-create + the
//! `input` / `compositionend` feed listeners + the keydown's text gate)
//! stays in the emscripten window layer (the `oh-render` emscripten
//! `Window` — the platform path, not a patch of ported logic; the
//! shell's `#ime` element in the page is a named shell, not moved).
//! The characters feed the EXISTING entered-text pipeline (the
//! `TextEntered` fold → the `entered_text_sink` → the menu's
//! `handle_text_entered`, gated by the ported `isEnteringText`,
//! MenuGame.cpp:2862-2878) — fed, NOT a second pipeline built.

/// issue 025f / D-031 — the soft-input (the system IME, the on-screen
/// keyboard) controller: `show` on the menu's ENTER-TEXT entry, `hide`
/// on its leave (the C++ `isEnteringText` gate's transition — the
/// `MenuSlotAdapter::refresh_input` docs: the edge is computed there,
/// once per transition, not per frame).
///
/// The C++'s Android text input came from SFML's own Android layer (the
/// IME wiring lived in SFML, D-031) — the port's deliberate platform
/// addition is this seam: the Android controller (the android-activity
/// `show_soft_input(true)` / `hide_soft_input(false)`, the
/// `oh-app::android` `ImeController`) and the desktop no-op (no IME on
/// the desktop window — the C++ desktop build never asked for one).
pub trait SoftInput: Send + Sync {
    /// Show the system IME (the android-activity `show_soft_input(true)`
    /// — `InputMethodManager.showSoftInput(decorView, SHOW_IMPLICIT)`).
    fn show(&self);
    /// Hide the system IME (the android-activity `hide_soft_input(false)`
    /// — `hideSoftInputFromWindow(token, 0)`: an unconditional hide, not
    /// only the implicitly shown IME).
    fn hide(&self);
}

/// The desktop / headless `SoftInput` (the trait's docs): the no-op —
/// no IME on the desktop window.
pub struct NoOpSoftInput;

impl SoftInput for NoOpSoftInput {
    fn show(&self) {}
    fn hide(&self) {}
}

/// issue 046 (F32) — the web `SoftInput` controller (the 025f
/// `ImeController`'s web twin — D-031's platform addition's web half):
/// the page's hidden-but-focusable IME input (the `oh-render` emscripten
/// `Window`'s `#ime` element — its `web_ime_input` accessor; the
/// element + its attribute contract live there, the `Window::create`
/// get-or-create) focused on the menu's ENTER-TEXT entry and blurred on
/// its leave — the SAME `is_entering_text` transition edge (the
/// `MenuSlotAdapter::refresh_input` docs — computed ONCE per transition,
/// never per frame: the per-frame call is the bug class the operator
/// hit with the Android touch input, 025f) that drives the Android
/// activity's `show_soft_input` / `hide_soft_input`.
///
/// The characters then feed the EXISTING entered-text pipeline — the
/// element's `input` / `compositionend` listeners (the `oh-render`
/// `attach_web_ime_listeners`) push the `Event::TextEntered`s into the
/// window's DOM→SFML event queue (the issue-025d chain: the `TextEntered`
/// fold → the `entered_text_sink` → the menu's `handle_text_entered`,
/// gated by the ported `isEnteringText`, MenuGame.cpp:2862-2878) — fed,
/// NOT a second pipeline built. The C++ has no web text path either (its
/// input came from SFML — D-031's framing, the F32 register row): this
/// is the deliberate platform addition, and ALL text-entry states
/// benefit (the gate is the menu's `is_entering_text()`, not a
/// name-entry special case — the dialog input box / the Lua console ride
/// the SAME edge if the state gate ever widens to them, exactly as the
/// Android arm does).
#[cfg(target_os = "emscripten")]
pub struct WebIme {
    /// The `#ime` element (`None` = the headless fallback / no DOM — the
    /// no-op arms below; the desktop / Android windows never have one —
    /// their `SoftInput` arms are the no-op / the android-activity).
    input: Option<web_sys::HtmlInputElement>,
}

#[cfg(target_os = "emscripten")]
impl WebIme {
    #[must_use]
    pub fn new(input: Option<web_sys::HtmlInputElement>) -> Self {
        Self { input }
    }
}

#[cfg(target_os = "emscripten")]
impl SoftInput for WebIme {
    /// Focus the hidden input (the browser's `showSoftInput` twin — a
    /// mobile browser raises the soft keyboard ONLY for a focused
    /// editable element: the task's diagnosis). The `prevent_scroll`
    /// option: the element is off-screen (`left: -10000px` — the
    /// `web_ime_input_get_or_create` style contract), so a focus-scroll
    /// would jump the page (the `body`'s `overflow: hidden` makes it a
    /// non-issue, the option is the belt).
    fn show(&self) {
        if let Some(el) = &self.input {
            let options = web_sys::FocusOptions::new();
            options.set_prevent_scroll(true);
            let _ = el.focus_with_options(&options);
        }
    }
    /// Blur it (the "keyboard must NOT appear in non-text states" half of
    /// the acceptance — a stray focused input would swallow the game's
    /// keys: the keydown's text gate is keyed on this element's focus,
    /// and a focused input steals the soft keyboard's target from the
    /// page while it should not be typing at all).
    fn hide(&self) {
        if let Some(el) = &self.input {
            let _ = el.blur();
        }
    }
}
