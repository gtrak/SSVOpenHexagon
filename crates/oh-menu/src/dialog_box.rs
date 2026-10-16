//! §12 / `Core/HexagonDialogBox` — the on-screen message / input dialog.
//!
//! Ports the `HexagonDialogBox` data model and its layout math (line-height
//! based `totalHeight`, the outer/backdrop frame quads per `DBoxDraw` mode).
//! The SFML glyph probes (`Utils::getFontHeight`, `ssvs::getGlobalWidth`) and
//! `Config::getWidth/Height` are passed in by `MenuGame` (which owns the
//! font oracles and resolution), so this type is pure and unit-testable.
//!
//! Frame emission is the coarse slice-13b render: [`HexagonDialogBox::draw`]
//! fills a caller-provided [`Vec<DialogQuad>`] using the exact C++
//! coordinates; a display/golden check is out of scope (D-011).

use oh_math::hg::color::Color;

/// C++ `enum class DBoxDraw`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DBoxDraw {
    #[default]
    TopLeft = 0,
    Center = 1,
    CenterUpperHalf = 2,
}

/// The minimal key vocabulary the dialog box uses for `keyToClose`
/// (C++ `sf::Keyboard::Key`; only `Unknown` = "any key" and `Enter` matter
/// to `MenuGame`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogKey {
    #[default]
    Unknown,
    Enter,
}

/// One frame quad the dialog box emits (coarse render output).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialogQuad {
    pub color: Color,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// C++ `inline constexpr float fontHeightDifferential`.
const FONT_HEIGHT_DIFFERENTIAL: f32 = 0.9;

/// `hg::HexagonDialogBox`.
#[derive(Debug, Clone, Default)]
pub struct HexagonDialogBox {
    draw_mode: DBoxDraw,
    dialog_text: Vec<String>,
    line_height: f32,
    dialog_width: f32,
    frame_size: f32,
    double_frame_size: f32,
    total_height: f32,
    x_pos: f32,
    y_pos: f32,
    key_to_close: DialogKey,
    input_box: bool,
    input_box_password: bool,
    input: String,
    /// C++ `txtDialog`'s character size — `create` measures the line height
    /// and width AT `charSize` (C++ `Utils::getFontHeight(txtDialog,
    /// charSize)` sets the size on the text, HexagonDialogBox.cpp:44-47),
    /// so the draw pass renders the lines at this size.
    char_size: u32,
}

impl HexagonDialogBox {
    /// C++ `HexagonDialogBox(font, window)` — the font/window are external
    /// oracles owned by `MenuGame`, so the Rust ctor is empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // The C++ signatures take the same 10 parameters (dialog_box.hpp);
    // keeping the arity is the point of a 1:1 port.
    #[allow(clippy::too_many_arguments)]
    /// C++ `create(output, charSize, frameSize, drawMode, xPos, yPos,
    /// inputBox)`. `line_height` and `dialog_width` are the pre-measured glyph
    /// values (C++ computes them from `getFontHeight` / `getGlobalWidth`);
    /// `char_size` is kept for fidelity and is otherwise unused here.
    pub fn create(
        &mut self,
        output: &str,
        char_size: u32,
        frame_size: f32,
        draw_mode: DBoxDraw,
        x_pos: f32,
        y_pos: f32,
        input_box: bool,
        line_height: f32,
        dialog_width: f32,
    ) {
        self.line_height = line_height;
        self.dialog_width = dialog_width;
        self.frame_size = frame_size;
        self.double_frame_size = 2.0 * frame_size;
        self.draw_mode = draw_mode;
        self.x_pos = x_pos;
        self.y_pos = y_pos;
        self.input_box = input_box;
        self.input_box_password = false;
        self.input.clear();
        self.char_size = char_size;

        // C++ splits `output` on '\n'.
        let mut dialog_text = Vec::new();
        let mut temp = String::new();
        for c in output.chars() {
            if c == '\n' {
                dialog_text.push(std::mem::take(&mut temp));
            } else {
                temp.push(c);
            }
        }
        dialog_text.push(temp);
        self.dialog_text = dialog_text;

        let size = self.dialog_text.len() as f32;
        let dialog_height = self.line_height * size + (self.line_height / 2.0) * (size - 1.0);
        self.total_height = dialog_height + 2.0 * self.double_frame_size;

        if input_box {
            self.total_height += self.line_height * 4.0;
        }
    }

    // The C++ signatures take the same 10 parameters (dialog_box.hpp);
    // keeping the arity is the point of a 1:1 port.
    #[allow(clippy::too_many_arguments)]
    /// C++ `create(..., keyToClose, xPos, yPos)` overload.
    pub fn create_with_key(
        &mut self,
        output: &str,
        char_size: u32,
        frame_size: f32,
        draw_mode: DBoxDraw,
        key_to_close: DialogKey,
        x_pos: f32,
        y_pos: f32,
        line_height: f32,
        dialog_width: f32,
    ) {
        self.create(
            output,
            char_size,
            frame_size,
            draw_mode,
            x_pos,
            y_pos,
            false,
            line_height,
            dialog_width,
        );
        self.key_to_close = key_to_close;
    }

    /// C++ `createInput(output, charSize, frameSize, drawMode)`.
    pub fn create_input(
        &mut self,
        output: &str,
        char_size: u32,
        frame_size: f32,
        draw_mode: DBoxDraw,
        line_height: f32,
        dialog_width: f32,
    ) {
        self.create(
            output,
            char_size,
            frame_size,
            draw_mode,
            0.0,
            0.0,
            true,
            line_height,
            dialog_width,
        );
        self.key_to_close = DialogKey::Enter;
    }

    /// C++ `clearDialogBox()`.
    pub fn clear(&mut self) {
        self.dialog_text.clear();
        self.input.clear();
        self.input_box = false;
        self.input_box_password = false;
        self.key_to_close = DialogKey::Unknown;
    }

    #[must_use]
    pub fn key_to_close(&self) -> DialogKey {
        self.key_to_close
    }

    #[must_use]
    pub fn empty(&self) -> bool {
        self.dialog_text.is_empty()
    }

    #[must_use]
    pub fn is_input_box(&self) -> bool {
        self.input_box
    }

    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn input_mut(&mut self) -> &mut String {
        &mut self.input
    }

    pub fn set_input_box_password(&mut self, x: bool) {
        self.input_box_password = x;
    }

    #[must_use]
    pub fn input_box_password(&self) -> bool {
        self.input_box_password
    }

    // --- layout accessors (used by `MenuGame`'s draw + tests) ---

    #[must_use]
    pub fn total_height(&self) -> f32 {
        self.total_height
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.dialog_text.len()
    }

    /// C++ `draw(txtColor, backdropColor)` — emits the outer + backdrop frame
    /// quads for the active [`DBoxDraw`] mode into `out` (coarse render).
    /// `config_width`/`config_height` mirror `Config::getWidth/Height`.
    pub fn draw(
        &self,
        txt_color: Color,
        backdrop_color: Color,
        config_width: f32,
        config_height: f32,
        out: &mut Vec<DialogQuad>,
    ) {
        let push_box =
            |out: &mut Vec<DialogQuad>, color: Color, x1: f32, x2: f32, y1: f32, y2: f32| {
                out.push(DialogQuad {
                    color,
                    x1,
                    y1,
                    x2,
                    y2,
                });
            };

        match self.draw_mode {
            DBoxDraw::TopLeft => {
                let x = self.x_pos;
                let y = self.y_pos;
                push_box(
                    out,
                    txt_color,
                    x,
                    2.0 * self.double_frame_size + self.dialog_width + x,
                    y,
                    self.total_height + y,
                );
                push_box(
                    out,
                    backdrop_color,
                    self.frame_size + x,
                    self.double_frame_size + self.frame_size + self.dialog_width + x,
                    self.frame_size + y,
                    self.total_height - self.frame_size + y,
                );
            }
            DBoxDraw::Center => {
                let fmax = (1024.0 / config_width).max(768.0 / config_height);
                let w = config_width * fmax;
                let h = config_height * fmax / 2.0 + self.y_pos;
                let left = (w - self.dialog_width) / 2.0 + self.x_pos;
                let right = (w + self.dialog_width) / 2.0 + self.x_pos;
                let half = self.total_height / 2.0;
                push_box(
                    out,
                    txt_color,
                    left - self.double_frame_size,
                    right + self.double_frame_size,
                    h - half,
                    h + half,
                );
                push_box(
                    out,
                    backdrop_color,
                    left - self.frame_size,
                    right + self.frame_size,
                    h - half + self.frame_size,
                    h + half - self.frame_size,
                );
            }
            DBoxDraw::CenterUpperHalf => {
                let fmax = (1024.0 / config_width).max(768.0 / config_height);
                let w = config_width * fmax;
                let h = config_height * fmax / 2.0 + self.y_pos;
                let left = (w - self.dialog_width) / 2.0 + self.x_pos;
                let right = (w + self.dialog_width) / 2.0 + self.x_pos;
                push_box(
                    out,
                    txt_color,
                    left - self.double_frame_size,
                    right + self.double_frame_size,
                    h - self.total_height,
                    h,
                );
                push_box(
                    out,
                    backdrop_color,
                    left - self.frame_size,
                    right + self.frame_size,
                    h - self.total_height + self.frame_size,
                    h - self.frame_size,
                );
            }
        }
    }

    /// The y baseline the C++ `drawText` starts at for a given mode
    /// (exposed so the text-draw pass can be checked in slice 13b).
    #[must_use]
    pub fn text_top_offset(&self) -> f32 {
        self.line_height * FONT_HEIGHT_DIFFERENTIAL
    }

    // ---- draw-pass accessors (slice 13b: the bridge's dialog text/quad
    // pass reads these; C++ `HexagonDialogBox` members of the same names) ----

    #[must_use]
    pub fn draw_mode(&self) -> DBoxDraw {
        self.draw_mode
    }
    #[must_use]
    pub fn x_pos(&self) -> f32 {
        self.x_pos
    }
    #[must_use]
    pub fn y_pos(&self) -> f32 {
        self.y_pos
    }
    #[must_use]
    pub fn frame_size(&self) -> f32 {
        self.frame_size
    }
    #[must_use]
    pub fn double_frame_size(&self) -> f32 {
        self.double_frame_size
    }
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.line_height
    }
    #[must_use]
    pub fn dialog_width(&self) -> f32 {
        self.dialog_width
    }
    /// C++ `txtDialog`'s character size (set by `create`, see the field).
    #[must_use]
    pub fn char_size(&self) -> u32 {
        self.char_size
    }
    #[must_use]
    pub fn input_box(&self) -> bool {
        self.input_box
    }
    /// C++ `dialogText` (the `\n`-split lines).
    #[must_use]
    pub fn dialog_text(&self) -> &[String] {
        &self.dialog_text
    }
}
