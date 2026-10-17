//! `oh-render` — the SFML analogue: the platform backend the rest of the game
//! draws and reads input through.
//!
//! One Rust module per SFML/SSVStart subsystem (plan 001, ground rule 1;
//! D-000/D-002). Backend decision: **real-GL stack** (`winit` + `glutin` +
//! `glow` + `rodio` + `fontdue`), NOT `rsfml` — see DECISIONS.md D-004 (SFML
//! entry). Pack content ships GLSL fragment shaders that must execute on a
//! real GL context, so a CPU rasterizer was rejected.
//!
//! Modules:
//! - `input` — `ssvs::Input::InputState` (per-key/button pressed bitsets over
//!   the exact SFML `Keyboard::Key` / `Mouse::Button` integers).
//! - `input_manager` — the SSVStart input runtime over `InputState` + the
//!   `oh-config` `Trigger`/`Combo` model (issue 010b): `Type` / `Mode`,
//!   `combo_is_down` (`Combo.inl`), `Bind` (the `released` latch, the
//!   on/off callbacks, the priority) and `Manager` (per-frame refresh +
//!   per-tick update, the `processedInput`, `refreshTriggers`, the ignore
//!   flags).
//! - `draw` — `sf::Vertex` / `sf::PrimitiveType` / `sf::RenderStates` + the
//!   `hg::Utils::FastVertexVector` batch the game's draw code feeds.
//! - `view` — `sf::View` + `ssvs::Camera` (world↔screen, pan/zoom/turn/skew).
//! - `camera` — the game's camera management (initial views, `refreshPulse`,
//!   `updateCameraShake` + pre-shake centres, draw-time centre setting).
//! - `graphics` — the drawing layer (issue 11): `RenderStage` + per-stage
//!   shader binding, the `draw()` frame pipeline (`draw_frame`), the
//!   `draw*` methods, `StyleData::drawBackground*`, flash polygon, key
//!   icons, level info, particles.
//! - `text_draw` — the `updateText` content build + `drawText_*` layout.
//! - `batching` — `FastVertexVectorTris` → GPU buffer upload (live GL only).
//! - `timer` — `ssvs::TimerStatic` (issue 15c): the fixed-timestep
//!   accumulation timer the windowed loop ticks the game states with.
//! - `texture` / `font` / `shader` / `audio` / `window` — added in later
//!   steps (see the issue-10 file map).
//!
//! The crate builds and tests **headless** (no display / GPU required): the
//! pure-logic surface (input, vertex order, text metrics, enum tables, camera
//! math) is asserted without a GL context; the GPU path is covered by a single
//! smoke test that skips with a printed reason when no context is available.

pub mod audio;
pub mod batching;
pub mod camera;
pub mod draw;
pub mod error;
pub mod font;
pub mod gl;
pub mod gl_passes;
pub mod glyphs;
pub mod graphics;
pub mod input;
pub mod input_manager;
pub mod pack_shaders;
pub mod shader;
pub mod text_draw;
pub mod texture;
pub mod timer;
pub mod types;
pub mod view;
pub mod window;

// Compatibility-surface re-exports (exact SFML enum integers via `oh-config`,
// and the `sf::Vector2f` / `sf::Color` analogues via `oh-math`).
pub use audio::{AudioDevice, Music, Sound, SoundBuffer};
pub use batching::GlBatchUploader;
pub use camera::{
    death_shake_camera, init_cameras, refresh_pulse, update_camera_shake, PreShakeCenters,
};
pub use draw::{BlendMode, DrawCall, PrimitiveType, RenderStates, Vertex, VertexBatch};
pub use error::RenderError;
pub use font::{Font, Text};
pub use gl::GlParts;
pub use gl_passes::{rectangle_vertices, GlPasses};
pub use glyphs::{
    dilate_alpha, sprite_quad, text_quads, uv_rect, CellRect, FillEntry, GlyphAtlas, GlyphStore,
    SpriteQuad, TextQuad,
};
pub use graphics::{
    death_flash_effect, draw_background, draw_background_menu, draw_frame, draw_key_icons,
    draw_level_info, draw_particles, draw_swap_particles, draw_trail_particles, init_flash_effect,
    init_key_icons, init_text, render_states_for_stage, update_flash, update_key_icons,
    update_level_info, update_particles, update_swap_particles, update_trail_particles, DrawState,
    DrawTarget, FragmentShaderIds, FrameInput, PBParticleUpdateParams, Particle, RenderStage,
    SwapParticle, SwapParticleSpawnInfo, SwapParticleUpdateParams, TrailParticle,
    TrailParticleUpdateParams,
};
pub use input::InputState;
pub use input_manager::{combo_is_down, Bind, Manager, Mode, Type};
pub use oh_config::{Button, Key};
pub use oh_math::hg::color::Color;
pub use oh_math::ssvs::vector2::Vec2;
pub use pack_shaders::{
    PackShaderCache, PackShaderProgram, PackShaderProvider, PackShaderSnapshot,
};
pub use shader::{Shader, ShaderRegistry, ShaderType, Uniform};
pub use text_draw::{
    diff_format, draw_text, draw_text_message, draw_text_personal_best, draw_text_time_and_status,
    format_time, get_scaled_character_size, time_format, update_text, update_text_sizes,
    TextUpdateInput,
};
pub use texture::{Image, RenderTexture, Sprite, Texture};
pub use timer::TimerStatic;
pub use types::{FloatRect, RectangleShape, VideoMode};
pub use view::{Camera, View};
pub use window::{
    map_window_event, winit_button_to_sfml, winit_key_to_sfml, Event, JoystickAxis, Window,
};
