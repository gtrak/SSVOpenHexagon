//! 29b1 — the favorites drawer (audit row 1): F1's add/remove favorite
//! (MenuGame.cpp:4835-4931), F2's switch to/from the favorites view
//! (MenuGame.cpp:4933-4955), the profile-switch rebuild
//! `changeFavoriteLevelsToProfile` (MenuGame.cpp:4781-4833), and the two
//! favorites-conditional halves the rebuild activates:
//! `getSelectablePackInfosSize`'s single-pack arm (MenuGame.cpp:4981-4983)
//! and `playSelectedLevel`'s active-drawer lookup (MenuGame.cpp:2369-2383).
//!
//! The toggle is wired through the 029c1 profile store — the acceptance
//! test (`a_favorited_level_survives_save_and_reload`) drives F1 over the
//! REAL `oh_assets::Assets` store and proves a favorited level is still
//! favorited after a save/reload (a fresh `Assets` over the same tree =
//! the C++'s restart).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::seams::{
    FakeAssets, FakeConfig, FakeWindow, MenuAssets, MenuJoystick, NewGameCallback, RecordingAudio,
};
use oh_menu::{MenuGame, NoMenuJoystick, NoopNetwork, NoopTouch, States};

fn menu_fixture() -> (MenuGame, Rc<FakeAssets>, Rc<RecordingAudio>) {
    // Two packs: pA (three levels whose LOWERCASED names sort in a
    // different order than their ids — the rebuild's sort proof) + pB
    // (one level, the `play_selected_level` active-drawer proof).
    let assets = FakeAssets::new();
    assets.add_pack("pA", &["Zebra", "apple", "Mango"]);
    assets.add_pack("pB", &["beta"]);
    assets.add_profile("p1");
    assets.profile_set_current("p1");
    let audio = RecordingAudio::new();
    let menu = MenuGame::new(
        assets.clone(),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        audio.clone(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        None, // `unlock_achievement` — the 29b2/29b8 Steam seam (None in the tests)
        Rc::new(LinearFontHeight),
    );
    (menu, assets, audio)
}

/// SMain → `ok_action` (the "LEVEL SELECT" item) → the level selection.
fn into_level_selection(menu: &MenuGame) {
    menu.shared().borrow_mut().change_state_to(States::SMain);
    menu.ok_action();
    assert_eq!(menu.state(), States::LevelSelection);
}

/// C++ `addRemoveFavoriteLevel`'s add arm (MenuGame.cpp:4893-4931): F1
/// favorites the level under the active drawer's index — the profile's
/// favorite set, the favorites list, the offsets, `isLevelFavorite`, and
/// the select sound all follow.
#[test]
fn f1_adds_the_level_to_the_profile_and_the_favorites_list() {
    let (menu, assets, audio) = menu_fixture();
    into_level_selection(&menu);
    let level_id = menu
        .shared()
        .borrow()
        .lvl_slct
        .level_data_ids
        .first()
        .cloned()
        .expect("the boot pack has levels");
    menu.add_remove_favorite_level();
    let s = menu.shared().borrow();
    assert!(
        s.is_level_favorite,
        "the C++ `isLevelFavorite = true` (the add arm, :4895)"
    );
    assert_eq!(
        s.fav_slct.level_data_ids,
        vec![level_id.clone()],
        "the level is in the favorites list (the `favoriteLevelDataIds` insert)"
    );
    assert_eq!(
        s.fav_slct.lvl_offsets,
        vec![0.0f32],
        "the C++ `favSlct.lvlOffsets.emplace_back(0.f)` (:4894)"
    );
    assert_eq!(s.fav_slct.current_index, 0, "the C++ :4926 clamp");
    drop(s);
    assert!(
        assets.is_level_favorite(&level_id),
        "the 029c1 store carries the favorite (the `getCurrentLocalProfile().addFavoriteLevel` half)"
    );
    assert!(
        audio.played.borrow().iter().any(|s| s == "select.ogg"),
        "the C++ :4930 select sound"
    );
}

/// C++ `addRemoveFavoriteLevel`'s remove arm (MenuGame.cpp:4841-4892) when
/// it EMPTIES the favorites list: the profile write, the list erase, the
/// offset pop, the index reset, the favorites-view exit
/// (`favSlct.XOffset = 0` + `lvlDrawer = &lvlSlct`), and the `setIndex`
/// re-derivation of `isLevelFavorite` (the C++ :4891 — the regular view
/// re-reads the profile, where the level was just removed).
#[test]
fn f1_removes_the_level_and_exits_the_favorites_view_when_the_list_empties() {
    let (menu, assets, _audio) = menu_fixture();
    assets.add_favorite_level("pA_apple");
    menu.shared()
        .borrow_mut()
        .change_favorite_levels_to_profile();
    into_level_selection(&menu);
    menu.switch_to_from_favorite_levels(); // the F2 switch in
    assert!(menu.shared().borrow().favorites_active);
    menu.add_remove_favorite_level(); // the F1 toggle off
    let s = menu.shared().borrow();
    assert!(
        !s.favorites_active,
        "the empty list forces the regular drawer (the C++ `lvlDrawer = &lvlSlct`, :4871-4872)"
    );
    assert_eq!(
        s.fav_slct.x_offset, 0.0,
        "the C++ `favSlct.XOffset = 0.f` (:4871)"
    );
    assert!(s.fav_slct.level_data_ids.is_empty());
    assert_eq!(s.fav_slct.current_index, 0, "the C++ :4858 index reset");
    assert!(
        !s.is_level_favorite,
        "the C++ :4891 `setIndex` re-derivation: the regular view re-reads the profile (the level is no longer a favorite)"
    );
    drop(s);
    assert!(
        !assets.is_level_favorite("pA_apple"),
        "the store erase (the C++ `removeFavoriteLevel`, :4845)"
    );
}

/// C++ `changeFavoriteLevelsToProfile` (MenuGame.cpp:4781-4833): the
/// rebuild sorts by the LOWERCASED level name (the :4794-4798 comparator,
/// NOT the id order), clamps `favSlct.currentIndex` (:4812), and resizes
/// `favSlct.lvlOffsets` (:4815 — the C++ `resize` keeps the existing
/// values, value-initializing the new tail).
#[test]
fn change_favorite_levels_to_profile_sorts_by_lowercased_name_clamps_and_resizes() {
    let (menu, assets, _audio) = menu_fixture();
    // The fake's set iterates in id (byte) order:
    // "pA_Mango" < "pA_Zebra" < "pA_apple" (uppercase < lowercase) —
    // the lowercased-name sort must reorder that to apple, Mango, Zebra.
    assets.add_favorite_level("pA_Zebra");
    assets.add_favorite_level("pA_apple");
    assets.add_favorite_level("pA_Mango");
    {
        let mut s = menu.shared().borrow_mut();
        s.fav_slct.current_index = 99; // out of bounds: the clamp proof
        s.fav_slct.lvl_offsets = vec![1.0, 2.0]; // kept by the C++ `resize`
    }
    menu.shared()
        .borrow_mut()
        .change_favorite_levels_to_profile();
    let s = menu.shared().borrow();
    assert_eq!(
        s.fav_slct.level_data_ids,
        vec!["pA_apple".to_string(), "pA_Mango".into(), "pA_Zebra".into()],
        "the C++ :4794-4798 lowercased-name sort (NOT the id order)"
    );
    assert_eq!(
        s.fav_slct.current_index, 2,
        "the C++ :4812 clamp to size − 1"
    );
    assert_eq!(
        s.fav_slct.lvl_offsets,
        vec![1.0, 2.0, 0.0],
        "the C++ :4815 resize (keeps the two existing, zero-fills the new)"
    );
}

/// C++ `changeFavoriteLevelsToProfile`'s no-favorites arm (MenuGame.cpp:
/// 4804-4808): an empty favorites list forces the regular drawer even
/// when the favorites view was active.
#[test]
fn change_favorite_levels_to_profile_with_no_favorites_forces_the_regular_drawer() {
    let (menu, _assets, _audio) = menu_fixture();
    {
        let mut s = menu.shared().borrow_mut();
        s.favorites_active = true;
        s.fav_slct.level_data_ids = vec!["pA_apple".into()];
    }
    menu.shared()
        .borrow_mut()
        .change_favorite_levels_to_profile();
    assert!(
        !menu.shared().borrow().favorites_active,
        "the C++ `lvlDrawer = &lvlSlct` (:4806)"
    );
}

/// C++ `switchToFromFavoriteLevels` (MenuGame.cpp:4933-4955): the F2
/// round trip (the drawer switch, the `setIndex` looks-update on the NEW
/// drawer, the offsets, the select sound) + both no-op guards (no
/// favorites list; outside the level selection).
#[test]
fn f2_switches_to_and_from_the_favorites_view() {
    let (menu, assets, audio) = menu_fixture();
    assets.add_favorite_level("pA_apple");
    assets.add_favorite_level("pA_Mango");
    menu.shared()
        .borrow_mut()
        .change_favorite_levels_to_profile();
    into_level_selection(&menu);

    menu.switch_to_from_favorite_levels();
    let s = menu.shared().borrow();
    assert!(s.favorites_active, "the C++ :4950 switch in");
    assert_eq!(s.fav_slct.current_index, 0);
    assert!(
        s.fav_slct.is_favorites && !s.lvl_slct.is_favorites,
        "the per-drawer `isFavorites` flags (the C++ `LevelDrawer::isFavorites`)"
    );
    drop(s);
    assert!(
        audio
            .played
            .borrow()
            .iter()
            .filter(|s| *s == "select.ogg")
            .count()
            >= 2,
        "the C++ :4954 select sound (ok_action's + the switch's)"
    );

    menu.switch_to_from_favorite_levels();
    assert!(
        !menu.shared().borrow().favorites_active,
        "the C++ :4950 switch out"
    );

    // Guard 1: an empty favorites list is a no-op (the C++ :4934-4937).
    let (menu2, _a2, audio2) = menu_fixture();
    menu2.switch_to_from_favorite_levels();
    assert!(
        !menu2.shared().borrow().favorites_active,
        "the empty-list guard"
    );
    assert!(audio2.played.borrow().is_empty(), "the guard plays nothing");
    // Guard 2: outside the level selection it is a no-op too.
    menu2.shared().borrow_mut().change_state_to(States::SMain);
    menu2.switch_to_from_favorite_levels();
    assert!(!menu2.shared().borrow().favorites_active, "the state guard");
}

/// C++ `switchToFromFavoriteLevels`'s in-flight-fold arm (MenuGame.cpp:
/// 4940-4943): a `PackChange::Folding` pack change is finished FIRST
/// (`changePack()` on the outgoing drawer), then the view switches.
#[test]
fn f2_completes_an_in_flight_pack_fold() {
    let (menu, assets, _audio) = menu_fixture();
    assets.add_favorite_level("pA_apple");
    menu.shared()
        .borrow_mut()
        .change_favorite_levels_to_profile();
    into_level_selection(&menu);
    let pack_before = menu.shared().borrow().lvl_slct.pack_idx;
    {
        let mut s = menu.shared().borrow_mut();
        s.pack_change_state = oh_menu::PackChange::Folding;
        s.pack_change_direction = 1;
    }
    menu.switch_to_from_favorite_levels();
    let s = menu.shared().borrow();
    assert!(s.favorites_active, "the switch ran");
    assert_eq!(
        s.pack_change_state,
        oh_menu::PackChange::Rest,
        "the C++ :4944 reset"
    );
    assert_eq!(
        s.lvl_slct.pack_idx,
        pack_before + 1,
        "the in-flight fold was finished (the C++ `changePack()`, :4941-4942 — the pack switched)"
    );
}

/// The audit's D1: C++ `playSelectedLevel` (MenuGame.cpp:2369-2383) plays
/// the ACTIVE drawer's level (`lvlDrawer`, not always `lvlSlct`) — latent
/// while favorites were unreachable, live now: with the favorites view
/// on, the played level + pack come from `favSlct`.
#[test]
fn play_selected_level_uses_the_active_drawer_when_favorites_are_active() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let calls_cb = calls.clone();
    let cb: NewGameCallback = Box::new(move |pack, level, _fp, _dm, _er| {
        calls_cb
            .borrow_mut()
            .push((pack.to_string(), level.to_string()));
    });
    let assets = FakeAssets::new();
    assets.add_pack("pA", &["Zebra", "apple", "Mango"]);
    assets.add_pack("pB", &["beta"]);
    assets.add_profile("p1");
    assets.profile_set_current("p1");
    let audio = RecordingAudio::new();
    let menu = MenuGame::new(
        assets.clone(),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        audio,
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        Some(cb),
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    into_level_selection(&menu);
    // The favorites list holds pB's level; the regular drawer stays on pA.
    assets.add_favorite_level("pB_beta");
    menu.shared()
        .borrow_mut()
        .change_favorite_levels_to_profile();
    menu.switch_to_from_favorite_levels();
    assert!(menu.shared().borrow().favorites_active);
    menu.shared().borrow_mut().play_selected_level();
    assert_eq!(
        calls.borrow().as_slice(),
        &[("pB".to_string(), "pB_beta".to_string())],
        "the C++ `lvlDrawer` (the favorites drawer) drives the pack + level"
    );
    assert_eq!(
        menu.shared().borrow().lvl_slct.pack_idx,
        0,
        "the regular drawer was untouched"
    );
}

/// The audit's D7: C++ `getSelectablePackInfosSize` (MenuGame.cpp:
/// 4981-4983) counts the favorites view as a SINGLE pack — which is what
/// the `upAction` / `downAction` single-pack wrap arms and
/// `changePackAction`'s guard key on (the favorites list wraps in place
/// instead of folding to the next pack).
#[test]
fn favorites_view_counts_as_a_single_pack() {
    let (menu, assets, audio) = menu_fixture();
    assets.add_favorite_level("pA_apple");
    assets.add_favorite_level("pA_Mango");
    menu.shared()
        .borrow_mut()
        .change_favorite_levels_to_profile();
    into_level_selection(&menu);
    assert_eq!(menu.shared().borrow().get_selectable_pack_infos_size(), 2);
    menu.switch_to_from_favorite_levels();
    let s = menu.shared().borrow();
    assert_eq!(
        s.get_selectable_pack_infos_size(),
        1,
        "the C++ :4982 `isFavoriteLevels() ? 1 : …`"
    );
    drop(s);
    // The top wrap (the C++ :1964-1981 single-pack arm on the favorites
    // drawer): up at index 0 jumps to the list's end.
    menu.up_action();
    let s = menu.shared().borrow();
    assert_eq!(
        s.fav_slct.current_index, 1,
        "the single-pack top wrap (the C++ :1969 `setIndex(size − 1)`) — NOT a pack change"
    );
    assert_eq!(
        s.lvl_slct.pack_idx, 0,
        "the regular drawer's pack is untouched"
    );
    drop(s);
    assert!(
        audio.played.borrow().iter().any(|s| s == "beep.ogg"),
        "the C++ :2000 beep"
    );
}

// ---------------------------------------------------------------------------
// The joystick rising edges (MenuGame.cpp:2746-2754) — the C++'s TWO
// separate `if`s (both may fire in one frame).
// ---------------------------------------------------------------------------

struct MachineMenuJoystick {
    poll: Rc<RefCell<oh_config::JoystickPoll>>,
}

impl MachineMenuJoystick {
    fn set_poll(&self, poll: oh_config::JoystickPoll) {
        *self.poll.borrow_mut() = poll;
    }
}

impl MenuJoystick for MachineMenuJoystick {
    fn update(&self, deadzone: f32) {
        oh_config::joystick::update(deadzone, &self.poll.borrow());
    }
    fn pressed_focus(&self) -> bool {
        oh_config::joystick::pressed(oh_config::Jid::Focus)
    }
    fn rising_edge(&self, jid: oh_config::Jid) -> bool {
        oh_config::joystick::rising_edge(jid)
    }
    fn rising_edge_dir(&self, jdir: oh_config::Jdir) -> bool {
        oh_config::joystick::rising_edge_dir(jdir)
    }
    fn ignore_all_presses(&self, ignore: bool) {
        oh_config::joystick::ignore_all_presses(ignore)
    }
}

/// The machine is process-global → ONE `#[test]`, sequential scenarios
/// (the `joystick.rs` discipline).
#[test]
fn f1_f2_fire_on_the_joystick_rising_edges() {
    // The C++ `HexagonGame` ctor's `loadAllJoystickBinds()` defaults put
    // `j_add_favorite` / `j_favorite_menu` on buttons 8/9 — out of the
    // SFML 8-button range (never pressable); the rebind path's machine
    // half (the `joystick.rs` 7/6 pattern).
    let cfg = oh_config::Config::default();
    cfg.load_all_joystick_binds();
    oh_config::joystick::set_joystick_bind(5, oh_config::Jid::AddToFavorites as i32);
    oh_config::joystick::set_joystick_bind(3, oh_config::Jid::FavoritesMenu as i32);

    let joy = Rc::new(MachineMenuJoystick {
        poll: Rc::new(RefCell::new(oh_config::JoystickPoll::default())),
    });
    let assets = FakeAssets::new();
    assets.add_pack("pA", &["Zebra", "apple", "Mango"]);
    assets.add_pack("pB", &["beta"]);
    assets.add_profile("p1");
    assets.profile_set_current("p1");
    let audio = RecordingAudio::new();
    let menu = MenuGame::new(
        assets.clone(),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        audio.clone(),
        Rc::new(NoopTouch),
        joy.clone(),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    into_level_selection(&menu);
    menu.update(0.016); // one frame, empty poll: the machine settles

    // The `j_add_favorite` rising edge: F1's add (MenuGame.cpp:2746-2749).
    let mut add = oh_config::JoystickPoll::default();
    add.buttons[5] = true;
    joy.set_poll(add);
    menu.update(0.016);
    assert!(
        menu.shared().borrow().is_level_favorite,
        "the AddToFavorites rising edge favorited the boot level"
    );
    joy.set_poll(oh_config::JoystickPoll::default());
    menu.update(0.016); // release

    // The `j_favorite_menu` rising edge: F2's switch in (MenuGame.cpp:
    // 2751-2754).
    let mut fav = oh_config::JoystickPoll::default();
    fav.buttons[3] = true;
    joy.set_poll(fav);
    menu.update(0.016);
    assert!(
        menu.shared().borrow().favorites_active,
        "the FavoritesMenu rising edge entered the favorites view"
    );
    joy.set_poll(oh_config::JoystickPoll::default());
    menu.update(0.016);
}

// ---------------------------------------------------------------------------
// The acceptance point: the toggle round-trips through the 029c1 store.
// ---------------------------------------------------------------------------

/// A minimal pack tree (the `oh-assets/tests/profiles.rs` favorites
/// fixture shape): `Assets::new` headless needs the (possibly sparse)
/// `Packs/` + `Profiles/` folders; the level ids are
/// `<disamb>_<author>_<name>_<version>_<id>`.
fn minimal_release_tree(dir: &std::path::Path) {
    let pack = dir.join("Packs").join("favtest");
    std::fs::create_dir_all(pack.join("Levels")).expect("pack dir");
    std::fs::write(
        pack.join("pack.json"),
        r#"{
    "disambiguator": "ohvrfav",
    "name": "favtest",
    "author": "tester",
    "version": 1
}
"#,
    )
    .expect("pack.json");
    std::fs::write(pack.join("Levels").join("zeta.json"), r#"{ "id": "zeta" }"#)
        .expect("level zeta");
    std::fs::write(
        pack.join("Levels").join("alpha.json"),
        r#"{ "id": "alpha" }"#,
    )
    .expect("level alpha");
    std::fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
}

/// THE acceptance point (the task's point 1): a favorited level must still
/// be favorited after a save/reload — F1 drives the REAL `oh_assets::Assets`
/// store (the 029c1 single-source-of-truth), `profile_save_current`
/// persists it, and a FRESH `Assets` over the same tree (the C++'s
/// restart) reads it back: the profile's set AND the menu's rebuilt
/// favorites list both carry the level.
#[test]
fn a_favorited_level_survives_save_and_reload() {
    let dir = std::env::temp_dir().join(format!(
        "oh-menu-favorites-{}-{}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace('-', "_")
    ));
    let _ = std::fs::remove_dir_all(&dir); // a stale dir from a prior run
    std::fs::create_dir_all(&dir).expect("temp dir");
    minimal_release_tree(&dir);
    let assets = Arc::new(oh_assets::Assets::new(&dir, true, false).expect("headless load"));
    let menu_assets = oh_menu::RealMenuAssets::from_arc(assets.clone());
    // The C++ `pCreate` + `pSetCurrent` (the boot profile's creation).
    menu_assets.profile_create("p1");
    menu_assets.profile_set_current("p1");

    let audio = RecordingAudio::new();
    let menu = MenuGame::new(
        Rc::new(menu_assets),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        audio,
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    into_level_selection(&menu);
    let level_id = menu
        .shared()
        .borrow()
        .lvl_slct
        .level_data_ids
        .first()
        .cloned()
        .expect("the fixture pack has levels");
    menu.add_remove_favorite_level(); // the F1 toggle on
    assert!(
        assets.is_level_favorite(&level_id),
        "F1 wrote the favorite into the profile (the 029c1 store)"
    );
    assert!(menu.shared().borrow().is_level_favorite);
    assert_eq!(
        menu.shared().borrow().fav_slct.level_data_ids,
        vec![level_id.clone()],
        "the menu's favorites list carries it too"
    );

    // The save (the C++ `pSaveCurrent` — the exit-save / `playLocally`
    // call site) + the reload (a FRESH `Assets` over the same tree — the
    // C++'s restart re-running `loadAllLocalProfiles`).
    assets.profile_save_current();
    let assets2 = oh_assets::Assets::new(&dir, true, false).expect("reload");
    assets2.profile_set_current("p1");
    assert!(
        assets2.is_level_favorite(&level_id),
        "the favorite SURVIVED the save/reload (the point of the feature)"
    );

    // The new menu (the restart's `MenuGame` ctor over the reloaded
    // store) rebuilds the favorites list from the profile.
    let menu2 = MenuGame::new(
        Rc::new(oh_menu::RealMenuAssets::from_arc(Arc::new(assets2))),
        FakeConfig::new(),
        Rc::new(NoopNetwork),
        RecordingAudio::new(),
        Rc::new(NoopTouch),
        Rc::new(NoMenuJoystick),
        FakeWindow::new(),
        None,
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );
    menu2
        .shared()
        .borrow_mut()
        .change_favorite_levels_to_profile();
    assert_eq!(
        menu2.shared().borrow().fav_slct.level_data_ids,
        vec![level_id],
        "the rebuilt favorites list still carries the level"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
