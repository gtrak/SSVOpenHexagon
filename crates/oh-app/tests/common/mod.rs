//! Test-side composition for the draw-bridge tests (issue 15 step (a)).
//!
//! The composition itself (`LuaScriptHost` / `RealAssetsApi` / `Composed` /
//! the two-phase build) lives in `oh_app::compose` — promoted from this
//! file by issue 15 step (b) so the `open-hexagon` binary and the step (a)
//! tests share ONE composition (the spec's dedup; the two-phase build and
//! the `SharedLua` drop-order contract are documented there). What remains
//! test-local: the C++ test's `testProfile` profile seam, the `_RELEASE`
//! root idiom, the C++ `initText` font pair, and the recording
//! `DrawTarget` for the stage-sequence assertions.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oh_game::seams::Profiles;

pub use oh_app::compose::{compose_with, Composed};

/// The C++ `ReplayExecution.t.cpp` profile:
/// `ProfileData{GAME_VERSION, "testProfile"}` + `addLocalProfile` +
/// `pSetCurrent("testProfile")`. oh-assets ports profile *loading* only —
/// the mutation API (`addLocalProfile` / `pSetCurrent`) is deferred (see
/// `oh_assets` module docs), so the engine's `Profiles` seam gets its own
/// implementation reporting the C++ test's current profile ("testProfile"
/// is what `death_createReplayFile` records as `player_name`).
///
/// Test-local: the step-(b) binary uses `oh_game::seams::NullProfiles`
/// instead (the real `HGAssets` profile surface lands with issue 17).
/// `allow(dead_code)`: each test binary compiles this module separately
/// and not all of them drive a live replay (which is what reads the
/// profile name).
#[allow(dead_code)]
pub struct TestProfiles;

impl Profiles for TestProfiles {
    fn any_local_profile_active(&self) -> bool {
        true
    }

    fn current_local_profile_name(&self) -> String {
        "testProfile".to_string()
    }

    fn get_local_score(&self, _level_id: &str) -> f32 {
        0.0
    }

    fn set_local_score(&self, _level_id: &str, _score: f32) {}
}

/// The step-(a) test composition: the shared `oh_app::compose` build with
/// the C++ test's `testProfile` seam. `allow(dead_code)`: not every test
/// binary composes an engine (each compiles this module separately).
#[allow(dead_code)]
#[must_use]
pub fn compose(assets: Arc<oh_assets::Assets>, root: PathBuf, window: bool) -> Composed {
    compose_with(
        assets,
        root,
        window,
        Arc::new(TestProfiles),
        Arc::new(oh_game::seams::NoneSteamSink),
    )
}

/// The `_RELEASE` root (the process-CWD equivalent; the same idiom as
/// `oh-assets/tests/release_tree.rs`).
#[must_use]
pub fn release_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../_RELEASE")
        .to_path_buf()
}

/// The C++ `initText` fonts (`HexagonGame.cpp:289-290`: the assets
/// registry's `OpenSquare-Regular.ttf` / `OpenSquare-Bold.ttf`).
/// `allow(dead_code)`: only the bridge test binary loads fonts.
#[allow(dead_code)]
#[must_use]
pub fn test_fonts() -> (oh_render::font::Font, oh_render::font::Font) {
    let root = release_dir().join("Assets");
    (
        oh_render::font::Font::load(&root.join("OpenSquare-Regular.ttf"))
            .expect("OpenSquare-Regular.ttf"),
        oh_render::font::Font::load(&root.join("OpenSquare-Bold.ttf"))
            .expect("OpenSquare-Bold.ttf"),
    )
}

// ---------------------------------------------------------------------
// A recording DrawTarget for the stage-sequence assertions — the same
// event model as `oh-render`'s private `Recording` target
// (`graphics.rs:1561`, not importable — private to that crate's test
// module).
// ---------------------------------------------------------------------

use oh_render::draw::RenderStates;
use oh_render::graphics::DrawTarget;
use oh_render::view::View;

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // only the bridge test binary records events
pub enum DrawEvent {
    Clear,
    SetView(View),
    Vertices { count: usize, states: RenderStates },
    Sprite,
    Text { states: RenderStates },
    Rectangle { states: RenderStates },
    Screenshot(String),
}

#[derive(Default)]
#[allow(dead_code)] // only the bridge test binary records events
pub struct Recording {
    pub events: Vec<DrawEvent>,
}

impl DrawTarget for Recording {
    fn clear_black(&mut self) {
        self.events.push(DrawEvent::Clear);
    }
    fn set_view(&mut self, view: View) {
        self.events.push(DrawEvent::SetView(view));
    }
    fn draw_vertices(&mut self, vertices: &[oh_render::draw::Vertex], states: RenderStates) {
        self.events.push(DrawEvent::Vertices {
            count: vertices.len(),
            states,
        });
    }
    fn draw_sprite(&mut self, _sprite: &oh_render::texture::Sprite) {
        self.events.push(DrawEvent::Sprite);
    }
    fn draw_text(&mut self, _text: &oh_render::font::Text, states: RenderStates) {
        self.events.push(DrawEvent::Text { states });
    }
    fn draw_rectangle(&mut self, _rect: &oh_render::types::RectangleShape, states: RenderStates) {
        self.events.push(DrawEvent::Rectangle { states });
    }
    fn save_screenshot(&mut self, path: &str) {
        self.events.push(DrawEvent::Screenshot(path.to_string()));
    }
}
