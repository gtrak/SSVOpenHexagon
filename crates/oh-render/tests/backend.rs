//! Headless contract tests for `oh-render` (the SFML-analogue backend).
//!
//! Everything here runs without a display: the SFML enum table
//! (verification item 3), the winit→SFML key mapping, the quad vertex order,
//! text layout (item 4), input-state bit semantics, camera math, render
//! states, and the window's headless view/framerate surface. `gpu_smoke`
//! (verification item 2) attempts a real GL context and, when no display/GPU
//! is available, prints a reason and skips — the gate must stay green in a
//! headless CI.

use oh_render::draw::{BlendMode, PrimitiveType, RenderStates, VertexBatch};
use oh_render::font::Font;
use oh_render::input::InputState;
use oh_render::view::{Camera, View};
use oh_render::window::{winit_button_to_sfml, winit_key_to_sfml, Window};
use oh_render::{Button, Key, Vec2};
use std::path::Path;
use winit::event::MouseButton;
use winit::keyboard::{Key as WKey, KeyCode, KeyLocation, NamedKey, NativeKeyCode, PhysicalKey};

// ---------------------------------------------------------------------------
// Verification item 3 — the exact SFML integer values (oracle-vectors.md:
// "SFML key/button enum values (from the pinned fork)").
// ---------------------------------------------------------------------------

#[test]
fn enum_table_matches_oracle() {
    // `enum Key` (KeyCount = 101), sequential from Unknown = -1.
    let keys: [(Key, i32); 102] = [
        (Key::UNKNOWN, -1),
        (Key::A, 0),
        (Key::B, 1),
        (Key::C, 2),
        (Key::D, 3),
        (Key::E, 4),
        (Key::F, 5),
        (Key::G, 6),
        (Key::H, 7),
        (Key::I, 8),
        (Key::J, 9),
        (Key::K, 10),
        (Key::L, 11),
        (Key::M, 12),
        (Key::N, 13),
        (Key::O, 14),
        (Key::P, 15),
        (Key::Q, 16),
        (Key::R, 17),
        (Key::S, 18),
        (Key::T, 19),
        (Key::U, 20),
        (Key::V, 21),
        (Key::W, 22),
        (Key::X, 23),
        (Key::Y, 24),
        (Key::Z, 25),
        (Key::NUM0, 26),
        (Key::NUM1, 27),
        (Key::NUM2, 28),
        (Key::NUM3, 29),
        (Key::NUM4, 30),
        (Key::NUM5, 31),
        (Key::NUM6, 32),
        (Key::NUM7, 33),
        (Key::NUM8, 34),
        (Key::NUM9, 35),
        (Key::ESCAPE, 36),
        (Key::L_CONTROL, 37),
        (Key::L_SHIFT, 38),
        (Key::L_ALT, 39),
        (Key::L_SYSTEM, 40),
        (Key::R_CONTROL, 41),
        (Key::R_SHIFT, 42),
        (Key::R_ALT, 43),
        (Key::R_SYSTEM, 44),
        (Key::MENU, 45),
        (Key::L_BRACKET, 46),
        (Key::R_BRACKET, 47),
        (Key::SEMICOLON, 48),
        (Key::COMMA, 49),
        (Key::PERIOD, 50),
        (Key::APOSTROPHE, 51),
        (Key::SLASH, 52),
        (Key::BACKSLASH, 53),
        (Key::GRAVE, 54),
        (Key::EQUAL, 55),
        (Key::HYPHEN, 56),
        (Key::SPACE, 57),
        (Key::ENTER, 58),
        (Key::BACKSPACE, 59),
        (Key::TAB, 60),
        (Key::PAGE_UP, 61),
        (Key::PAGE_DOWN, 62),
        (Key::END, 63),
        (Key::HOME, 64),
        (Key::INSERT, 65),
        (Key::DELETE, 66),
        (Key::ADD, 67),
        (Key::SUBTRACT, 68),
        (Key::MULTIPLY, 69),
        (Key::DIVIDE, 70),
        (Key::LEFT, 71),
        (Key::RIGHT, 72),
        (Key::UP, 73),
        (Key::DOWN, 74),
        (Key::NUMPAD0, 75),
        (Key::NUMPAD1, 76),
        (Key::NUMPAD2, 77),
        (Key::NUMPAD3, 78),
        (Key::NUMPAD4, 79),
        (Key::NUMPAD5, 80),
        (Key::NUMPAD6, 81),
        (Key::NUMPAD7, 82),
        (Key::NUMPAD8, 83),
        (Key::NUMPAD9, 84),
        (Key::F1, 85),
        (Key::F2, 86),
        (Key::F3, 87),
        (Key::F4, 88),
        (Key::F5, 89),
        (Key::F6, 90),
        (Key::F7, 91),
        (Key::F8, 92),
        (Key::F9, 93),
        (Key::F10, 94),
        (Key::F11, 95),
        (Key::F12, 96),
        (Key::F13, 97),
        (Key::F14, 98),
        (Key::F15, 99),
        (Key::PAUSE, 100),
    ];
    assert_eq!(Key::KEY_COUNT, 101);
    for (key, expected) in keys {
        assert_eq!(key.0, expected, "Key::{:?} must be {}", key, expected);
    }

    // `enum Button` (ButtonCount = 5).
    let buttons: [(Button, i32); 5] = [
        (Button::LEFT, 0),
        (Button::RIGHT, 1),
        (Button::MIDDLE, 2),
        (Button::X_BUTTON1, 3),
        (Button::X_BUTTON2, 4),
    ];
    assert_eq!(Button::BUTTON_COUNT, 5);
    for (btn, expected) in buttons {
        assert_eq!(btn.0, expected, "Button must be {}", expected);
    }
}

// ---------------------------------------------------------------------------
// winit -> SFML key/button mapping (headless-pure).
// ---------------------------------------------------------------------------

#[test]
fn winit_keys_map_to_sfml_integers() {
    // letters / digits
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Character("a".into()),
            &PhysicalKey::Code(KeyCode::KeyA),
            KeyLocation::Left
        ),
        Key::A
    );
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Character("5".into()),
            &PhysicalKey::Code(KeyCode::Digit5),
            KeyLocation::Left
        ),
        Key::NUM5
    );
    // numpad digits are physical-key disambiguation (logically Character)
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Character("5".into()),
            &PhysicalKey::Code(KeyCode::Numpad5),
            KeyLocation::Left
        ),
        Key::NUMPAD5
    );
    // named keys
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Named(NamedKey::Space),
            &PhysicalKey::Code(KeyCode::Space),
            KeyLocation::Left
        ),
        Key::SPACE
    );
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Named(NamedKey::ArrowDown),
            &PhysicalKey::Code(KeyCode::ArrowDown),
            KeyLocation::Left
        ),
        Key::DOWN
    );
    // modifier sides are location-sensitive
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Named(NamedKey::Shift),
            &PhysicalKey::Code(KeyCode::ShiftRight),
            KeyLocation::Right
        ),
        Key::R_SHIFT
    );
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Named(NamedKey::Control),
            &PhysicalKey::Code(KeyCode::ControlLeft),
            KeyLocation::Left
        ),
        Key::L_CONTROL
    );
    // punctuation
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Character("[".into()),
            &PhysicalKey::Code(KeyCode::IntlBackslash),
            KeyLocation::Left
        ),
        Key::L_BRACKET
    );
}

/// Issue 025d (on-device finding): Android delivers the Enter key as a
/// physical `Code(Enter)` + logical `Character("\n")` pair (the device
/// character map emits '\n' for KEYCODE_ENTER — the winit android
/// `to_logical` path). The mapper must return SFML's `Keyboard::Enter` for
/// that shape: before the fix both halves fell to `Key::UNKNOWN`,
/// `map_window_event` dropped the event, and the release edge never reached
/// the sink — the boot screens were uncloseable with the hardware Enter on
/// the phone (while the desktop `Named(Enter)` shape mapped fine, which is
/// why the host tests stayed green).
#[test]
fn android_enter_character_form_maps_to_sfml_enter() {
    // The exact shape the phone logged for `adb shell input keyevent 66`
    // (physical Code(Enter), logical Character("\n"), location Standard).
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Character("\n".into()),
            &PhysicalKey::Code(KeyCode::Enter),
            KeyLocation::Standard
        ),
        Key::ENTER
    );
    // The logical-only shape (a non-`Code` physical half): the '\n'
    // character arm still lands on ENTER.
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Character("\n".into()),
            &PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
            KeyLocation::Standard
        ),
        Key::ENTER
    );
    // The desktop shape is unchanged (the `Named(Enter)` logical half).
    assert_eq!(
        winit_key_to_sfml(
            &WKey::Named(NamedKey::Enter),
            &PhysicalKey::Code(KeyCode::Enter),
            KeyLocation::Standard
        ),
        Key::ENTER
    );
}

#[test]
fn winit_mouse_buttons_map_to_sfml_integers() {
    assert_eq!(winit_button_to_sfml(&MouseButton::Left), Some(Button::LEFT));
    assert_eq!(
        winit_button_to_sfml(&MouseButton::Middle),
        Some(Button::MIDDLE)
    );
    assert_eq!(
        winit_button_to_sfml(&MouseButton::Other(2)),
        Some(Button::X_BUTTON2)
    );
    assert_eq!(winit_button_to_sfml(&MouseButton::Other(7)), None);
}

// ---------------------------------------------------------------------------
// Verification item 2 (CPU half) — the SFML quad vertex emission order.
// ---------------------------------------------------------------------------

#[test]
fn quad_vertex_order_is_sfml() {
    let mut batch = VertexBatch::new_tris();
    let c = oh_render::Color {
        r: 1,
        g: 2,
        b: 3,
        a: 255,
    };
    batch.push_quad(
        c,
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(1.0, 0.0),
    );
    assert_eq!(batch.primitive(), PrimitiveType::Triangles);
    assert_eq!(batch.len(), 6);
    let v = batch.as_slice();
    // (nw, sw, se, nw, se, ne)
    let expected: [Vec2; 6] = [
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(1.0, 0.0),
    ];
    for (got, want) in v.iter().zip(expected) {
        assert_eq!(got.position, want);
        assert_eq!(got.color, c);
    }
    let call = batch.draw_call(RenderStates::with_shader(1));
    assert_eq!(call.count, 6);
    assert_eq!(call.states.shader, Some(1));
}

// ---------------------------------------------------------------------------
// Verification item 4 — text layout with the repo's bundled TTF.
// ---------------------------------------------------------------------------

#[test]
fn text_layout_is_non_degenerate_and_stable() {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../_RELEASE/Assets/OpenSquare-Regular.ttf"
    );
    let font = Font::load(Path::new(root)).expect("OpenSquare-Regular.ttf must load");

    let (w, h, lines) = oh_render::Text::new(font.clone(), "HELLO", 64).measure();
    assert_eq!(lines, 1);
    assert!(w > 0.0, "width must be non-degenerate");
    assert!(h > 0.0, "height must be non-degenerate");

    // stable across independent measurements.
    let (w2, h2, _) = oh_render::Text::new(font.clone(), "HELLO", 64).measure();
    assert_eq!((w, h), (w2, h2));

    // longer string is wider; bigger character size is wider.
    let (w_long, _, _) = oh_render::Text::new(font.clone(), "HELLO WORLD", 64).measure();
    assert!(w_long > w);
    let (w_small, _, _) = oh_render::Text::new(font, "HELLO", 16).measure();
    assert!(w > w_small);
}

// ---------------------------------------------------------------------------
// Input state — SSVStart's `InputState` bit semantics.
// ---------------------------------------------------------------------------

#[test]
fn input_state_uses_key_plus_one_bits() {
    let mut input = InputState::new();
    // key bit index = int(key) + 1 (inputBitOffset = 1).
    input.set_key_pressed(Key::SPACE, true); // Space = 57 -> bit 58
    assert!(input.is_key_pressed(Key::SPACE));
    let keys = input.get_keys();
    assert_eq!(keys.len(), 102, "KeyCount + 1 bits");
    assert!(keys[58] && !keys[57] && !keys[59]);
    // Unknown = -1 -> bit 0 (the unbound marker).
    input.set_key_pressed(Key::UNKNOWN, true);
    assert!(input.get_keys()[0]);
    // Pause = 100 -> bit 101 (the last of 102).
    input.set_key_pressed(Key::PAUSE, true);
    assert!(input.get_keys()[101]);
    // release clears the bit.
    input.set_key_pressed(Key::SPACE, false);
    assert!(!input.is_key_pressed(Key::SPACE));

    // button bit index = int(btn) + 1.
    input.set_button_pressed(Button::LEFT, true); // Left = 0 -> bit 1
    input.set_button_pressed(Button::X_BUTTON2, true); // XButton2 = 4 -> bit 5
    let btns = input.get_btns();
    assert_eq!(btns.len(), 6, "ButtonCount + 1 bits");
    assert!(btns[1] && btns[5] && !btns[0]);
    assert!(input.is_button_pressed(Button::X_BUTTON2));

    // fingers are 0-indexed, 16 of them.
    input.set_finger(3, true);
    assert!(input.is_finger(3));
    assert_eq!(input.get_finger_down_count(), 1);
    input.set_finger(3, false);
    input.reset();
    assert!(!input.is_key_pressed(Key::PAUSE) && input.get_finger_down_count() == 0);
}

// ---------------------------------------------------------------------------
// Camera math (`ssvs::Camera` — mapping/rendering-menu.md §2).
// ---------------------------------------------------------------------------

#[test]
fn camera_apply_bakes_skew_and_offset() {
    let mut cam = Camera::at_center_size(Vec2::new(100.0, 50.0), Vec2::new(400.0, 300.0));
    let view = cam.apply();
    assert_eq!(view.get_center(), Vec2::new(100.0, 50.0));
    assert_eq!(view.get_size(), Vec2::new(400.0, 300.0));

    // skew multiplies the size.
    cam.set_skew(Vec2::new(1.5, 2.0));
    let view = cam.apply();
    assert!((view.get_size().x - 600.0).abs() < 1e-4);
    assert!((view.get_size().y - 600.0).abs() < 1e-4);

    // offset (polar, subtracted from the centre, rotated by view rotation).
    cam.set_offset(Vec2::new(100.0, 0.0));
    let view = cam.apply();
    assert!((view.get_center().x - 0.0).abs() < 1e-4, "centre x");
    assert!((view.get_center().y - 50.0).abs() < 1e-4, "centre y");
}

#[test]
fn camera_update_applies_pending_and_clears() {
    let mut cam = Camera::at_center_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
    cam.pan(Vec2::new(10.0, 20.0));
    // C++ parity: zoomIn(2) -> nextZoomFactor = 1/2, update applies
    // view.zoom(pow(1/2, ft)) which *divides* the size by 1/2 (i.e. doubles
    // it) — SFML's zoom semantics, transcribed 1:1.
    cam.zoom_in(2.0);
    cam.turn(90.0);
    cam.update(1.0);
    assert!((cam.get_view().get_center().x - 10.0).abs() < 1e-4);
    assert!((cam.get_view().get_center().y - 20.0).abs() < 1e-4);
    assert!(
        (cam.get_view().get_size().x - 200.0).abs() < 1e-4,
        "zoom(0.5) doubles the size"
    );
    assert!((cam.get_rotation() - 90.0).abs() < 1e-4);
    // pending state cleared: a second update changes nothing.
    cam.update(1.0);
    assert!((cam.get_view().get_center().x - 10.0).abs() < 1e-4);
    assert!((cam.get_view().get_size().x - 200.0).abs() < 1e-4);
}

#[test]
fn view_zoom_divides_size_and_maps_pixels() {
    let mut view = View::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
    view.zoom(2.0);
    assert!((view.get_size().x - 50.0).abs() < 1e-5);

    let mut win = Window::headless(800, 600);
    win.set_view(view);
    // pixel (0,0) of an 800x600 target, view centred at the origin with a
    // 50-unit span -> world (-25, -25).
    let p = win.map_pixel_to_coords(Vec2::new(0.0, 0.0));
    assert!((p.x - -25.0).abs() < 1e-4);
    assert!((p.y - -25.0).abs() < 1e-4);
    // the default view is the 1:1 pixel mapping.
    let win = Window::headless(800, 600);
    assert_eq!(
        win.map_pixel_to_coords(Vec2::new(10.0, 20.0)),
        Vec2::new(10.0, 20.0)
    );
    assert_eq!(win.get_default_view().get_center(), Vec2::new(400.0, 300.0));
}

// ---------------------------------------------------------------------------
// Render states + the window's headless surface.
// ---------------------------------------------------------------------------

#[test]
fn render_states_default_and_with_shader() {
    let d = RenderStates::new();
    assert_eq!(d.texture, None);
    assert_eq!(d.shader, None);
    assert_eq!(d.blend_mode, BlendMode::Alpha);
    let s = RenderStates::with_shader(7);
    assert_eq!(s.shader, Some(7));
    assert_eq!(s.texture, None);
    assert_eq!(s.blend_mode, BlendMode::Alpha);
}

#[test]
fn window_headless_surface() {
    let mut win = Window::headless(800, 600);
    assert!(win.is_open());
    assert!(win.gl().is_none(), "headless window has no GL context");
    assert_eq!(win.get_size(), (800, 600));

    // framerate-limit semantics (ssvs::GameWindow applies maxFPS only when
    // fps-limited, else 0).
    win.set_framerate_limit(60.0);
    assert_eq!(win.effective_framerate_limit(), 60.0);
    win.set_fps_limited(false);
    assert_eq!(win.effective_framerate_limit(), 0.0);

    win.set_vsync(true);
    win.set_mouse_cursor_visible(false);
    assert!(win.get_vsync());
    assert!(!win.is_mouse_cursor_visible());

    // joystick surface is a clean no-op headless.
    assert!(!win.is_joystick_connected(0));
    assert_eq!(win.get_joystick_axis(0, oh_render::JoystickAxis::X), 0);

    // save_screenshot fails cleanly without a GL context.
    assert!(!win.save_screenshot("/tmp/oh-shot.png"));

    win.close();
    assert!(!win.is_open());
}

// ---------------------------------------------------------------------------
// Verification item 2 (GPU half) — offscreen clear + glReadPixels.
// Skips with a printed reason when no display/GPU is available.
// ---------------------------------------------------------------------------

#[test]
fn gpu_smoke() {
    let win = Window::create(64, 64, "oh-render-gpu-smoke");
    let Some(parts) = win.gl() else {
        println!(
            "SKIP: no display/GPU in this environment (winit/GL bootstrap failed or is unavailable); headless path active"
        );
        return;
    };
    let version = parts
        .version_string()
        .expect("a live GL context reports a version");
    assert!(!version.is_empty());
    // clear the framebuffer to blue and read back the centre pixel.
    let px = parts
        .clear_and_read_center([0.0, 0.0, 1.0, 1.0])
        .expect("pixel readback from the live context");
    eprintln!("GPU smoke: {version}, centre pixel = {px:?}");
    assert_eq!(px[2], 255, "blue channel must be the cleared colour");
    assert!(px[0] < 16 && px[1] < 16, "cleared colour is pure blue");

    // The screenshot path (the C++ `GameWindow::saveScreenshot`,
    // GameWindow.hpp:167-174): whole-frame `glReadPixels` (bottom-up) →
    // row flip → PNG encode → file. Decode the file back and assert the
    // size + the cleared colour (a uniform clear pins the readback; the
    // row ORDER is pinned by the in-crate `flip_rows` tests).
    let shot = std::env::temp_dir().join(format!(
        "oh-render-gpu-smoke-shot-{}-{}.png",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
    ));
    assert!(
        parts.save_screenshot(shot.to_str().expect("temp path")),
        "save_screenshot writes the PNG"
    );
    {
        let file = std::fs::File::open(&shot).expect("screenshot file exists");
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().expect("screenshot is a valid PNG");
        let mut buf = vec![0u8; reader.output_buffer_size().expect("output size")];
        let out = reader.next_frame(&mut buf).expect("screenshot frame");
        assert_eq!(
            (out.width, out.height),
            (64, 64),
            "the screenshot is the full window"
        );
        assert_eq!(out.color_type, png::ColorType::Rgba);
        let rgba = &buf[..out.buffer_size()];
        // centre pixel = the cleared blue (top-down row 32, column 32).
        let i = (32 * 64 + 32) * 4;
        assert_eq!(rgba[i + 2], 255, "the decoded screenshot's centre is blue");
        assert!(rgba[i] < 16 && rgba[i + 1] < 16);
    }
    let _ = std::fs::remove_file(&shot);
}

// 025j — the swap-interval wiring: `Window::set_vsync` applies the live
// interval through `GlParts::set_swap_interval` (glutin's
// `GlSurface::set_swap_interval` — the pinned SFML's `eglSwapInterval`
// equivalent, `GlParts::set_swap_interval`'s docs). glutin has no
// read-back of the interval, so this is the no-panic round trip on a live
// context (the device lane verifies the observable swap behaviour on the
// phone); the headless store-only half is the `window_headless_surface`
// test above.
#[test]
fn set_vsync_round_trips_and_applies_on_a_live_context() {
    let mut win = Window::create(64, 64, "oh-render-swap-interval");
    if win.gl().is_none() {
        println!(
            "SKIP: no display/GPU in this environment (winit/GL bootstrap failed or is unavailable); headless path active"
        );
        return;
    }
    win.set_vsync(true);
    assert!(win.get_vsync());
    win.set_vsync(false);
    assert!(!win.get_vsync());
}

// 047 — the fullscreen wiring (the config's `fullscreen` value → the C++
// `GameWindow::setFullscreen`'s observable half, `GameWindow.hpp:90`
// `fullscreen ? sf::Style::Fullscreen : sf::Style::Default`). `Window::
// set_fullscreen` now applies the style to the live winit window (desktop;
// `Borderless(None)` = borderless fullscreen on the current monitor, the
// `sf::Style::Fullscreen` equivalent) instead of only recording the flag.
// winit's `fullscreen()` reads the window's own state (the pre-`run`
// `desired_fullscreen` store, applied on the first visibility notify), NOT
// the WM's live state, so the read-back is reliable in a headless CI —
// unlike a real WM round trip (the device/manual lane verifies the
// observable fullscreen window). Skips when there is no display/GPU
// (headless path: the flag is still recorded, `get_fullscreen` round-trips).
#[cfg(not(target_os = "android"))]
#[test]
fn set_fullscreen_round_trips_and_requests_the_winit_window() {
    let mut win = Window::create(64, 64, "oh-render-fullscreen");
    // The store half always round-trips (headless or not).
    win.set_fullscreen(true);
    assert!(win.get_fullscreen());
    if win.gl().is_none() {
        println!(
            "SKIP: no display/GPU in this environment (winit/GL bootstrap failed or is unavailable); headless path active"
        );
        return;
    }
    // The apply half: the live winit window carries the borderless request
    // (the `desired_fullscreen` store pre-`run`, applied on visibility).
    let fs = win.gl().as_ref().unwrap().winit_window().fullscreen();
    assert!(
        matches!(fs, Some(winit::window::Fullscreen::Borderless(_))),
        "expected a borderless-fullscreen request on the winit window, got {fs:?}"
    );
    // Back to windowed: the request is cleared (a `None` fullscreen).
    win.set_fullscreen(false);
    assert!(!win.get_fullscreen());
    let fs = win.gl().as_ref().unwrap().winit_window().fullscreen();
    assert!(
        fs.is_none(),
        "expected a cleared (windowed) fullscreen state, got {fs:?}"
    );
}

// ---------------------------------------------------------------------------
// issue 049 (K2) — the C++ SFML frame-limiter arithmetic (host-pinned: the
// device pacing itself is the operator's on-device confirmation, the device
// cannot be reproduced here). The C++ paces inside `Window::display()` —
// the pinned SFML `dacceddbd`'s `src/SFML/Window/Window.cpp:207-219`:
// after the swap, `sleep(m_frameTimeLimit − m_clock.getElapsedTime());
// m_clock.restart();` — with SFML's `sleep` a no-op for negative
// durations (`src/SFML/System/Sleep.cpp:41-48`).
// ---------------------------------------------------------------------------

#[test]
fn frame_limiter_sleep_matches_the_cpp_arithmetic() {
    use oh_render::window::frame_limiter_sleep;
    use std::time::Duration;

    // The C++ `setFramerateLimit(0)` arm (`Window.cpp:175-181` —
    // `Time::Zero`, the `fpsLimited` false / `max_fps` 0 configuration):
    // no limit, no sleep, whatever the iteration took.
    assert_eq!(frame_limiter_sleep(0.0, Duration::ZERO), Duration::ZERO);
    assert_eq!(
        frame_limiter_sleep(0.0, Duration::from_millis(20)),
        Duration::ZERO
    );

    // The shipped config (`limit_fps=true, max_fps=200`): a 0 ms
    // iteration sleeps the FULL frame time — 1/200 s = 5 ms exactly
    // (the f32 `1.0/200.0` floors to 5_000_000 ns).
    assert_eq!(
        frame_limiter_sleep(200.0, Duration::ZERO),
        Duration::from_millis(5)
    );

    // The subtraction (`sleep(1/limit − elapsed)`, `Window.cpp:216`): a
    // 3 ms iteration sleeps the remaining 2 ms.
    assert_eq!(
        frame_limiter_sleep(200.0, Duration::from_millis(3)),
        Duration::from_millis(2)
    );

    // The SFML `sleep`'s negative no-op: an iteration that already ran
    // exactly the frame time (5 ms) — or far past it (a heavy 20 ms
    // frame, the in-game GPU-limited case) — sleeps 0 (the C++'s "a ≥5 ms
    // iteration sleeps 0").
    assert_eq!(
        frame_limiter_sleep(200.0, Duration::from_millis(5)),
        Duration::ZERO
    );
    assert_eq!(
        frame_limiter_sleep(200.0, Duration::from_millis(20)),
        Duration::ZERO
    );

    // A non-shipped limit (the options slider's 60): 1/60 s, within the
    // f32 frame-time rounding (the C++'s own `seconds(1.f / limit)`
    // carries the same f32 precision — `Window.cpp:178`).
    let sleep60 = frame_limiter_sleep(60.0, Duration::ZERO);
    let ideal = Duration::from_secs_f64(1.0 / 60.0);
    let diff = sleep60.abs_diff(ideal);
    assert!(
        diff < Duration::from_micros(1),
        "60 fps frame time: {sleep60:?} vs the ideal {ideal:?} (diff {diff:?})"
    );
}
