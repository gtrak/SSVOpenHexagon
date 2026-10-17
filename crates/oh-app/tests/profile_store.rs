//! The profile store's single-source-of-truth (issue 029c1's design crux):
//! the C++ has ONE `HGAssets` — the menu and the engine observe the SAME
//! profile state (main.cpp:381/:434), so a score set during play is
//! immediately visible in the menu. The port's `compose::RealProfiles`
//! (the engine's `Profiles` seam) and `oh_menu::RealMenuAssets` must be
//! backed by ONE `Arc<oh_assets::Assets>`; a design where the two
//! disagree after a run is a failed lane — this test is the proof.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use oh_app::compose::RealProfiles;
use oh_game::seams::Profiles;
use oh_menu::seams::MenuAssets;
use oh_menu::RealMenuAssets;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oh-app-profile-store-{}-{}-{tag}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace('-', "_")
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn the_engine_and_the_menu_observe_one_profile_store() {
    let dir = temp_dir("one_store");
    fs::create_dir_all(dir.join("Packs")).expect("packs dir");
    fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
    let assets = Arc::new(oh_assets::Assets::new(&dir, true, false).expect("headless load"));

    // The menu side (C++ `MenuGame`'s `assets` reference): create +
    // activate the profile — the C++'s `pCreate` + `pSetCurrent`
    // (MenuGame.cpp:2235-2236).
    let menu = RealMenuAssets::from_arc(assets.clone());
    menu.profile_create("player1");
    menu.profile_set_current("player1");
    assert!(menu.has_current_profile(), "the menu sees its own creation");
    assert_eq!(menu.current_profile_name().as_deref(), Some("player1"));

    // The engine side (C++ `HexagonGame`'s `assets` reference): the death
    // path's PB write (HexagonGame.cpp:871,908,915 → `setLocalScore`).
    let profiles: Arc<dyn Profiles> = Arc::new(RealProfiles::new(assets.clone()));
    assert!(
        profiles.any_local_profile_active(),
        "the engine sees the menu's activation"
    );
    assert_eq!(profiles.current_local_profile_name(), "player1");
    profiles.set_local_score("ohvrvanilla_vittorio_romeo_cube_1_pi", 42.0);

    // The crux: the menu reads the engine's write with no sync step
    // (C++ `assets.getCurrentLocalProfile().getScore(validator)`,
    // MenuGame.cpp:5494 — the local personal best).
    assert_eq!(
        menu.local_profile_score("ohvrvanilla_vittorio_romeo_cube_1_pi"),
        42.0,
        "the menu observes the engine's score in the SAME store"
    );
    assert_eq!(
        profiles.get_local_score("ohvrvanilla_vittorio_romeo_cube_1_pi"),
        42.0
    );

    // And the exit save (the C++ `mainClient` pSaveAll guard,
    // main.cpp:382-388) persists it: both sides feed the same store the
    // guard would read.
    assets.profile_save_all();
    let written = fs::read_to_string(dir.join("Profiles").join("player1.json")).expect("file");
    assert!(
        written.contains("\"ohvrvanilla_vittorio_romeo_cube_1_pi\" : 42.0"),
        "the run's PB reached disk: {written}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn no_profile_active_reports_the_cpp_nullptr_state_on_both_sides() {
    let dir = temp_dir("no_profile");
    fs::create_dir_all(dir.join("Packs")).expect("packs dir");
    fs::create_dir_all(dir.join("Profiles")).expect("profiles dir");
    // A profile file exists on disk, but the C++ never sets the current
    // pointer at load (only `pSetCurrent` does): both sides must report
    // the `currentProfilePtr == nullptr` state.
    fs::write(
        dir.join("Profiles").join("p1.json"),
        "{\n\t\"name\" : \"p1\"\n}\n",
    )
    .expect("seed");
    let assets = Arc::new(oh_assets::Assets::new(&dir, true, false).expect("headless load"));
    let profiles: Arc<dyn Profiles> = Arc::new(RealProfiles::new(assets.clone()));
    let menu = RealMenuAssets::from_arc(assets);
    assert!(!profiles.any_local_profile_active());
    assert_eq!(profiles.current_local_profile_name(), "");
    assert!(!menu.has_current_profile());
    assert!(menu.current_profile_name().is_none());
    // The C++ `shouldSaveScore` reject (HexagonGame.cpp:1178-1183) gates
    // the score path: a `set_local_score` here would be an assert in the
    // C++ — the port's legacy no-op arm keeps both sides coherent.
    profiles.set_local_score("any_level", 1.0);
    assert_eq!(menu.local_profile_score("any_level"), 0.0);
    let _ = fs::remove_dir_all(&dir);
}
