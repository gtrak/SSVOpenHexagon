//! The desktop-mode probe child (the winit single-`EventLoop` fix).
//!
//! The C++ `mainClient` bootstrap reads `sf::VideoMode::getDesktopMode()`
//! (Config.cpp:480, :487 — the `applyAuto*Resolution` inputs) BEFORE the
//! window exists, and feeds it to `reapplyResolution` (Config.cpp:521-536)
//! — but the C++ only CONSUMES the read inside
//! `applyAutoWindowedResolution` / `applyAutoFullscreenResolution`
//! (Config.cpp:478-490), which `reapplyResolution` runs only when the
//! corresponding auto flag is on (both default false in the shipped
//! `_RELEASE/config.json`).
//!
//! The port's `oh_render::VideoMode::get_desktop_mode()` is a winit
//! primary-monitor query, and winit 0.30 allows ONE `EventLoop` per
//! process: `EventLoopBuilder::build` sets a process-global
//! `EVENT_LOOP_CREATED` that only the web backend clears, so a probe loop
//! built in the game process makes the window's own `EventLoop::new()`
//! (the `oh-render` `GlBackend::create`'s) return
//! `Err(RecreationAttempt)` — the window degrades headless and the run
//! exits after one frame (`[::mainClient] Finished`). This module is the
//! fix: the probe runs in a CHILD process (mirroring the repo's existing
//! one-loop workaround — `examples/pack_shaders_gl.rs`'s `gles` child
//! mode), so the parent's single `EventLoop` stays free for the game
//! window.
//!
//! * [`run_probe_child_if_requested`] — the `dispatch::run` top-of-`main`
//!   hook: when `argv` carries [`PROBE_ARG`] this process IS the probe
//!   child — run the throwaway loop HERE, print the mode, exit 0, before
//!   any FS/config/window work.
//! * [`wants_desktop_mode`] — the lazy probe gate: the query is only
//!   needed when an auto-resolution flag is on (Config.cpp:478-490 via
//!   :521-536).
//! * [`probe_desktop_mode`] — the parent side: spawn the child, parse the
//!   mode out of its stdout; ANY failure (no exe, spawn error, the
//!   ~5s wait timeout, non-zero exit, unparseable output) degrades to
//!   the zero mode (the C++'s
//!   no-monitor `VideoMode{0, 0}` fallback).

// The process plumbing is desktop-only: on android/emscripten both
// functions below take their `#[cfg(any(...))]` early-return arms and
// never spawn, so the imports would be dead there. `VideoMode` stays
// unconditional (the android/web arms construct the zero mode).
#[cfg(not(any(target_os = "android", target_os = "emscripten")))]
use std::io::Read;
#[cfg(not(any(target_os = "android", target_os = "emscripten")))]
use std::process::{Command, Stdio};

use oh_config::Config;
use oh_render::VideoMode;

/// The hidden internal argument marking the probe child (no C++
/// equivalent — the C++ has no one-`EventLoop`-per-process restriction,
/// so its `main` has no such hidden arg).
pub const PROBE_ARG: &str = "--oh-desktop-mode-probe";

/// The `dispatch::run` top-of-`main` hook (the module docs): when `argv`
/// carries [`PROBE_ARG`], this process is the probe child the parent
/// spawned — run the throwaway winit probe loop here (the
/// `get_desktop_mode` primary-monitor read), print
/// `OH_DESKTOP_MODE={width}x{height}` to stdout, and report exit 0.
///
/// Returns `None` when the arg is absent (the normal game start — the
/// caller proceeds to the chdir / config / window work) and on
/// android/emscripten (the probe never runs there — those targets'
/// `get_desktop_mode` arm already hardcodes the zero mode, and the arg is
/// internal: the platform never passes it).
#[must_use]
pub fn run_probe_child_if_requested(argv: &[String]) -> Option<i32> {
    if !argv.iter().any(|a| a == PROBE_ARG) {
        return None;
    }
    #[cfg(any(target_os = "android", target_os = "emscripten"))]
    {
        // Never runs the probe on these targets (the zero mode is
        // hardcoded in `get_desktop_mode`'s own android/web arm — the
        // platform window is screen-/canvas-sized independently of the
        // config values, the `get_desktop_mode` docs).
        return None;
    }
    #[cfg(not(any(target_os = "android", target_os = "emscripten")))]
    {
        // The child's ONE job: the display query in its own process (the
        // parent's `EventLoop` slot is untouched — the fix). The zero
        // mode is the C++'s no-monitor / headless-host fallback
        // (`get_desktop_mode`'s docs), printed like any other mode.
        let mode = VideoMode::get_desktop_mode();
        println!("OH_DESKTOP_MODE={}x{}", mode.width, mode.height);
        Some(0)
    }
}

/// The lazy probe gate: the desktop-mode query is only needed when an
/// auto-resolution flag is on (Config.cpp:478-490 via :521-536 — the
/// C++'s `applyAuto*Resolution` branches are the only consumers of the
/// `getDesktopMode` read).
#[must_use]
pub fn wants_desktop_mode(config: &Config) -> bool {
    config.get_windowed_auto_resolution() || config.get_fullscreen_auto_resolution()
}

/// The parent side (the module docs): spawn the probe child (this
/// process's own executable with [`PROBE_ARG`]), wait for it (bounded —
/// a hung display query must not hang the boot), read its stdout, and
/// parse the mode out of the `OH_DESKTOP_MODE={width}x{height}` line.
///
/// ANY failure — no current exe, spawn error, the ~5s wait timeout,
/// non-zero exit, unparseable output — returns `VideoMode::new(0, 0)` (the C++'s no-monitor
/// `VideoMode{0, 0}` fallback — the auto-resolution branches then take
/// the zero size, exactly the C++'s `getDesktopMode` failure mode).
/// Android/emscripten never spawn: the zero mode returns directly (the
/// `get_desktop_mode` docs — the platform window is screen-sized
/// independently of the config values).
#[must_use]
pub fn probe_desktop_mode() -> VideoMode {
    #[cfg(any(target_os = "android", target_os = "emscripten"))]
    {
        return VideoMode::new(0, 0);
    }
    #[cfg(not(any(target_os = "android", target_os = "emscripten")))]
    {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(_) => return VideoMode::new(0, 0),
        };
        let mut cmd = Command::new(exe);
        cmd.arg(PROBE_ARG)
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // Windows: no console flash for the probe (the parent is a
        // windowed game; a visible console window on every auto-flag-on
        // boot would be a regression). CREATE_NO_WINDOW.
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(_) => return VideoMode::new(0, 0),
        };
        // Bound the wait: a display query that HANGS must not hang the
        // boot. Poll `try_wait` against a ~5s deadline (std only); on
        // expiry kill the child, reap it, and take the zero mode (the
        // "any failure" contract above).
        const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
        const PROBE_POLL: std::time::Duration = std::time::Duration::from_millis(100);
        let deadline = std::time::Instant::now() + PROBE_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return VideoMode::new(0, 0);
                    }
                    std::thread::sleep(PROBE_POLL);
                }
                Err(_) => return VideoMode::new(0, 0),
            }
        };
        // Drain the child's stdout AFTER it has exited: the pipe's write
        // end closed with the child and the OS holds its bytes until
        // read, so this read cannot block (and the probe prints a single
        // short line — far below any pipe buffer — so even the child's
        // own write could never have blocked).
        let mut stdout_buf: Vec<u8> = Vec::new();
        if let Some(mut out) = child.stdout.take() {
            if out.read_to_end(&mut stdout_buf).is_err() {
                return VideoMode::new(0, 0);
            }
        }
        if !status.success() {
            return VideoMode::new(0, 0);
        }
        parse_probe_output(&String::from_utf8_lossy(&stdout_buf))
            .map(|(w, h)| VideoMode::new(w, h))
            .unwrap_or_else(|| VideoMode::new(0, 0))
    }
}

/// Parse the probe child's stdout: the first line (anywhere in the
/// output) whose trimmed form starts with `OH_DESKTOP_MODE=` and whose
/// `{width}x{height}` tail parses as two `u32`s. `None` on garbage
/// (no marker, a missing `x`, or non-numeric fields — the caller's zero
/// fallback).
#[cfg(not(any(target_os = "android", target_os = "emscripten")))]
fn parse_probe_output(stdout: &str) -> Option<(u32, u32)> {
    stdout.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("OH_DESKTOP_MODE=")?;
        let (w, h) = rest.split_once('x')?;
        let w = w.trim().parse::<u32>().ok()?;
        let h = h.trim().parse::<u32>().ok()?;
        Some((w, h))
    })
}

#[cfg(all(test, not(any(target_os = "android", target_os = "emscripten"))))]
mod tests {
    use super::parse_probe_output;

    /// The probe child's exact output (the `run_probe_child_if_requested`
    /// print).
    #[test]
    fn parse_the_probe_line() {
        assert_eq!(
            parse_probe_output("OH_DESKTOP_MODE=1920x1080\n"),
            Some((1920, 1080))
        );
    }

    /// The marker is accepted ANYWHERE in the output (the child's stdout
    /// may carry other lines before the marker — the parse is
    /// line-scanned, not position-anchored).
    #[test]
    fn parse_the_marker_anywhere_in_the_output() {
        assert_eq!(
            parse_probe_output(
                "some boot noise\nanother line\nOH_DESKTOP_MODE=2560x1440\ntrailing\n",
            ),
            Some((2560, 1440))
        );
    }

    /// Garbage is rejected (the caller's zero fallback — the C++'s
    /// no-monitor `VideoMode{0, 0}`): no marker, a missing `x`, and
    /// non-numeric fields all parse to `None`.
    #[test]
    fn parse_rejects_garbage() {
        assert_eq!(parse_probe_output(""), None, "empty output");
        assert_eq!(
            parse_probe_output("nothing to see here\n"),
            None,
            "no marker"
        );
        assert_eq!(
            parse_probe_output("OH_DESKTOP_MODE=\n"),
            None,
            "marker with empty tail"
        );
        assert_eq!(
            parse_probe_output("OH_DESKTOP_MODE=1920\n"),
            None,
            "missing the x separator"
        );
        assert_eq!(
            parse_probe_output("OH_DESKTOP_MODE=abcxdef\n"),
            None,
            "non-numeric fields"
        );
        assert_eq!(
            parse_probe_output("OH_DESKTOP_MODE=-1x1080\n"),
            None,
            "a negative width is not a u32"
        );
    }

    /// The FIRST well-formed marker wins (later lines cannot override it
    /// — the probe child prints exactly one).
    #[test]
    fn parse_the_first_marker_wins() {
        assert_eq!(
            parse_probe_output("OH_DESKTOP_MODE=800x600\nOH_DESKTOP_MODE=1920x1080\n"),
            Some((800, 600))
        );
    }
}

#[cfg(test)]
mod gate_tests {
    use super::wants_desktop_mode;
    use oh_config::Config;

    /// Both auto flags default false (the shipped `_RELEASE/config.json`
    /// state — Config.cpp:478-490: the C++'s `applyAuto*Resolution`
    /// branches never run) → the probe is never needed.
    #[test]
    fn default_config_wants_no_desktop_mode() {
        assert!(!wants_desktop_mode(&Config::default()));
    }

    /// `windowed_auto_resolution` on → the probe is needed (the C++'s
    /// `applyAutoWindowedResolution`, Config.cpp:478-490). No spawn:
    /// the gate reads pure config state. `set_current_resolution_auto`
    /// (the C++'s `setCurrentResolutionAuto`) is the public setter that
    /// turns the windowed flag on (not fullscreen by default).
    #[test]
    fn windowed_auto_flag_wants_the_desktop_mode() {
        let mut config = Config::default();
        config.set_current_resolution_auto(1920, 1080);
        assert!(config.get_windowed_auto_resolution(), "flag set");
        assert!(wants_desktop_mode(&config));
    }

    /// `fullscreen_auto_resolution` on → the probe is needed (the C++'s
    /// `applyAutoFullscreenResolution`, Config.cpp:478-490).
    #[test]
    fn fullscreen_auto_flag_wants_the_desktop_mode() {
        let mut config = Config::default();
        config.set_fullscreen(true);
        config.set_current_resolution_auto(1920, 1080);
        assert!(config.get_fullscreen_auto_resolution(), "flag set");
        assert!(wants_desktop_mode(&config));
    }
}
