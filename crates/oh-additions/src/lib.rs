//! `oh-additions` — the home for everything the C++ does not have.
//!
//! This crate holds the port's additions: the code with no counterpart in
//! the C++ original. UI is the first family — the additions span the
//! port's UX / UI surface, its modes and features, its platform
//! affordances, and its content / audio generation.
//!
//! # The boundary contract
//!
//! Our layer may (1) inject inputs through the existing per-`Tid` /
//! input-state seams, (2) draw through the draw target, and
//! (3) own platform affordances. It may not modify the ported
//! simulation or menu logic — only *fixes* to that logic, i.e. correct
//! ports, stay inside.
//!
//! The one requirement for an addition that changes what happens in a
//! run: it must be driven through recorded inputs (so a replay
//! reproduces the run) or through config — never through silent state.
//!
//! # Modules
//!
//! * [`touch_overlay`] — the gameplay touch overlay (the F28
//!   pointer-feed state machine + the key-icon buttons: the two gates,
//!   the press-edge queue, the draw).
//! * [`input_injection`] — the overlay-side input injection (the
//!   contract's item 1): the F31 action-row press edges' apply into the
//!   game's `Tid` bodies, the F36 menu-side `Exit` edge's apply into
//!   the menu's `exit_action`.
//! * [`soft_input`] — the soft-input (system IME) controller: the
//!   `SoftInput` seam + the desktop no-op + the web's `#ime`
//!   focus / blur (the F32 row's Rust controller half).
//! * [`fullscreen`] — the web's `#fs` click / change-sink install
//!   (the F33 row's Rust half: the browser Fullscreen API's apply +
//!   the `fullscreenchange`'s config re-sync).

pub mod fullscreen;
pub mod input_injection;
pub mod soft_input;
pub mod touch_overlay;
