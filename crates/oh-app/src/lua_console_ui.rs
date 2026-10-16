//! Issue 014b — the windowed egui Lua console overlay.
//!
//! The C++ `postUpdate_ImguiLuaConsole`'s UI tail (`HGUpdate.cpp:1287-1646`)
//! plus `drawImguiLuaConsole` (`HGGraphics.cpp:318-336`), on the egui stack
//! (DECISIONS.md D-022: `egui 0.37` + `egui-winit 0.37` + `egui_glow 0.37`,
//! verified against the D-011 stack). This module is the composition's
//! windowed-only half: it reads/writes the engine's `ilc*` state through
//! the landed 14a getters/setters (`oh-game` stays egui-free — headless
//! stays clean).
//!
//! # Per-frame order (the C++'s, exactly)
//!
//! * **Events** — C++ `runEvents`'s per-event `gameEngine->handleEvent`
//!   (GameWindow.hpp:42-83; the game's `onAnyEvent = Imgui::processEvent`,
//!   HexagonGame.cpp:335) → [`IlcConsole::on_event`] (fed through the
//!   `oh-render` event hook — AFTER the SFML input-state fold, the C++'s
//!   order; the game input is gated by the engine's
//!   `imgui_lua_console_has_input`, NOT by dropping events here — the C++
//!   updates its `inputState` unconditionally and lets the gate suppress).
//! * **Post-update** — after the engine's `post_update` (the landed 14a
//!   gate/toggle prefix), when it left the console SHOWN: the egui pass
//!   ([`IlcConsole::ui`] — the C++ `ImGui::SFML::Update` + the whole UI,
//!   HGUpdate.cpp:1314-1643) + the gate's `wantCapture*` conjunct
//!   ([`console_gate`] over the egui context's
//!   `egui_wants_keyboard_input() || egui_wants_pointer_input()`, written
//!   back via `HexagonGame::set_ilc_capture` for the NEXT frame's gate —
//!   the C++'s one-frame lag: `ImGui::SFML::Update` computes
//!   `WantCapture*` during this frame's post-update, so this frame's
//!   `update_input` gate read the previous frame's values).
//! * **Draw** — after the game's draw: [`IlcConsole::paint`] (the C++
//!   `drawImguiLuaConsole` → `Imgui::render`, HGGraphics.cpp:318-336).
//!
//! # Gates (the C++'s, verbatim)
//!
//! * `window == nullptr` (HGUpdate.cpp:1290-1293 / HGGraphics.cpp:319-322)
//!   — the port's `GlParts`-present check (no GL context = no console).
//! * `!Config::getDebug() || Config::getOfficial()` force-hide
//!   (HGUpdate.cpp:1295-1300) — the landed 14a engine `post_update` prefix;
//!   `ilc_show_console` is only ever `true` when the console may be shown.
//! * The overlay is created lazily on the FIRST shown frame (zero runtime
//!   cost when the console is never shown — the D-022 "behind the same
//!   gates" discipline).
//!
//! # The glow version split (D-022)
//!
//! `egui_glow 0.37` pulls `glow 0.17`; `oh-render` uses `glow 0.18`. They
//! are different Rust types over the SAME GL context, so the console builds
//! its OWN glow-0.17 context from `oh_render::GlParts::proc_address` (never
//! from `oh-render`'s context object) — the same loader pattern `oh-render`
//! itself uses at `GlBackend::create`. The GL context must be CURRENT on
//! this thread at construction (the windowed frame's — the C++'s
//! `renderWindow.setActive(true)`, GameWindow.hpp:125).
//!
//! # MANUAL check (no display in CI — D-011 skip-with-reason)
//!
//! `cargo run` (the windowed boot), set `debug: true` in the config (the
//! menu's options or `config.json`), start a level, press F1 (the default
//! `Tid::LuaConsole` trigger — 010b's `set_ilc_show_console_next`): the
//! `Lua Console` window appears (600×700 first use). Type `!help` + Enter
//! (the verbatim help block lands in the log, `# !help` in orange), a bad
//! Lua expr (`1 + y` + Enter → the `[error]:` line in red — the landed
//! interpreter's `u_log` fallback), Tab completion over a Lua prefix
//! (`u_l` + Tab → `u_log ` with the nice casing), Up/Down history, the
//! Track field + `Untrack All`, the `Timescale` drag + `Invincible`
//! checkbox (config write-back), and the `update ms` / `draw ms` footer.
//! With the console open, game input is ignored while the console has
//! focus; move the mouse off the console and the game's input comes back
//! (the `wantCapture*` conjunct, HexagonGame.cpp:272-273).

use std::cell::RefCell;
use std::sync::{Arc, RwLock};

use egui_winit::State as EguiWinitState;
use oh_config::Config;
use oh_game::hexagon_game::HexagonGame;
use oh_game::lua_console::IlcEventKey;
use oh_game::lua_state::SharedStore;
use oh_render::window::Window;
use oh_render::GlParts;

/// The egui console's persistent widget id (the C++ `InputText("Command")`
/// — one stable widget; the salt makes the `TextEdit`'s stored cursor
/// state persist across frames, D-022's `TextEditState::store` half).
const CMD_EDIT_ID_SALT: &str = "ilc_command";

/// C++ `HexagonGame::imguiLuaConsoleHasInput()` (`HexagonGame.cpp:270-274`):
/// `ilcShowConsole && (Imgui::wantCaptureKeyboard() ||
/// Imgui::wantCaptureMouse())` — the port's `wantCapture` half is the egui
/// context's `egui_wants_keyboard_input() || egui_wants_pointer_input()`
/// (D-022). The engine's [`HexagonGame::set_ilc_capture`] stores the
/// `(wantKeyboard || wantPointer)` term; the engine gate supplies the
/// `ilcShowConsole` term — this function is the C++ whole, unit-tested
/// headless with the fake focus values.
#[must_use]
pub fn console_gate(shown: bool, want_keyboard: bool, want_pointer: bool) -> bool {
    shown && (want_keyboard || want_pointer)
}

/// egui's text cursors count CHARS; the landed `ilc_complete` /
/// `ilc_history_key` cursors count BYTES (the C++ `data->CursorPos` byte
/// offsets into `std::string`). The char→byte conversion of the caret the
/// egui widget reports.
fn char_offset_to_byte(text: &str, char_off: usize) -> usize {
    text.char_indices()
        .nth(char_off)
        .map_or(text.len(), |(byte, _)| byte)
}

/// The byte→char inverse (the caret position to store back into the egui
/// widget state after the landed engine methods moved the buffer).
fn byte_offset_to_char(text: &str, byte_off: usize) -> usize {
    text.get(..byte_off.min(text.len()))
        .map_or(0, |prefix| prefix.chars().count())
}

/// C++ `postUpdate_ImguiLuaConsole`'s log-line colour lambda
/// (`HGUpdate.cpp:1335-1363`) — the per-tag colours, in the C++'s
/// precedence (the first match wins; `[error]`/`[warning]`/`[lua]` are
/// substring searches, `# ` is a prefix, `[?]` last).
fn log_line_color(line: &str) -> Option<egui::Color32> {
    if line.contains("[error]") {
        // C++ `ImVec4(1.0f, 0.4f, 0.4f, 1.0f)` — `[error]` (red).
        Some(egui::Color32::from_rgb(255, 102, 102))
    } else if line.contains("[warning]") {
        // C++ `ImVec4(1.0f, 0.4f, 1.0f, 1.0f)` — `[warning]` (magenta).
        Some(egui::Color32::from_rgb(255, 102, 255))
    } else if line.contains("[lua]") {
        // C++ `ImVec4(0.4f, 1.0f, 0.4f, 1.0f)` — `[lua]` (green).
        Some(egui::Color32::from_rgb(102, 255, 102))
    } else if line.starts_with("# ") {
        // C++ `strncmp(item, "# ", 2) == 0` (orange).
        Some(egui::Color32::from_rgb(255, 204, 153))
    } else if line.contains("[?]") {
        // C++ `ImVec4(0.4f, 0.4f, 1.0f, 1.0f)` — `[?]` (blue).
        Some(egui::Color32::from_rgb(102, 102, 255))
    } else {
        None
    }
}

/// C++ `postUpdate_ImguiLuaConsole`'s log-line split
/// (`HGUpdate.cpp:1370-1386`): each `ilcCmdLog` entry is split on `'\n'`
/// (the empty trailing piece after a final `'\n'` is DROPPED — the C++
/// `if(!lastPiece.empty())`).
fn log_lines(entry: &str) -> Vec<&str> {
    entry
        .split('\n')
        .filter(|piece| !piece.is_empty())
        .collect()
}

/// C++ `postUpdate_ImguiLuaConsole`'s `lineLimit` chunking
/// (`HGUpdate.cpp:1388-1406`): pieces over the 80-char `lineLimit` become
/// `ceil(len/80)` `s.substr(j * 80, 80)` substrings (the last clamped).
/// Byte chunks, char-boundary safe (the log is ASCII by construction —
/// the interpreter's lines — but a typed non-ASCII command char is
/// guarded: a chunk start backing off to the previous boundary).
fn byte_chunks80(piece: &str) -> Vec<&str> {
    if piece.len() <= 80 {
        return vec![piece];
    }
    let mut out = Vec::new();
    let mut start = 0;
    while start < piece.len() {
        let mut end = (start + 80).min(piece.len());
        if end < piece.len() && !piece.is_char_boundary(end) {
            end = (0..end)
                .rev()
                .find(|&i| piece.is_char_boundary(i))
                .unwrap_or(start);
        }
        out.push(&piece[start..end]);
        start = end;
    }
    out
}

/// C++ `postUpdate_ImguiLuaConsole`'s `ImVec2(600, 700)`
/// `ImGuiCond_FirstUseEver` default size (HGUpdate.cpp:1316) — the egui
/// `default_size` (applies until the user resizes; the egui memory then
/// persists the rect, the FirstUseEver analogue).
const WINDOW_DEFAULT_SIZE: [f32; 2] = [600.0, 700.0];

/// C++ `postUpdate_ImguiLuaConsole`'s footer reservation
/// (`HGUpdate.cpp:1322-1324`): `ItemSpacing.y + GetFrameHeightWithSpacing()
/// + 150` — the egui analogue (the style's item spacing + the interact
/// (frame) height + the C++'s `150`).
fn footer_reserve_formula(item_spacing_y: f32, interact_size_y: f32) -> f32 {
    item_spacing_y + interact_size_y + 150.0
}

fn footer_reserve(ui: &egui::Ui) -> f32 {
    footer_reserve_formula(ui.spacing().item_spacing.y, ui.spacing().interact_size.y)
}

/// C++ `static float simSpeed` / `static bool invincible`
/// (HGUpdate.cpp:1549, 1555-1557): the function-local statics — initialized ONCE
/// (the first frame that reaches the lines — the first SHOWN frame) and
/// OWNING the `Config` value for as long as the console is shown
/// (`Config::setTimescale(simSpeed)` / `setInvincible(invincible)` every
/// frame, :1551/:1557 — a menu change is overwritten while the console is
/// up; the C++ quirk, kept).
struct FooterStatics {
    sim_speed: f32,
    invincible: bool,
    /// The C++ statics initialize on the first frame that reaches their
    /// lines (the first shown frame) — the flag records that.
    initialized: bool,
}

impl FooterStatics {
    #[must_use]
    fn new() -> Self {
        Self {
            sim_speed: 1.0,
            invincible: false,
            initialized: false,
        }
    }
}

/// The last egui pass's output, held until [`IlcConsole::paint`] (the C++
/// splits `ImGui::SFML::Update` (post-update) from `ImGui::render` (draw)).
struct PendingPaint {
    shapes: Vec<egui::epaint::ClippedShape>,
    pixels_per_point: f32,
    textures_delta: egui::TexturesDelta,
}

/// One windowed client's console overlay (created lazily on the first
/// shown frame — see the module docs' Gates).
///
/// The `egui_winit::State` + `egui_glow::Painter` split mirrors the
/// `egui_glow::EguiGlow` integration (this 0.37's canonical glue) minus
/// the `ActiveEventLoop` construction requirement: `egui_winit::State`
/// takes a `&dyn HasDisplayHandle` (the GL backend's winit window —
/// `GlParts::winit_window`), so the overlay can be built on any frame with
/// the context current.
pub struct IlcConsole {
    ctx: egui::Context,
    winit_state: EguiWinitState,
    painter: egui_glow::Painter,
    footer: FooterStatics,
    /// The previous frame's `ilc_show_console` (drives the
    /// false→true "just shown" focus grant, the C++
    /// `ProcessEvent(GainedFocus)` equivalent — below).
    shown_last_frame: bool,
    pending: Option<PendingPaint>,
}

impl IlcConsole {
    /// Create the overlay over the shared GL context (D-022). `parts`'s
    /// context must be CURRENT on this thread (the windowed frame's).
    pub fn new(parts: &GlParts) -> Result<Self, String> {
        // The console's OWN glow-0.17 context (D-022 caveat): the same
        // display loader `oh-render` used for its glow-0.18 context
        // (`GlBackend::create`), over the same GL context — never the
        // `oh-render` context object (different Rust types).
        let gl17: Arc<egui_glow::glow::Context> = Arc::new(unsafe {
            egui_glow::glow::Context::from_loader_function_cstr(|name| {
                parts.proc_address(name) as *const std::ffi::c_void
            })
        });
        // C++ `Imgui::initialize(*window)` (the backend init the C++ does
        // at boot) — the egui analogue: the shader-compile /
        // max-texture-size probe (`egui_glow::Painter::new`, auto shader
        // version).
        let painter = egui_glow::Painter::new(gl17, "", None, false)
            .map_err(|e| format!("egui console GL painter init: {e:?}"))?;

        let ctx = egui::Context::default();
        let winit_window = parts.winit_window();
        // C++ `ImGui::SFML::Init(window)`'s display/size plumbing — the
        // egui-winit state (clipboard display handle + native scale factor
        // from the winit window).
        let winit_state = EguiWinitState::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            winit_window,
            Some(winit_window.scale_factor() as f32),
            winit_window.theme(),
            Some(painter.max_texture_side()),
        );

        Ok(Self {
            ctx,
            winit_state,
            painter,
            footer: FooterStatics::new(),
            shown_last_frame: false,
            pending: None,
        })
    }

    /// C++ `Imgui::processEvent` (the `onAnyEvent` feed,
    /// HexagonGame.cpp:335; the SFML `GainedFocus`/`LostFocus` halves feed
    /// the console's focus, the rest its input state). The returned
    /// `EventResponse` is the egui-winit's — the game input is NOT gated
    /// on it (the C++ updates its `inputState` unconditionally; the gate
    /// is the engine's `imgui_lua_console_has_input`).
    pub fn on_event(&mut self, window: &winit::window::Window, event: &winit::event::WindowEvent) {
        // The C++ `handleEvent` feed (HexagonGame.cpp:335) uses
        // `Imgui::processEvent`'s return for nothing — the game input gate
        // is the engine's `imgui_lua_console_has_input`, not this response.
        let _ = self.winit_state.on_window_event(window, event);
    }

    /// The previous frame's `ilc_show_console` (the caller's
    /// false→true "just shown" detector — the C++ toggle's
    /// `ProcessEvent(GainedFocus)`, HGUpdate.cpp:1305).
    #[must_use]
    pub fn shown_last_frame(&self) -> bool {
        self.shown_last_frame
    }

    /// The console was hidden this frame (the caller's bookkeeping so the
    /// next shown frame counts as "just shown" again).
    pub fn note_hidden(&mut self) {
        self.shown_last_frame = false;
    }

    /// C++ `postUpdate_ImguiLuaConsole`'s `ImGui::SFML::Update` + the whole
    /// console UI (HGUpdate.cpp:1314-1643) — one egui pass. Called AFTER
    /// the engine's `post_update` (the 14a gate/toggle prefix) left the
    /// console shown. The gate's `wantCapture*` conjunct is written back
    /// by the caller (`console_gate` over [`Self::capture_wants`]).
    pub fn ui<S: SharedStore>(
        &mut self,
        game: &HexagonGame<S>,
        window: &Window,
        winit_window: &winit::window::Window,
        config: &RwLock<Config>,
    ) {
        // C++ `ImGui::SFML::Update(*window, ilcDeltaClock.restart())`
        // (HGUpdate.cpp:1314): the frame's input-state computation; the
        // egui backend derives the frame delta from its own clock (the
        // C++ `ilcDeltaClock.restart()`'s purpose).
        let raw_input = self.winit_state.take_egui_input(winit_window);
        let IlcConsole {
            ctx,
            winit_state,
            painter: _,
            footer,
            shown_last_frame,
            pending,
        } = self;
        let just_shown = !*shown_last_frame;
        let output = ctx.run_ui(raw_input, |root| {
            draw_console_window(root, game, window, footer, just_shown, config);
        });
        *shown_last_frame = true;
        // The egui-winit platform output (cursor icon / clipboard — the
        // C++ `ImGui::SFML::Update`'s platform half). Viewport commands
        // are the root viewport only (a single-window console — no spawn
        // or resize commands are ever issued).
        winit_state.handle_platform_output(winit_window, output.platform_output);
        *pending = Some(PendingPaint {
            shapes: output.shapes,
            pixels_per_point: output.pixels_per_point,
            textures_delta: output.textures_delta,
        });
    }

    /// The egui context's `wantCaptureKeyboard` analogue (D-022):
    /// `egui_wants_keyboard_input()` — true while an egui widget (the
    /// console's command / track fields) holds keyboard focus.
    #[must_use]
    pub fn wants_keyboard(&self) -> bool {
        self.ctx.egui_wants_keyboard_input()
    }

    /// The egui context's `wantCaptureMouse` analogue (D-022):
    /// `egui_wants_pointer_input()` — true while the pointer is over egui
    /// content (the console's window rect).
    #[must_use]
    pub fn wants_pointer(&self) -> bool {
        self.ctx.egui_wants_pointer_input()
    }

    /// C++ `drawImguiLuaConsole` (`HGGraphics.cpp:318-336`): the console's
    /// GL paint — AFTER the frame's game draws (the C++'s `draw()` calls
    /// `drawImguiLuaConsole()` as its LAST step, HGGraphics.cpp:315). The
    /// C++'s `window->setView(renderWindow.getDefaultView())` (:334) is
    /// implicit: egui paints the full window rect in its own ortho
    /// (top-left origin) — the same the `ImGui::SFML::Render` uses
    /// (the default-view `ImGui::GetDrawData` viewport).
    pub fn paint(&mut self, winit_window: &winit::window::Window) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        // The `egui_glow` paint order (its `EguiGlow::paint`, winit.rs:106-128):
        // the texture updates BEFORE the primitives, the frees AFTER.
        let PendingPaint {
            shapes,
            pixels_per_point,
            mut textures_delta,
        } = pending;
        for (id, image_deltas) in textures_delta.set.drain() {
            for image_delta in image_deltas {
                self.painter.set_texture(id, &image_delta);
            }
        }
        let clipped = self.ctx.tessellate(shapes, pixels_per_point);
        let dimensions: [u32; 2] = winit_window.inner_size().into();
        self.painter
            .paint_primitives(dimensions, pixels_per_point, &clipped);
        for id in textures_delta.free.drain() {
            self.painter.free_texture(id);
        }
    }
}

// ---------------------------------------------------------------------------
// The per-frame wiring (the C++'s postUpdate / draw call sites)
// ---------------------------------------------------------------------------

/// C++ `postUpdate_ImguiLuaConsole`'s position for one frame
/// (HGUpdate.cpp:1287-1646 — the C++ runs it INSIDE
/// `HexagonGame::postUpdate`, HexagonGame.cpp:331, so it fires only while
/// the game state is current — the caller's state gate): the egui UI tail
/// (HGUpdate.cpp:1314-1643) + the NEXT frame's `wantCapture*` conjunct
/// (HexagonGame.cpp:270-274 — the C++'s one-frame lag, module docs).
///
/// The 14a engine `post_update` prefix (the `window == nullptr` /
/// debug-official force-hide / toggle prefix, HGUpdate.cpp:1287-1311) has
/// already run when the caller calls this — the engine's
/// `ilc_show_console` is the toggle's settled value. `ilc` is the
/// client's lazy-creation slot (`None` until the first shown frame).
pub fn frame_post_update<S: SharedStore>(
    window: &Window,
    game: &HexagonGame<S>,
    config: &RwLock<Config>,
    ilc: &RefCell<Option<IlcConsole>>,
) {
    if game.ilc_show_console() {
        // C++ `ImGui::SFML::Update` + the console UI
        // (HGUpdate.cpp:1314-1643) — the GL-gate half is the C++'s
        // `window == nullptr` early return (HGUpdate.cpp:1290-1293).
        if let Some(gl) = window.gl() {
            // Lazy creation on the first shown frame (module docs: Gates).
            let mut slot = ilc.borrow_mut();
            if slot.is_none() {
                *slot = match IlcConsole::new(gl) {
                    Ok(console) => Some(console),
                    Err(e) => {
                        eprintln!("[lua-console] egui console unavailable: {e}");
                        None
                    }
                };
            }
            let Some(console) = slot.as_mut() else {
                return;
            };
            let winit_window = gl.winit_window();
            console.ui(game, window, winit_window, config);
            // C++ `imguiLuaConsoleHasInput` (HexagonGame.cpp:270-274):
            // the engine's next-frame gate value, from the egui context's
            // `wantCapture*` analogue (the C++ computes `WantCapture*`
            // during this frame's `ImGui::SFML::Update`, so this frame's
            // `update_input` gate read the previous frame's values — the
            // one-frame lag, kept).
            game.set_ilc_capture(console_gate(
                game.ilc_show_console(),
                console.wants_keyboard(),
                console.wants_pointer(),
            ));
        }
    } else if let Some(console) = ilc.borrow_mut().as_mut() {
        // The toggle-off frame (the C++'s `if (!ilcShowConsole) return;`,
        // HGUpdate.cpp:1308-1311) — the "just shown" detector resets, so
        // the next shown frame grants the focus again (the C++'s
        // `ProcessEvent(GainedFocus)`, HGUpdate.cpp:1305).
        console.note_hidden();
    }
}

/// C++ `drawImguiLuaConsole` (`HGGraphics.cpp:318-336`) — the console's
/// GL paint, called AFTER the frame's game draw (the C++ `draw()`'s LAST
/// step, HGGraphics.cpp:315; the C++ runs it inside
/// `HexagonGame::draw`, HexagonGame.cpp:333, so the caller's state gate
/// is the game state). The C++'s `window == nullptr` /
/// `!ilcShowConsole` early returns (HGGraphics.cpp:319-323) are the
/// caller's `gl()` + state gates + this function's shown gate.
pub fn frame_draw<S: SharedStore>(
    window: &Window,
    game: &HexagonGame<S>,
    ilc: &RefCell<Option<IlcConsole>>,
) {
    if !game.ilc_show_console() {
        return;
    }
    let Some(gl) = window.gl() else {
        return;
    };
    if let Some(console) = ilc.borrow_mut().as_mut() {
        console.paint(gl.winit_window());
    }
}

// ---------------------------------------------------------------------------
// The console window's UI (the C++'s `ImGui::Begin("Lua Console")` … `End`)
// ---------------------------------------------------------------------------

/// C++ `postUpdate_ImguiLuaConsole`'s `ImGui::Begin("Lua Console")` …
/// `ImGui::End()` (HGUpdate.cpp:1316-1643) — the window + its contents.
fn draw_console_window<S: SharedStore>(
    root: &mut egui::Ui,
    game: &HexagonGame<S>,
    window: &Window,
    footer: &mut FooterStatics,
    just_shown: bool,
    config: &RwLock<Config>,
) {
    // C++ `ImGui::SetNextWindowSize(ImVec2(600, 700),
    // ImGuiCond_FirstUseEver)` + `ImGui::Begin("Lua Console")`
    // (HGUpdate.cpp:1316-1317).
    egui::Window::new("Lua Console")
        .default_size(WINDOW_DEFAULT_SIZE)
        .show(root, |ui| {
            console_contents(ui, game, window, footer, just_shown, config);
        });
}

/// The console window's contents, in the C++'s order.
fn console_contents<S: SharedStore>(
    ui: &mut egui::Ui,
    game: &HexagonGame<S>,
    window: &Window,
    footer: &mut FooterStatics,
    just_shown: bool,
    config: &RwLock<Config>,
) {
    // C++ `ImGui::Text("Enter `!help` to show help.");` +
    // `ImGui::Separator();` (HGUpdate.cpp:1319-1320).
    ui.label("Enter `!help` to show help.");
    ui.separator();

    // C++ `footer_height_to_reserve` (HGUpdate.cpp:1322-1324).
    let footer_reserve = footer_reserve(ui);

    // C++ `ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, ImVec2(4, 1));`
    // — "Tighten spacing" (HGUpdate.cpp:1326-1327) — the log region only.
    let prev_item_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing = egui::Vec2::new(4.0, 1.0);

    // C++ `ImGui::BeginChild("ScrollingRegion", ImVec2(0,
    // -footer_height_to_reserve), false,
    // ImGuiWindowFlags_HorizontalScrollbar)` (HGUpdate.cpp:1329-1331).
    // `stick_to_bottom` is the C++ `ImGui::SetScrollHereY(1.0f)`
    // (HGUpdate.cpp:1415) — the log auto-follows its tail.
    let log: Vec<String> = { game.state_handle().lock().shared().ilc_cmd_log.clone() };
    egui::ScrollArea::both()
        .id_salt("ilc_scrolling_region")
        .max_height(ui.available_height().max(0.0) - footer_reserve)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            // C++ log loop (HGUpdate.cpp:1332-1412): per-entry tag colour
            // (PushStyleColor/PopStyleColor around the entry's lines), the
            // `'\n'` split, the 80-char `lineLimit` chunking
            // (`ImGui::TextUnformatted` — no wrap; the egui analogue is a
            // per-line horizontal scope, which never wraps).
            for entry in &log {
                let color = log_line_color(entry);
                for line in log_lines(entry) {
                    for chunk in byte_chunks80(line) {
                        ui.horizontal(|ui| {
                            let mut rich = egui::RichText::new(chunk);
                            if let Some(c) = color {
                                rich = rich.color(c);
                            }
                            ui.label(rich);
                        });
                    }
                }
            }
        });

    // C++ `ImGui::PopStyleVar();` + `ImGui::EndChild();`
    // (HGUpdate.cpp:1416-1417).
    ui.spacing_mut().item_spacing = prev_item_spacing;

    // C++ `ImGui::Separator();` (HGUpdate.cpp:1418).
    ui.separator();

    // C++ `ImGui::InputText("Command", &ilcCmdBuffer,
    // EnterReturnsTrue | CallbackCompletion | CallbackHistory,
    // &ilcTextEditCallbackStub, ...)` (HGUpdate.cpp:1419-1549).
    command_input(ui, game, just_shown);

    // C++ `ImGui::Separator();` (HGUpdate.cpp:1551).
    ui.separator();

    // C++ `ImGui::Text("update ms: %.2f", window->getMsUpdate());
    // ImGui::SameLine(); ImGui::Text("draw ms: %.2f",
    // window->getMsDraw());` (HGUpdate.cpp:1545-1547) — the `%.2f` is the
    // C-locale two-decimal format.
    let update_ms = window.get_ms_update();
    let draw_ms = window.get_ms_draw();
    ui.horizontal(|ui| {
        ui.label(format!("update ms: {update_ms:.2}"));
        ui.label(format!("draw ms: {draw_ms:.2}"));
    });

    // C++ `static float simSpeed = Config::getTimescale();
    // ImGui::DragFloat("Timescale", &simSpeed, 0.005f);
    // Config::setTimescale(simSpeed);` (HGUpdate.cpp:1549-1551) +
    // `ImGui::SameLine();` + `static bool invincible =
    // Config::getInvincible(); ImGui::Checkbox("Invincible", &invincible);
    // Config::setInvincible(invincible);` (HGUpdate.cpp:1555-1557).
    // The `DragFloat`'s unbounded value + `0.005f` speed is the egui
    // `DragValue`'s (`Slider` would need a range the C++ has none).
    if !footer.initialized {
        // The C++ statics' first-reach initialization (once, on the first
        // shown frame).
        let c = config.read().expect("config poisoned");
        footer.sim_speed = c.get_timescale();
        footer.invincible = c.get_invincible();
        footer.initialized = true;
    }
    let mut sim_speed = footer.sim_speed;
    let mut invincible = footer.invincible;
    ui.horizontal(|ui| {
        // C++ `DragFloat("Timescale", ...)` — the label left of the drag.
        ui.label("Timescale");
        ui.add(egui::DragValue::new(&mut sim_speed).speed(0.005));
        // C++ `ImGui::SameLine();` + `Checkbox("Invincible", ...)`.
        ui.checkbox(&mut invincible, "Invincible");
    });
    footer.sim_speed = sim_speed;
    footer.invincible = invincible;
    // The C++ per-frame write-backs (`Config::setTimescale(simSpeed)` /
    // `setInvincible(invincible)`, HGUpdate.cpp:1560,1564).
    {
        let mut c = config.write().expect("config poisoned");
        c.set_timescale(sim_speed);
        c.set_invincible(invincible);
    }

    // C++ `ImGui::Separator();` (HGUpdate.cpp:1566).
    ui.separator();

    // C++ `ImGui::InputText("Track", &ilcTrackBuffer,
    // EnterReturnsTrue)` + `ImGui::SameLine();` +
    // `ImGui::Button("Untrack All")` (HGUpdate.cpp:1561-1581).
    track_input(ui, game);

    // C++ tracked-results loop + table (HGUpdate.cpp:1584-1641).
    tracked_results_table(ui, game);
}

/// C++ `ImGui::InputText("Command", ...)` (HGUpdate.cpp:1419-1549): the
/// buffer bound to `ilcCmdBuffer` (the widget's text is written into the
/// engine member each frame it edits — the C++'s direct member binding),
/// the `EnterReturnsTrue` command execution (the landed
/// `post_update_imgui_lua_console_command`), the `CallbackCompletion` Tab
/// completion (the landed `ilc_complete`), the `CallbackHistory` Up/Down
/// (the landed `ilc_history_key`), and the post-command
/// `SetItemDefaultFocus` / `SetKeyboardFocusHere(-1)` (HGUpdate.cpp:
/// 1539-1540 — keep the keyboard in the field).
///
/// The engine methods apply to `ilcCmdBuffer` immediately; the field's
/// REPAINT of the changed text lands on the next frame (the egui widget
/// paints once per pass) — one frame, the C++'s same-frame callback
/// repaint is indistinguishable at 60fps.
fn command_input<S: SharedStore>(ui: &mut egui::Ui, game: &HexagonGame<S>, just_shown: bool) {
    // The widget's persistent id (the `id_salt` — resolved through the
    // ui's id scope, exactly the `TextEdit`'s own `id_salt` path).
    let cmd_id = ui.make_persistent_id(CMD_EDIT_ID_SALT);

    // C++ toggle's `ImGui::SFML::ProcessEvent(GainedFocus)`
    // (HGUpdate.cpp:1305): the egui equivalent of "the console window just
    // gained focus — keyboard goes to it" is requesting focus on the
    // command field (egui manages per-widget focus; for a freshly opened
    // console the C++'s context focus lands on the last-focused widget,
    // which is the command input — the default focus of a new ImGui
    // window with a text input).
    //
    // The context borrow is scoped at every use (never held across the
    // `&mut Ui` calls below).
    if just_shown {
        ui.ctx().memory_mut(|m| m.request_focus(cmd_id));
    }

    // The field's focus from the PREVIOUS frame (the egui single-line
    // `return_key` — Enter — surrenders the focus DURING this frame's
    // `show`, so the key decisions below use the pre-show focus, exactly
    // like the C++'s `InputText` returning `true` while focused).
    let had_focus = ui.ctx().memory(|m| m.has_focus(cmd_id));

    // C++ `&ilcCmdBuffer` binding (HGUpdate.cpp:1425-1426): the widget
    // edits a local copy each frame; the edits are written back to the
    // engine member (the C++'s direct binding) — and the engine methods
    // below read/mutate the member, so the local always starts from it.
    let mut cmd_text = game.ilc_cmd_buffer();

    // The event filter keeps Tab / Up / Down / Escape for the WIDGET
    // while it has focus (the C++ `CallbackCompletion` /
    // `CallbackHistory` semantics — they fire on the input's own keys,
    // and the C++ InputText does not blur on them; the C++'s sticky
    // `WantCapture*` also keeps Escape OUT of the game's `goToMenu` bind
    // while the console is up — `escape: true` is the egui half of that
    // sticky capture: without it, egui's default Escape handling would
    // surrender the field's focus and open the gate to the game).
    let output = egui::TextEdit::singleline(&mut cmd_text)
        .id_salt(CMD_EDIT_ID_SALT)
        .event_filter(egui::EventFilter {
            tab: true,
            vertical_arrows: true,
            escape: true,
            ..Default::default()
        })
        .desired_width(f32::INFINITY)
        .show(ui);
    let response = &output.response.response;

    // The C++ member binding: every edit lands in `ilcCmdBuffer`.
    if response.changed() {
        game.ilc_set_cmd_buffer(&cmd_text);
    }

    // The caret (D-022: `TextEditOutput.cursor_range`): the egui
    // character offset → the landed methods' byte offset.
    let caret_byte = output
        .cursor_range
        .and_then(|r| r.single())
        .map(|c| char_offset_to_byte(&cmd_text, c.index.0))
        .unwrap_or(cmd_text.len());

    // C++ `CallbackCompletion` (the `ImGuiInputTextFlags_CallbackCompletion`
    // branch, HGUpdate.cpp:1154-1244, via the landed `ilc_complete`).
    if had_focus && key_pressed_in(ui, egui::Key::Tab) {
        game.ilc_set_cmd_buffer(&cmd_text); // the pre-Tab text first (binding)
        let new_caret = game.ilc_complete(caret_byte);
        store_caret(ui, &output, game, new_caret);
    }

    // C++ `CallbackHistory` (the `ImGuiInputTextFlags_CallbackHistory`
    // branch, HGUpdate.cpp:1246-1282, via the landed `ilc_history_key`).
    if had_focus && key_pressed_in(ui, egui::Key::ArrowUp) {
        let new_caret = game.ilc_history_key(IlcEventKey::UpArrow);
        store_caret(ui, &output, game, new_caret);
    } else if had_focus && key_pressed_in(ui, egui::Key::ArrowDown) {
        let new_caret = game.ilc_history_key(IlcEventKey::DownArrow);
        store_caret(ui, &output, game, new_caret);
    }

    // C++ `ImGuiInputTextFlags_EnterReturnsTrue` (the Enter branch,
    // HGUpdate.cpp:1427-1541, via the landed
    // `post_update_imgui_lua_console_command` — it reads the member's
    // content as `cmdString`, clears `ilcCmdBuffer`, and interprets).
    if had_focus && key_pressed_in(ui, egui::Key::Enter) {
        game.post_update_imgui_lua_console_command(&game.ilc_cmd_buffer());
        // C++ `ImGui::SetItemDefaultFocus();` +
        // `ImGui::SetKeyboardFocusHere(-1);` (HGUpdate.cpp:1539-1540):
        // after running the command the keyboard stays in the field (the
        // egui single-line Enter surrendered the focus — re-request it).
        ui.ctx().memory_mut(|m| m.request_focus(response.id));
    }
}

/// The key-pressed probe (a free function — a closure capturing `ui`
/// would hold the `&mut Ui`'s borrow across the `store_caret` calls).
fn key_pressed_in(ui: &egui::Ui, key: egui::Key) -> bool {
    ui.input(|i| i.key_pressed(key))
}

/// Write the post-`ilc_complete` / `ilc_history_key` caret back into the
/// widget's stored state (D-022: `TextEditState::store`): the engine
/// methods moved `ilcCmdBuffer`; the next frame's `show` must open with
/// the caret where the C++ `DeleteChars`/`InsertChars` left it.
fn store_caret<S: SharedStore>(
    ui: &mut egui::Ui,
    output: &egui::widgets::text_edit::TextEditOutput,
    game: &HexagonGame<S>,
    byte_caret: usize,
) {
    let text = game.ilc_cmd_buffer();
    let char_caret = byte_offset_to_char(&text, byte_caret);
    let mut state = output.state.clone();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(char_caret),
        )));
    state.store(ui.ctx(), output.response.response.id);
}

/// C++ `ImGui::InputText("Track", &ilcTrackBuffer, EnterReturnsTrue)` +
/// `ImGui::SameLine();` + `ImGui::Button("Untrack All")`
/// (HGUpdate.cpp:1561-1581): the track buffer bound to `ilcTrackBuffer`
/// (the landed `ilc_set_track_buffer` seam — the engine method reads the
/// member and clears it), the Enter branch (the landed `ilc_track`:
/// LR-trim + record + clear), and the `Untrack All` button (the landed
/// `ilc_untrack_all`).
fn track_input<S: SharedStore>(ui: &mut egui::Ui, game: &HexagonGame<S>) {
    let track_id = ui.make_persistent_id("ilc_track");
    let had_focus = ui.ctx().memory(|m| m.has_focus(track_id));

    // The C++ member binding: the widget shows `ilcTrackBuffer`'s content
    // (the engine clears it after a record, so a fresh frame starts from
    // the member — the C++'s `InputText("Track", &ilcTrackBuffer)`).
    let mut track_text = game.ilc_track_buffer();
    let output = egui::TextEdit::singleline(&mut track_text)
        .id_salt("ilc_track")
        // The C++'s sticky-capture half (the track field has no
        // completion / history callbacks — Tab / Up / Down / Escape just
        // stay with the widget, the command field's filter minus its
        // callbacks).
        .event_filter(egui::EventFilter {
            tab: true,
            vertical_arrows: true,
            escape: true,
            ..Default::default()
        })
        .show(ui);
    if output.response.response.changed() {
        // The C++ member binding (each edit lands in `ilcTrackBuffer`).
        game.ilc_set_track_buffer(&track_text);
    }

    // C++ Enter branch (HGUpdate.cpp:1564-1573): read the member, LR-trim,
    // record the snippet + name, clear the member — the landed
    // `ilc_track` (the `ilc_set_track_buffer` seam above synced the
    // widget text into the member).
    if had_focus && key_pressed_in(ui, egui::Key::Enter) {
        game.ilc_track();
        track_text.clear(); // the C++ `ilcTrackBuffer.clear()`
        ui.ctx()
            .memory_mut(|m| m.request_focus(output.response.response.id));
    }

    // C++ `ImGui::SameLine();` + `ImGui::Button("Untrack All")`
    // (HGUpdate.cpp:1577-1581) → the landed `ilc_untrack_all`.
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("Untrack All").clicked() {
            game.ilc_untrack_all();
        }
    });
}

/// C++ tracked-results loop + `TrackedResults` table
/// (HGUpdate.cpp:1584-1641): the per-frame execute-and-erase-on-error loop
/// (the landed `ilc_update_tracked_results` — it clears the results,
/// executes each snippet, logs + erases the FIRST failing pair), then —
/// only when nothing failed this frame (no error ⇒ the loop finished and
/// the results align 1:1 with the tracked pairs, the C++'s
/// `SSVOH_ASSERT` pair, :1623-1624) — the separator + the two-column
/// `name | result` table.
fn tracked_results_table<S: SharedStore>(ui: &mut egui::Ui, game: &HexagonGame<S>) {
    if game.ilc_lua_tracked().is_empty() {
        // C++ `if(ilcLuaTracked.size() > 0)` (HGUpdate.cpp:1584) — the
        // loop (and the table) run only with tracked snippets.
        return;
    }

    // The C++'s per-frame loop (HGUpdate.cpp:1589-1617) — the landed
    // engine method (it runs the execute-and-erase logic and the error
    // logging; the UI only observes the aftermath).
    game.ilc_update_tracked_results();

    let tracked_after = game.ilc_lua_tracked();
    let results = game.ilc_lua_tracked_results();
    // The C++ `if(!problem)` (:1620): no error this frame ⇒ the loop
    // finished ⇒ the results align with the tracked pairs (the engine's
    // debug-asserted invariant). An error erased a pair AND left the
    // results short — the table is skipped exactly as the C++'s.
    if tracked_after.len() == results.len() && !tracked_after.is_empty() {
        // C++ `ImGui::Separator();` (HGUpdate.cpp:1621).
        ui.separator();
        // C++ `ImGui::BeginTable("TrackedResults", 2)` + the
        // `TableNextRow` / `TableSetColumnIndex` name/result pairs
        // (HGUpdate.cpp:1626-1640).
        egui::Grid::new("ilc_tracked_results")
            .num_columns(2)
            .show(ui, |ui| {
                for (name, result) in tracked_after.iter().zip(&results) {
                    ui.label(egui::RichText::new(name));
                    ui.label(egui::RichText::new(result));
                    ui.end_row();
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- the gate's conjunct (the C++ `imguiLuaConsoleHasInput`, tested
    //    with the FAKE egui focus values — the real ones come from the
    //    egui context in the windowed frame) -----------------------------

    #[test]
    fn gate_is_shown_and_either_capture() {
        // The C++ `ilcShowConsole && (wantCaptureKeyboard ||
        // wantCaptureMouse)` (HexagonGame.cpp:272-273).
        assert!(console_gate(true, true, false)); // shown + keyboard focus
        assert!(console_gate(true, false, true)); // shown + pointer over it
        assert!(console_gate(true, true, true));
        assert!(!console_gate(true, false, false)); // shown, no focus: the game's input passes
        assert!(!console_gate(false, true, true)); // hidden: the first term
    }

    // -- the char/byte caret conversions ----------------------------------

    #[test]
    fn caret_conversions_round_trip_ascii() {
        let s = "u_log('x')";
        for char_off in 0..=s.chars().count() {
            let byte = char_offset_to_byte(s, char_off);
            assert_eq!(byte_offset_to_char(s, byte), char_off);
        }
    }

    #[test]
    fn caret_conversion_is_char_safe_on_multibyte() {
        // The log / buffer are ASCII by construction, but the conversion
        // must be char-safe regardless (an accented typed char).
        let s = "héllo"; // 'é' is two bytes.
        assert_eq!(char_offset_to_byte(s, 2), 3); // after 'él'
        assert_eq!(char_offset_to_byte(s, 5), 6); // the end
        assert_eq!(byte_offset_to_char(s, 3), 2);
    }

    // -- the log-line colours (the C++'s per-tag lambda, HGUpdate.cpp:
    //      1335-1363) ------------------------------------------------------

    #[test]
    fn log_colours_follow_the_cpp_precedence() {
        assert_eq!(
            log_line_color("[error]: boom\n").unwrap(),
            egui::Color32::from_rgb(255, 102, 102)
        );
        assert_eq!(
            log_line_color("[warning]: careful\n").unwrap(),
            egui::Color32::from_rgb(255, 102, 255)
        );
        assert_eq!(
            log_line_color("[lua] ran\n").unwrap(),
            egui::Color32::from_rgb(102, 255, 102)
        );
        assert_eq!(
            log_line_color("# !help\n").unwrap(),
            egui::Color32::from_rgb(255, 204, 153)
        );
        assert_eq!(
            log_line_color("[?]: docs").unwrap(),
            egui::Color32::from_rgb(102, 102, 255)
        );
        assert!(log_line_color("plain line\n").is_none());
        // The C++ precedence: `[error]` wins over `[warning]` when both
        // occur (the first `strstr` hit).
        assert_eq!(
            log_line_color("[error]: [warning] nested").unwrap(),
            egui::Color32::from_rgb(255, 102, 102)
        );
        // The `# ` test is a PREFIX (the C++ `strncmp(item, "# ", 2)`) —
        // a mid-line `# ` is not orange.
        assert!(log_line_color("x # y").is_none());
    }

    // -- the log-entry split + 80-char chunking (the C++'s, HGUpdate.cpp:
    //      1362-1395) -------------------------------------------------------

    #[test]
    fn log_split_drops_the_empty_trailing_piece() {
        // C++ `if(!lastPiece.empty())` (HGUpdate.cpp:1383-1386): a final
        // `'\n'` yields no empty last piece.
        assert_eq!(log_lines("a\nb\n"), vec!["a", "b"]);
        assert_eq!(log_lines("a\nb"), vec!["a", "b"]);
        assert_eq!(log_lines("solo"), vec!["solo"]);
    }

    #[test]
    fn log_chunking_matches_the_cpp_substr_rule() {
        // `s.substr(j * 80, 80)` (HGUpdate.cpp:1398-1404) — the last
        // chunk clamps to the end.
        let s = "a".repeat(200);
        assert_eq!(
            byte_chunks80(&s),
            vec!["a".repeat(80), "a".repeat(80), "a".repeat(40)]
        );
        let short = "x";
        assert_eq!(byte_chunks80(short), vec![short]);
        // Char-boundary safety: 79 'a's + a 2-byte 'é' = 81 bytes; the
        // 80-byte chunk start would split the 'é' → back off to 79.
        let mb = format!("{}é", "a".repeat(79));
        let chunks = byte_chunks80(&mb);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 79);
    }

    // -- the footer reservation (the C++'s, HGUpdate.cpp:1322-1324) --------

    #[test]
    fn footer_reserve_formula_is_the_cpp_sum() {
        // `ItemSpacing.y + GetFrameHeightWithSpacing() + 150` — the egui
        // analogue, style-independent (the live frame reads the real
        // style values through `footer_reserve`).
        assert_eq!(footer_reserve_formula(1.0, 20.0), 171.0);
        assert_eq!(footer_reserve_formula(0.0, 0.0), 150.0);
    }
}
