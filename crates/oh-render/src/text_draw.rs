//! Text drawing + per-frame text-content update — the `HexagonGame` text
//! pipeline (`HGGraphics.cpp` mapping §3.9–3.12): `updateText`,
//! `drawText_TimeAndStatus`, `drawText_Message`, `drawText_PersonalBest`,
//! `drawText`, plus the exact sizing/formatting helpers.
//!
//! Layout accessors (`getGlobalNW` etc.) come from
//! [`crate::types::FloatRect`] corner methods (mapping §10.1).
//!
//! String-formatting note (D-000): C++ streams use `std::to_string`
//! (6 decimal places) for `formatTime` / FPS and default `<< float`
//! (up to 6 significant digits) for the timescale; the Rust equivalents
//! below reproduce those for the game's value ranges and are documented
//! at each site.

use oh_config::Config;
use oh_math::hg::color::Color;
use oh_math::hg::string::to_uppercase;
use oh_math::ssvs::vector2::Vec2;
use oh_math::ssvu::math::PI;

use crate::draw::RenderStates;
use crate::font::Text;
use crate::graphics::{DrawState, DrawTarget};

/// C++ file-static `formatTime` (mapping §3.1):
/// `ssvu::toStr(std::floor(x * 1000) / 1000.f)` — `std::to_string(double)`
/// keeps exactly 6 decimal digits (trailing zeros included).
#[must_use]
pub fn format_time(x: f64) -> String {
    format!("{:.6}", (x * 1000.0).floor() / 1000.0)
}

/// C++ `getScaledCharacterSize` (mapping §3.9):
/// `unsigned(size / zoomFactor * textScaling)` — truncated.
#[must_use]
pub fn get_scaled_character_size(size: f32, zoom_factor: f32, text_scaling: f32) -> u32 {
    (size / zoom_factor * text_scaling) as u32
}

/// C++ `HexagonGame::diffFormat` (`snprintf "%g", diff` — 6 significant
/// digits; exponent form when the exponent is `< -4` or `>= 6`). Rust has no
/// `%g`, so the C formatting rules are emulated on the f64 promotion (C++
/// `float` arguments are promoted to `double` for `printf` variadics):
/// fixed 6-significant-digit `%e`, trailing-zero strip, then the exponent
/// decision.
#[must_use]
pub fn diff_format(diff: f32) -> String {
    let v = diff as f64;
    if v == 0.0 {
        return "0".to_string();
    }

    // 6 significant digits: mantissa with 5 decimals.
    let s = format!("{:.5e}", v);
    let (mant, exp) = s.split_once('e').expect("%e format");
    let exp: i32 = exp.parse().expect("exponent");
    let sign = if v < 0.0 { "-" } else { "" };
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();

    if !(-4..6).contains(&exp) {
        // C `%g` exponent form: "d[.dddd]e±dd" (exponent >= 2 digits).
        let mut m = digits;
        while m.len() > 1 && m.ends_with('0') {
            m.pop();
        }
        // "d.ddd..." — `str::strip_prefix(Predicate)` removes at most ONE
        // character, so trim the fraction's leading zeros with a matcher.
        let (digits_part, frac_part) = m.split_at(1);
        let frac = frac_part.trim_start_matches('0');
        let m = if frac.is_empty() {
            digits_part.to_string()
        } else {
            format!("{digits_part}.{frac}")
        };
        return format!("{sign}{m}e{exp:+03}");
    }

    if exp >= 0 {
        if (exp + 1) as usize <= digits.len() {
            let (i, f) = digits.split_at((exp + 1) as usize);
            // `strip_suffix(Predicate)` removes at most one char — use the
            // char-matcher form for the whole trailing-zero run.
            let f = f.trim_end_matches('0');
            if f.is_empty() {
                format!("{sign}{i}")
            } else {
                format!("{sign}{i}.{f}")
            }
        } else {
            format!(
                "{sign}{digits}{}",
                "0".repeat((exp + 1) as usize - digits.len())
            )
        }
    } else {
        // exp in [-4, -1]: 0.00d... — the zeros after the decimal point
        // number `-exp - 1` (1e-4 = 0.0001 → 3 zeros), then the 6
        // significant digits, then trailing-zero strip.
        let mut f = format!("{}{}", "0".repeat((-exp - 1) as usize), digits);
        while f.ends_with('0') {
            f.pop();
        }
        format!("{sign}0.{f}")
    }
}

/// C++ `HexagonGame::timeFormat` (`snprintf "%.3f"`).
#[must_use]
pub fn time_format(time: f32) -> String {
    format!("{:.3}", time)
}

/// Inputs for [`update_text`] — the status/level/lua values the C++
/// method reads from `HexagonGame` members.
pub struct TextUpdateInput<'a> {
    /// `mFT` (fract of the frame time, ssvu::FT).
    pub ft: f32,
    /// `debugPause`.
    pub debug_pause: bool,
    /// `levelStatus.tutorialMode`.
    pub tutorial_mode: bool,
    /// `cwManager.count()` / `cwManager.maxHandles()`.
    pub custom_wall_count: usize,
    pub custom_wall_max_handles: usize,
    /// `status.started`.
    pub started: bool,
    /// `levelStatus.swapEnabled`.
    pub swap_enabled: bool,
    /// `status.scoreInvalid` + `status.invalidReason`.
    pub score_invalid: bool,
    pub invalid_reason: &'a str,
    /// `status.hasDied` + `status.restartInput` / `status.replayInput`.
    pub has_died: bool,
    pub restart_input: &'a str,
    pub replay_input: &'a str,
    /// `calledDeprecatedFunctions.size()`.
    pub called_deprecated_functions: usize,
    /// `levelStatus.trackedVariables` as `(lua variable name, display)`
    /// pairs (C++ `map<variableName, display>`).
    pub tracked_variables: &'a [(String, String)],
    /// `lua.doesVariableExist(name)` → `lua.readVariable<std::string>(name)`;
    /// `None` when the function is not available or the variable is absent
    /// (C++ throws / `continue`s — see the tracked-variable loop).
    #[allow(clippy::type_complexity)] // mirrors the C++ `std::optional<std::function<...>>`
    pub lua_variable: Option<&'a dyn Fn(&str) -> Option<String>>,
    /// `status.getTimeSeconds()` (double in C++ → f64).
    pub time_seconds: f64,
    /// `levelStatus.scoreOverridden` + `levelStatus.scoreOverride` (the Lua
    /// variable holding the alternate score string).
    pub score_overridden: bool,
    pub score_override: &'a str,
    /// `window->getFPS()`.
    pub fps: f32,
    /// `mustShowReplayUI()`.
    pub must_show_replay_ui: bool,
    /// `activeReplay->replayFile`: `played_seconds()`, `_played_score`,
    /// `_player_name`.
    pub replay_played_seconds: f64,
    pub replay_played_score: f64,
    pub replay_player_name: &'a str,
}

/// C++ `HexagonGame::updateText(mFT)` (mapping §3.9) — verbatim control
/// flow: `pbTextGrowth` sawtooth, the status-text string build, the timer /
/// FPS / message / PB / replay text content, then the per-frame
/// character-size / origin half ([`update_text_sizes`]).
///
/// Runs every frame (even during `debugPause`); the C++ guards on
/// `window != nullptr` (the caller only invokes this with a live draw
/// state).
///
/// In the windowed composition the engine's per-tick `update_text`
/// (the C++'s `updateText` call site, `HGUpdate.cpp:384`) owns the
/// string-content half + the growth increment, and the bridge applies
/// [`update_text_sizes`] per frame from the copied values (a pure
/// function of them — observationally identical to the C++'s same-call
/// application). This full port is the C++ method itself, host-tested
/// below.
pub fn update_text(state: &mut DrawState, config: &Config, input: &TextUpdateInput) {
    // --------------------------------------------------------------------
    // "personal best" text animation.
    state.pb_text_growth += 0.08 * input.ft;
    if state.pb_text_growth > PI * 2.0 {
        state.pb_text_growth = 0.0;
    }

    // --------------------------------------------------------------------
    let mut os = String::new();

    if input.debug_pause {
        os.push_str("(!) PAUSED (!)\n");
    }

    if input.tutorial_mode {
        os.push_str("TUTORIAL MODE\n");
    } else if config.get_official() {
        os.push_str("OFFICIAL MODE\n");
    }

    if config.get_debug() {
        os.push_str("DEBUG MODE\n");

        os.push_str(&format!(
            "CUSTOM WALLS: {} / {}\n",
            input.custom_wall_count, input.custom_wall_max_handles
        ));
    }

    if input.started {
        if input.swap_enabled {
            os.push_str("SWAP ENABLED\n");
        }

        if config.get_invincible() {
            os.push_str("INVINCIBILITY ON\n");
        }

        let timescale = config.get_timescale();
        if timescale != 1.0 {
            // C++ `os << "TIMESCALE " << timescale << '\n'` — default float
            // streaming (≤ 6 significant digits); Rust's f32 Display is the
            // shortest round-trip form and matches for the game's timescale
            // values.
            os.push_str(&format!("TIMESCALE {}\n", timescale));
        }

        if input.score_invalid {
            os.push_str(&format!("SCORE INVALIDATED ({})\n", input.invalid_reason));
        }

        if input.has_died {
            os.push_str(input.restart_input);
            os.push_str(input.replay_input);
        }

        if input.called_deprecated_functions > 1 {
            os.push_str(&format!(
                "{} WARNINGS RAISED (CHECK CONSOLE)\n",
                input.called_deprecated_functions
            ));
        } else if input.called_deprecated_functions > 0 {
            os.push_str("1 WARNING RAISED (CHECK CONSOLE)\n");
        }

        if config.get_show_tracked_variables() && !input.tracked_variables.is_empty() {
            os.push('\n');
            for (variable_name, display) in input.tracked_variables {
                let Some(value) = input.lua_variable.and_then(|read| read(variable_name)) else {
                    continue;
                };

                os.push_str(&format!(
                    "{}: {}\n",
                    to_uppercase(display.clone()),
                    to_uppercase(value)
                ));
            }
        }
    } else if config.get_rotate_to_start() {
        os.push_str("ROTATE TO START\n");
        state.message_text.set_string("ROTATE TO START");
    }

    // Set in-game timer text.
    if !input.score_overridden {
        // By default, use the timer for scoring.
        state.time_text.set_string(if input.started {
            format_time(input.time_seconds)
        } else {
            "0".to_string()
        });
    } else {
        // Alternative scoring — the Lua variable holds the string.
        let value = input
            .lua_variable
            .and_then(|read| read(input.score_override))
            .unwrap_or_default();
        state.time_text.set_string(value);
    }

    // Set information text.
    state.text.set_string(os);

    // Set FPS text, if option is enabled.
    if config.get_show_fps() {
        // C++ `ssvu::toStr(window->getFPS())` = `std::to_string` (6 decimals).
        state
            .fps_text
            .set_string(format!("{:.6}", input.fps as f64));
    }

    // The per-frame character-size / origin half (HGGraphics.cpp:545-571,
    // the replay-text size :593 — the replay STRING below keeps the C++'
    // branch position; setting it before or after the size is
    // observationally identical: the replay text's origin/position are the
    // draw's, and its width needs neither).
    update_text_sizes(state, config, input.must_show_replay_ui);

    // --------------------------------------------------------------------
    if input.must_show_replay_ui {
        let mut replay = String::new();

        if !input.score_overridden {
            replay.push_str(&format!("{}s", format_time(input.replay_played_seconds)));
        } else {
            replay.push_str(&format_time(input.replay_played_score));
        }

        replay.push_str(&format!(" BY {}", input.replay_player_name));

        state.replay_text.set_string(replay);
    } else {
        state.replay_text.set_string("");
    }
}

/// C++ `updateText`'s per-frame character-size / origin half
/// (HGGraphics.cpp:545-571 + the replay-text size at :593 — inside the
/// `mustShowReplayUI()` branch): the `getScaledCharacterSize` closure
/// (`size / zoomFactor * textScaling`, truncated to unsigned) applied to
/// `timeText` 70, `text` 20 + origin `(0, 0)`, `fpsText` 20 (the
/// `Config::getShowFPS()` gate), `messageText` 32 + origin
/// `getGlobalWidth/2`, `pbText` `64 + sin(pbTextGrowth) * 10` + origin
/// `getGlobalWidth/2` (the unsigned base widened to float, the sum
/// truncated back), and `replayText` 16 (the `mustShowReplayUI()` gate).
///
/// Pure in its inputs: it writes NO strings and does NOT advance
/// `state.pb_text_growth` (the C++'s increment lives in the engine's
/// per-tick `update_text` — the port's split of the C++'s single
/// `updateText` call, `HGUpdate.cpp:384`; the bridge copies the engine's
/// strings + growth into the state each frame, then applies this half —
/// the C++ applies it in the same call, a pure function of the same
/// values, so the rendered frame is identical).
pub fn update_text_sizes(state: &mut DrawState, config: &Config, must_show_replay_ui: bool) {
    let scaled = |size: f32| -> u32 {
        get_scaled_character_size(size, config.get_zoom_factor(), config.get_text_scaling())
    };

    // C++ `timeText.setCharacterSize(getScaledCharacterSize(70.f))`
    // (HGGraphics.cpp:552).
    state.time_text.set_character_size(scaled(70.0));

    // C++ `text.setCharacterSize(getScaledCharacterSize(20.f));
    // text.setOrigin({0.f, 0.f})` (HGGraphics.cpp:556-557).
    state.text.set_character_size(scaled(20.0));
    state.text.set_origin(Vec2::new(0.0, 0.0));

    // C++ `fpsText.setCharacterSize(getScaledCharacterSize(20.f))` — the
    // `Config::getShowFPS()` gate (HGGraphics.cpp:560-564).
    if config.get_show_fps() {
        state.fps_text.set_character_size(scaled(20.0));
    }

    // C++ `messageText.setCharacterSize(getScaledCharacterSize(32.f));
    // messageText.setOrigin({getGlobalWidth(messageText) / 2.f, 0.f})`
    // (HGGraphics.cpp:566-567).
    state.message_text.set_character_size(scaled(32.0));
    state.message_text.set_origin(Vec2::new(
        state.message_text.get_global_bounds().get_width() / 2.0,
        0.0,
    ));

    // C++ `const float growth = std::sin(pbTextGrowth);
    // pbText.setCharacterSize(getScaledCharacterSize(64.f) + growth * 10.f);
    // pbText.setOrigin({getGlobalWidth(pbText) / 2.f, 0.f})`
    // (HGGraphics.cpp:569-571) — `state.pb_text_growth` is the engine's
    // per-tick sawtooth (the bridge's per-frame copy).
    let growth = state.pb_text_growth.sin();
    state
        .pb_text
        .set_character_size((scaled(64.0) as f32 + growth * 10.0) as u32);
    state.pb_text.set_origin(Vec2::new(
        state.pb_text.get_global_bounds().get_width() / 2.0,
        0.0,
    ));

    // C++ `replayText.setCharacterSize(getScaledCharacterSize(16.f))` —
    // inside the `mustShowReplayUI()` branch (HGGraphics.cpp:593).
    if must_show_replay_ui {
        state.replay_text.set_character_size(scaled(16.0));
    }
}

/// C++ `drawText_TimeAndStatus` (mapping §3.10).
pub fn draw_text_time_and_status(
    state: &mut DrawState,
    config: &Config,
    color_text: Color,
    offset_color: Color,
    must_show_replay_ui: bool,
    states: RenderStates,
    target: &mut dyn DrawTarget,
) {
    if config.get_draw_text_outlines() {
        state.time_text.set_outline_color(offset_color);
        state.text.set_outline_color(offset_color);
        state.fps_text.set_outline_color(offset_color);
        state.replay_text.set_outline_color(offset_color);

        state.time_text.set_outline_thickness(2.0);
        state.text.set_outline_thickness(1.0);
        state.fps_text.set_outline_thickness(1.0);
        state.replay_text.set_outline_thickness(1.0);
    } else {
        state.time_text.set_outline_thickness(0.0);
        state.text.set_outline_thickness(0.0);
        state.fps_text.set_outline_thickness(0.0);
        state.replay_text.set_outline_thickness(0.0);
    }

    let padding =
        (config.get_text_padding() * config.get_text_scaling()) / config.get_zoom_factor();

    if config.get_show_timer() {
        state.time_text.set_fill_color(color_text);
        state
            .time_text
            .set_origin(state.time_text.get_local_bounds().get_nw());
        state.time_text.set_position(Vec2::new(padding, padding));

        target.draw_text(&state.time_text, states);
    }

    if config.get_show_status_text() {
        state.text.set_fill_color(color_text);
        state
            .text
            .set_origin(state.text.get_local_bounds().get_nw());
        // C++: `getGlobalBottom(timeText)` — the timer's CURRENT global
        // bounds (position/origin as set above, or from the last frame when
        // the timer is hidden).
        let time_bottom = state.time_text.get_global_bounds().get_bottom();
        state
            .text
            .set_position(Vec2::new(padding, time_bottom + padding));

        target.draw_text(&state.text, states);
    }

    if config.get_show_fps() {
        state.fps_text.set_fill_color(color_text);
        state
            .fps_text
            .set_origin(state.fps_text.get_local_bounds().get_sw());

        if config.get_show_level_info() || must_show_replay_ui {
            state.fps_text.set_position(Vec2::new(
                padding,
                state.level_info_rectangle.get_global_bounds().get_top() - padding,
            ));
        } else {
            state
                .fps_text
                .set_position(Vec2::new(padding, config.get_height() as f32 - padding));
        }

        target.draw_text(&state.fps_text, states);
    }

    if must_show_replay_ui {
        let scaling = config.get_key_icons_scale() / config.get_zoom_factor();
        let replay_padding = 8.0 * scaling;

        state.replay_text.set_fill_color(color_text);
        state
            .replay_text
            .set_origin(state.replay_text.get_local_bounds().get_center_e());
        // C++: `getGlobalCenterW(replayIcon) - {replayPadding, 0}`.
        let center_w = state.replay_icon.get_global_bounds().get_center_w();
        state
            .replay_text
            .set_position(Vec2::new(center_w.x - replay_padding, center_w.y));
        target.draw_text(&state.replay_text, states);
    }
}

/// C++ `drawTextMessagePBImpl` (mapping §3.11) — the shared message/PB path:
/// skip when empty, outline on/off (exact thickness), position, fill, draw.
fn draw_text_message_pb_impl<R>(
    text: &mut Text,
    offset_color: Color,
    pos: Vec2,
    color: Color,
    outline_thickness: f32,
    draw_outlines: bool,
    render: R,
) where
    R: FnOnce(&Text),
{
    if text.is_empty() {
        return;
    }

    if draw_outlines {
        text.set_outline_color(offset_color);
        text.set_outline_thickness(outline_thickness);
    } else {
        text.set_outline_thickness(0.0);
    }

    text.set_position(pos);
    text.set_fill_color(color);

    render(text);
}

/// C++ `drawText_Message` (mapping §3.11) — at `(width/2, height/5.5)`,
/// outline thickness `1.f`.
pub fn draw_text_message(
    state: &mut DrawState,
    config: &Config,
    color_text: Color,
    offset_color: Color,
    states: RenderStates,
    target: &mut dyn DrawTarget,
) {
    draw_text_message_pb_impl(
        &mut state.message_text,
        offset_color,
        Vec2::new(
            config.get_width() as f32 / 2.0,
            config.get_height() as f32 / 5.5,
        ),
        color_text,
        1.0, /* outlineThickness */
        config.get_draw_text_outlines(),
        |t| target.draw_text(t, states),
    );
}

/// C++ `drawText_PersonalBest` (mapping §3.11) — at
/// `(width/2, height - height/4)`, outline thickness `4.f`.
pub fn draw_text_personal_best(
    state: &mut DrawState,
    config: &Config,
    color_text: Color,
    offset_color: Color,
    states: RenderStates,
    target: &mut dyn DrawTarget,
) {
    draw_text_message_pb_impl(
        &mut state.pb_text,
        offset_color,
        Vec2::new(
            config.get_width() as f32 / 2.0,
            config.get_height() as f32 - config.get_height() as f32 / 4.0,
        ),
        color_text,
        4.0, /* outlineThickness */
        config.get_draw_text_outlines(),
        |t| target.draw_text(t, states),
    );
}

/// C++ `drawText` (mapping §3.12) — computes the offset colour
/// (`Black` in B&W / no colours, else `getColor(0)`) and draws
/// TimeAndStatus → Message → PersonalBest in that order.
pub fn draw_text(
    state: &mut DrawState,
    config: &Config,
    background_colors: &[Color],
    color_text: Color,
    must_show_replay_ui: bool,
    states: RenderStates,
    target: &mut dyn DrawTarget,
) {
    let offset_color = if config.get_black_and_white() || background_colors.is_empty() {
        Color::BLACK
    } else {
        // C++ `getColor(0)` = `getByModIdx(currentColors, 0)` = first.
        background_colors[0]
    };

    draw_text_time_and_status(
        state,
        config,
        color_text,
        offset_color,
        must_show_replay_ui,
        states,
        target,
    );
    draw_text_message(state, config, color_text, offset_color, states, target);
    draw_text_personal_best(state, config, color_text, offset_color, states, target);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::VertexBatch;
    use crate::font::Font;
    use crate::texture::Sprite;
    use crate::texture::Texture;
    use crate::types::RectangleShape;

    #[test]
    fn format_time_matches_std_to_string_semantics() {
        assert_eq!(format_time(12.5), "12.500000");
        assert_eq!(format_time(0.1234567), "0.123000");
        assert_eq!(format_time(0.0), "0.000000");
        assert_eq!(format_time(99.9996), "99.999000"); // floor(99999.6)/1000
    }

    #[test]
    fn scaled_character_size_truncates() {
        assert_eq!(get_scaled_character_size(70.0, 1.0, 1.0), 70);
        assert_eq!(get_scaled_character_size(70.0, 2.0, 1.0), 35);
        assert_eq!(get_scaled_character_size(70.0, 1.0, 0.5), 35);
        assert_eq!(get_scaled_character_size(70.4, 1.0, 1.0), 70);
    }

    #[test]
    fn diff_format_uses_six_significant_digits() {
        assert_eq!(diff_format(1.0), "1");
        assert_eq!(diff_format(1.5), "1.5");
        assert_eq!(diff_format(0.25), "0.25");
        assert_eq!(diff_format(1.23456789_f64 as f32), "1.23457");
    }

    #[test]
    fn time_format_three_decimals() {
        assert_eq!(time_format(1.2), "1.200");
        assert_eq!(time_format(0.0), "0.000");
    }

    // -----------------------------------------------------------------
    // 029e2 (the graphics audit's M2): the per-frame character-size /
    // origin half — a pure computation, pinned here against the C++'
    // `getScaledCharacterSize` values (HGGraphics.cpp:545-571, :593).
    // -----------------------------------------------------------------

    fn test_font() -> Font {
        // `OpenSquare-Regular.ttf` (the C++ `initText` regular font).
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("_RELEASE");
        Font::load(&dir.join("Assets/OpenSquare-Regular.ttf")).expect("font loads")
    }

    fn init_text(font: &Font, s: &str, size: f32, zoom: f32) -> Text {
        // The C++ `initText` ctor sizes (character size = size / zoom).
        Text::new(font.clone(), s, (size / zoom) as u32)
    }

    /// The C++ constructor's initial text sizes (`HexagonGame.cpp:276-331`)
    /// at the default `zoomFactor` 1.27 — the sizes the pre-fix port kept
    /// FOREVER (the per-frame update was never called): time 55, status
    /// 19, fps 19, message 29, pb 51, replay 15.
    fn make_state(font: &Font) -> DrawState {
        let zoom = 1.27;
        DrawState {
            background_camera: None,
            overlay_camera: None,
            pre_shake_centers: None,
            flash_polygon: VertexBatch::new_tris(),
            particles: Vec::new(),
            trail_particles: Vec::new(),
            swap_particles: Vec::new(),
            key_icon_left: Sprite::new(Texture::new()),
            key_icon_right: Sprite::new(Texture::new()),
            key_icon_focus: Sprite::new(Texture::new()),
            key_icon_swap: Sprite::new(Texture::new()),
            replay_icon: Sprite::new(Texture::new()),
            level_info_rectangle: RectangleShape::default(),
            level_info_text_level: init_text(font, "LVL", 20.0, zoom),
            level_info_text_pack: init_text(font, "PACK", 14.0, zoom),
            level_info_text_author: init_text(font, "AUTHOR", 20.0, zoom),
            level_info_text_by: init_text(font, "BY", 12.0, zoom),
            level_info_text_dm: init_text(font, "", 14.0, zoom),
            fps_text: init_text(font, "0", 25.0, zoom),
            time_text: init_text(font, "0", 70.0, zoom),
            text: init_text(font, "", 25.0, zoom),
            replay_text: init_text(font, "", 20.0, zoom),
            message_text: init_text(font, "", 38.0, zoom),
            pb_text: init_text(font, "", 65.0, zoom),
            pb_text_growth: 0.0,
        }
    }

    /// The C++ per-frame values at `Config::default` (zoom 1.27, text
    /// scaling 1.0) — `getScaledCharacterSize(size)` =
    /// `(size / zoomFactor * textScaling) as u32` (HGGraphics.cpp:545-550):
    /// time 70 → 55, status 20 → 15, fps 20 → 15, message 32 → 25,
    /// pb `64 → 50 + sin(growth) * 10`, replay 16 → 12 — plus the origins
    /// `(0, 0)` for the status text and `getGlobalWidth/2` for the message
    /// / PB texts.
    #[test]
    fn update_text_sizes_applies_the_cpp_per_frame_sizes_and_origins() {
        let font = test_font();
        let mut state = make_state(&font);
        let mut config = Config::default(); // zoom 1.27, textScaling 1.0
        config.set_show_fps(true);

        // Non-empty message/PB strings (the C++ sets them via events;
        // the size half only reads their widths for the origins).
        state.message_text.set_string("HELLO");
        state.pb_text.set_string("NEW PERSONAL BEST!");
        state.pb_text_growth = PI / 2.0; // sin = 1 → +10

        update_text_sizes(&mut state, &config, true);

        assert_eq!(
            state.time_text.get_character_size(),
            55,
            "70 / 1.27 = 55.11 → 55 (coincides with the ctor's 55 — the C++ re-applies it per frame regardless)"
        );
        assert_eq!(
            state.text.get_character_size(),
            15,
            "20 / 1.27 = 15.74 → 15 (the ctor kept 19 = 25 / 1.27)"
        );
        assert_eq!(
            state.text.get_origin(),
            Vec2::new(0.0, 0.0),
            "the status-text origin (HGGraphics.cpp:557)"
        );
        assert_eq!(state.fps_text.get_character_size(), 15);
        assert_eq!(
            state.message_text.get_character_size(),
            25,
            "32 / 1.27 = 25.19 → 25 (the ctor kept 29 = 38 / 1.27)"
        );
        assert!(
            (state.message_text.get_origin().x
                - state.message_text.get_global_bounds().get_width() / 2.0)
                .abs()
                < 1e-4,
            "the message origin = getGlobalWidth(messageText) / 2"
        );
        assert_eq!(state.message_text.get_origin().y, 0.0);
        assert_eq!(
            state.pb_text.get_character_size(),
            60,
            "50 + 10 (sin(π/2) = 1) — the ctor kept the static 51"
        );
        assert!(
            (state.pb_text.get_origin().x - state.pb_text.get_global_bounds().get_width() / 2.0)
                .abs()
                < 1e-4,
            "the PB origin = getGlobalWidth(pbText) / 2"
        );
        assert_eq!(state.pb_text.get_origin().y, 0.0);
        assert_eq!(
            state.replay_text.get_character_size(),
            12,
            "16 / 1.27 = 12.59 → 12 (the ctor kept 15 = 20 / 1.27)"
        );

        // The size half writes NO strings (the port's split: the engine
        // owns the string-content half).
        assert_eq!(state.time_text.get_string(), "0");
        assert_eq!(state.fps_text.get_string(), "0");
        assert_eq!(state.replay_text.get_string(), "");
        assert_eq!(state.message_text.get_string(), "HELLO");
    }

    /// The PB pulse: `64 → 50 + sin(growth) * 10` across the sawtooth
    /// (HGGraphics.cpp:569-571).
    #[test]
    fn update_text_sizes_pb_pulse_follows_the_growth_sawtooth() {
        let font = test_font();
        let mut state = make_state(&font);
        let config = Config::default();
        for (growth, expected) in [
            (0.0, 50),            // sin 0 = 0 → the base
            (PI / 2.0, 60),       // sin = 1 → +10
            (PI, 50),             // sin = 0
            (3.0 * PI / 2.0, 40), // sin = -1 → -10
        ] {
            state.pb_text_growth = growth;
            update_text_sizes(&mut state, &config, false);
            assert_eq!(
                state.pb_text.get_character_size(),
                expected,
                "growth {growth} → 50 + sin × 10 = {expected}"
            );
        }
    }

    /// The C++'s `Config::getShowFPS()` gate (HGGraphics.cpp:560-564) and
    /// `mustShowReplayUI()` gate (:574-598): a gated-off text keeps its
    /// previous size (the ctor's, in the pre-fix port's only path).
    #[test]
    fn update_text_sizes_fps_and_replay_gates_keep_the_previous_sizes() {
        let font = test_font();
        let mut state = make_state(&font);
        let config = Config::default(); // showFPS = false (the C++ default)

        update_text_sizes(&mut state, &config, false);

        assert_eq!(
            state.fps_text.get_character_size(),
            19,
            "the showFPS-off fps text keeps its ctor size (25 / 1.27)"
        );
        assert_eq!(
            state.replay_text.get_character_size(),
            15,
            "the replay-UI-off replay text keeps its ctor size (20 / 1.27)"
        );
    }
}
