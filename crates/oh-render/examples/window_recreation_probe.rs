//! 29w — the Xvfb recreation probe: the C++ `mustRecreate` →
//! `recreateWindow` (GameWindow.hpp:123,85-101) runtime behaviour on a
//! REAL window — the antialiasing-level change lands on a LIVE GL
//! context without a restart: create the context at level 0, change the
//! level to 4 (the options-menu's write — the C++ `Config::
//! setAntialiasingLevel`'s window-call half, Config.cpp:703-709),
//! consume the request (`maybe_recreate_window` — the C++'s loop head,
//! GameWindow.hpp:123), and assert the context's MSAA sample count
//! changed 0 → ≥4 and the NEW context draws (the C++'s `recreateWindow`
//! re-reads the stored level into the `ContextSettings`,
//! GameWindow.hpp:91 — the port's `GlParts::recreate` reuses the boot
//! creation path: `pick_msaa_config` + the shared `create_gl_context_on`
//! tail, so there is no second creation path).
//!
//! Run (a display is needed — `xvfb-run`'s software GL):
//! ```sh
//! timeout 300 xvfb-run -a -s "-screen 0 800x600x24" \
//!   cargo run -p oh-render --example window_recreation_probe
//! ```
//! WITHOUT a display / software GL the probe prints its skip-with-reason
//! and exits 0 (the D-011 shape, the `pack_shaders_gl` example's — say so
//! in the report, do not claim the GL column).

#[cfg(not(any(target_os = "emscripten", target_os = "android")))]
fn main() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use oh_render::Window;

    const W: u32 = 800;
    const H: u32 = 600;

    // Level 0: no MSAA request (`pick_msaa_config`'s level-0 arm — the
    // first config; `GL_SAMPLES` is 0 on the usual desktop drivers).
    let mut window = Window::create(W, H, "window_recreation_probe (29w)", 0);

    // The recreation's consumer (the `WindowedClient::run`'s hook shape —
    // the egui console's stale-GL-state clear; here: a fire counter).
    // Installed BEFORE the `gl()` borrow (the hook and the probe's
    // context handle both borrow the window).
    let fired = Arc::new(AtomicU32::new(0));
    let fired_hook = fired.clone();
    window.set_on_recreation(Box::new(move || {
        fired_hook.fetch_add(1, Ordering::SeqCst);
    }));

    let Some(gl) = window.gl() else {
        eprintln!(
            "window_recreation_probe: SKIPPED (D-011 skip-with-reason) — no \
             GL context on this box (no display / no software GL). The \
             window degraded to headless; the GL recreation column was NOT \
             run. The headless half (the request's set + consume + the \
             onRecreation hook + the input reset) is the `oh-app` \
             integration test `options_menu_antialiasing_change_sets_and_
             consumes_the_recreation_request`."
        );
        return;
    };
    let samples_before = gl.gl_samples();
    println!(
        "window_recreation_probe: boot context — GL_SAMPLES = {samples_before} \
         (level 0 requested; {})",
        gl.version_string().unwrap_or_default()
    );

    // The boot context draws (the pre-recreation usability baseline).
    gl.clear([1.0, 0.0, 0.0, 1.0]);
    gl.present();
    let boot_pixel = gl.read_pixel(W as i32 / 2, H as i32 / 2);
    assert_eq!(boot_pixel, [255, 0, 0, 255], "the boot context's red clear");

    // The SPLIT's SET half (29w): the options-menu's antialiasing write
    // (the C++ MenuGame.cpp:1446-1464's `Config::setAntialiasingLevel`'s
    // window-call half, Config.cpp:703-709) — the change-guarded setter
    // stores the level and marks `mustRecreate` (GameWindow.hpp:189-190).
    window.set_antialiasing_level(4);
    assert!(
        window.must_recreate(),
        "the recreation request is SET (the setting changed)"
    );

    // The SPLIT's REBUILD half: the frame loop's consumption — the C++'s
    // `if(mustRecreate) recreateWindow();` (GameWindow.hpp:123) at the
    // loop head: the desktop GL context's rebuild + the vsync re-apply
    // (GameWindow.hpp:94) + `mustRecreate = false` (:99) + `onRecreation()`
    // (:100). (The C++'s `inputState.reset()` half, :96, is the
    // `reset_input_state` operation — the frame body's post-update
    // application; applied here too, the probe's shape.)
    let recreated = window.maybe_recreate_window();
    assert!(recreated, "the request was CONSUMED (no restart needed)");
    window.reset_input_state();
    assert_eq!(
        fired.load(Ordering::SeqCst),
        1,
        "the onRecreation hook fired (GameWindow.hpp:100)"
    );

    // The observable (the task's (b) — the setting applied WITHOUT a
    // restart): the live context's sample count moved. The C++'s
    // `recreateWindow`'s `ContextSettings{0, 0, antialiasingLevel, 0, 0}`
    // (GameWindow.hpp:91) on the NEW level — `pick_msaa_config`'s
    // smallest-samples-≥-level pick (its shortfall note: a driver with no
    // ≥4-sample config would land on its largest — asserted below).
    let gl2 = window
        .gl()
        .expect("the window still has its GL after the recreation");
    let samples_after = gl2.gl_samples();
    println!(
        "window_recreation_probe: post-recreation context — GL_SAMPLES = \
         {samples_after} (level 4 requested; {})",
        gl2.version_string().unwrap_or_default()
    );
    assert!(
        samples_after >= 4,
        "the changed level reached the live context (0 → {samples_after})"
    );

    // The NEW context is usable (the recreation did not strand the
    // window): a clear + present + read-back on it.
    gl2.clear([0.0, 1.0, 0.0, 1.0]);
    gl2.present();
    let new_pixel = gl2.read_pixel(W as i32 / 2, H as i32 / 2);
    assert_eq!(
        new_pixel,
        [0, 255, 0, 255],
        "the rebuilt context draws (the green clear's read-back)"
    );

    // The flag rests consumed (a re-tick would be a no-op — the C++'s
    // `if(mustRecreate)` false arm, GameWindow.hpp:123).
    assert!(!window.must_recreate());

    println!(
        "window_recreation_probe: PASSED — the runtime antialiasing change \
         (0 → {samples_after} samples) landed on the live GL context \
         without a restart: the request set (the setter's mark) + \
         consumed (the loop head's `recreateWindow`) + the rebuilt \
         context draws"
    );
}

// The android / web shapes have no window-style to recreate (the per-
// platform reasons, the `Window::recreate_window`'s docs — the android
// activity window's no-style + the web's browser-API fullscreen; the C++
// has no android/web build at all), so the probe is desktop-only.
#[cfg(any(target_os = "emscripten", target_os = "android"))]
fn main() {
    eprintln!(
        "window_recreation_probe: not applicable — the recreation is \
         desktop-only (the android activity window has no window-style; \
         the web's shape is a browser-API matter, lane 046)"
    );
}
