//! The font-atlas CPU side of the GL text pass (issue-025c SG3) — the
//! shelf-packed glyph atlas, the outline dilation, the text→quad layout and
//! the sprite→quad math. Everything here is host-testable (no GL): the GL
//! wiring (the program, the texture objects, the per-draw upload) lives in
//! [`crate::gl_passes`].
//!
//! # SFML semantics (the literal-port pins)
//!
//! * **Font cache** — SFML `sf::Font` caches the rasterized glyph per
//!   `(codePoint, characterSize, bold, outlineThickness)` in its internal
//!   texture (`Font::getGlyph`); the port's equivalent is [`GlyphStore`]
//!   (the fill slot per `(font, size, char)`, the outline slot per
//!   `(font, size, char, thickness)`, the line metrics per `(font, size)`).
//!   Bold is a separate font face (the C++'s `openSquare` /
//!   `openSquareBold`), carried by the `Text`'s own `Font` — nothing extra
//!   here. Letter spacing: the C++ never calls `setLetterSpacing`
//!   (SFML's default 0), so the pen advance is exactly fontdue's
//!   `advance_width` — the same value the port's `Text::measure` uses.
//!   The cache entries key the atlas by **slot index** (never by
//!   coordinates): the atlas's width growth re-packs the existing slots
//!   into new cells (the content-preserving move, `GlyphAtlas::
//!   grow_width`), and the slot ORDER is what the re-pack preserves — the
//!   index is the stable reference, the cell is read at the end of the
//!   layout (the `text_quads`'s second pass), so a mid-string growth
//!   (height, or the width re-pack) can never leave a cached entry
//!   pointing at a moved / stale cell.
//! * **Per-glyph quads** — SFML `Text::draw` emits, per character in
//!   order: the OUTLINE quad (if a thickness > 0 is set) followed by the
//!   FILL quad; the quad's top-left is `(pen + bearingX, lineTop +
//!   bearingY)`, its size the glyph bitmap size; the pen advances by
//!   `advanceWidth` AFTER the glyph; a glyph with no texture area (the
//!   space) advances the pen but emits no quad. `bearingX` = fontdue's
//!   `Metrics.left` (pen → bitmap left edge); `bearingY` = the line
//!   ascent − fontdue's `Metrics.top` (top-of-line → bitmap top; fontdue
//!   measures `top` from the baseline).
//! * **Outlines** — SFML `Font::generateOutline`: for every source pixel
//!   with alpha > 0, a `(2t+1) × (2t+1)` square of OPAQUE pixels at
//!   distance `t` (the port's shipped thicknesses are the integral 1/2/4 —
//!   a fractional thickness rounds to the nearest integer, as the C++'s
//!   integer cast does for every shipped value). The outline quad is the
//!   fill quad's position offset by `(−t, −t)`, grown by `2t` per side, in
//!   the text's outline colour, drawn BEFORE the fill of the same glyph.
//! * **Texture convention** — the atlas RGBA8 buffer is uploaded to GL
//!   UNFLIPPED (row 0 = the top row) and addressed with a TOP-DOWN UV
//!   (`v = 0` at the texture's top edge). That is SFML's own
//!   Image→Texture→shape-geometry convention (SFML uploads the image rows
//!   as-is and its geometry addresses the texture top-down); no row flip
//!   anywhere in the pipeline.
//! * **Straight alpha** — the glyph pixels are `(255, 255, 255, mask)`:
//!   white RGB, the alpha mask. The pass's `tex × vertexColor` product then
//!   yields exactly the SFML fill/outline colour at the mask's alpha (the
//!   `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` blend — see `gl_passes`).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use oh_math::ssvs::vector2::Vec2;

use crate::font::{Font, Text};
use crate::types::FloatRect;
use crate::Color;

// ---------------------------------------------------------------------------
// The UV math (the texture-convention pin above).
// ---------------------------------------------------------------------------

/// The UV of a content slot `(x, y, w, h)` in an atlas of `(atlas_w,
/// atlas_h)`: `(u0, v_top, u1, v_bottom)` — `v` grows DOWNWARD (the
/// top-down convention). `x`/`y` are the content's first pixel (the slot's
/// 1-px zero padding is excluded).
#[must_use]
pub fn uv_rect(x: u32, y: u32, w: u32, h: u32, atlas_w: u32, atlas_h: u32) -> (f32, f32, f32, f32) {
    (
        x as f32 / atlas_w as f32,
        y as f32 / atlas_h as f32,
        (x + w) as f32 / atlas_w as f32,
        (y + h) as f32 / atlas_h as f32,
    )
}

// ---------------------------------------------------------------------------
// The shelf-packed glyph atlas.
// ---------------------------------------------------------------------------

const ATLAS_START_W: u32 = 1024;
const ATLAS_START_H: u32 = 512;
/// The ES-3.0 / desktop-conservative texture-size cap (the atlas grows
/// height-first, width second; a TTF glyph's raster can never approach it —
/// the cap is a guard, not an expectation).
const ATLAS_MAX: u32 = 16384;

/// A reserved slot's padded-cell coordinates (the content pixels live at
/// `(x + 1, y + 1)`; `w`/`h` are the CONTENT size).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// The shelf-packed RGBA8 glyph atlas: 1-px zero-alpha padding around every
/// slot (the bilinear-sampling bleed guard), row (shelf) packing,
/// height-first growth (a width growth re-packs the existing slots).
///
/// The slot's `(x, y)` is the padded CELL's top-left corner; the content
/// pixels live at `(x + 1, y + 1)` (the `paint` / UV math — the
/// `glyphs::uv_rect` calls — carry the `+1`).
pub struct GlyphAtlas {
    width: u32,
    height: u32,
    /// RGBA8, top-down, `width * height * 4` bytes.
    pixels: Vec<u8>,
    /// Pixels changed since the last GL upload (any reserve/paint/grow).
    /// The uploader's gate — the size-only gate missed the no-growth
    /// insertions (on-device finding: the glyphs first used AFTER the
    /// initial upload — V, T, R, N on the real menu — had cells + UVs but
    /// never-reached pixels: invisible letters).
    dirty: bool,
    slots: Vec<Slot>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

impl GlyphAtlas {
    /// The empty start atlas (the `Font`'s internal texture's analogue —
    /// it grows on demand, the port's analogue of SFML's in-font texture
    /// `create`/`update` sequence).
    #[must_use]
    pub fn new() -> Self {
        Self {
            width: ATLAS_START_W,
            height: ATLAS_START_H,
            pixels: vec![0u8; (ATLAS_START_W * ATLAS_START_H * 4) as usize],
            dirty: false,
            slots: Vec::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        }
    }

    /// `(width, height)`.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The raw RGBA8 buffer (top-down; the GL upload source — unflipped,
    /// module docs).
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Take the dirty flag (the uploader's re-upload gate; `true` also on
    /// the very first upload — the empty atlas starts dirty).
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// The reserved slot at `idx` (the `reserve` return's index).
    #[must_use]
    pub fn slot(&self, idx: usize) -> Option<&Slot> {
        self.slots.get(idx)
    }

    /// Reserve a content `(w, h)` slot; returns the padded cell's top-left
    /// (the content pixels at `(x + 1, y + 1)`) + the slot index (for
    /// [`Self::paint`]). Grows the atlas (height, or width with a full
    /// re-pack) as needed.
    ///
    /// # Panics
    /// If the content exceeds [`ATLAS_MAX`] on either axis (a TTF raster
    /// cannot — the guard catches a corrupt metrics path instead of
    /// overflowing the texture).
    #[must_use]
    pub fn reserve(&mut self, w: u32, h: u32) -> ((u32, u32), usize) {
        let sw = w + 2; // the padded cell width
        if sw > self.width {
            self.grow_width(sw);
        }
        if self.cursor_x + sw > self.width {
            self.cursor_x = 0;
            self.cursor_y += self.row_height + 2;
            self.row_height = 0;
        }
        if self.cursor_y + h + 2 > self.height {
            self.grow_height(self.cursor_y + h + 2);
        }
        let idx = self.slots.len();
        let pos = (self.cursor_x, self.cursor_y);
        self.slots.push(Slot {
            x: pos.0,
            y: pos.1,
            w,
            h,
        });
        self.dirty = true;
        self.cursor_x += sw;
        self.row_height = self.row_height.max(h);
        (pos, idx)
    }

    /// Paint the content's RGBA8 bytes into the reserved slot (`idx` from
    /// [`Self::reserve`] with the same `w`/`h`; the content lands at the
    /// cell's `(x + 1, y + 1)` — the padded corner is left zero).
    ///
    /// # Panics
    /// If `rgba.len() != w * h * 4` (the slot's content size).
    pub fn paint(&mut self, idx: usize, rgba: &[u8]) {
        self.dirty = true;
        let slot = self.slots[idx];
        assert_eq!(
            rgba.len(),
            (slot.w * slot.h * 4) as usize,
            "atlas paint: buffer is not the slot's content size"
        );
        let row = (slot.w * 4) as usize;
        for y in 0..slot.h {
            let dst = (((slot.y + 1 + y) * self.width + slot.x + 1) * 4) as usize;
            let src = (y as usize) * row;
            self.pixels[dst..dst + row].copy_from_slice(&rgba[src..src + row]);
        }
    }

    /// Grow the height (the content above is kept; the new rows are zero).
    fn grow_height(&mut self, min_h: u32) {
        self.dirty = true;
        let new_h = (self.height * 2).max(min_h).min(ATLAS_MAX);
        assert!(
            min_h <= ATLAS_MAX,
            "atlas grow: content exceeds the {ATLAS_MAX}px texture cap"
        );
        assert!(new_h > self.height, "atlas grow: no room for the content");
        let old = std::mem::take(&mut self.pixels);
        self.pixels = vec![0u8; (new_h * self.width * 4) as usize];
        self.pixels[..old.len()].copy_from_slice(&old);
        self.height = new_h;
    }

    /// Grow the width (a glyph wider than the atlas): re-pack EVERY
    /// existing slot into the new width (the pixel blocks are moved,
    /// content-preserving, in the original order — the slot indices stay
    /// valid, the consumers' `paint`/coordinate reads see the new cells).
    fn grow_width(&mut self, min_w: u32) {
        self.dirty = true;
        let mut new_w = ATLAS_START_W;
        while new_w < min_w {
            new_w *= 2;
        }
        assert!(
            new_w <= ATLAS_MAX,
            "atlas grow: content exceeds the {ATLAS_MAX}px texture cap"
        );
        // Extract every slot's content block (the OLD coordinates).
        let mut blocks: Vec<Vec<u8>> = Vec::with_capacity(self.slots.len());
        for s in &self.slots {
            let mut block = vec![0u8; (s.w * s.h * 4) as usize];
            if s.w > 0 && s.h > 0 {
                let row = (s.w * 4) as usize;
                for y in 0..s.h {
                    let src = (((s.y + 1 + y) * self.width + s.x + 1) * 4) as usize;
                    let dst = (y as usize) * row;
                    block[dst..dst + row].copy_from_slice(&self.pixels[src..src + row]);
                }
            }
            blocks.push(block);
        }
        // Clear the slots (the re-pack re-records them, in order — the
        // `reserve` pushes land exactly where the old indices point).
        let old_slots = std::mem::take(&mut self.slots);
        self.width = new_w;
        self.pixels = vec![0u8; (new_w * self.height * 4) as usize];
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_height = 0;
        for (i, s) in old_slots.iter().enumerate() {
            let ((_x, _y), idx) = self.reserve(s.w, s.h);
            assert_eq!(idx, i, "the re-pack preserves the slot order");
            self.paint(idx, &blocks[i]);
        }
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// The outline dilation (SFML `Font::generateOutline`).
// ---------------------------------------------------------------------------

/// The outline bitmap of a glyph's RGBA8 content: for every pixel with
/// alpha > 0, paint a `(2t+1) × (2t+1)` square of OPAQUE white at distance
/// `t` (the SFML `generateOutline` rule — the source alpha value is not
/// propagated, the outline is the solid mask). Output size `(w + 2t, h +
/// 2t)`, RGBA8 `(255, 255, 255, 255)` on the dilated pixels, zero
/// elsewhere.
#[must_use]
pub fn dilate_alpha(rgba: &[u8], w: u32, h: u32, t: u32) -> Vec<u8> {
    let ow = w + 2 * t;
    let oh = h + 2 * t;
    let mut out = vec![0u8; (ow * oh * 4) as usize];
    if w == 0 || h == 0 {
        return out;
    }
    if t == 0 {
        // The identity dilation (no outline growth): the content unchanged.
        return rgba.to_vec();
    }
    for y in 0..h {
        for x in 0..w {
            let a = rgba[(((y * w) + x) as usize) * 4 + 3];
            if a == 0 {
                continue;
            }
            for oy in y..(y + 2 * t + 1) {
                for ox in x..(x + 2 * t + 1) {
                    let i = (((oy * ow) + ox) as usize) * 4;
                    out[i] = 255;
                    out[i + 1] = 255;
                    out[i + 2] = 255;
                    out[i + 3] = 255;
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The per-(font, size) glyph store (the SFML `Font` glyph-cache analogue).
// ---------------------------------------------------------------------------

/// The `sf::Font` identity for the cache keys: the `fontdue::Font`
/// allocation address (the `Font`'s `Rc` interior — stable for the font's
/// lifetime; two `Font` objects that loaded the same file are DISTINCT
/// SFML `sf::Font`s, so they get distinct caches, as in the C++).
#[derive(Clone, Copy)]
struct FontId(*const fontdue::Font);

impl Hash for FontId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as usize).hash(state);
    }
}

impl PartialEq for FontId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for FontId {}

#[derive(Clone, Copy)]
struct FillKey {
    font: FontId,
    size: u32,
    ch: u32,
}

#[derive(Clone, Copy)]
struct OutlineKey {
    font: FontId,
    size: u32,
    ch: u32,
    t: u32,
}

impl PartialEq for FillKey {
    fn eq(&self, o: &Self) -> bool {
        self.font == o.font && self.size == o.size && self.ch == o.ch
    }
}
impl Eq for FillKey {}
impl Hash for FillKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font.hash(state);
        self.size.hash(state);
        self.ch.hash(state);
    }
}

impl PartialEq for OutlineKey {
    fn eq(&self, o: &Self) -> bool {
        self.font == o.font && self.size == o.size && self.ch == o.ch && self.t == o.t
    }
}
impl Eq for OutlineKey {}
impl Hash for OutlineKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font.hash(state);
        self.size.hash(state);
        self.ch.hash(state);
        self.t.hash(state);
    }
}

/// A cached fill glyph (one fontdue `rasterize` per `(font, size, char)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillEntry {
    /// The slot index into the shared atlas (the stable reference — the
    /// width-growth re-pack preserves the slot ORDER, module docs; the
    /// cell's current coordinates are read from the atlas at the layout's
    /// end, `text_quads`'s second pass). Valid only when `w * h > 0`.
    pub slot: usize,
    /// The bitmap size (0 when the glyph has no texture area — the space).
    pub w: u32,
    pub h: u32,
    /// fontdue `Metrics.xmin` — the left-edge offset (the pen → bitmap-
    /// left offset; SFML's `bearingX`).
    pub left: i32,
    /// The baseline → bitmap-top distance (fontdue 0.9's `ymin` is the
    /// BOTTOM-edge offset in typography coordinates — y up, baseline 0: the
    /// top edge sits at `ymin + height` above the baseline; SFML's
    /// `bearingY` is that same distance measured the other way).
    pub top: i32,
    /// fontdue `Metrics.advance_width` (SFML's `advanceWidth`; the
    /// C++'s zero letter-spacing keeps the advance as-is).
    pub advance: f32,
}

/// The `(font, size)` glyph cache + the shared atlas (the SFML `Font`'s
/// internal texture + glyph map, lifted into the port's target so many
/// `Text`s share it — the C++ shares one `sf::Font` per face).
pub struct GlyphStore {
    atlas: GlyphAtlas,
    fills: HashMap<FillKey, FillEntry>,
    /// `(font, size, char, t)` → the outline slot's INDEX + content size
    /// (absent when the fill has no texture area — no outline for a space,
    /// as in SFML). The slot index, never the cell: the width-growth
    /// re-pack moves the cell (module docs' font-cache pin).
    outlines: HashMap<OutlineKey, (usize, u32, u32)>,
    /// `(font, size)` → fontdue line metrics `(ascent, descent)` (the
    /// descent is fontdue's negative below-baseline value).
    lines: HashMap<(FontId, u32), (f32, f32)>,
}

impl Default for GlyphStore {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            atlas: GlyphAtlas::new(),
            fills: HashMap::new(),
            outlines: HashMap::new(),
            lines: HashMap::new(),
        }
    }

    /// The shared atlas (the GL upload source + growth detector).
    #[must_use]
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// The shared atlas, mutably (the uploader's dirty-take).
    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.atlas
    }

    /// The `(font, size)` line metrics (SFML's `Font::getLineMetrics` —
    /// fontdue's `horizontal_line_metrics`); `(px, 0.0)` when fontdue
    /// reports none (size 0 / an invalid face — the `Text::measure`
    /// fallback).
    #[must_use]
    pub fn line_metrics(&mut self, font: &Font, size: u32) -> (f32, f32) {
        let id = FontId(font.inner() as *const fontdue::Font);
        *self.lines.entry((id, size)).or_insert_with(|| {
            font.line_metrics(size as f32)
                .map(|lm| (lm.ascent, lm.descent))
                .unwrap_or((size as f32, 0.0))
        })
    }

    /// The cached fill glyph (fontdue `rasterize` on a miss — the
    /// `Font::getGlyph` cache hit/miss, the module docs' font-cache pin).
    /// Returns the entry BY COPY (the slot index — the stable reference;
    /// the cell's current coordinates are the atlas's, read at the layout's
    /// end).
    #[must_use]
    pub fn fill(&mut self, font: &Font, size: u32, ch: char) -> FillEntry {
        let id = FontId(font.inner() as *const fontdue::Font);
        let key = FillKey {
            font: id,
            size,
            ch: ch as u32,
        };
        if let Some(entry) = self.fills.get(&key) {
            return *entry;
        }
        let (metrics, bitmap) = font.rasterize(ch, size as f32);
        // fontdue 0.9's `Metrics`: `xmin` = the bearingX (the left-edge
        // offset), `ymin` = the BOTTOM-edge offset (typography y-up) — the
        // baseline → top distance is `ymin + height` (the module docs' pin).
        let top = metrics.ymin + metrics.height as i32;
        let entry = if metrics.width == 0 || metrics.height == 0 {
            // The "no texture area" glyph (the space): SFML advances the
            // pen, emits no quad, stores no bitmap (no slot).
            FillEntry {
                slot: usize::MAX,
                w: 0,
                h: 0,
                left: metrics.xmin,
                top,
                advance: metrics.advance_width,
            }
        } else {
            let (_pos, idx) = self
                .atlas
                .reserve(metrics.width as u32, metrics.height as u32);
            // The alpha mask → the atlas's straight-alpha glyph pixels
            // `(255, 255, 255, mask)` (the module docs' straight-alpha pin:
            // the pass's `tex × color` product yields the fill colour at
            // the mask's alpha).
            let mut rgba = vec![0u8; metrics.width * metrics.height * 4];
            for (i, &a) in bitmap.iter().enumerate() {
                rgba[i * 4] = 255;
                rgba[i * 4 + 1] = 255;
                rgba[i * 4 + 2] = 255;
                rgba[i * 4 + 3] = a;
            }
            self.atlas.paint(idx, &rgba);
            FillEntry {
                slot: idx,
                w: metrics.width as u32,
                h: metrics.height as u32,
                left: metrics.xmin,
                top,
                advance: metrics.advance_width,
            }
        };
        self.fills.insert(key, entry);
        entry
    }

    /// The cached outline slot (`None` when the fill has no texture area):
    /// the atlas slot index + the dilated content size `(w + 2t, h + 2t)`
    /// (the cell is the atlas's — read at the layout's end, the module
    /// docs' font-cache pin).
    #[must_use]
    pub fn outline(
        &mut self,
        font: &Font,
        size: u32,
        ch: char,
        t: u32,
    ) -> Option<(usize, u32, u32)> {
        let fill = self.fill(font, size, ch);
        let (fw, fh) = (fill.w, fill.h);
        if fw == 0 || fh == 0 || t == 0 {
            return None;
        }
        let id = FontId(font.inner() as *const fontdue::Font);
        let key = OutlineKey {
            font: id,
            size,
            ch: ch as u32,
            t,
        };
        if let Some(slot) = self.outlines.get(&key) {
            return Some(*slot);
        }
        // The outline dilates the ALPHA of the cached fill bitmap (read it
        // back from the atlas's CURRENT cell — the slot index is the
        // stable reference; the fill was painted there by `fill`, and a
        // width-growth re-pack may have moved it).
        let cell = self.atlas.slot(fill.slot).expect("the fill's slot lives");
        let (fw, fh) = (cell.w, cell.h);
        let row = (fw * 4) as usize;
        let mut src = vec![0u8; row * fh as usize];
        for y in 0..fh {
            let a = (((cell.y + 1 + y) * self.atlas.size().0 + cell.x + 1) * 4) as usize;
            let b = (y as usize) * row;
            src[b..b + row].copy_from_slice(&self.atlas.pixels()[a..a + row]);
        }
        let dilated = dilate_alpha(&src, fw, fh, t);
        let idx = self.atlas.reserve(fw + 2 * t, fh + 2 * t).1;
        self.atlas.paint(idx, &dilated);
        let slot = (idx, fw + 2 * t, fh + 2 * t);
        self.outlines.insert(key, slot);
        Some(slot)
    }
}

// ---------------------------------------------------------------------------
// The text → quads layout (the SFML `Text::draw` geometry, pre-transform).
// ---------------------------------------------------------------------------

/// The atlas content rectangle of a glyph quad: the content's first pixel
/// at `(x, y)` in the atlas's RGBA8 buffer (the slot's 1-px zero padding
/// excluded, `w × h` content pixels) — the [`uv_rect`] input. The
/// coordinates are the atlas's CURRENT ones (read at the layout's end —
/// the width-growth re-pack's moved cells, the height growth's unchanged
/// top rows, the module docs' font-cache pin).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// One glyph quad in LOCAL text coordinates (the `Text`'s Transformable is
/// applied by the caller — SFML's `Transformable::getTransform` at draw
/// time), emission order preserved (per character: the outline quad, then
/// the fill quad; lines top to bottom; characters left to right). The
/// atlas cell (not a UV): the caller ([`crate::gl_passes`]) turns it into
/// the quad's UV with the atlas's final size (the texture the upload uses
/// — a mid-string atlas growth can never leave a quad's UV normalised by
/// a stale size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextQuad {
    /// The quad corners, the port's `(nw, sw, se, ne)` y-down order.
    pub nw: Vec2,
    pub sw: Vec2,
    pub se: Vec2,
    pub ne: Vec2,
    /// The glyph's atlas content rectangle (`uv_rect(cell.x, cell.y,
    /// cell.w, cell.h, atlasW, atlasH)` — the 1-px zero padding excluded;
    /// the bilinear sample lands on the content).
    pub cell: CellRect,
    /// `outline == true`: the outline quad (the text's outline colour);
    /// `false`: the fill quad (the text's fill colour).
    pub outline: bool,
}

/// The text's glyph quads (the SFML `Text::draw` geometry — the module
/// docs' per-glyph pins). The layout consumes the `GlyphStore` (raster
/// cache; the atlas may grow as a side effect — the GL layer re-uploads on
/// growth, and the cells are read AFTER every growth, the module docs'
/// font-cache pin).
pub fn text_quads(text: &Text, store: &mut GlyphStore) -> Vec<TextQuad> {
    let string = text.get_string();
    let size = text.get_character_size();
    if string.is_empty() || size == 0 {
        // SFML: an empty string / a zero character size draws nothing
        // (fontdue's raster is degenerate at px 0 — the guard, not the
        // raster, owns the no-op).
        return Vec::new();
    }
    let font = text.get_font();
    let (ascent, descent) = store.line_metrics(font, size);
    let line_h = ascent - descent;
    let thickness = text.get_outline_thickness();
    let outline_t = if thickness > 0.0 {
        // The C++'s integral outline (the shipped values 1/2/4 are exact;
        // a fractional value rounds, as the integer cast does).
        (thickness + 0.5) as u32
    } else {
        0
    };
    // The first pass: the raster / reserve / paint (the atlas may grow
    // height-first, or re-pack its width, per character). The geometry
    // (the pen's position, the bearing offsets) is size-stable — only the
    // atlas cells are growth-sensitive (the second pass's reads).
    struct Pending {
        entry: FillEntry,
        outline_slot: Option<(usize, u32, u32)>,
        left: f32,
        top: f32,
    }
    let mut pending: Vec<Pending> = Vec::new();
    let mut line_y = 0.0f32;
    for line in string.split('\n') {
        let mut pen_x = 0.0f32;
        for ch in line.chars() {
            let entry = store.fill(font, size, ch);
            let (ew, eh, eleft, etop, eadv) =
                (entry.w, entry.h, entry.left, entry.top, entry.advance);
            if ew > 0 && eh > 0 {
                // SFML `Text::draw`'s quad position: `glyphLeft = x +
                // bearingX` (the pen BEFORE the advance), `glyphTop = y +
                // bearingY` (the line top; `bearingY = ascent − top`, the
                // module docs' pin).
                let left = pen_x + eleft as f32;
                let top = line_y + (ascent - etop as f32);
                let outline_slot = if outline_t > 0 {
                    store.outline(font, size, ch, outline_t)
                } else {
                    None
                };
                pending.push(Pending {
                    entry,
                    outline_slot,
                    left,
                    top,
                });
            }
            // SFML: the pen advances AFTER the glyph (`x += advance`).
            pen_x += eadv;
        }
        line_y += line_h;
    }
    // The second pass: every rasterize / reserve / paint is done (all the
    // growth / re-packs included) — read each slot's CURRENT cell into the
    // quads (the re-pack's moved cells, the growth's unchanged top rows).
    let mut quads = Vec::with_capacity(pending.len() * 2);
    for p in &pending {
        if let Some((oslot, ow, oh)) = p.outline_slot {
            let cell = store.atlas.slot(oslot).expect("the outline slot lives");
            quads.push(TextQuad {
                nw: Vec2::new(p.left - outline_t as f32, p.top - outline_t as f32),
                sw: Vec2::new(
                    p.left - outline_t as f32,
                    p.top - outline_t as f32 + oh as f32,
                ),
                se: Vec2::new(
                    p.left - outline_t as f32 + ow as f32,
                    p.top - outline_t as f32 + oh as f32,
                ),
                ne: Vec2::new(
                    p.left - outline_t as f32 + ow as f32,
                    p.top - outline_t as f32,
                ),
                cell: CellRect {
                    x: cell.x + 1,
                    y: cell.y + 1,
                    w: ow,
                    h: oh,
                },
                outline: true,
            });
        }
        let cell = store.atlas.slot(p.entry.slot).expect("the fill slot lives");
        quads.push(TextQuad {
            nw: Vec2::new(p.left, p.top),
            sw: Vec2::new(p.left, p.top + p.entry.h as f32),
            se: Vec2::new(p.left + p.entry.w as f32, p.top + p.entry.h as f32),
            ne: Vec2::new(p.left + p.entry.w as f32, p.top),
            cell: CellRect {
                x: cell.x + 1,
                y: cell.y + 1,
                w: p.entry.w,
                h: p.entry.h,
            },
            outline: false,
        });
    }
    quads
}

// ---------------------------------------------------------------------------
// The sprite → quad (the sf::Sprite geometry).
// ---------------------------------------------------------------------------

/// The sprite's textured quad: the texture-rect's corners through the
/// sprite's Transformable (position / rotation / scale / origin — the
/// C++ `sf::Sprite`'s `translate(position) · rotate · scale ·
/// translate(-origin)`), + the UV of the texture rect in the full
/// texture (top-down `v`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteQuad {
    /// `(nw, sw, se, ne)` — world coordinates, the port's y-down quad
    /// order.
    pub corners: [Vec2; 4],
    /// `(u0, v_top, u1, v_bottom)` of the texture rect.
    pub uv: (f32, f32, f32, f32),
    /// `sf::Sprite::getColor()` (the per-vertex tint — white default).
    pub color: Color,
}

/// The sprite's quad (the C++ `sf::Sprite`'s geometry — the module docs'
/// transform pin). `getTextureRect`/`getTexture`/`transformPoint` are the
/// exact C++ surface calls.
#[must_use]
pub fn sprite_quad(sprite: &crate::texture::Sprite) -> SpriteQuad {
    let (tex_w, tex_h) = sprite.get_texture().get_size();
    let rect: FloatRect = sprite.get_texture_rect();
    let w = rect.get_width();
    let h = rect.get_height();
    // The local corners of the texture rect (top-left at the origin, y
    // down): nw (0,0), sw (0,h), se (w,h), ne (w,0).
    let nw = sprite.transform_point(Vec2::new(0.0, 0.0));
    let sw = sprite.transform_point(Vec2::new(0.0, h));
    let se = sprite.transform_point(Vec2::new(w, h));
    let ne = sprite.transform_point(Vec2::new(w, 0.0));
    SpriteQuad {
        corners: [nw, sw, se, ne],
        uv: (
            rect.get_left() / tex_w as f32,
            rect.get_top() / tex_h as f32,
            (rect.get_left() + w) / tex_w as f32,
            (rect.get_top() + h) / tex_h as f32,
        ),
        color: sprite.get_color(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        // The repo's bundled face (the `font.rs` tests' source).
        let root = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../_RELEASE/Assets/OpenSquare-Regular.ttf"
        );
        Font::load(std::path::Path::new(root)).expect("OpenSquare-Regular.ttf must load")
    }

    // ---- the UV math -------------------------------------------------------

    #[test]
    fn uv_rect_is_top_down_and_fractional() {
        // A 128×128 atlas, a 4×4 content slot at (1, 1): the UV is the
        // content (the 1-px padding excluded), v grows downward.
        let (u0, v_top, u1, v_bot) = uv_rect(1, 1, 4, 4, 128, 128);
        assert!((u0 - 1.0 / 128.0).abs() < 1e-6);
        assert!((v_top - 1.0 / 128.0).abs() < 1e-6);
        assert!((u1 - 5.0 / 128.0).abs() < 1e-6);
        assert!((v_bot - 5.0 / 128.0).abs() < 1e-6);
        // The quad corners' UV mapping (the `gl_passes` vertex build):
        // nw/ne take v_top, sw/se take v_bot.
        assert!(v_top < v_bot);
    }

    // ---- the atlas ---------------------------------------------------------

    #[test]
    fn atlas_shelf_packs_left_to_right_then_down() {
        let mut a = GlyphAtlas::new();
        let ((x0, y0), _i0) = a.reserve(10, 8);
        let ((x1, y1), _i1) = a.reserve(10, 8);
        let ((x2, y2), _i2) = a.reserve(10, 20);
        let ((x3, y3), _i3) = a.reserve(1000, 5);
        // The shelf: left to right (each slot takes its content + 2 padding
        // px per axis);
        assert_eq!((x0, y0), (0, 0));
        assert_eq!((x1, y1), (12, 0)); // + (10 + 2) padded width
        assert_eq!((x2, y2), (24, 0)); // the taller slot extends the shelf
                                       // ...but a slot wider than the remaining row wraps to the next row,
                                       // BELOW the shelf's (content + 2 padding) height: 20 + 2.
        assert_eq!((x3, y3), (0, 22));
    }

    #[test]
    fn atlas_paint_lands_in_the_padded_cell() {
        let mut a = GlyphAtlas::new();
        let ((x, y), idx) = a.reserve(2, 1);
        assert_eq!((x, y), (0, 0));
        let rgba = [
            9, 9, 9, 200, // (0,0)
            1, 2, 3, 255, // (1,0)
        ];
        a.paint(idx, &rgba);
        // The content pixels at the cell's (x + 1, y + 1):
        let i0 = (((y + 1) * a.size().0 + (x + 1)) * 4) as usize;
        assert_eq!(
            &a.pixels()[i0..i0 + 8],
            &[9, 9, 9, 200, 1, 2, 3, 255],
            "the content starts at the cell's (x + 1, y + 1)"
        );
        // The padding (the 1-px zero ring) surrounds the content:
        assert_eq!(
            &a.pixels()[i0 - 4..i0],
            &[0, 0, 0, 0],
            "the left padding column"
        );
        let ib = (((y + 2) * a.size().0 + x) * 4) as usize;
        assert_eq!(
            &a.pixels()[ib..ib + 16],
            &[0u8; 16],
            "the bottom padding row (cell width 4)"
        );
        let it = (x * 4) as usize;
        assert_eq!(
            &a.pixels()[it..it + 16],
            &[0u8; 16],
            "the top padding row (row 0, cell width 4)"
        );
    }

    #[test]
    fn atlas_grows_height_when_the_rows_run_out() {
        let mut a = GlyphAtlas::new();
        // Fill the start height (512) with rows of 1-px-tall slots:
        // each slot is (w + 2) wide, 3 tall (content 1 + 2 padding).
        // 10 slots/row (1020 of 1024 px) × ~171 rows ≈ 1710 slots.
        let mut slots = 0usize;
        while a.size().1 == 512 {
            let w = 100u32;
            let _ = a.reserve(w, 1);
            slots += 1;
            if slots > 2000 {
                break;
            }
        }
        assert!(
            a.size().1 > 512,
            "the atlas grew its height (got {})",
            a.size().1
        );
        assert!(a.size().1 <= ATLAS_MAX);
    }

    /// Issue 36 — the abort's `memory allocation of 8388608 bytes failed`
    /// names the atlas's second height doubling: 8388608 = 1024 × 2048 × 4
    /// exactly. Drive the atlas through both doublings (512 → 1024 →
    /// 2048) and pin the buffer size at each step — the 2048 row IS the
    /// 8 MiB buffer a full menu glyph set forces (the width stays 1024).
    #[test]
    fn atlas_height_growth_to_2048_is_exactly_the_8_mib_abort_size() {
        let mut a = GlyphAtlas::new();
        assert_eq!(a.pixels().len(), 1024 * 512 * 4, "the 2 MiB start atlas");
        loop {
            if a.size().1 == 2048 {
                break;
            }
            let _ = a.reserve(100, 1);
        }
        assert_eq!(
            a.size(),
            (1024, 2048),
            "the 2048 row (the width never moves — only the height doubles)"
        );
        assert_eq!(
            a.pixels().len(),
            8388608,
            "the 8388608-byte (8 MiB) buffer the web abort named"
        );
    }

    #[test]
    fn atlas_width_growth_repacks_and_preserves_content() {
        let mut a = GlyphAtlas::new();
        // Two painted slots, then a glyph WIDER than the start width:
        let ((x0, y0), i0) = a.reserve(8, 4);
        let block0: Vec<u8> = (0..8 * 4 * 4).map(|i| (i % 251) as u8).collect();
        a.paint(i0, &block0);
        let ((x1, y1), i1) = a.reserve(6, 2);
        let block1: Vec<u8> = (100..100 + 6 * 2 * 4).map(|i| (i % 251) as u8).collect();
        a.paint(i1, &block1);
        // A 2000-px-wide content forces the width growth (re-pack):
        let (_x2, _y2, _i2) = (a.reserve(2000, 2), 0, 0);
        let _ = (_y2, _i2);
        // Every slot's content is preserved at its (new) coordinates:
        let check = |a: &GlyphAtlas, s: &Slot, block: &[u8]| {
            let row = (s.w * 4) as usize;
            for y in 0..s.h {
                let p = (((s.y + 1 + y) * a.size().0 + s.x + 1) * 4) as usize;
                let b = (y as usize) * row;
                assert_eq!(&a.pixels()[p..p + row], &block[b..b + row]);
            }
        };
        let s0 = a.slot(0).expect("slot 0 after the re-pack");
        assert_eq!((s0.x, s0.y, s0.w, s0.h), (x0, y0, 8, 4));
        check(&a, s0, &block0);
        let s1 = a.slot(1).expect("slot 1 after the re-pack");
        assert_eq!((s1.x, s1.y, s1.w, s1.h), (x1, y1, 6, 2));
        check(&a, s1, &block1);
    }

    #[test]
    fn uv_rect_samples_the_painted_content_rows() {
        // The orientation pin end-to-end (the "is the text upside-down?"
        // check): a 2×2 glyph with a DISTINCT alpha per content pixel,
        // painted at a NON-ORIGIN cell — the `uv_rect` of the cell's
        // content (the 1-px padding excluded) must map (u0, v_top) to the
        // TOP-LEFT content pixel and (u1, v_bot) just PAST the bottom-right
        // (the v axis grows DOWNWARD — the top-down convention, module
        // docs).
        let mut a = GlyphAtlas::new();
        let ((_x, _y), _i0) = a.reserve(10, 4); // a neighbour, so the second cell is not at (0, 0)
        let ((x, y), idx) = a.reserve(2, 2);
        let rgba = [
            1, 1, 1, 11, // (0,0) top-left
            1, 1, 1, 22, // (1,0) top-right
            1, 1, 1, 33, // (0,1) bottom-left
            1, 1, 1, 44, // (1,1) bottom-right
        ];
        a.paint(idx, &rgba);
        let (atlas_w, atlas_h) = a.size();
        let (u0, v_top, u1, v_bot) = uv_rect(x + 1, y + 1, 2, 2, atlas_w, atlas_h);
        let pixel =
            |px: u32, py: u32| -> u8 { a.pixels()[(((py * atlas_w) + px) * 4) as usize + 3] };
        // The content's first pixel (the cell's x + 1, y + 1 — the 1-px
        // padding excluded) is exactly where (u0, v_top) lands:
        assert_eq!(
            (u0, v_top),
            (
                (x + 1) as f32 / atlas_w as f32,
                (y + 1) as f32 / atlas_h as f32
            )
        );
        // The quad's sampling range spans EXACTLY the 2×2 content pixels:
        // the far corner (u1, v_bot) sits one content pixel PAST the
        // bottom-right (the boundary between the content and its 1-px zero
        // padding — the bilinear interior samples never leave the content,
        // the corner samples land on these content-pixel edges):
        assert_eq!(
            (u1, v_bot),
            (
                (x + 1 + 2) as f32 / atlas_w as f32,
                (y + 1 + 2) as f32 / atlas_h as f32
            )
        );
        // The pixel content at the four corners' content rows: the v axis
        // grows DOWNWARD (the top-down pin) — (u0, v_top) samples the
        // TOP-LEFT content pixel; a flipped UV would hand the bottom row's
        // alpha here:
        assert_eq!(pixel(x + 1, y + 1), 11, "the top-left content pixel");
        assert_eq!(pixel(x + 2, y + 1), 22, "the top-right content pixel");
        assert_eq!(pixel(x + 1, y + 2), 33, "the bottom-left content pixel");
        assert_eq!(pixel(x + 2, y + 2), 44, "the bottom-right content pixel");
        // The 1-px zero ring around the content (the bilinear bleed guard —
        // the quad's edge samples land on this zero padding, never on a
        // neighbour glyph's pixels):
        assert_eq!(pixel(x, y + 1), 0, "the left padding column");
        assert_eq!(pixel(x + 3, y + 1), 0, "the right padding column");
        assert_eq!(pixel(x + 1, y), 0, "the top padding row");
        assert_eq!(pixel(x + 1, y + 3), 0, "the bottom padding row");
        // The v ordering: the quad's nw/ne take v_top, sw/se take v_bot
        // (the `gl_passes` vertex build) — v_top < v_bot (v grows downward):
        assert!(v_top < v_bot);
    }

    // ---- the dilation ------------------------------------------------------

    #[test]
    fn dilate_grows_by_two_t_per_side_and_solidifies() {
        // A 2×2 half-alpha square, t = 1 → a 4×4 OPAQUE square (the SFML
        // rule: the dilated pixels are 255, the source alpha not kept).
        let src = [1, 1, 1, 128, 2, 2, 2, 128, 3, 3, 3, 128, 4, 4, 4, 128];
        let out = dilate_alpha(&src, 2, 2, 1);
        assert_eq!(out.len(), 4 * 4 * 4);
        // Every pixel is opaque white:
        assert!(out.iter().all(|&b| b == 255));
    }

    #[test]
    fn dilate_zero_thickness_and_empty_input() {
        let src = [0u8, 0, 0, 255, 0, 0, 0, 255];
        let out = dilate_alpha(&src, 2, 1, 0);
        assert_eq!(out.len(), 8);
        assert_eq!(&out, &src);
        let empty = dilate_alpha(&[], 0, 0, 3);
        assert!(empty.iter().all(|&b| b == 0));
        assert_eq!(empty.len(), 144);
    }

    #[test]
    fn dilate_t2_reaches_distance_two() {
        // A single 1×1 opaque pixel, t = 2 → a 5×5 opaque square.
        let src = [255u8, 0, 0, 255];
        let out = dilate_alpha(&src, 1, 1, 2);
        assert_eq!(out.len(), 5 * 5 * 4);
        // The centre 5×5 region is fully opaque; (there is no outside —
        // the output IS the dilated box):
        assert!(out.iter().all(|&b| b == 255));
    }

    // ---- the text layout ---------------------------------------------------

    #[test]
    fn text_layout_advances_the_pen_and_breaks_lines() {
        let f = font();
        let mut t = Text::new(f.clone(), "H H\nAB", 32);
        t.set_fill_color(Color::WHITE);
        let mut store = GlyphStore::new();
        let quads = text_quads(&t, &mut store);
        // "H H" = H, space, H → 2 fill quads on line 0; "AB" → 2 on line 1.
        assert_eq!(quads.len(), 4);
        assert!(quads.iter().all(|q| !q.outline));
        let (ascent, descent) = store.line_metrics(&f, 32);
        let line_h = ascent - descent;
        // The first H: top-left at its bearing (left, ascent − top), the
        // fontdue-0.9 `Metrics` mapping (the module docs' pin: `top = ymin
        // + height` — the baseline → bitmap-top distance).
        let m_h = f.glyph_metrics('H', 32.0);
        let top_from_line = |m: &fontdue::Metrics| ascent - (m.ymin + m.height as i32) as f32;
        assert!((quads[0].nw.x - m_h.xmin as f32).abs() < 1e-4);
        assert!((quads[0].nw.y - top_from_line(&m_h)).abs() < 1e-4);
        assert!((quads[0].se.x - quads[0].nw.x - m_h.width as f32).abs() < 1e-4);
        assert!((quads[0].sw.y - quads[0].nw.y - m_h.height as f32).abs() < 1e-4);
        // The space advanced the pen: the second H starts at the first's
        // advance + the space's advance.
        let m_sp = f.glyph_metrics(' ', 32.0);
        let expect_x = m_h.advance_width + m_sp.advance_width + m_h.xmin as f32;
        assert!((quads[1].nw.x - expect_x).abs() < 1e-3);
        // Line 1: the same y advance (the line height), x back to 0 + A's
        // bearing; B after A's advance.
        let m_a = f.glyph_metrics('A', 32.0);
        // Line 1: the LINE's top is at line_h; the glyph's top adds the
        // bearing (ascent − top), the module docs' pin.
        assert!((quads[2].nw.y - (line_h + top_from_line(&m_a))).abs() < 1e-4);
        assert!((quads[2].nw.x - m_a.xmin as f32).abs() < 1e-4);
        let m_b = f.glyph_metrics('B', 32.0);
        assert!((quads[3].nw.x - (m_a.advance_width + m_b.xmin as f32)).abs() < 1e-3);
        // The cells are inside the atlas and top-down UV-orderable (the
        // GL pass's `uv_rect` input — the content rect, 1-px padding
        // excluded, never past the atlas's right/bottom edges):
        let (atlas_w, atlas_h) = store.atlas.size();
        for q in &quads {
            let c = q.cell;
            assert!(c.x + c.w < atlas_w, "the content stays off the right edge");
            assert!(c.y + c.h < atlas_h, "the content stays off the bottom edge");
            let (u0, v_t, u1, v_b) = uv_rect(c.x, c.y, c.w, c.h, atlas_w, atlas_h);
            assert!((0.0..=1.0).contains(&u0) && (0.0..=1.0).contains(&u1));
            assert!(v_t <= v_b);
        }
    }

    #[test]
    fn text_layout_cells_survive_the_width_repack() {
        // The module docs' font-cache pin: the cache keys the atlas by
        // slot INDEX (never coordinates) — the width-growth re-pack moves
        // the slots' cells (the content-preserving move), and the layout's
        // second pass must read the MOVED cells (a coordinate-keyed cache
        // would paint the moved glyphs' UVs at the old positions).
        let f = font();
        let mut store = GlyphStore::new();
        // Seven 200-px fills (each ~152-px cell): the first row of the
        // 1024-px start width fits six — the seventh lands on row 2.
        let mut last = None;
        for ch in ['H', 'A', 'B', 'C', 'D', 'E', 'F'] {
            last = Some(store.fill(&f, 260, ch));
        }
        let f_entry = last.expect("the last fill");
        // `Slot` is `Copy` — the BY-VALUE copy ends the atlas borrow (the
        // reserve/paint below mutate the atlas; a held `&Slot` would outlive
        // the re-pack's moved cell).
        let moved_before = *store.atlas.slot(f_entry.slot).expect("the F slot");
        assert!(
            moved_before.y > 0,
            "the seventh slot wrapped to row 2 at the start width (the pin's premise)"
        );
        // A 1500-px content forces the width growth (re-pack into the
        // 2048-px canvas) — the wider first row pulls the later slots
        // (incl. the cached F) back to row 1: their cells MOVE.
        let (_pos, wide_idx) = store.atlas.reserve(1500, 4);
        let wide_bytes: Vec<u8> = (0..1500 * 4 * 4).map(|i| (i % 251) as u8).collect();
        store.atlas.paint(wide_idx, &wide_bytes);
        let moved_after = *store
            .atlas
            .slot(f_entry.slot)
            .expect("the F slot after the re-pack");
        // The re-pack preserves the slot ORDER (the module docs' pin): F
        // moves from row 2 (the start-width wrap, x = 0) to row 1, where
        // its x is the six preceding cells' padded widths (content + 2
        // padding per slot).
        let expected_x: u32 = (0..6)
            .map(|i| store.atlas.slot(i).expect("an earlier slot").w + 2)
            .sum();
        assert_eq!(
            (moved_after.x, moved_after.y),
            (expected_x, 0),
            "the re-pack moved the F slot back to row 1 (the cells moved)"
        );
        // The cache HIT returns the same slot index — and the slot's
        // content is the ORIGINAL raster (the re-pack preserved it):
        let f_hit = store.fill(&f, 260, 'F');
        assert_eq!(
            f_hit.slot, f_entry.slot,
            "the cache hit keeps the slot index"
        );
        let (metrics, bitmap) = f.rasterize('F', 260.0);
        for y in 0..metrics.height as u32 {
            let p =
                (((moved_after.y + 1 + y) * store.atlas.size().0 + moved_after.x + 1) * 4) as usize;
            for x in 0..metrics.width as u32 {
                let i = p + (x as usize) * 4;
                let pix = &store.atlas.pixels()[i..i + 4];
                assert_eq!(
                    &pix[..3],
                    &[255, 255, 255],
                    "the RGB stays the straight-alpha white"
                );
                assert_eq!(
                    pix[3],
                    bitmap[y as usize * metrics.width + x as usize],
                    "the re-pack preserved the alpha mask"
                );
            }
        }
        // The layout's second pass reads the MOVED cell for the cached
        // fill (the quad's cell == the slot's current cell, + the 1-px
        // padding offset):
        let quads = text_quads(&Text::new(f.clone(), "F", 260), &mut store);
        assert_eq!(quads.len(), 1);
        assert_eq!(
            quads[0].cell,
            CellRect {
                x: moved_after.x + 1,
                y: moved_after.y + 1,
                w: moved_after.w,
                h: moved_after.h,
            }
        );
    }

    #[test]
    fn text_layout_cells_survive_the_height_growth() {
        // The height-growth path: rows run past the 512-px start height
        // MID-STRING — the first pass's early glyphs were laid out at the
        // old height, the second pass's cell reads are atlas-relative (no
        // normalisation at layout time), and the GL pass's UV uses the
        // FINAL size (the texture the upload uses) — so no quad's UV is
        // normalised by a stale height.
        let f = font();
        let mut store = GlyphStore::new();
        // Fill the start height (512 px) with 1-px-tall slots (each 3 px
        // tall with padding): ~170 rows × 10 slots/row.
        let mut slots = 0usize;
        while store.atlas.size().1 == 512 && slots < 5000 {
            let _ = store.atlas.reserve(100, 1);
            slots += 1;
        }
        let grown = store.atlas.size().1;
        assert!(grown > 512, "the atlas grew its height");
        // Now the text (fresh glyphs after the growth): every quad's cell
        // is inside the FINAL buffer, and its UV (at the final size) is in
        // range + top-down ordered.
        let quads = text_quads(&Text::new(f.clone(), "XYZ", 32), &mut store);
        assert_eq!(quads.len(), 3);
        let (atlas_w, atlas_h) = store.atlas.size();
        assert_eq!(atlas_h, grown);
        for q in &quads {
            let c = q.cell;
            assert!(c.x + c.w < atlas_w);
            assert!(c.y + c.h < atlas_h);
            let (u0, v_t, u1, v_b) = uv_rect(c.x, c.y, c.w, c.h, atlas_w, atlas_h);
            assert!(u0 >= 0.0 && u1 <= 1.0 && v_t >= 0.0 && v_b <= 1.0 && v_t <= v_b);
        }
    }

    #[test]
    fn text_layout_emits_the_outline_before_the_fill_per_glyph() {
        let f = font();
        let mut t = Text::new(f, "X", 32);
        t.set_outline_thickness(2.0);
        t.set_fill_color(Color::WHITE);
        t.set_outline_color(Color::BLACK);
        let mut store = GlyphStore::new();
        let quads = text_quads(&t, &mut store);
        // X: the outline quad (thickness 2) FIRST, then the fill.
        assert_eq!(quads.len(), 2);
        assert!(quads[0].outline);
        assert!(!quads[1].outline);
        // The outline is the fill's box offset by (−2, −2), grown 4 per
        // side (2t per axis):
        assert!((quads[0].nw.x - (quads[1].nw.x - 2.0)).abs() < 1e-4);
        assert!((quads[0].nw.y - (quads[1].nw.y - 2.0)).abs() < 1e-4);
        let ow = quads[0].se.x - quads[0].nw.x;
        let oh = quads[0].sw.y - quads[0].nw.y;
        let fw = quads[1].se.x - quads[1].nw.x;
        let fh = quads[1].sw.y - quads[1].nw.y;
        assert!((ow - (fw + 4.0)).abs() < 1e-4);
        assert!((oh - (fh + 4.0)).abs() < 1e-4);
    }

    #[test]
    fn text_layout_zero_size_and_empty_string_draw_nothing() {
        let f = font();
        let mut store = GlyphStore::new();
        assert!(text_quads(&Text::new(f.clone(), "HI", 0), &mut store).is_empty());
        assert!(text_quads(&Text::new(f, "", 32), &mut store).is_empty());
    }

    #[test]
    fn text_layout_fractional_thickness_rounds() {
        // thickness 1.5 → the integral 2 (the C++'s integer cast, the
        // module docs' outline pin): the same geometry as 2.0.
        let f = font();
        let mut t = Text::new(f.clone(), "X", 32);
        t.set_outline_thickness(1.5);
        let mut s1 = GlyphStore::new();
        let q1 = text_quads(&t, &mut s1);
        let mut t = Text::new(f, "X", 32);
        t.set_outline_thickness(2.0);
        let mut s2 = GlyphStore::new();
        let q2 = text_quads(&t, &mut s2);
        assert_eq!(
            q1.iter().map(|q| (q.nw, q.se)).collect::<Vec<_>>(),
            q2.iter().map(|q| (q.nw, q.se)).collect::<Vec<_>>(),
            "1.5 rounds to the 2-px outline, exactly as the integral case"
        );
    }

    // ---- the sprite quad ---------------------------------------------------

    #[test]
    fn sprite_quad_applies_the_transformable_and_maps_the_rect() {
        let tex = crate::texture::Texture::create(100, 50);
        let mut s = crate::texture::Sprite::new(tex);
        // Full-rect default (Sprite::new): origin (32, 16) centre-ish, a
        // half scale, a position, no rotation.
        s.set_origin(Vec2::new(32.0, 16.0));
        s.set_position(Vec2::new(200.0, 100.0));
        s.set_scale(Vec2::new(0.5, 0.5));
        let q = sprite_quad(&s);
        // Local corners (0,0)/(0,50)/(100,50)/(100,0) through
        // (p − origin)·scale + position:
        assert_eq!(q.corners[0], Vec2::new(200.0 - 16.0, 100.0 - 8.0));
        assert_eq!(q.corners[1], Vec2::new(200.0 - 16.0, 100.0 - 8.0 + 25.0));
        assert_eq!(
            q.corners[2],
            Vec2::new(200.0 - 16.0 + 50.0, 100.0 - 8.0 + 25.0)
        );
        assert_eq!(q.corners[3], Vec2::new(200.0 - 16.0 + 50.0, 100.0 - 8.0));
        // The full-rect UV:
        assert_eq!(q.uv, (0.0, 0.0, 1.0, 1.0));
        assert_eq!(q.color, Color::WHITE);
    }

    #[test]
    fn sprite_quad_texture_rect_maps_the_sub_uv() {
        let tex = crate::texture::Texture::create(100, 50);
        let mut s = crate::texture::Sprite::new(tex);
        // A sub-rect: set it through the port's texture-rect surface.
        s.set_texture_rect(FloatRect::new(10.0, 20.0, 32.0, 16.0));
        let q = sprite_quad(&s);
        assert!((q.uv.0 - 0.10).abs() < 1e-6);
        assert!((q.uv.1 - 0.40).abs() < 1e-6);
        assert!((q.uv.2 - 0.42).abs() < 1e-6);
        assert!((q.uv.3 - 0.72).abs() < 1e-6);
        // The quad size follows the rect (32×16), not the texture:
        assert!((q.corners[2].x - q.corners[0].x - 32.0).abs() < 1e-4);
        assert!((q.corners[1].y - q.corners[0].y - 16.0).abs() < 1e-4);
    }

    #[test]
    fn sprite_quad_rotates_about_the_origin() {
        let tex = crate::texture::Texture::create(10, 10);
        let mut s = crate::texture::Sprite::new(tex);
        s.set_origin(Vec2::new(5.0, 5.0));
        s.set_position(Vec2::new(0.0, 0.0));
        s.set_rotation(90.0);
        let q = sprite_quad(&s);
        // 90° (y-down: the local +x edge points DOWN): the nw corner
        // (local (−5, −5) about the origin) lands at (+5, −5)… compute
        // through the port's own transform (the pin is the transform's
        // application, the sign convention is `Sprite::transform_point`'s
        // documented math):
        for (c, local) in q.corners.iter().zip([
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(10.0, 0.0),
        ]) {
            assert_eq!(*c, s.transform_point(local));
        }
    }
}
