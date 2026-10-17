//! 29b5 — the level-list scroll machinery (the C++ float math, hand-computed
//! expectations): `scrollLevelListToTargetY`'s easing + epsilon gate,
//! `calcLevelChangeScroll`'s up/down targets (the bottom-branch target past
//! the window height), the single-pack top/bottom `YScrollTo` targets,
//! `calcScrollSpeed`'s level-count scaling, the `changePackQuick` drawer
//! jump, the fold/stretch machine's full cycle (the `-2` up-wrap through
//! `changePack`), and the focus rising/falling edge's
//! `quickPackFoldStretch`.
//!
//! The layout constants are hand-computed from the C++
//! `updateLevelSelectionDrawingParameters` (MenuGame.cpp:4298-4312) on the
//! `LinearFontHeight` oracle (height = char size × 0.7) at the 16:9 char
//! sizes (selection medium 19 / big 28 / small 14 — the `FakeWindow`
//! 800×450's `refresh_camera` picks `char_sizes(false)`):
//!
//! - `textToQuadBorder = 19 × 0.7 × 0.6 = 7.98`
//! - `slctFrameSize = 7.98 × 0.3 = 2.394`
//! - `packLabelHeight = 13.3 + 2 × 7.98 + 2.394 = 31.654`
//! - `levelLabelHeight = 19.6 + 9.8 + 7.98 + 2 × 7.98 − 2.394 = 50.946`
//!
//! (`h` = 768 — both the `Shared` default and the `FakeWindow` 800×450's
//! `refresh_view` height.)

use oh_menu::seams::{FakeAssets, FakeConfig, FakeWindow, MenuAssets};
use oh_menu::{MenuGame, NoMenuJoystick, NoopNetwork, NoopTouch, States};

fn lv(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn make_menu(packs: &[(&str, Vec<String>)]) -> MenuGame {
    let fa = {
        let fa = FakeAssets::new();
        for (pack, levels) in packs {
            let refs: Vec<&str> = levels.iter().map(|l| l.as_str()).collect();
            fa.add_pack(pack, &refs);
        }
        fa.add_profile("P1");
        fa
    };
    let assets: std::rc::Rc<dyn MenuAssets> = fa;
    let menu = MenuGame::new(
        assets,
        FakeConfig::new(),
        std::rc::Rc::new(NoopNetwork),
        oh_menu::RecordingAudio::new(),
        std::rc::Rc::new(NoopTouch),
        std::rc::Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        None,
        std::rc::Rc::new(oh_menu::layout::LinearFontHeight),
    );
    // Boot through the two boot screens to the main menu.
    menu.advance_boot_screen();
    menu.advance_boot_screen();
    menu.ok_action();
    assert_eq!(menu.state(), States::SMain);
    // LEVEL SELECT → LevelSelection.
    menu.ok_action();
    assert_eq!(menu.state(), States::LevelSelection);
    // The production layout state (menu_draw's `refresh_if_stale` runs
    // this every frame — the FakeWindow 800×450 → w 1365.33 × h 768,
    // 16:9 char sizes).
    menu.refresh_camera();
    menu
}

#[test]
fn level_selection_layout_constants_match_the_cpp_derivation() {
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"]))]);
    let layout = menu.shared().borrow().level_selection_layout();
    assert!((layout.text_to_quad_border - 7.98).abs() < 1e-3);
    assert!((layout.slct_frame_size - 2.394).abs() < 1e-3);
    assert!((layout.pack_label_height - 31.654).abs() < 1e-3);
    assert!((layout.level_label_height - 50.946).abs() < 1e-3);
}

#[test]
fn get_level_list_height_counts_the_levels_or_one_row_when_focus_held() {
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"]))]);
    {
        let s = menu.shared().borrow();
        // C++ `getLevelListHeight` (MenuGame.cpp:4388-4395):
        // `levelLabelHeight * levelCount + slctFrameSize`.
        assert!((s.get_level_list_height() - (3.0 * 50.946 + 2.394)).abs() < 1e-3);
    }
    menu.set_focus_held(true);
    {
        let s = menu.shared().borrow();
        // The focused list shows ONE collapsed row (`focusHeld ? 1 : size`).
        assert!((s.get_level_list_height() - (50.946 + 2.394)).abs() < 1e-3);
    }
}

#[test]
fn calc_scroll_speed_scales_with_the_level_count() {
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"]))]);
    {
        let mut s = menu.shared().borrow_mut();
        // 3 levels: `max(3 / 16.f, 1.f)` = 1 → base speed 30.
        s.calc_scroll_speed();
        assert_eq!(s.scroll_speed, 30.0);
    }
    // 32 levels: `max(32 / 16.f, 1.f)` = 2 → 60.
    let menu = make_menu(&[("big", (0..32).map(|i| format!("b{i}")).collect())]);
    {
        let mut s = menu.shared().borrow_mut();
        s.calc_scroll_speed();
        assert_eq!(s.scroll_speed, 60.0);
    }
    // Exactly 16 levels: `max(16 / 16.f, 1.f)` = 1 → 30 (the boundary).
    let menu = make_menu(&[("sixteen", (0..16).map(|i| format!("s{i}")).collect())]);
    {
        let mut s = menu.shared().borrow_mut();
        s.calc_scroll_speed();
        assert_eq!(s.scroll_speed, 30.0);
    }
}

#[test]
fn down_action_at_the_bottom_of_a_long_pack_sets_the_bottom_scroll_target() {
    // C++ `downAction` (MenuGame.cpp:2072-2076) → `calcLevelChangeScroll(2)`
    // (MenuGame.cpp:4438-4449): at the pack's bottom (not the last pack),
    // the target keeps the last level label + the next pack label on
    // screen: `packLabelHeight × (packIdx + 1 + (2 − (size − 1 − current)))
    // + levelLabelHeight × size + 3 × slctFrameSize`. With a 20-level pack
    // at 16:9 the required height (≈1121) exceeds the 768 window, so the
    // target `h − scroll` is NEGATIVE (the bottom-boundary fire —
    // `checkWindowBottomScroll`, MenuGame.cpp:4683).
    let levels: Vec<String> = (0..20).map(|i| format!("b{i}")).collect();
    let menu = make_menu(&[("big", levels.clone()), ("small", lv(&["s0"]))]);
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.pack_idx, 0);
        assert_eq!(s.lvl_slct.level_data_ids.len(), 20);
    }
    // Index 18 → down → index 19 (the last level). Prime the index first
    // (the boot leaves it at 0).
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.current_index = 18;
    }
    menu.down_action();
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.current_index, 19);
        // Hand-computed: scroll = 31.654 × (0 + 1 + (2 − 0)) + 50.946 × 20
        // + 3 × 2.394 = 94.962 + 1018.92 + 7.182 = 1121.064;
        // target = 768 − 1121.064 = −353.064.
        assert!(
            (s.lvl_slct.y_scroll_to - (-353.064)).abs() < 1e-2,
            "got {}",
            s.lvl_slct.y_scroll_to
        );
        // The target is set on `YScrollTo` only — `YOffset` eases toward it
        // on the next frames (the C++ lambdas never touch `YOffset`).
        assert_eq!(s.lvl_slct.y_offset, 0.0);
        // C++ :4406 — the level-change scroll runs at the base speed.
        assert_eq!(s.scroll_speed, 30.0);
    }
}

#[test]
fn up_action_near_the_pack_top_keeps_the_first_labels_on_screen() {
    // C++ `upAction` (MenuGame.cpp:1996-1999) → `calcLevelChangeScroll(-2)`
    // (MenuGame.cpp:4419-4424): `currentIndex < 2` →
    // `packLabelHeight × (packIdx + 1 − (2 − currentIndex))`.
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"])), ("ortho", lv(&["o0"]))]);
    // Pre-scroll the list down (a wheel/long-pack scroll did this): the
    // top labels are off-screen, so the top-boundary check fires.
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.current_index = 2;
        s.lvl_slct.y_offset = -40.0;
        s.lvl_slct.y_scroll_to = -40.0;
    }
    // Up: index 2 → 1 (`setIndex(prevIdx)` FIRST, the C++ order), then the
    // scroll math on the NEW index (1 < 2): `31.654 × (0 + 1 − (2 − 1)) = 0`
    // → `YScrollTo = 0` (the list scrolls back to its top to show the first
    // level label + the next pack label).
    menu.up_action();
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.current_index, 1);
        assert_eq!(s.lvl_slct.y_scroll_to, 0.0);
    }
    // Up again: index 1 → 0, the math on index 0:
    // `31.654 × (0 + 1 − 2) = −31.654` → `YScrollTo = +31.654` (the C++'s
    // virtual "two previous pack labels" — a POSITIVE target; the
    // per-frame `YOffset = min(YOffset, 0)` clamp, MenuGame.cpp:2930-2932,
    // is what keeps the offset at the list's top).
    menu.up_action();
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.current_index, 0);
        assert!((s.lvl_slct.y_scroll_to - 31.654).abs() < 1e-3);
    }
}

#[test]
fn single_pack_top_wrap_jumps_to_the_end_and_sets_the_scroll_target() {
    // C++ `upAction`'s single-pack arm (MenuGame.cpp:1964-1981): at the
    // top, jump to the pack's end, recompute the speed, and
    // `if(scroll > h − YOffset) YScrollTo = h − scroll` with
    // `scroll = packLabelHeight + levelLabelHeight × (current + 1) + 2 ×
    // slctFrameSize`. With a 40-level pack the scroll (≈1962) exceeds the
    // 768 window → the target is negative (the list scrolls down to its
    // end).
    let levels: Vec<String> = (0..40).map(|i| format!("b{i}")).collect();
    let menu = make_menu(&[("big", levels)]);
    menu.up_action(); // index 0 → 39
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.current_index, 39);
        // Hand-computed: scroll = 31.654 + 50.946 × 40 + 2 × 2.394 =
        // 2074.282; YScrollTo = 768 − 2074.282 = −1306.282.
        assert!(
            (s.lvl_slct.y_scroll_to - (-1306.282)).abs() < 1e-2,
            "got {}",
            s.lvl_slct.y_scroll_to
        );
        // `calcScrollSpeed` on the 40-level pack: `30 × max(40/16, 1)` = 75.
        assert_eq!(s.scroll_speed, 75.0);
    }
    // The twin: `downAction`'s single-pack arm (MenuGame.cpp:2056-2070) —
    // skip to the first level label, recompute the speed, `YScrollTo = 0`.
    menu.down_action(); // index 39 → 0
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.current_index, 0);
        assert_eq!(s.lvl_slct.y_scroll_to, 0.0);
        assert_eq!(s.scroll_speed, 75.0);
    }
}

#[test]
fn rest_arm_eases_y_offset_to_y_scroll_to() {
    // C++ `scrollLevelListToTargetY` (MenuGame.cpp:4578-4598) — the
    // `PackChange::Rest` arm (MenuGame.cpp:2884): `YOffset` approaches
    // `YScrollTo` at `scrollSpeed` units/s, clamped to land EXACTLY on the
    // target, with the `Utils::epsilon` (1e-4) arrival gate.
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"]))]);

    // Scrolling UP (target above): −40 → −40 + 0.1 × 30 = −37.
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.y_offset = -40.0;
        s.lvl_slct.y_scroll_to = -10.0;
    }
    menu.update(0.1);
    {
        let s = menu.shared().borrow();
        assert!(
            (s.lvl_slct.y_offset - (-37.0)).abs() < 1e-4,
            "got {}",
            s.lvl_slct.y_offset
        );
    }

    // Scrolling DOWN (target below): −40 → −40 − 0.1 × 30 = −43.
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.y_offset = -40.0;
        s.lvl_slct.y_scroll_to = -100.0;
    }
    menu.update(0.1);
    {
        let s = menu.shared().borrow();
        assert!(
            (s.lvl_slct.y_offset - (-43.0)).abs() < 1e-4,
            "got {}",
            s.lvl_slct.y_offset
        );
    }

    // The overshoot clamp: one frame steps PAST the target → the offset
    // lands exactly on it (the C++'s `if(YOffset >= YScrollTo) YOffset =
    // YScrollTo`).
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.y_offset = -41.0;
        s.lvl_slct.y_scroll_to = -40.0;
    }
    menu.update(0.5); // +15 units → −26 → clamped to −40
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.y_offset, -40.0);
    }

    // The epsilon gate: a sub-1e-4 distance is "arrived" (no movement, no
    // clamp noise).
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.y_offset = -40.0;
        s.lvl_slct.y_scroll_to = -40.00005;
    }
    menu.update(1.0);
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.y_offset, -40.0);
    }
}

#[test]
fn up_wrap_runs_the_full_fold_stretch_cycle_to_the_previous_packs_last_level() {
    // C++ `upAction`'s multi-pack top arm (MenuGame.cpp:1990-1994) →
    // `changePackAction(-2)` (MenuGame.cpp:2184-2198) → the fold/stretch
    // machine (MenuGame.cpp:2882-2934) with the `-2` "go to the previous
    // pack and skip to the LAST level of the list" semantics
    // (MenuGame.cpp:2143).
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"])), ("ortho", lv(&["o0"]))]);
    assert_eq!(menu.current_pack_id(), "cube");
    assert_eq!(menu.shared().borrow().lvl_slct.current_index, 0);

    menu.up_action(); // index 0 of cube → `-2` fold to ortho's last level
    {
        let s = menu.shared().borrow();
        assert_eq!(s.pack_change_state, oh_menu::PackChange::Folding);
        assert_eq!(s.pack_change_direction, -2);
        // C++ :2189 — stop scrolling for safety.
        assert_eq!(s.lvl_slct.y_scroll_to, 0.0);
        assert_eq!(s.lvl_slct.y_offset, 0.0);
    }

    // Drive the fold: 3 levels → list height ≈ 155.232, speed 30 →
    // frames of 0.5 add 15 units: after 10 frames (150) the fold is
    // still running, the 11th clamps to the list height.
    for _ in 0..10 {
        menu.update(0.5);
    }
    {
        let s = menu.shared().borrow();
        assert_eq!(
            s.pack_change_state,
            oh_menu::PackChange::Folding,
            "still folding just before the list height"
        );
    }
    menu.update(0.5);
    {
        let s = menu.shared().borrow();
        assert_eq!(s.pack_change_state, oh_menu::PackChange::Stretching);
        assert_eq!(s.lvl_slct.pack_idx, 1, "the pack switched mid-fold");
        assert_eq!(menu.current_pack_id(), "ortho");
        // C++ :2143 — `-2` lands on the NEW pack's last level (ortho has
        // one: index 0).
        assert_eq!(s.lvl_slct.current_index, 0);
        // C++ :2905 — the stretch starts at the new pack's list height
        // (one row: ≈ 53.34), then the speed recomputes (1 level → 30).
        assert!((s.pack_change_offset - (50.946 + 2.394)).abs() < 1e-3);
        assert_eq!(s.scroll_speed, 30.0);
    }

    // Drive the stretch back to Rest (≈ 11 frames of 0.5).
    for _ in 0..20 {
        menu.update(0.5);
    }
    {
        let s = menu.shared().borrow();
        assert_eq!(s.pack_change_state, oh_menu::PackChange::Rest);
        assert_eq!(s.pack_change_offset, 0.0);
        // The `-2` last-pack stretch (MenuGame.cpp:4534-4549): both the
        // top and the bottom targets stay off-screen at the list's top, so
        // the drawer's scroll values are untouched.
        assert_eq!(s.lvl_slct.y_scroll_to, 0.0);
        assert_eq!(s.lvl_slct.y_offset, 0.0);
    }
}

#[test]
fn up_action_is_suppressed_during_the_pack_change_animation() {
    // C++ `upAction`'s head (MenuGame.cpp:1914-1918): no navigation while
    // the fold/stretch is running.
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"])), ("ortho", lv(&["o0"]))]);
    menu.up_action(); // starts the `-2` fold
    assert_eq!(menu.shared().borrow().lvl_slct.current_index, 0);
    menu.down_action(); // suppressed (the animation is running)
    assert_eq!(menu.shared().borrow().lvl_slct.current_index, 0);
}

#[test]
fn change_pack_quick_jumps_the_drawer_to_the_pack_labels() {
    // C++ `changePackQuick` (MenuGame.cpp:2156-2181) — the focus-held pack
    // jump: after the switch, the drawer JUMPS (`YScrollTo = YOffset =
    // target`, no easing) to keep the pack labels on screen — the previous
    // pack label's top prevails.
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"])), ("ortho", lv(&["o0"]))]);
    // Start on the last pack (ortho, idx 1) and jump up to cube (idx 0):
    // the previous pack label's top is `packLabelHeight × (0 − 1) =
    // −31.654` → target `+31.654` (the C++'s virtual previous label — the
    // drawer jumps there; the next frame's `YOffset = min(YOffset, 0)`
    // clamp, MenuGame.cpp:2930-2932, then holds the list at its top).
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.pack_idx = 1;
        s.lvl_slct.level_data_ids = vec!["ortho_o0".to_string()];
        s.lvl_slct.current_index = 0;
        s.lvl_slct.y_offset = -200.0;
        s.lvl_slct.y_scroll_to = -200.0;
    }
    menu.shared().borrow_mut().change_pack_quick(-1);
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.pack_idx, 0);
        assert_eq!(
            s.lvl_slct.level_data_ids,
            vec![
                "cube_c1".to_string(),
                "cube_c2".to_string(),
                "cube_c3".to_string()
            ]
        );
        // The jump (the C++ `YScrollTo = YOffset = target`):
        assert!((s.lvl_slct.y_scroll_to - 31.654).abs() < 1e-3);
        assert!((s.lvl_slct.y_offset - 31.654).abs() < 1e-3);
    }
}

#[test]
fn focus_rising_and_falling_edges_run_quick_pack_fold_stretch() {
    // C++ `update`'s `focusHeld` block (MenuGame.cpp:2645-2675): the
    // rising edge's `setIndex(0)` + `quickPackFoldStretch()` (:4563-4576)
    // and the falling edge's stretch. At pack 0 with focus held the list
    // is one row (≈50.54); the previous pack label's top is `31.654 ×
    // (0 − 1) = −31.654` → the top-boundary check fires (target `+31.654`
    // > `YOffset` 0) → `YScrollTo = YOffset = 31.654`, and the per-frame
    // clamp (MenuGame.cpp:2930-2932) immediately brings `YOffset` back to
    // 0 (the C++'s virtual-label quirk, faithful).
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"])), ("ortho", lv(&["o0"]))]);
    // Focus at a non-zero index: the rising edge's `setIndex(0)` fires.
    {
        let mut s = menu.shared().borrow_mut();
        s.lvl_slct.current_index = 2;
    }
    menu.set_focus_held(true);
    menu.update(0.01);
    {
        let s = menu.shared().borrow();
        assert_eq!(s.lvl_slct.current_index, 0, "the rising edge's setIndex(0)");
        assert!(s.focus_held && s.was_focus_held);
        assert!((s.lvl_slct.y_scroll_to - 31.654).abs() < 1e-3);
        // The clamp ran (the `step_pack_change` tail, MenuGame.cpp:2930):
        assert_eq!(s.lvl_slct.y_offset, 0.0);
        // C++ `adjustLevelsOffset` (MenuGame.cpp:3435): the focused row's
        // offset is `maxOffset` (100), the rest 0.
        assert_eq!(s.lvl_slct.lvl_offsets[0], 100.0);
        assert_eq!(s.lvl_slct.lvl_offsets[1], 0.0);
    }

    // The falling edge: the list stretches back to all rows and the pack
    // labels re-layout (the same top-boundary jump at pack 0 — the full
    // list's `scrollBottom` (≈180.88) still fits, so only the top fires).
    menu.set_focus_held(false);
    menu.update(0.01);
    {
        let s = menu.shared().borrow();
        assert!(!s.focus_held && !s.was_focus_held);
        assert!((s.lvl_slct.y_scroll_to - 31.654).abs() < 1e-3);
        assert_eq!(s.lvl_slct.y_offset, 0.0);
    }
}

#[test]
fn get_level_selection_height_sums_the_pack_labels_and_the_list() {
    // C++ `getLevelSelectionHeight` (MenuGame.cpp:4315-4326) — no C++ call
    // site (ported for API parity): `packLabelHeight × packCount +
    // getLevelListHeight() − packChangeOffset + (notLastPack ? 2 : 1) ×
    // slctFrameSize`.
    let menu = make_menu(&[("cube", lv(&["c1", "c2", "c3"])), ("ortho", lv(&["o0"]))]);
    {
        let s = menu.shared().borrow();
        // 2 packs, 3 levels, offset 0, pack 0 (not last):
        // 31.654 × 2 + (50.946 × 3 + 2.394) + 2 × 2.394 = 223.328.
        assert!(
            (s.get_level_selection_height() - 223.328).abs() < 1e-3,
            "got {}",
            s.get_level_selection_height()
        );
    }
}
