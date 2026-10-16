//! "List every pack/level the real `_RELEASE` contains" test (issue 013
//! verification). Loads the real `_RELEASE` through [`RealMenuAssets`] and
//! drives `MenuGame`'s pack switching, asserting the menu lists exactly the
//! level count the assets report for each selectable pack.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use oh_menu::layout::LinearFontHeight;
use oh_menu::{
    FakeConfig, MenuAssets, MenuConfig, MenuGame, NoopNetwork, NoopTouch, RealMenuAssets,
    RecordingAudio,
};

fn release_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let root = Path::new(&manifest)
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root");
    let rel = root.join("_RELEASE");
    assert!(rel.is_dir(), "_RELEASE not found at {rel:?}");
    rel
}

#[test]
fn menu_lists_every_pack_and_level() {
    let assets: Rc<dyn MenuAssets> =
        Rc::new(RealMenuAssets::new(release_dir()).expect("_RELEASE loads"));
    let config: Rc<dyn MenuConfig> = FakeConfig::new();

    let menu = MenuGame::new(
        assets.clone(),
        config,
        Rc::new(NoopNetwork),
        RecordingAudio::new(),
        Rc::new(NoopTouch),
        None,
        None,
        None,
        None,
        Rc::new(LinearFontHeight),
    );

    let packs: Vec<String> = assets.selectable_pack_ids();
    assert!(!packs.is_empty(), "no selectable packs in _RELEASE?");
    assert_eq!(menu.selectable_pack_count(), packs.len());

    let mut total_levels = 0usize;
    for (i, pack) in packs.iter().enumerate() {
        // Drive the menu's pack switch, then read back what it lists.
        menu.shared().borrow_mut().change_pack_to(i as i32);
        assert_eq!(
            menu.current_pack_id(),
            *pack,
            "wrong pack after change_pack_to({i})"
        );

        let expected = assets.level_ids_for_pack(pack);
        assert_eq!(
            menu.current_level_count(),
            expected.len(),
            "menu level count for pack {pack} does not match the assets"
        );

        // The selected level (index 0) must resolve to a real level id in that
        // pack (i.e. the menu lists real levels, not placeholders).
        if !expected.is_empty() {
            assert!(
                assets.level(&expected[0]).is_some(),
                "level {:#?} missing",
                expected[0]
            );
            assert_eq!(menu.selected_level_id(), expected[0]);
        }
        total_levels += expected.len();
    }

    assert!(total_levels > 0, "no levels found across _RELEASE packs");
}
