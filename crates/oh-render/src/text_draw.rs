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
use oh_math::ssvu::math::PI;
use oh_math::ssvs::vector2::Vec2;

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

    if exp < -4 || exp >= 6 {
        // C `%g` exponent form: "d[.dddd]e±dd" (exponent >= 2 digits).
        let mut m = digits;
        while m.len() > 1 && m.ends_with('0') {
            m.pop();
        }
        let (digits_part, frac_part) = m.split_at(1);
        let frac = frac_part.strip_prefix(|c: char| c == '0').unwrap_or(frac_part);
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
            let f = f.strip_suffix(|c: char| c == '0').unwrap_or(f);
            if f.is_empty() {
                format!("{sign}{i}")
            } else {
                format!("{sign}{i}.{f}")
            }
        } else {
            format!("{sign}{digits}{}", "0".repeat((exp + 1) as usize - digits.len()))
        }
    } else {
        // exp in [-4, -1]: "0.0...d..."
        let mut f = format!("{}{}", "0".repeat(-exp as usize), digits);
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
/// FPS / message / PB / replay text content + char sizes + origins.
///
/// Runs every frame (even during `debugPause`); the C++ guards on
/// `window != nullptr` (the caller only invokes this with a live draw
/// state).
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
                let Some(value) = input
                    .lua_variable
                    .map(|read| read(variable_name))
                    .flatten()
                else {
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
        state
            .time_text
            .set_string(if input.started {
                format_time(input.time_seconds)
            } else {
                "0".to_string()
            });
    } else {
        // Alternative scoring — the Lua variable holds the string.
        let value = input
            .lua_variable
            .map(|read| read(input.score_override))
            .flatten()
            .unwrap_or_default();
        state.time_text.set_string(value);
    }

    let scaled = |size: f32| -> u32 {
        get_scaled_character_size(size, config.get_zoom_factor(), config.get_text_scaling())
    };

    state.time_text.set_character_size(scaled(70.0));

    // Set information text.
    state.text.set_string(os);
    state
        .text
        .set_character_size(scaled(20.0));
    state.text.set_origin(Vec2::new(0.0, 0.0));

    // Set FPS text, if option is enabled.
    if config.get_show_fps() {
        // C++ `ssvu::toStr(window->getFPS())` = `std::to_string` (6 decimals).
        state.fps_text.set_string(format!("{:.6}", input.fps as f64));
        state
            .fps_text
            .set_character_size(scaled(20.0));
    }

    state
        .message_text
        .set_character_size(scaled(32.0));
    state.message_text.set_origin(Vec2::new(
        state.message_text.get_global_bounds().get_width() / 2.0,
        0.0,
    ));

    let growth = state.pb_text_growth.sin();
    // C++: `getScaledCharacterSize(64.f) + growth * 10.f` — the unsigned
    // base is widened to float, the sum truncated back to unsigned.
    state.pb_text.set_character_size(
        (scaled(64.0) as f32 + growth * 10.0) as u32,
    );
    state
        .pb_text
        .set_origin(Vec2::new(state.pb_text.get_global_bounds().get_width() / 2.0, 0.0));

    // --------------------------------------------------------------------
    if input.must_show_replay_ui {
        let mut replay = String::new();

        if !input.score_overridden {
            replay.push_str(&format!("{}s", format_time(input.replay_played_seconds)));
        } else {
            replay.push_str(&format_time(input.replay_played_score));
        }

        replay.push_str(&format!(" BY {}", input.replay_player_name));

        state
            .replay_text
            .set_character_size(scaled(16.0));
        state.replay_text.set_string(replay);
    } else {
        state.replay_text.set_string("");
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
    target: &mut impl DrawTarget,
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
        state.text.set_origin(state.text.get_local_bounds().get_nw());
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
            state.fps_text.set_position(Vec2::new(
                padding,
                config.get_height() as f32 - padding,
            ));
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
    target: &mut impl DrawTarget,
) {
    draw_text_message_pb_impl(
        &mut state.message_text,
        offset_color,
        Vec2::new(
            config.get_width() as f32 / 2.0,
            config.get_height() as f32 / 5.5,
        ),
        color_text,
        1.0 /* outlineThickness */,
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
    target: &mut impl DrawTarget,
) {
    draw_text_message_pb_impl(
        &mut state.pb_text,
        offset_color,
        Vec2::new(
            config.get_width() as f32 / 2.0,
            config.get_height() as f32 - config.get_height() as f32 / 4.0,
        ),
        color_text,
        4.0 /* outlineThickness */,
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
    target: &mut impl DrawTarget,
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
        assert_eq!(diff_format(1.23456789), "1.23457");
    }

    #[test]
    fn time_format_three_decimals() {
        assert_eq!(time_format(1.2), "1.200");
        assert_eq!(time_format(0.0), "0.000");
    }
}
