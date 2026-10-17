//! 029n1 (K6) §4.1 — the headless/in-process column: the three shipped
//! shader levels (`pointless`, `text_shader`, `shaders_required`) against
//! the full Lua + `oh-assets` + engine stack.
//!
//! The audit finding (`.agents/audits/graphics.md` #5): the per-stage pack
//! fragment shaders are *described* (`RenderStates::shader`, the
//! `fragment_shader_ids` bindings, the `onRenderStage` dispatch) but never
//! *executed* — no GL program is ever compiled from a pack's `.frag`
//! source and no uniform is ever uploaded, because the asset registry
//! keeps no shader source or uniform store and every `draw_vertices`
//! ignores `states.shader`.
//!
//! These tests pin:
//!
//! * the description half that ALREADY works (the stage→shader bindings
//!   the levels' Lua makes land in `fragment_shader_ids`; the
//!   `shadersRequired` score-invalidation effect,
//!   `HGUpdate.cpp:420-423`);
//! * the execution half this lane adds (the `oh-assets` shader SOURCE +
//!   uniform store the GL layer compiles / uploads from — the
//!   `shader_snapshot` / `set_shader_uniform_*` API, C++ `sf::Shader`'s
//!   per-object uniform cache);
//! * the `onRenderStage` hook's C++ argument + suppression semantics
//!   (`HGGraphics.cpp:60-77` — `shaders_enabled == false` returns
//!   `RenderStates::Default` BEFORE the hook line; a stage without a
//!   bound id never fires the hook).
//!
//! The GL columns (§4.2 the four shipped `.frag` files' compile guard;
//! §4.3 the shader-bound draw) live in `examples/pack_shaders_gl.rs`
//! (skip-with-reason without a display, D-011).

mod common;

use std::sync::{Arc, RwLock};

use oh_assets::{ShaderType, UniformValue};
use oh_config::{Config, TIME_STEP};
use oh_game::seams::{NoJoystick, NoneSteamSink, NullReplayStore};

use common::{compose, release_dir};

/// The shipped level ids (`{packId}_{levelId}` — the `.json` `id` field).
const ARTWORK_PACK: &str = "thing_Synth_Morxemplum_Artwork_1";
const ARTWORK_POINTLESS: &str = "thing_Synth_Morxemplum_Artwork_1_shadertest";
const EXPERIMENTAL_PACK: &str = "ohvrvanilla_vittorio_romeo_experimental_1";
const EXPERIMENTAL_TEXT_SHADER: &str = "ohvrvanilla_vittorio_romeo_experimental_1_text_shader";
const EXPERIMENTAL_SHADERS_REQUIRED: &str =
    "ohvrvanilla_vittorio_romeo_experimental_1_ThreeDimensions-shRequired-test";

/// The non-headless load (C++ `!headless`): the `Shaders/` scan runs and
/// the registry registers every shipped pack shader. (The GL COMPILE is
/// the port's lazy draw-time half — the registry keeps the source, the
/// C++ keeps the compiled `sf::Shader`.)
fn full_assets() -> Arc<oh_assets::Assets> {
    Arc::new(
        oh_assets::Assets::new(release_dir(), false, false)
            .expect("_RELEASE loads full (non-headless)"),
    )
}

/// `_RELEASE` + a few frames of the level's real `onUpdate` (the uniform
/// sets run every frame for the shipped shader levels). `pack` and `level`
/// are the explicit C++ `newGame(mPackId, mId)` arguments (`m_id` is the
/// full `packId_levelId` asset id — `is_valid_level_id` keys on it). The
/// pack is NOT derived by slicing the level id: level ids may contain
/// `_` themselves (`text_shader`), so `rfind('_')` would cut inside it.
fn run_level(
    assets: &Arc<oh_assets::Assets>,
    pack: &str,
    level: &str,
    frames: u32,
) -> common::Composed {
    let c = compose(assets.clone(), release_dir(), true);
    c.game.new_game(pack, level, true, 1.0, false);
    c.game.set_must_start(true);
    for _ in 0..frames {
        c.game.update(TIME_STEP, 1.0);
        c.game.post_update();
    }
    c
}

/// `pointless.lua`: `shdr_getShaderId("steamart.frag")` at the top level,
/// `shdr_setActiveFragmentShader` × 8 in `onInit` (`:110-117`), and the
/// three uniform sets in `onUpdate` (`:164-166`).
#[test]
fn pointless_binds_steamart_to_the_eight_scene_stages_and_the_uniforms_land_in_the_store() {
    let assets = full_assets();
    let c = run_level(&assets, ARTWORK_PACK, ARTWORK_POINTLESS, 30);

    let steamart = assets
        .get_shader_id_by_path("Packs/artwork/Shaders/steamart.frag")
        .expect("the full load registers steamart.frag");

    // The 8 stage→shader assignments land in `fragment_shader_ids`
    // (the C++ `hexagonGameStatus.fragmentShaderIds`,
    // `LuaScripting.cpp:1549-1557`).
    let status = c.game.get_status();
    for stage in 0..8 {
        assert_eq!(
            status.fragment_shader_ids[stage],
            Some(steamart as u64),
            "stage {stage} (BACKGROUNDTRIS … PLAYERTRIS)"
        );
    }
    assert_eq!(
        status.fragment_shader_ids[8], None,
        "the TEXT stage stays unbound"
    );

    // The uniform store receives exactly the sets the Lua performed —
    // written by `shdr_setUniform*` (the C++ `setUniformUnsafe`'s
    // per-shader-object cache), readable at draw time:
    // `u_resolution` FVec2(`u_getWidth`, `u_getHeight`), `u_time` F,
    // `u_rotation` F.
    let snap = assets
        .shader_snapshot(steamart)
        .expect("the steamart snapshot");
    assert_eq!(snap.shader_type, ShaderType::Fragment);
    assert_eq!(
        snap.source,
        std::fs::read_to_string(release_dir().join("Packs/artwork/Shaders/steamart.frag"))
            .expect("steamart.frag"),
        "the registry keeps the file's source verbatim"
    );
    // the config's windowed 800×600 default (`u_getWidth`/`u_getHeight`).
    assert_eq!(
        snap.uniforms.get("u_resolution"),
        Some(&UniformValue::FVec2(800.0, 600.0))
    );
    assert!(
        matches!(snap.uniforms.get("u_time"), Some(UniformValue::F(t)) if *t > 0.0),
        "u_time = l_getLevelTime() after 30 started frames: {:?}",
        snap.uniforms.get("u_time")
    );
    assert!(
        snap.uniforms.contains_key("u_rotation"),
        "u_rotation = math.rad(l_getRotation()): {:?}",
        snap.uniforms.get("u_rotation")
    );
}

/// `text_shader.lua`: `text.frag` on `RenderStage.TEXT` (`:5-6`) + the
/// `color0` (FVec4) / `time` (F) uniform sets in `onUpdate`.
#[test]
fn text_shader_binds_text_frag_to_the_text_stage() {
    let assets = full_assets();
    let c = run_level(&assets, EXPERIMENTAL_PACK, EXPERIMENTAL_TEXT_SHADER, 30);

    let text_frag = assets
        .get_shader_id_by_path("Packs/experimental/Shaders/text.frag")
        .expect("the full load registers text.frag");

    let status = c.game.get_status();
    assert_eq!(
        status.fragment_shader_ids[8],
        Some(text_frag as u64),
        "the TEXT stage"
    );
    for stage in 0..8 {
        assert_eq!(
            status.fragment_shader_ids[stage], None,
            "stage {stage} stays unbound"
        );
    }

    let snap = assets
        .shader_snapshot(text_frag)
        .expect("the text.frag snapshot");
    assert_eq!(snap.shader_type, ShaderType::Fragment);
    assert_eq!(
        snap.source,
        std::fs::read_to_string(release_dir().join("Packs/experimental/Shaders/text.frag"))
            .expect("text.frag"),
    );
    // `onUpdate`: `shdr_setUniformFVec4(text_shader, "color0", s_getColor(0))`
    // + `shdr_setUniformF(text_shader, "time", l_getLevelTime())`.
    assert!(
        matches!(
            snap.uniforms.get("color0"),
            Some(UniformValue::FVec4(_, _, _, _))
        ),
        "color0 (the style's color 0, normalized to [0,1]): {:?}",
        snap.uniforms.get("color0")
    );
    assert!(
        matches!(snap.uniforms.get("time"), Some(UniformValue::F(t)) if *t > 0.0),
        "time: {:?}",
        snap.uniforms.get("time")
    );
}

/// `shaders_required.lua` (`l_setShadersRequired(true)`, `:57`) — the
/// C++ `HGUpdate.cpp:420-423` check: `!Config::getShaders() &&
/// levelStatus.shadersRequired` → `invalidateScore("SHADERS REQUIRED")`.
#[test]
fn shaders_required_invalidates_the_score_only_when_shaders_are_off() {
    let assets = full_assets();
    let root = release_dir();

    // Shaders ON (the default): the level loads and the score stays valid.
    {
        let c = run_level(
            &assets,
            EXPERIMENTAL_PACK,
            EXPERIMENTAL_SHADERS_REQUIRED,
            10,
        );
        let st = c.game.get_status();
        assert!(
            !st.score_invalid,
            "shaders on ⇒ no invalidation (reason: {:?})",
            st.invalid_reason
        );
        // the level's `l_setShadersRequired(true)` landed.
        assert!(c.game.get_level_status().shaders_required);
    }

    // Shaders OFF: the score is invalidated with the EXACT reason string.
    {
        let config = Arc::new(RwLock::new({
            let mut c = Config::default();
            c.set_shaders(false);
            c
        }));
        let c = oh_app::compose::compose_with_config(
            assets,
            root,
            true, /* window (the C++ `window != nullptr`) */
            Arc::new(common::TestProfiles),
            config,
            Arc::new(NoneSteamSink),
            None, /* discord */
            None, /* renderer */
            None, /* audio */
            Arc::new(NullReplayStore),
            Arc::new(NoJoystick),
        );
        c.game.new_game(
            EXPERIMENTAL_PACK,
            EXPERIMENTAL_SHADERS_REQUIRED,
            true,
            1.0,
            false,
        );
        c.game.set_must_start(true);
        for _ in 0..10 {
            c.game.update(TIME_STEP, 1.0);
            c.game.post_update();
        }
        let st = c.game.get_status();
        assert!(
            st.score_invalid,
            "shaders off + shadersRequired ⇒ invalidated"
        );
        assert_eq!(st.invalid_reason, "SHADERS REQUIRED");
    }
}

/// `RenderStage::COUNT` = 9 → the index→value table (the C++'s
/// `RenderStage` enum, `HGStatus.hpp:55-63`).
fn stage(i: usize) -> oh_render::graphics::RenderStage {
    use oh_render::graphics::RenderStage;
    match i {
        0 => RenderStage::BackgroundTris,
        1 => RenderStage::WallQuads3D,
        2 => RenderStage::PivotQuads3D,
        3 => RenderStage::PlayerTris3D,
        4 => RenderStage::WallQuads,
        5 => RenderStage::CapTris,
        6 => RenderStage::PivotQuads,
        7 => RenderStage::PlayerTris,
        8 => RenderStage::Text,
        _ => panic!("stage {i}"),
    }
}

/// `pointless` (the shipped level the C++ runs with the shader ON): the
/// `onRenderStage` hook fires ONCE per stage that has a bound shader,
/// with the C++ `(stage, 60.f / fps)` argument, for every such stage —
/// and NOT for the stages without one (the C++ `getRenderStates` lambda
/// returns `Default` before the hook line for both the
/// `!Config::getShaders()` and the no-bound-id cases,
/// `HGGraphics.cpp:62-71`).
#[test]
fn pointless_stage_hook_semantics_fire_only_for_bound_stages_with_the_cpp_argument() {
    use oh_render::graphics::{render_states_for_stage, FragmentShaderIds, RenderStage};
    use std::cell::RefCell;
    use std::rc::Rc;

    let assets = full_assets();
    let c = run_level(&assets, ARTWORK_PACK, ARTWORK_POINTLESS, 5);

    let steamart = assets
        .get_shader_id_by_path("Packs/artwork/Shaders/steamart.frag")
        .expect("steamart registered");

    // The REAL bindings the level made (8 scene stages, not TEXT).
    let ids: FragmentShaderIds = c
        .game
        .get_status()
        .fragment_shader_ids
        .map(|id| id.map(|id| id as usize));
    assert_eq!(
        ids,
        [
            Some(steamart),
            Some(steamart),
            Some(steamart),
            Some(steamart),
            Some(steamart),
            Some(steamart),
            Some(steamart),
            Some(steamart),
            None,
        ]
    );

    let calls = Rc::new(RefCell::new(Vec::new()));
    let calls_box = calls.clone();
    let hook: Box<dyn Fn(RenderStage, f32)> =
        Box::new(move |stage, ft| calls_box.borrow_mut().push((stage, ft)));

    // fps 120 → the C++ frametime argument `60.f / fps` = 0.5.
    let st = render_states_for_stage(true, &ids, RenderStage::BackgroundTris, 0.5, Some(&hook));
    assert_eq!(st, oh_render::draw::RenderStates::with_shader(steamart));
    assert_eq!(*calls.borrow(), vec![(RenderStage::BackgroundTris, 0.5)]);

    // The unbound TEXT stage: default states, NO hook call (C++ :70-71).
    let st = render_states_for_stage(true, &ids, RenderStage::Text, 0.5, Some(&hook));
    assert_eq!(st, oh_render::draw::RenderStates::default());
    assert_eq!(*calls.borrow(), vec![(RenderStage::BackgroundTris, 0.5)]);

    // Shaders disabled: default states + the hook suppressed (C++ :62-64)
    // even for a bound stage.
    let st = render_states_for_stage(false, &ids, RenderStage::WallQuads, 0.5, Some(&hook));
    assert_eq!(st, oh_render::draw::RenderStates::default());
    assert_eq!(*calls.borrow(), vec![(RenderStage::BackgroundTris, 0.5)]);

    // And the hook fires once per bound stage, in stage order, when the
    // full stage sweep runs (the `draw_frame`'s `getRenderStates` call
    // sites, `HGGraphics.cpp:119/:261-263/:275-278`).
    calls.borrow_mut().clear();
    for i in 0..RenderStage::COUNT {
        let _ = render_states_for_stage(true, &ids, stage(i), 0.5, Some(&hook));
    }
    let mut expected = Vec::new();
    for i in 0..8 {
        expected.push((stage(i), 0.5));
    }
    assert_eq!(
        *calls.borrow(),
        expected,
        "one call per bound stage, in order"
    );
}

/// The `shdr_getShaderId` log-string quirk the C++ ships (a function
/// named `shdr_getShaderId` logging `` `u_getShaderId` `` —
/// `LuaScripting.cpp:1257-1259`): a NON-headless run of the level's
/// real `shdr_getShaderId` for a name the pack does NOT ship returns the
/// `size_t(-1)` sentinel and the store stays untouched.
#[test]
fn shdr_get_shader_id_for_a_missing_name_returns_the_sentinel_headless_and_non() {
    let assets = full_assets();
    let c = run_level(&assets, ARTWORK_PACK, ARTWORK_POINTLESS, 1);

    // The real Lua function (non-headless composition): the lookup runs
    // against the registry and misses (no `missing.frag` in artwork).
    // The C++ returns `static_cast<std::size_t>(-1)` through
    // `lua_pushinteger(_state, nb)` (LuaWrapper.hpp's `_push` — every
    // integer type is an integer push), and the C++ build's LuaJIT has
    // `LUA_INTEGER` = `ptrdiff_t` (luaconf.h:106): the 64-bit cast of
    // 2^64-1 is exactly -1. The port's `IntoLuaValue for u64` wraps the
    // same way (`*self as i64`) — so the Lua-visible sentinel is the
    // integer -1 (read back as `i64`).
    let id: i64 = c
        .lua()
        .execute_code_typed("return shdr_getShaderId('missing.frag')")
        .expect("shdr_getShaderId callable");
    assert_eq!(
        id, -1,
        "the C++ `static_cast<std::size_t>(-1)` sentinel's Lua integer"
    );

    // The store has no entry for the sentinel — the `shdr_setUniform*`
    // guards (`withValidShaderId`) no-op + log for it. The Lua-side -1
    // wraps back to the `u64::MAX` the engine stores (the C++'s
    // `lua_tointeger` → `std::size_t` read path).
    assert!(assets.shader_snapshot(usize::MAX).is_none());
}

/// The headless C++ semantics (`Assets.cpp:873-879` + the Lua
/// `if(headless) return` guards): a headless load registers NO shaders
/// at all — `shdr_getShaderId` returns the sentinel without even
/// looking, and nothing lands in `fragment_shader_ids` (the
/// `shdr_setActiveFragmentShader` headless no-op).
#[test]
fn headless_load_registers_no_shaders_and_the_lua_path_is_a_noop() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    assert_eq!(
        assets.shader_count(),
        0,
        "the headless load skips the Shaders/ scan (C++ :873-879)"
    );
    assert!(assets
        .get_shader_id_by_path("Packs/artwork/Shaders/steamart.frag")
        .is_none());

    // And the engine side stays empty: the C++ headless
    // `shdr_setActiveFragmentShader` is a no-op, so a headless run of
    // `pointless` binds nothing.
    let c = {
        let a = assets.clone();
        let r = release_dir();
        let c = compose(a, r, false);
        c.game
            .new_game(ARTWORK_PACK, ARTWORK_POINTLESS, true, 1.0, false);
        c.game.set_must_start(true);
        for _ in 0..5 {
            c.game.update(TIME_STEP, 1.0);
            c.game.post_update();
        }
        c
    };
    assert_eq!(
        c.game.get_status().fragment_shader_ids,
        [None; 9],
        "headless ⇒ the Lua shdr_* path never binds a stage"
    );
}

// ---------------------------------------------------------------------------
// The uniform-store write surface (C++ `setUniformUnsafe`'s storage half):
// the eight kinds the Lua API exposes, round-tripping through the store
// the GL layer uploads at bind time.
// ---------------------------------------------------------------------------

#[test]
fn the_eight_uniform_kinds_round_trip_through_the_store() {
    let assets = full_assets();
    let id = assets
        .get_shader_id_by_path("Packs/artwork/Shaders/gradient.frag")
        .expect("gradient.frag registered");

    assets.set_shader_uniform_f(id, "u_blend", 0.5);
    assets.set_shader_uniform_f_vec2(id, "u_resolution", 1280.0, 720.0);
    assets.set_shader_uniform_f_vec3(id, "c3", 1.0, 2.0, 3.0);
    assets.set_shader_uniform_f_vec4(id, "c4", 1.0, 2.0, 3.0, 4.0);
    assets.set_shader_uniform_i(id, "n1", 7);
    assets.set_shader_uniform_i_vec2(id, "n2", 1, 2);
    assets.set_shader_uniform_i_vec3(id, "n3", 1, 2, 3);
    assets.set_shader_uniform_i_vec4(id, "n4", 1, 2, 3, 4);

    let snap = assets.shader_snapshot(id).expect("the snapshot");
    assert_eq!(snap.uniforms.get("u_blend"), Some(&UniformValue::F(0.5)));
    assert_eq!(
        snap.uniforms.get("u_resolution"),
        Some(&UniformValue::FVec2(1280.0, 720.0))
    );
    assert_eq!(
        snap.uniforms.get("c3"),
        Some(&UniformValue::FVec3(1.0, 2.0, 3.0))
    );
    assert_eq!(
        snap.uniforms.get("c4"),
        Some(&UniformValue::FVec4(1.0, 2.0, 3.0, 4.0))
    );
    assert_eq!(snap.uniforms.get("n1"), Some(&UniformValue::I(7)));
    assert_eq!(snap.uniforms.get("n2"), Some(&UniformValue::IVec2(1, 2)));
    assert_eq!(snap.uniforms.get("n3"), Some(&UniformValue::IVec3(1, 2, 3)));
    assert_eq!(
        snap.uniforms.get("n4"),
        Some(&UniformValue::IVec4(1, 2, 3, 4))
    );

    // An invalid id is a silent no-op (the `withValidShaderId` guard is
    // the Lua layer's; the store simply has no such entry).
    assets.set_shader_uniform_f(usize::MAX, "u_blend", 1.0);
    assert!(assets.shader_snapshot(usize::MAX).is_none());
}

/// The snapshot's uniform ordering is deterministic (the C++'s
/// `unordered_map` iteration is never observable — the port's
/// `BTreeMap` keeps the order name-sorted, which the GL upload's
/// per-name location lookup does not depend on).
#[test]
fn the_snapshot_uniforms_are_name_sorted() {
    let assets = full_assets();
    let id = assets
        .get_shader_id_by_path("Packs/experimental/Shaders/pixelate.frag")
        .expect("pixelate.frag registered");

    assets.set_shader_uniform_i_vec3(id, "z_last", 1, 1, 1);
    assets.set_shader_uniform_f_vec4(id, "color2", 0.0, 0.0, 1.0, 1.0);
    assets.set_shader_uniform_f_vec3(id, "color1", 1.0, 0.0, 0.0);
    assets.set_shader_uniform_f_vec2(id, "u_resolution", 100.0, 100.0);

    let snap = assets.shader_snapshot(id).expect("the snapshot");
    let names: Vec<&String> = snap.uniforms.keys().collect();
    assert_eq!(
        names,
        vec![
            &"color1".to_string(),
            &"color2".to_string(),
            &"u_resolution".to_string(),
            &"z_last".to_string(),
        ]
    );
}

// ---------------------------------------------------------------------------
// The compile guard's data side: the four shipped `.frag` files the §4.2
// guard compiles (raw + `translate_for_gles`) — the registry must expose
// them with their verbatim sources.
// ---------------------------------------------------------------------------

#[test]
fn the_four_shipped_frag_files_are_registered_with_verbatim_sources() {
    let assets = full_assets();
    for (path, marker) in [
        ("Packs/artwork/Shaders/gradient.frag", "u_resolution"),
        ("Packs/artwork/Shaders/steamart.frag", "u_rotation"),
        ("Packs/experimental/Shaders/pixelate.frag", "color1"),
        ("Packs/experimental/Shaders/text.frag", "#version 130"),
    ] {
        let id = assets
            .get_shader_id_by_path(path)
            .unwrap_or_else(|| panic!("{path} not registered by the full load"));
        let snap = assets.shader_snapshot(id).expect("the snapshot");
        assert_eq!(snap.shader_type, ShaderType::Fragment);
        assert!(
            snap.source.contains(marker),
            "{path} source must contain `{marker}`"
        );
        assert_eq!(
            snap.source,
            std::fs::read_to_string(release_dir().join(path)).expect("the file"),
            "the registry keeps the source verbatim"
        );
    }
    // Exactly four pack shaders ship (the two packs' Shaders/ folders).
    assert_eq!(assets.shader_count(), 4);
}
