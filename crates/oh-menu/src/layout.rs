//! §11.11 / §11.12 / §11.13 — layout constants and the font-sizing /
//! offset math (the part of the menu layout that is pure arithmetic and
//! therefore unit-testable).
//!
//! The menu lays out in a virtual **1024×768** space that is derived from
//! the current window resolution; every draw position below is a function of
//! `w`, `h`, `four_by_three`, and the cached font *heights*. The font-height
//! measurement itself is an SFML glyph probe (`hg::Utils::getFontHeight`),
//! which is the render-backend seam: this crate exposes a
//! [`FontHeightOracle`] so the layout formulas can be exercised without a
//! display (see `render` test skip, D-011).

use oh_math::ssvu::math::get_clamped;

/// C++ file-level `inline constexpr float maxOffset{100.f}`.
pub const MAX_OFFSET: f32 = 100.0;
/// §11.14 `fontHeightOffset` — the top-glyph-gap compensation.
pub const FONT_HEIGHT_OFFSET: f32 = 0.55;
/// §11.14 `frameSizeMulti`.
pub const FRAME_SIZE_MULTI: f32 = 0.6;
/// §11.15 `profFrameSize`.
pub const PROF_FRAME_SIZE: f32 = 10.0;
/// §11.15 `profCharSize`.
pub const PROF_CHAR_SIZE: u32 = 30;
/// §11.15 `profSelectedCharSize` (`30 + 12`).
pub const PROF_SELECTED_CHAR_SIZE: u32 = 42;
/// §11.8 `maxProfilesOnScreen`.
pub const MAX_PROFILES_ON_SCREEN: i32 = 6;
/// §11.8 `maxErrorsOnScreen`.
pub const MAX_ERRORS_ON_SCREEN: i32 = 7;
/// §11.13 `offsetSpeed`.
pub const OFFSET_SPEED: f32 = 4.0;
/// §11.13 `offsetSnap`.
pub const OFFSET_SNAP: f32 = 0.25;
/// §11.1 `baseScrollSpeed` (level-list scroll speed).
pub const BASE_SCROLL_SPEED: f32 = 30.0;
/// §11.1 `difficultyBumpEffectMax`.
pub const DIFFICULTY_BUMP_EFFECT_MAX: f32 = 24.0;

/// C++ `getFPSMult()` — `200.f / window.getFPS()` (MenuGame.cpp:5985-5989;
/// the menu's offset animations run at the C++'s 200-unit rate — NOT the
/// game's 60-unit `60.f / fps` frametime).
#[must_use]
pub fn fps_mult(fps: f32) -> f32 {
    200.0 / fps
}

/// §11.13 `calcMenuOffset(offset, maxOffset, revertOffset, speedUp)` —
/// returns the *panel* offset `maxOffset - offset` (the indent the panel
/// slides by). Eases out with `+speed * (1 - offset/max)`.
#[must_use]
pub fn calc_menu_offset(
    offset: &mut f32,
    max_offset: f32,
    revert: bool,
    speed_up: bool,
    fps_mult: f32,
) -> f32 {
    if revert {
        let speed = (if speed_up { 12.0 } else { 8.0 }) * OFFSET_SPEED * fps_mult;
        *offset = (*offset - speed).max(0.0);
        if *offset <= OFFSET_SNAP {
            *offset = 0.0;
        }
    } else if *offset < max_offset {
        let speed = (if speed_up { 12.0 } else { 8.0 }) * OFFSET_SPEED * fps_mult;
        *offset += speed * (1.0 - *offset / max_offset);
        if *offset >= max_offset - OFFSET_SNAP {
            *offset = max_offset;
        }
    }
    max_offset - *offset
}

/// §11.13 `calcMenuItemOffset(offset, selected)` — the selected item eases in
/// to `maxOffset`; a deselected item decays at 1.5× proportional speed.
pub fn calc_menu_item_offset(offset: &mut f32, selected: bool, fps_mult: f32) {
    if selected {
        if *offset < MAX_OFFSET {
            let speed = OFFSET_SPEED * fps_mult;
            *offset += speed * (1.0 - *offset / MAX_OFFSET);
            if *offset >= MAX_OFFSET - OFFSET_SNAP {
                *offset = MAX_OFFSET;
            }
        }
    } else if *offset > 0.0 {
        let speed = OFFSET_SPEED * fps_mult;
        *offset -= 1.5 * speed * *offset / MAX_OFFSET;
        if *offset <= OFFSET_SNAP {
            *offset = 0.0;
        }
    }
}

/// §11.11 `refreshCamera()` view math. Given the window pixel size and the
/// zoom factor, returns the virtual menu width/height (`w`/`h`), the
/// `fourByThree` aspect flag, and the background-camera vertical skew.
///
/// C++:
/// ```cpp
/// const float fw{1024.f / windowWidth}, fh{768.f / windowHeight};
/// const float fmax{std::max(fw, fh)};
/// w = windowWidth * fmax; h = windowHeight * fmax;
/// fourByThree = 10.f * windowWidth / windowHeight < 16;
/// backgroundCamera.setSkew({1.f, fourByThree ? 0.8f : 0.6f});
/// ```
#[must_use]
pub fn refresh_view(size_x: f32, size_y: f32) -> (f32, f32, bool, f32) {
    let fw = 1024.0 / size_x;
    let fh = 768.0 / size_y;
    let fmax = fw.max(fh);
    let w = size_x * fmax;
    let h = size_y * fmax;
    let four_by_three = 10.0 * size_x / size_y < 16.0;
    let skew_y = if four_by_three { 0.8 } else { 0.6 };
    (w, h, four_by_three, skew_y)
}

/// The per-font character sizes. `MenuGame` sets these in `refreshCamera()`
/// (the aspect-dependent ones) and in the constructor (the fixed ones).
/// §11.11 (per-aspect) + §11.2 (constructor defaults).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharSizes {
    pub version: f32, // 40 (fixed)
    pub prof: f32,    // 18 (fixed)
    pub load_big: f32,
    pub load_small: f32,
    pub random_tip: f32,
    pub menu_big: f32,
    pub menu_small: f32,
    pub menu_tiny: f32,
    pub profile: f32,          // 32 (fixed)
    pub instructions_big: f32, // 46 (fixed)
    pub instructions_medium: f32,
    pub instructions_small: f32, // 20 (fixed)
    pub entering_text: f32,      // 54 (fixed)
    pub selection_big: f32,
    pub selection_medium: f32, // 19 (fixed, bold)
    pub selection_small: f32,
    pub selection_score: f32, // 28 (fixed)
    pub selection_ranked: f32,
}

/// §11.11 `refreshCamera` — the aspect-dependent char sizes + the fixed
/// constructor sizes (§11.2). `instructions_medium` is never set by either
/// the constructor or `refreshCamera`; it keeps the SFML `sf::Text` default
/// character size of 30 (D-014: the C++ `txtInstructionsMedium{
/// .font{"", openSquare}}`, MenuGame.cpp:196 — the constructor's `sf::Text`
/// default size is 30, not 24).
#[must_use]
pub fn char_sizes(four_by_three: bool) -> CharSizes {
    if four_by_three {
        CharSizes {
            version: 40.0,
            prof: 18.0,
            load_big: 56.0,
            load_small: 16.0,
            random_tip: 24.0,
            menu_big: 26.0,
            menu_small: 16.0,
            menu_tiny: 9.0,
            profile: 32.0,
            instructions_big: 46.0,
            instructions_medium: 30.0,
            instructions_small: 20.0,
            entering_text: 54.0,
            selection_big: 24.0,
            selection_medium: 19.0,
            selection_small: 14.0,
            selection_score: 28.0,
            selection_ranked: 10.0,
        }
    } else {
        CharSizes {
            version: 40.0,
            prof: 18.0,
            load_big: 70.0,
            load_small: 24.0,
            random_tip: 32.0,
            menu_big: 36.0,
            menu_small: 24.0,
            menu_tiny: 14.0,
            profile: 32.0,
            instructions_big: 46.0,
            instructions_medium: 30.0,
            instructions_small: 20.0,
            entering_text: 54.0,
            selection_big: 28.0,
            selection_medium: 19.0,
            selection_small: 14.0,
            selection_score: 28.0,
            selection_ranked: 10.0,
        }
    }
}

/// The SFML glyph-height probe (`hg::Utils::getFontHeight`) as a seam. A real
/// backend returns the measured font height for a character size; the tests
/// use a deterministic oracle so the layout math is reproducible (D-011: the
/// render path that needs a real font skips with reason).
pub trait FontHeightOracle: Send + Sync {
    /// Height of a font drawn at `char_size`.
    fn height(&self, char_size: f32) -> f32;
}

/// A linear, deterministic [`FontHeightOracle`] (height ≈ 0.7 × char size),
/// used by tests and as the default before the render backend lands.
#[derive(Debug, Clone, Copy)]
pub struct LinearFontHeight;

impl FontHeightOracle for LinearFontHeight {
    fn height(&self, char_size: f32) -> f32 {
        char_size * 0.7
    }
}

/// Convenience for the load-screen scrollbar notch math
/// (§11.14 `getScrollbarNotches`).
#[must_use]
pub fn scrollbar_notches(size: i32, max_size: i32) -> (i32, i32) {
    if size > max_size {
        (size - max_size, max_size)
    } else {
        (0, size)
    }
}

/// §11.8 `ssvu::getClamped` re-export so call sites read naturally (the
/// clamp helpers in navigation use this).
#[must_use]
pub fn clamped<T: PartialOrd + Copy>(x: T, min: T, max: T) -> T {
    get_clamped(x, min, max)
}

/// `ssvu::getMod(val, lb, ub)` (the 3-arg wrap into `[lb, ub)`) — the pack
/// index math in navigation (§11.8).
#[must_use]
pub fn mod_range<T>(v: T, lb: T, ub: T) -> T
where
    T: Copy
        + std::cmp::PartialOrd
        + std::convert::From<u8>
        + std::ops::Add<Output = T>
        + std::ops::AddAssign
        + std::ops::Sub<Output = T>
        + std::ops::Mul<Output = T>
        + std::ops::Div<Output = T>
        + std::ops::Rem<Output = T>,
{
    oh_math::ssvu::math::get_mod_range(v, lb, ub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_view_16x9() {
        // 800x450 (16:9): fmax = max(1024/800, 768/450) = max(1.28, 1.7067)
        // = 1.7067; w = 800*1.7067 = 1365.33, h = 450*1.7067 = 768.
        let (w, h, fbt, skew) = refresh_view(800.0, 450.0);
        assert!((h - 768.0).abs() < 1e-3);
        assert!(w > 1024.0);
        assert!(!fbt);
        assert_eq!(skew, 0.6);
    }

    #[test]
    fn refresh_view_4x3() {
        // 800x600 (4:3): fmax = max(1024/800, 768/600) = max(1.28, 1.28) = 1.28;
        // w = 1024, h = 768, four_by_three = (10*800/600)=13.33 < 16 → true.
        let (w, h, fbt, skew) = refresh_view(800.0, 600.0);
        assert!((w - 1024.0).abs() < 1e-3);
        assert!((h - 768.0).abs() < 1e-3);
        assert!(fbt);
        assert_eq!(skew, 0.8);
    }

    /// issue 025g — the desktop (window == config) case: when the actual
    /// window size equals the config size, the C++ formula's output is the
    /// value the pre-025g port computed from the config — the desktop must
    /// not change. 1024×768: fmax = max(1024/1024, 768/768) = 1 → w = 1024,
    /// h = 768; fourByThree = 10×1024/768 = 13.33 < 16 → true, skew 0.8.
    #[test]
    fn refresh_view_matching_window_desktop_unchanged() {
        let (w, h, fbt, skew) = refresh_view(1024.0, 768.0);
        assert!((w - 1024.0).abs() < 1e-3);
        assert!((h - 768.0).abs() < 1e-3);
        assert!(fbt);
        assert_eq!(skew, 0.8);
        // Scale-invariance: the desktop window is config × pixelMult — a
        // uniform factor leaves the math unchanged (2×: fmax = 0.5, w =
        // 2048×0.5, h = 1536×0.5).
        let (w2, h2, fbt2, skew2) = refresh_view(2048.0, 1536.0);
        assert!((w2 - 1024.0).abs() < 1e-3);
        assert!((h2 - 768.0).abs() < 1e-3);
        assert!(fbt2);
        assert_eq!(skew2, 0.8);
    }

    /// issue 025g — the mismatched (mobile-like) case: the phone's actual
    /// window 1530×678 vs a 1024×768-ish config. The formula must run on
    /// the ACTUAL window: fmax = max(1024/1530, 768/678) = 768/678 ≈
    /// 1.1327 → w = 1530×768/678 = 1733.097…, h = 678×768/678 = 768;
    /// fourByThree = 10×1530/678 = 22.57 > 16 → false, skew 0.6. The
    /// config-size input (1024×768) gives (1024, 768) — a different
    /// space: the pre-025g port's defect.
    #[test]
    fn refresh_view_mismatched_mobile_window() {
        let (w, h, fbt, skew) = refresh_view(1530.0, 678.0);
        assert!((w - 1530.0 * 768.0 / 678.0).abs() < 1e-3); // 1733.097…
        assert!((w - 1733.097).abs() < 1e-1);
        assert!((h - 768.0).abs() < 1e-3);
        assert!(!fbt);
        assert_eq!(skew, 0.6);
        // The config input would have produced a different space — the
        // divergence the fix removes (the probe read (1024, 1024) for the
        // port's config-derived phone space).
        let (cw, ch, cfbt, _c_skew) = refresh_view(1024.0, 768.0);
        assert!((cw - w).abs() > 100.0);
        assert!((ch - h).abs() < 1e-3); // h happens to coincide (768)
        assert_ne!(cfbt, fbt);
    }

    #[test]
    fn calc_menu_offset_eases_to_max() {
        let mut offset = 0.0;
        let mut panel = 0.0;
        for _ in 0..100 {
            panel = calc_menu_offset(&mut offset, MAX_OFFSET, false, false, 1.0);
        }
        assert_eq!(offset, MAX_OFFSET);
        assert!(panel.abs() < 1e-3);
    }

    #[test]
    fn calc_menu_offset_revert_goes_to_zero() {
        let mut offset = MAX_OFFSET;
        for _ in 0..100 {
            let _ = calc_menu_offset(&mut offset, MAX_OFFSET, true, false, 1.0);
        }
        assert_eq!(offset, 0.0);
    }

    #[test]
    fn char_sizes_per_aspect() {
        let wide = char_sizes(false);
        let narrow = char_sizes(true);
        assert_eq!(wide.menu_big, 36.0);
        assert_eq!(narrow.menu_big, 26.0);
        assert_eq!(wide.load_big, 70.0);
        assert_eq!(narrow.load_big, 56.0);
        // fixed sizes (constructor) are identical in both
        assert_eq!(wide.version, narrow.version);
        assert_eq!(wide.entering_text, narrow.entering_text);
    }

    #[test]
    fn scrollbar_notches_math() {
        assert_eq!(scrollbar_notches(5, 7), (0, 5));
        assert_eq!(scrollbar_notches(10, 7), (3, 7));
    }
}
