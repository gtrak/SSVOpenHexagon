//! 053 — the Xvfb live-resolution probe: the C++'s LIVE resolution
//! change (the 025g creation-frozen-surface deviation's closure) on a
//! REAL window — `MenuGame::changeResolutionTo` (MenuGame.cpp:1039-1053)
//! is a LIVE `getRenderWindow().setSize(w, h)` (MenuGame.cpp:1047 — never
//! a `mustRecreate` mark), and the port's `Window::set_inner_size` (the
//! same live call) must move the size on the LIVE winit window with the
//! GL viewport + backbuffer following — through the ONE resize path: the
//! `Resized` event the request delivers (the fold's desktop
//! `GlParts::resize` arm — the GLX backbuffer's no-op + the viewport +
//! the `GlParts` size bookkeeping, the issue 32 web arm's same shape).
//! No context recreation, no restart, no cache reset.
//!
//! Run (a display is needed — `xvfb-run`'s software GL):
//! ```sh
//! timeout 300 xvfb-run -a -s "-screen 0 800x600x24" \
//!   cargo run -p oh-render --example resolution_live_probe
//! ```
//! WITHOUT a display / software GL the probe prints its skip-with-reason
//! and exits 0 (the D-011 shape, the `window_recreation_probe`'s — say
//! so in the report, do not claim the GL column). The headless half (the
//! config-follow triage + the sync's set) is the `oh-app` integration
//! test `options_menu_resolution_change_moves_the_window_and_the_config_
//! follows_an_os_resize`.

#[cfg(not(any(target_os = "emscripten", target_os = "android")))]
fn main() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use oh_render::Window;

    const W0: u32 = 800;
    const H0: u32 = 600;
    const W1: u32 = 640;
    const H1: u32 = 480;
    /// The Poll-spin's frame budget for the `Resized` delivery (the
    /// `request_inner_size`'s ConfigureNotify lands within microseconds
    /// on X11; the budget is the runaway guard, not an expectation).
    const MAX_FRAMES: u32 = 500;

    let mut window = Window::create(W0, H0, "resolution_live_probe (053)", 0);

    // The delivery's frame counter (the `on_frame`'s closure borrow).
    let frames = Arc::new(AtomicU32::new(0));
    let frames_probe = frames.clone();

    let Some(gl) = window.gl() else {
        eprintln!(
            "resolution_live_probe: SKIPPED (D-011 skip-with-reason) — no \
             GL context on this box (no display / no software GL). The \
             window degraded to headless; the GL live-resize column was \
             NOT run. The headless half (the `Resized` triage's \
             config-follow + the sync's set) is the `oh-app` \
             integration test `options_menu_resolution_change_moves_the_
             window_and_the_config_follows_an_os_resize`."
        );
        return;
    };
    assert_eq!(
        (gl.width, gl.height),
        (W0, H0),
        "the boot backbuffer's bookkeeping"
    );
    assert_eq!(
        gl.viewport(),
        [0, 0, W0 as i32, H0 as i32],
        "the boot viewport"
    );
    // The boot context draws (the pre-resize usability baseline): the
    // `clear_and_read_center`'s read-before-swap order (the back buffer
    // is well-defined before the `swap_buffers`; after it, on a
    // double-buffered GLX drawable, the back contents are the
    // GL-unspecified ones).
    gl.clear([1.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        gl.read_pixel(W0 as i32 / 2, H0 as i32 / 2),
        [255, 0, 0, 255],
        "the boot context's red clear"
    );
    gl.present();

    // The C++'s `changeResolutionTo`'s window half (MenuGame.cpp:1047 —
    // the LIVE `renderWindow.setSize`, the `Window::set_inner_size`'s
    // docs): the winit request + the bookkeeping's move (the SFML
    // `m_size`'s immediate-update semantics). The GL surface + the
    // viewport follow on the `Resized` delivery (the fold's
    // `gl.resize` arm — the single resize path).
    window.set_inner_size(W1, H1);
    assert_eq!(
        window.get_size(),
        (W1, H1),
        "the bookkeeping moved with the request (SFML's immediate m_size)"
    );

    // Pump the REAL event loop until the delivery lands (the
    // `RunHandler::window_event`'s fold — the production path, not a
    // synthetic `apply_event`): the `WindowEvent::Resized` the
    // `request_inner_size`'s ConfigureNotify delivers.
    window.run(move |window| {
        let n = frames_probe.fetch_add(1, Ordering::SeqCst);
        let Some(gl) = window.gl() else {
            panic!("the window lost its GL mid-probe");
        };
        // The acceptance observable (b): the size ACTUALLY changed on
        // the LIVE window (winit's `inner_size` — the OS window's real
        // inner size, not the port's bookkeeping).
        let live = gl.winit_window().inner_size();
        if live.width == W1 && live.height == H1 {
            assert_eq!(
                window.get_size(),
                (W1, H1),
                "the fold's bookkeeping half (the `Resized`'s `set_size`)"
            );
            assert_eq!(
                (gl.width, gl.height),
                (W1, H1),
                "the backbuffer's bookkeeping followed (the fold's \
                 `gl.resize` half)"
            );
            assert_eq!(
                gl.viewport(),
                [0, 0, W1 as i32, H1 as i32],
                "the GL viewport followed (the `gl.resize`'s viewport)"
            );
            // The NEW backbuffer is usable (the resize did not strand
            // the context — no recreation, no cache reset): a clear +
            // read-back at the NEW size's centre (the read-before-swap
            // order, the boot check's note).
            gl.clear([0.0, 1.0, 0.0, 1.0]);
            assert_eq!(
                gl.read_pixel(W1 as i32 / 2, H1 as i32 / 2),
                [0, 255, 0, 255],
                "the resized backbuffer draws (the green clear's \
                 read-back at the new centre)"
            );
            gl.present();
            println!(
                "resolution_live_probe: live window {W0}x{H0} -> {W1}x{H1} \
                 (frame {n}) — viewport [0 0 {W1} {H1}], backbuffer \
                 bookkeeping ({W1}, {H1}), the new backbuffer draws"
            );
            window.close();
            return;
        }
        if n >= MAX_FRAMES {
            panic!(
                "resolution_live_probe: the `Resized` delivery never \
                 landed in {MAX_FRAMES} frames (live inner size \
                 {}x{})",
                live.width, live.height
            );
        }
    });

    println!(
        "resolution_live_probe: PASSED — the C++'s live resolution change \
         (MenuGame.cpp:1047's `renderWindow.setSize`'s port mirror) moved \
         the size on the live window (800x600 -> 640x480) with the GL \
         viewport + the backbuffer following, on the live context, \
         without a restart"
    );
}

// The android / web shapes have no window to drive with a "resolution"
// (the per-platform reasons, the `Window::set_inner_size`'s docs — the
// android activity window's surface is the screen's; the web canvas is
// the page's CSS box; the C++ has no android/web build at all), so the
// probe is desktop-only.
#[cfg(any(target_os = "emscripten", target_os = "android"))]
fn main() {
    eprintln!(
        "resolution_live_probe: not applicable — the live resolution \
         change is desktop-only (the android surface is the screen's; \
         the web canvas is CSS-sized — the browser owns its size)"
    );
}
