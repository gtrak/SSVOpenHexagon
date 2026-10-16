//! Issue 009 step (c): the Lua-callback firing-ORDER test
//! (mapping/lua-api.md §5 — the C++ call sites).
//!
//! A synthetic level (built in a temp dir — NO shipped content is modified)
//! whose script records the FIRST firing of each engine callback into the
//! Lua global table `fireOrder`; the engine runs it to death, and the table
//! is read back. The expected first-fire order is the C++ call-site order:
//!
//!   onInit      — `newGame`, right after `runLuaFile` (HexagonGame.cpp:746)
//!   onLoad      — `start()` (HGUpdate.cpp:549)
//!   onInput     — `update()`, per frame, BEFORE `updateLevel` (HGUpdate.cpp:244)
//!   onUpdate    — `updateLevel` (HGUpdate.cpp:738)
//!   onStep      — `updateLevel`, when the main timeline drains (HGUpdate.cpp:745)
//!   onPreDeath  — `death()`, before the invincibility check (HexagonGame.cpp:954)
//!   onDeath     — `death()` (HexagonGame.cpp:961)
//!
//! The kill is scheduled on the EVENT timeline (`e_wait(5); e_kill()`): it
//! lands in `updateEvents`, which runs BEFORE `updateLevel` on that frame
//! (HGUpdate.cpp:296 vs :305) — so `onStep` (main timeline, empty → drained
//! every frame) has already fired on earlier frames, and `onPreDeath` /
//! `onDeath` close the sequence on the kill frame.

mod common;

use std::io::Write;
use std::sync::Arc;

/// The synthetic pack's files (pack id `testport_tester_ordering_1` via
/// `buildPackId`; level id `testport_tester_ordering_1_seq`).
fn write_synthetic_pack(root: &std::path::Path) {
    let pack = root.join("Packs/testport_ordering");
    std::fs::create_dir_all(pack.join("Levels")).unwrap();
    std::fs::create_dir_all(pack.join("Styles")).unwrap();
    std::fs::create_dir_all(pack.join("Music")).unwrap();
    std::fs::create_dir_all(pack.join("Scripts")).unwrap();

    fn write_file(path: &std::path::Path, content: &str) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    write_file(
        &pack.join("pack.json"),
        r#"{
    "disambiguator": "testport",
    "name": "ordering",
    "author": "tester",
    "version": 1
}"#,
    );
    // Minimal style / music (all other fields take their C++ defaults).
    write_file(&pack.join("Styles/basic.json"), r#"{"id": "basic"}"#);
    write_file(&pack.join("Music/none.json"), r#"{"id": "none"}"#);
    write_file(
        &pack.join("Levels/seq.json"),
        r#"{"id": "seq", "styleId": "basic", "musicId": "none", "luaFile": "Scripts/seq.lua"}"#,
    );
    write_file(
        &pack.join("Scripts/seq.lua"),
        r#"-- Records the FIRST firing of each engine callback into the global
-- table `fireOrder` (issue 009c callback-order test).
fireOrder = {}
function _recordFirst(name)
    if not fireOrder[name] then
        fireOrder[name] = true
        fireOrder[#fireOrder + 1] = name
    end
end

function onInit()
    _recordFirst("onInit")
    -- Kill on frame ~6 via the EVENT timeline (e_kill → backend.death(true));
    -- the main timeline stays empty, so onStep fires every frame (updateLevel).
    e_wait(5)
    e_kill()
end
function onLoad()  _recordFirst("onLoad")  end
function onInput(mFT, mMovement, mFocus, mSwap) _recordFirst("onInput") end
function onUpdate(mFT) _recordFirst("onUpdate") end
function onStep()  _recordFirst("onStep")  end
function onPreDeath() _recordFirst("onPreDeath") end
function onDeath()   _recordFirst("onDeath")   end
function onIncrement() _recordFirst("onIncrement") end
function onCursorSwap() _recordFirst("onCursorSwap") end
function onPreUnload() _recordFirst("onPreUnload") end
function onUnload()  _recordFirst("onUnload")  end
"#,
    );
}

#[test]
fn callback_firing_order_matches_cpp_call_sites() {
    let dir = std::env::temp_dir().join(format!("oh-game-callback-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write_synthetic_pack(&dir);

    let assets = Arc::new(
        oh_assets::Assets::new(Arc::new(oh_vfs::RealFs::new(&dir)), true, false)
            .expect("synthetic pack loads headless"),
    );
    let c = common::compose(assets, dir.clone(), false);

    // No walls in the synthetic level — the `e_kill` event is the death.
    c.game.set_always_spin_right(true);
    c.game.new_game(
        "testport_tester_ordering_1",
        "testport_tester_ordering_1_seq",
        true,
        1.0,
        false,
    );
    let (score, rf) = common::run_live_to_death(&c.game);
    assert!(score > 0.0, "the e_kill event must have killed the player");
    assert!(
        rf.data.size() >= 5,
        "the replay recorded the pre-kill input frames"
    );

    // Read back the recorded first-fire order from the Lua context.
    let order: Vec<String> = c
        .lua()
        .execute_code_typed("return fireOrder")
        .expect("fireOrder table");
    assert_eq!(
        order,
        vec![
            "onInit",
            "onLoad",
            "onInput",
            "onUpdate",
            "onStep",
            "onPreDeath",
            "onDeath"
        ],
        "first-fire order must match the C++ call sites (mapping/lua-api.md §5)"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
