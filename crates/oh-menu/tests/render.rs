//! Coarse render / layout test (issue 013, slice 13a).
//!
//! The dialog-r#box frame geometry and the menu offset/layout math are pure
//! arithmetic and are asserted directly (no display required). A
//! pixel-golden capture of the main menu needs the `oh-render` backend and a
//! real font (D-011); that test skips with reason rather than faking a
//! result.

use oh_math::hg::color::Color;
use oh_menu::dialog_box::{DBoxDraw, DialogKey, HexagonDialogBox};

/// The dialog r#box splits on newlines — a line AT each `\n` ONLY (the
/// C++'s shape, HexagonDialogBox.cpp:46-58, D1: the segment after the
/// LAST `\n` is never pushed) — computes the height, and emits exactly
/// the outer + backdrop frame quads for a `Center` dialog.
#[test]
fn dialog_box_center_frame_geometry() {
    let mut r#box = HexagonDialogBox::new();
    // line_height 15.4 (LinearFontHeight(22)), dialog_width 300, frame 12.
    r#box.create_with_key(
        "LINE1\nLINE2\n",
        22,
        12.0,
        DBoxDraw::Center,
        DialogKey::Unknown,
        0.0,
        0.0,
        15.4,
        300.0,
    );
    assert_eq!(r#box.line_count(), 2); // "LINE1","LINE2" (D1: no trailing "")
    assert!(!r#box.empty());
    assert_eq!(r#box.key_to_close(), DialogKey::Unknown);
    // D1: the trailing `\n`-terminated message has NO extra line — the
    // total height is 2 lines + the frame (not 3): the old port's
    // trailing-`""` push made EVERY dialog `1.5 × lineHeight` taller.
    let dialog_height = 15.4 * 2.0 + (15.4 / 2.0) * (2.0 - 1.0);
    assert_eq!(r#box.total_height(), dialog_height + 2.0 * 24.0);

    let mut quads = Vec::new();
    r#box.draw(Color::WHITE, Color::BLACK, 800.0, 450.0, &mut quads);
    assert_eq!(quads.len(), 2, "center dialog emits outer + backdrop quads");

    let (outer, backdrop) = (&quads[0], &quads[1]);
    assert_eq!(outer.color, Color::WHITE);
    assert_eq!(backdrop.color, Color::BLACK);
    // The backdrop is inset by `frame` from the outer on all four sides.
    assert!(backdrop.x1 > outer.x1 && backdrop.x2 < outer.x2);
    assert!(backdrop.y1 > outer.y1 && backdrop.y2 < outer.y2);
    // Centered dialog: symmetric about the SCALED half-width, not w/2.
    // C++ `drawCenter` uses `calculateFMaxAndWAndH`:
    //   fmax = max(1024/w, 768/h); w' = w * fmax; centre = (w'/2) + xPos.
    // For w=800,h=450: fmax = 1024/800 = 1.28 (768/450 = 1.70666...) so fmax =
    // 1.706667, w' = 1365.3333, and the box centre is w'/2 = 682.66667.
    // (`dialogBox:195-203`, `:211-213`.) The earlier 400.0 expectation was
    // wrong: it assumed the unscaled w/2.
    let fmax = (1024.0f32 / 800.0f32).max(768.0f32 / 450.0f32);
    let scaled_half = 800.0 * fmax / 2.0;
    let mid = (outer.x1 + outer.x2) / 2.0;
    assert!(
        (mid - scaled_half).abs() < 1e-3,
        "mid={mid} expected={scaled_half}"
    );
    // Independently pin the absolute value computed from the C++ formula
    // (fmax = 768/450 = 1.7066667; w' = 800 * fmax = 1365.33334; w'/2 = 682.66667).
    // This is deliberately a hard-coded literal, NOT a re-evaluation of the same
    // expression above — otherwise the check would be vacuous.
    #[allow(clippy::excessive_precision)]
    const EXPECTED_MID: f32 = 682.666_67;
    assert!(
        (mid - EXPECTED_MID).abs() < 1e-3,
        "mid={mid} expected={EXPECTED_MID}"
    );
}

/// D1: the C++'s split drops the segment after the LAST `\n` — a
/// non-`\n`-terminated message yields `empty() == true` (the C++'s
/// `create("A")` shape, HexagonDialogBox.cpp:46-58), and a
/// `\n`-terminated message keeps every line up to the last `\n`.
#[test]
fn dialog_box_create_drops_the_trailing_segment() {
    // `"A"` (no `\n`): the C++ pushes nothing → `empty()`.
    let mut b = HexagonDialogBox::new();
    b.create(
        "A",
        22,
        12.0,
        DBoxDraw::Center,
        0.0,
        0.0,
        false,
        15.4,
        100.0,
    );
    assert!(
        b.empty(),
        "the C++'s `create(\"A\")` yields `empty() == true`"
    );
    assert_eq!(b.line_count(), 0);

    // `"A\nB"` (last segment `B` dropped → 1 line, NOT 2).
    let mut b = HexagonDialogBox::new();
    b.create(
        "A\nB",
        22,
        12.0,
        DBoxDraw::Center,
        0.0,
        0.0,
        false,
        15.4,
        100.0,
    );
    assert_eq!(b.line_count(), 1);
    assert_eq!(b.dialog_text(), ["A"]);

    // `"A\nB\n"` (ends in `\n`): all lines kept, no trailing `""`.
    let mut b = HexagonDialogBox::new();
    b.create(
        "A\nB\n",
        22,
        12.0,
        DBoxDraw::Center,
        0.0,
        0.0,
        false,
        15.4,
        100.0,
    );
    assert_eq!(b.line_count(), 2);
    assert_eq!(b.dialog_text(), ["A", "B"]);
    assert!(!b.empty());
}

/// A password input r#box grows the total height by `4 * line_height` and
/// reports itself as an input r#box.
#[test]
fn dialog_input_box_height() {
    let plain = {
        let mut b = HexagonDialogBox::new();
        b.create(
            "A",
            22,
            12.0,
            DBoxDraw::Center,
            0.0,
            0.0,
            false,
            15.4,
            100.0,
        );
        b.total_height()
    };
    let input = {
        let mut b = HexagonDialogBox::new();
        b.create("A", 22, 12.0, DBoxDraw::Center, 0.0, 0.0, true, 15.4, 100.0);
        (b.is_input_box(), b.total_height())
    };
    assert!(input.0);
    assert!((input.1 - plain - 4.0 * 15.4).abs() < 1e-3);
}

/// The main-menu pixel-golden frame requires the `oh-render` backend and a
/// real `sf::Font` (D-011). It is not faked here — this records the skip.
#[test]
fn main_menu_golden_frame_needs_render_backend() {
    // The coarse quad-emission slice (13b) is wired through `oh-render`,
    // which is not a dependency of this crate (sibling WIP, issue 010).
    eprintln!(
        "SKIP (render): a golden main-menu frame needs the oh-render backend + \
         a real font (D-011); the layout math is asserted in `layout::tests` and \
         the dialog geometry in this file."
    );
}
