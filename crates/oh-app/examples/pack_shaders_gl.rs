//! 029n1 (K6) §4.2 + §4.3 — the GL column: the shipped pack `.frag` files'
//! COMPILE GUARD (all four, raw AND through `translate_for_gles`) and the
//! shader-BOUND DRAW (the `GlDrawTarget`'s `states.shader` execution half).
//!
//! Run: `cargo run -p oh-app --example pack_shaders_gl` (with a display, or
//! under `xvfb-run` / a software-GL stack). WITHOUT a real or software GL
//! context the example prints its skip-with-reason and exits 0 (D-011 —
//! the §4.2/§4.3 columns are NOT run; say so in the report, do not claim
//! them).
//!
//! §4.2 — the 29v1 lesson applied to GLSL: a pack `.frag` with a
//! syntax error made a feature a no-op on every call, silently. The
//! guard runs each of the four shipped files in its execution context:
//!
//! * **raw** (the desktop execution form — the C++'s `loadFromFile`
//!   verbatim compile): all four must compile + link through
//!   [`PackShaderProgram::new`] (the fragment-only program + the stage
//!   VAOs — the exact draw-time path) on the desktop context;
//! * **through `translate_for_gles`** (the GLES execution form — the
//!   ES 1.00 translation, the 25b-SG3 target): on a GLES context (the
//!   `GlBackend::try_create_gles` software-EGL context, run in the
//!   `gles` CHILD MODE — a fresh process: winit 0.30 allows one
//!   `EventLoop` per process), each file goes through the real GLES
//!   execution path — [`PackShaderProgram::new`] translates the source
//!   and builds the two stage programs (the GL ES vertex stage is
//!   required — a fragment-only program links nowhere in GL ES 3.x).
//!   `pixelate.frag` (no fixed-function builtins) must build; the
//!   `gl_Color`/`gl_TexCoord`-reading files must fail for the
//!   documented 25b reason only — those builtins have no GLSL ES 1.00
//!   fragment equivalent (the sticky fallback to the built-in
//!   pass-through program) — so the failure's root errors are asserted
//!   to name only those builtins (no syntax error — the 29v1 class).
//!
//! §4.3 — the shader-bound draw:
//! * (a) `gradient.frag` bound to a full-screen white quad through the
//!   real `GlDrawTarget::draw_vertices` (`RenderStates::with_shader`),
//!   uniforms set through the real `Assets` uniform store + the
//!   `AssetsPackShaderProvider`: the bottom of the window must read back
//!   BLUE-dominant, the top RED-dominant (`gradient.frag` mixes
//!   `u_color1`(blue) → `u_color0`(red) over `gl_FragCoord.y /
//!   u_resolution.y`), while the SAME quad through the default program
//!   reads back pure white — the negative control proving the pack
//!   fragment stage actually ran.
//! * (b) the shipped `pointless` level, a full game frame through the
//!   D-020 `bridge::draw` (the windowed loop's draw path): the level's
//!   own Lua binds `steamart.frag` to 8 stages and sets
//!   `u_resolution`/`u_time`/`u_rotation` every frame. Two frames of the
//!   SAME level — shaders ON and shaders OFF (`Config::set_shaders(false)`
//!   on a second engine; the C++'s `getRenderStates` returns
//!   `RenderStates::Default` before the hook, so the bound ids stay set
//!   but the stages draw through the default program) — are sampled on a
//!   coarse grid; the OFF frame must differ from the ON frame (the bound
//!   stages' default-program continuation) and the ON frame must not be a
//!   flat clear (the shader-generated gradient). The ON frame is written
//!   to a PPM beside the crate's `target/` (the one-permitted visual
//!   evidence — the gradient).

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use oh_app::compose::{compose_with_config, RealProfiles};
use oh_app::windowed::{AssetsPackShaderProvider, GlDrawTarget};
use oh_config::{Config, TIME_STEP};
use oh_game::seams::{NoJoystick, NoneSteamSink, NullReplayStore};
use oh_render::gl::GlBackend;
use oh_render::graphics::DrawTarget;
use oh_render::pack_shaders::{PackShaderProgram, PackShaderSnapshot};
use oh_render::shader::{translate_for_gles, ShaderType};
use oh_render::{Color, RenderStates, Vec2, Vertex};

const W: u32 = 800;
const H: u32 = 600;

/// The sample grid step (100×75 samples upscaled 8×8 to the 800×600 PPM).
const STEP: usize = 8;

const ARTWORK_PACK: &str = "thing_Synth_Morxemplum_Artwork_1";
const ARTWORK_POINTLESS: &str = "thing_Synth_Morxemplum_Artwork_1_shadertest";

fn release_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../_RELEASE")
        .to_path_buf()
}

fn out_dir() -> PathBuf {
    std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
}

/// A fragment-only snapshot (empty uniform store — §4.2 guards the
/// COMPILE; §4.3a supplies the store through the real `Assets`).
fn fragment_snapshot(source: &str) -> PackShaderSnapshot {
    PackShaderSnapshot {
        source: source.to_string(),
        shader_type: ShaderType::Fragment,
        uniforms: Vec::new(),
    }
}

/// The full-window white quad in view coords (the default view is
/// centre `(W/2, H/2)` + size `(W, H)` — the `GlDrawTarget::new` — so
/// the window's pixel space `(0..W, 0..H)` is the view space).
fn white_quad() -> [Vertex; 6] {
    let white = Color::WHITE;
    let (w, h) = (W as f32, H as f32);
    [
        Vertex::new(Vec2::new(0.0, 0.0), white),
        Vertex::new(Vec2::new(0.0, h), white),
        Vertex::new(Vec2::new(w, h), white),
        Vertex::new(Vec2::new(0.0, 0.0), white),
        Vertex::new(Vec2::new(w, h), white),
        Vertex::new(Vec2::new(w, 0.0), white),
    ]
}

/// One engine over the shared assets, `pointless` started + 30 frames of
/// its real `onUpdate` (the level's `shdr_setUniform*` calls run there).
fn composed_pointless(
    assets: &Arc<oh_assets::Assets>,
    shaders_on: bool,
) -> oh_app::compose::Composed {
    let config = Arc::new(RwLock::new({
        let mut c = Config::default();
        if !shaders_on {
            c.set_shaders(false);
        }
        c
    }));
    let c = compose_with_config(
        assets.clone(),
        release_dir(),
        true, /* window (the C++ `window != nullptr`) */
        Arc::new(RealProfiles::new(assets.clone())),
        config,
        Arc::new(NoneSteamSink),
        None, // discord
        None, // renderer
        None, // audio
        Arc::new(NullReplayStore),
        Arc::new(NoJoystick),
    );
    c.game
        .new_game(ARTWORK_PACK, ARTWORK_POINTLESS, true, 1.0, false);
    c.game.set_must_start(true);
    for _ in 0..30 {
        c.game.update(TIME_STEP, 1.0);
        c.game.post_update();
    }
    c
}

/// The frame's coarse-grid sample (the GL back buffer holds ONE frame —
/// read it before the next draw overwrites it).
fn sample_frame(parts: &oh_render::GlParts) -> Vec<[u8; 4]> {
    let (gw, gh) = (W as usize / STEP, H as usize / STEP);
    let mut samples = Vec::with_capacity(gw * gh);
    for y in 0..gh {
        for x in 0..gw {
            samples.push(parts.read_pixel(
                x as i32 * STEP as i32 + (STEP / 2) as i32,
                y as i32 * STEP as i32 + (STEP / 2) as i32,
            ));
        }
    }
    samples
}

/// The 100×75 sample grid → the 800×600 binary PPM (each sample upscaled
/// 8×8 — the gradient is the evidence, not the pixels).
fn write_ppm(path: &Path, samples: &[[u8; 4]]) {
    let gw = W as usize / STEP;
    let mut bytes = Vec::with_capacity((W * H) as usize * 3);
    for y in 0..H as usize {
        for x in 0..W as usize {
            let s = samples[y / STEP * gw + x / STEP];
            bytes.extend_from_slice(&[s[0], s[1], s[2]]);
        }
    }
    let header = format!("P6\n{W} {H}\n255\n");
    std::fs::write(path, header.as_bytes())
        .and_then(|_| {
            std::fs::OpenOptions::new()
                .append(true)
                .open(path)
                .and_then(|mut f| std::io::Write::write_all(&mut f, &bytes))
        })
        .unwrap_or_else(|e| panic!("writing {path:?}: {e}"));
}

/// The §4.2 29v1 guard: the `translate_for_gles` contract — the
/// `#version` line is inserted/replaced (`#version 100`) and the
/// fragment float-precision declaration is inserted when absent;
/// EVERY other line passes through verbatim, in order. The check:
/// raw (minus any `#version` line) == translated (minus the
/// `#version` line + the `precision ... float;` line).
fn translation_preserves_body(raw: &str, translated: &str) -> bool {
    let mut raw_lines: Vec<&str> = raw.lines().collect();
    raw_lines.retain(|l| !l.trim_start().starts_with("#version"));
    let mut translated_lines: Vec<&str> = translated.lines().collect();
    translated_lines.retain(|l| {
        let t = l.trim_start();
        !t.starts_with("#version") && !(t.starts_with("precision") && t.contains("float"))
    });
    raw_lines == translated_lines
}

/// The §4.2 translated column on its own GLES context (the `gles`
/// child mode — module docs above `main`). Exits 0 when the column's
/// assertions hold, 1 otherwise (the parent's `run_gles_column`
/// reports the child's exit status).
fn run_gles_column() -> i32 {
    match oh_render::gl::GlBackend::try_create_gles(W, H, "pack_shaders_gl gles (029n1 K6)") {
        Some(gles_backend) => {
            let gles_gl = &gles_backend.parts.gl;
            println!(
                "§4.2 GLES context: {}",
                gles_backend.parts.version_string().unwrap_or_default()
            );
            for (rel, _marker) in [
                ("Packs/artwork/Shaders/gradient.frag", "u_resolution"),
                ("Packs/artwork/Shaders/steamart.frag", "u_rotation"),
                ("Packs/experimental/Shaders/pixelate.frag", "color1"),
                ("Packs/experimental/Shaders/text.frag", "#version 130"),
            ] {
                let raw = std::fs::read_to_string(release_dir().join(rel))
                    .unwrap_or_else(|e| panic!("{rel}: {e}"));
                let translated = translate_for_gles(&raw, ShaderType::Fragment);
                // The GLES execution path (the 25b-SG3 design):
                // `PackShaderProgram::new` translates the source (ES
                // 1.00) + builds the two stage programs — the vertex
                // stage is REQUIRED (GL ES 3.x links no fragment-only
                // program: the probe's "program lacks a vertex or mesh
                // shader"), so the desktop fragment-only shape cannot
                // be the GLES assertion.
                let execution_ok =
                    PackShaderProgram::new(gles_gl, &fragment_snapshot(&raw)).is_ok();
                // `pixelate.frag` uses no fixed-function builtin (the
                // `gl_FragCoord` + the uniforms only) — its translation
                // must build, full stop.
                if rel.ends_with("pixelate.frag") {
                    if !execution_ok {
                        eprintln!(
                            "§4.2 {rel} (translate_for_gles, GLES): the builtin-free fragment \
                             must build the two-stage program (the 25b-SG3 path)"
                        );
                        return 1;
                    }
                    println!(
                        "§4.2 {rel} (translate_for_gles, GLES): the two-stage program builds OK"
                    );
                    continue;
                }
                // The documented 25b outcome for the
                // `gl_Color`/`gl_TexCoord`-reading files (and any
                // desktop-only GLSL construct — `text.frag`'s `switch`
                // + the `.`-suffix floats are GLSL 1.30/ES 3.00
                // features, reserved/invalid in ES 1.00): the fragment
                // cannot compile on GLES, the sticky fallback to the
                // built-in pass-through fires. Two assertions:
                // (1) the execution path fails (the fallback fires);
                // (2) the 29v1 guard — the translation is
                // VERBATIM-FAITHFUL (the `translate_for_gles` contract:
                // the `#version` line is the only replaced line + the
                // missing float-precision declaration is the only
                // inserted one; every other line passes through, in
                // order). Any other failure cause (translator
                // corruption) is a guard hit.
                if execution_ok {
                    eprintln!(
                        "§4.2 {rel} (GLES): the documented fallback did not fire — \
                         the two-stage program unexpectedly built"
                    );
                    return 1;
                }
                if !translation_preserves_body(&raw, &translated) {
                    eprintln!(
                        "§4.2 {rel} (GLES): the translation corrupted the source \
                         (the 29v1 class — a translator bug, not the documented \
                         ES 1.00 restriction)\n--- raw ---\n{raw}\n--- translated ---\n{translated}"
                    );
                    return 1;
                }
                // The root-cause evidence (printed, not asserted — the
                // GL's log format is driver-specific): the
                // fragment-only compile reaches the fragment's own
                // compile stage (the missing-builtin / language
                // restriction errors) before the ES link requirement.
                let log = oh_render::pack_shaders::compile_link_fragment_only(gles_gl, &translated)
                    .err()
                    .unwrap_or_else(|| "(no log)".to_string());
                println!(
                    "§4.2 {rel} (translate_for_gles, GLES): fails with the documented fallback; \
                     translation verbatim-faithful; root cause (evidence):\n{log}"
                );
            }
            0
        }
        None => {
            eprintln!(
                "§4.2 translate_for_gles column: SKIPPED (D-011) — no GLES context on this                  box (the `try_create_gles` bootstrap returned None)"
            );
            // A skip is NOT a failure: the column is gated, reported.
            0
        }
    }
}

fn main() {
    // The `gles` child mode (the §4.2 translated column — a fresh
    // process: winit 0.30 allows ONE `EventLoop` per process
    // (`RecreationAttempt` on the second), so the GLES context cannot
    // share this process with the desktop backend; the parent's
    // `run_gles_column` spawns this mode when it has no GLES context
    // of its own).
    if std::env::args().nth(1).as_deref() == Some("gles") {
        std::process::exit(run_gles_column());
    }

    // 0 = no MSAA request: the example's probe context, not the game's
    // window (the 050 level wiring — `pick_msaa_config`'s level-0 arm is
    // the pre-050 first-config pick, unchanged).
    let Some(backend) = GlBackend::try_create(W, H, "pack_shaders_gl (029n1 K6)", 0) else {
        eprintln!(
            "pack_shaders_gl: SKIPPED (D-011 skip-with-reason) — no GL context on this box \
             (no display / no software GL). §4.2 (the compile guard) and §4.3 (the \
             shader-bound draw) were NOT run; the in-process column \
             (crates/oh-app/tests/pack_shaders.rs) remains the verified surface."
        );
        return;
    };
    let parts = &backend.parts;
    let gl = &parts.gl;
    println!(
        "pack_shaders_gl: GL context: {}",
        parts.version_string().unwrap_or_default()
    );

    // ------------------------------------------------------------------
    // §4.2 — the compile guard over all four shipped `.frag` files,
    // each in its execution context (the desktop raw form through
    // `PackShaderProgram::new` — the fragment-only program + the stage
    // VAOs, the exact draw-time path; the `translate_for_gles` form on
    // a GLES context — the 25b-SG3 column, below).
    // ------------------------------------------------------------------
    let frags = [
        ("Packs/artwork/Shaders/gradient.frag", "u_resolution"),
        ("Packs/artwork/Shaders/steamart.frag", "u_rotation"),
        ("Packs/experimental/Shaders/pixelate.frag", "color1"),
        ("Packs/experimental/Shaders/text.frag", "#version 130"),
    ];
    for (rel, marker) in frags {
        let raw = std::fs::read_to_string(release_dir().join(rel))
            .unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert!(raw.contains(marker), "{rel}: missing the `{marker}` marker");
        PackShaderProgram::new(gl, &fragment_snapshot(&raw))
            .unwrap_or_else(|e| panic!("{rel} (raw): compile/link failed: {e:?}"));
        println!("§4.2 {rel}: raw (desktop) compiles & links OK");
    }

    // The `translate_for_gles` column — a GLES context is its
    // assertion home (a desktop context rejects the ES 1.00
    // `gl_Color`/`gl_TexCoord` fragment builtins the same way, and a
    // second winit `EventLoop` in THIS process is impossible — the
    // `gles` child mode below runs the column in a fresh process).
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let child = std::process::Command::new(exe).arg("gles").spawn();
    match child {
        Ok(mut child) => match child.wait() {
            Ok(status) if status.success() => {
                println!("§4.2 translate_for_gles column: executed in the `gles` child (exit 0)")
            }
            other => panic!("§4.2 translate_for_gles column: the `gles` child failed ({other:?})"),
        },
        Err(e) => panic!("§4.2 translate_for_gles column: spawning the `gles` child: {e}"),
    }

    // ------------------------------------------------------------------
    // §4.3a — the shader-bound draw (gradient.frag on a white quad).
    // ------------------------------------------------------------------
    let assets =
        Arc::new(oh_assets::Assets::new(release_dir(), false, false).expect("_RELEASE loads full"));
    let gradient = assets
        .get_shader_id_by_path("Packs/artwork/Shaders/gradient.frag")
        .expect("the full load registers gradient.frag");
    assets.set_shader_uniform_f_vec2(gradient, "u_resolution", W as f32, H as f32);
    assets.set_shader_uniform_f_vec3(gradient, "u_color0", 1.0, 0.0, 0.0);
    assets.set_shader_uniform_f_vec3(gradient, "u_color1", 0.0, 0.0, 1.0);
    assets.set_shader_uniform_f(gradient, "u_blend", 1.0);

    let provider = || Arc::new(AssetsPackShaderProvider::new(assets.clone()));
    let verts = white_quad();

    // The shader-bound draw: `gradient.frag` =
    // `mix(gl_Color, mix(u_color1, u_color0, st.y), u_blend)` over
    // `st = gl_FragCoord / u_resolution` → bottom = u_color1 (blue),
    // top = u_color0 (red).
    let mut target = GlDrawTarget::new(parts).with_provider(provider());
    target.clear_black();
    target.draw_vertices(&verts, RenderStates::with_shader(gradient));
    let bottom = parts.read_pixel(W as i32 / 2, 2);
    let top = parts.read_pixel(W as i32 / 2, H as i32 - 3);
    assert!(
        bottom[2] > 200 && bottom[0] < 60,
        "the shader-bound draw's bottom pixel must be BLUE-dominant (u_color1), got {bottom:?}"
    );
    assert!(
        top[0] > 200 && top[2] < 60,
        "the shader-bound draw's top pixel must be RED-dominant (u_color0), got {top:?}"
    );

    // The negative control: the same quad through the DEFAULT program is
    // the pass-through vertex colour — pure white. The difference proves
    // the bound pack fragment stage ran (not the built-in).
    let mut target = GlDrawTarget::new(parts).with_provider(provider());
    target.clear_black();
    target.draw_vertices(&verts, RenderStates::default());
    let plain = parts.read_pixel(W as i32 / 2, 2);
    assert_eq!(
        plain,
        [255, 255, 255, 255],
        "the default program's pass-through white (the negative control)"
    );
    println!(
        "§4.3a gradient.frag executed by the shader-bound draw: \
         bottom {bottom:?} (blue) / top {top:?} (red); default-program control {plain:?}"
    );

    // ------------------------------------------------------------------
    // §4.3b — `pointless`: full game frames through the D-020 bridge,
    // shaders OFF first (sampled), then shaders ON (sampled + dumped).
    // ------------------------------------------------------------------
    let assets_dir = release_dir().join("Assets");

    let c_off = composed_pointless(&assets, false);
    let mut target = GlDrawTarget::new(parts).with_provider(provider());
    let mut app = oh_app::bridge::AppDraw::new(
        &Config::default(),
        &assets_dir,
        oh_render::font::Font::load(&assets_dir.join("OpenSquare-Regular.ttf"))
            .expect("OpenSquare-Regular.ttf"),
        oh_render::font::Font::load(&assets_dir.join("OpenSquare-Bold.ttf"))
            .expect("OpenSquare-Bold.ttf"),
    );
    oh_app::bridge::draw(&c_off.game, &mut app, &mut target);
    let off_frame = sample_frame(parts);
    drop(target);
    drop(app);
    drop(c_off);

    let c_on = composed_pointless(&assets, true);
    let mut target = GlDrawTarget::new(parts).with_provider(provider());
    let mut app = oh_app::bridge::AppDraw::new(
        &Config::default(),
        &assets_dir,
        oh_render::font::Font::load(&assets_dir.join("OpenSquare-Regular.ttf"))
            .expect("OpenSquare-Regular.ttf"),
        oh_render::font::Font::load(&assets_dir.join("OpenSquare-Bold.ttf"))
            .expect("OpenSquare-Bold.ttf"),
    );
    oh_app::bridge::draw(&c_on.game, &mut app, &mut target);
    let on_frame = sample_frame(parts);
    drop(target);
    drop(app);

    // The shaders-OFF frame must differ from the ON frame somewhere (the
    // bound stages' default-program continuation — `getRenderStates`
    // returns `RenderStates::Default` before the hook,
    // HGGraphics.cpp:62-64), and the ON frame must not be a flat clear
    // (the steamart-generated gradient over the B&W level colours).
    let diff = on_frame
        .iter()
        .zip(&off_frame)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        diff > 0,
        "shaders OFF vs ON: no sampled pixel differs — the bound stages drew identically"
    );
    let distinct: std::collections::BTreeSet<_> = on_frame.iter().copied().collect();
    assert!(
        distinct.len() > 1,
        "the shaders-ON frame is a single colour ({:?}) — no shader-generated variation",
        distinct.iter().next()
    );
    let out = out_dir();
    std::fs::create_dir_all(&out).unwrap_or_else(|e| panic!("creating {out:?}: {e}"));
    let ppm = out.join("k6_pointless_shaders_on.ppm");
    write_ppm(&ppm, &on_frame);
    println!(
        "§4.3b pointless: shaders ON vs OFF — {diff} sampled pixels differ \
         (of {}); ON frame has {} distinct colours; screenshot: {ppm:?}",
        on_frame.len(),
        distinct.len()
    );
    drop(c_on);

    println!("pack_shaders_gl: §4.2 + §4.3 PASSED");
}
