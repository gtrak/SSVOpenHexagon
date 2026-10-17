//! Gameplay data types for the SSVOpenHexagon Rust port.
//!
//! One Rust module per C++ file, same names (snake_case), same
//! responsibilities (plan 001, ground rule 1; D-000/D-002). Every C++ `float`
//! is `f32`, every `double` is `f64`, and no floating-point expression is
//! reordered (this crate is where replay parity is won or lost).
//!
//! Modules (mapping `gameplay.md` sections):
//! - [`speed_data`] — §1 `SpeedData`
//! - [`wall`] — §2 `CWall` (+ the shared `Wall` trait)
//! - [`player`] — §3 `CPlayer`
//! - [`custom_wall`] / [`custom_wall_manager`] — §4 `CCustomWall` / manager
//! - [`status`] — §5 `HexagonGameStatus`
//! - [`level_status`] — §6 `LevelStatus`
//! - [`ticker`] — §8 `Ticker`
//! - [`timeline`] — §9 `timeline2` + runner
//! - [`custom_timeline`] — §10 `CustomTimeline(Manager)`
//! - [`vertex_batch`] — §11 `FastVertexVectorTris`
//! - [`polygon`] — §12 `pointInPolygon` / `pointInFourVertexPolygon`
//! - [`clock`] — `HRTimePoint` alias (game-time marker)

pub mod camera;
pub mod clock;
pub mod custom_timeline;
pub mod custom_wall;
pub mod custom_wall_manager;
pub mod hexagon_game;
pub mod level_status;
pub mod log_sink;
pub mod lua_console;
pub mod lua_state;
pub mod player;
pub mod polygon;
pub mod renderer;
pub mod seams;
pub mod speed_data;
pub mod status;
pub mod ticker;
pub mod timeline;
pub mod update;
pub mod vertex_batch;
pub mod wall;

pub use clock::HRTimePoint;
pub use custom_timeline::{CustomTimeline, CustomTimelineHandle, CustomTimelineManager};
pub use custom_wall::CCustomWall;
pub use custom_wall_manager::CCustomWallManager;
pub use level_status::LevelStatus;
pub use lua_state::{GameApi, ReentrantMutex, SharedState, SharedStore};
pub use player::CPlayer;
pub use speed_data::SpeedData;
pub use status::{HexagonGameStatus, StateChange};
pub use ticker::Ticker;
pub use timeline::{Timeline2, Timeline2Runner};
pub use vertex_batch::FastVertexVectorTris;
pub use wall::CWall;
