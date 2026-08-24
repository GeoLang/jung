//! TrueType/OpenType font rendering.
//!
//! Parses TTF/OTF font files and rasterizes glyphs at arbitrary sizes
//! with proper kerning and anti-aliasing. A `FontSet` holds the faces a
//! render can draw label text with, keyed by family name.

use crate::curved_label::PlacedChar;
use crate::renderer::PixelBuffer;
use jung_style::Color;
use std::collections::HashMap;

/// A loaded font face.
pub struct FontFace {
    data: Vec<u8>,
}

/// The font faces available to a render, keyed by the family name a style's
/// `text-font` asks for. Faces come from the caller: jung ships no font.
#[derive(Default)]
pub struct FontSet {
    faces: HashMap<String, FontFace>,
    fallback_family: Option<String>,
}

impl FontSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a face under a family name. The first family added is the fallback.
    pub fn insert(&mut self, family: impl Into<String>, face: FontFace) {
        let family = family.into();
        if self.fallback_family.is_none() {
            self.fallback_family = Some(family.clone());
        }
        self.faces.insert(family, face);
    }

    /// The face for a family, or the fallback face when the family is unknown
    /// or the style names none. `None` only when no face was added.
    pub fn get(&self, family: Option<&str>) -> Option<&FontFace> {
        family.and_then(|name| self.faces.get(name)).or_else(|| {
            self.fallback_family
                .as_deref()
                .and_then(|name| self.faces.get(name))
        })
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }
}

/// A rasterized glyph.
#[derive(Debug, Clone)]
pub struct RasterGlyph {
    /// Glyph bitmap width.
    pub width: u32,
    /// Glyph bitmap height.
    pub height: u32,
    /// Coverage values (0-255), one per pixel.
    pub coverage: Vec<u8>,
    /// Horizontal bearing (offset from pen position to left edge).
    pub bearing_x: i32,
    /// Vertical bearing (offset from baseline to top edge).
    pub bearing_y: i32,
    /// Horizontal advance (pen movement after this glyph).
    pub advance: f64,
}

/// Glyph metrics without rasterized data.
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    pub advance: f64,
    pub bearing_x: i32,
    pub bearing_y: i32,
    pub width: u32,
    pub height: u32,
}

/// Text metrics for a measured string.
#[derive(Debug, Clone)]
pub struct TextMetrics {
    pub width: f64,
    pub height: f64,
    pub ascent: f64,
    pub descent: f64,
}

impl FontFace {
    /// Load a font from raw TTF/OTF bytes.
    pub fn from_bytes(data: Vec<u8>) -> Option<Self> {
        // Validate that ttf-parser can parse it
        ttf_parser::Face::parse(&data, 0).ok()?;
        Some(Self { data })
    }

    /// Get the parsed face (borrows data).
    fn face(&self) -> ttf_parser::Face<'_> {
        ttf_parser::Face::parse(&self.data, 0).unwrap()
    }

    /// Rasterize a single glyph at the given pixel size.
    pub fn rasterize_glyph(&self, ch: char, size_px: f64) -> Option<RasterGlyph> {
        let face = self.face();
        let glyph_id = face.glyph_index(ch)?;
        let units_per_em = face.units_per_em() as f64;
        let scale = size_px / units_per_em;

        let bbox = face.glyph_bounding_box(glyph_id)?;
        let advance_raw = face.glyph_hor_advance(glyph_id)? as f64;

        let x_min = (bbox.x_min as f64 * scale).floor() as i32;
        let y_min = (bbox.y_min as f64 * scale).floor() as i32;
        let x_max = (bbox.x_max as f64 * scale).ceil() as i32;
        let y_max = (bbox.y_max as f64 * scale).ceil() as i32;

        let width = (x_max - x_min).max(1) as u32;
        let height = (y_max - y_min).max(1) as u32;

        // Rasterize using outline builder
        let mut rasterizer = GlyphRasterizer::new(width, height, x_min as f64, y_min as f64, scale);
        face.outline_glyph(glyph_id, &mut rasterizer)?;
        let coverage = rasterizer.finish();

        Some(RasterGlyph {
            width,
            height,
            coverage,
            bearing_x: x_min,
            bearing_y: y_max,
            advance: advance_raw * scale,
        })
    }

    /// Measure text without rasterizing.
    pub fn measure_text(&self, text: &str, size_px: f64) -> TextMetrics {
        let face = self.face();
        let units_per_em = face.units_per_em() as f64;
        let scale = size_px / units_per_em;

        let ascent = face.ascender() as f64 * scale;
        let descent = face.descender() as f64 * scale;
        let mut width = 0.0;

        let mut prev_glyph: Option<ttf_parser::GlyphId> = None;
        for ch in text.chars() {
            if let Some(glyph_id) = face.glyph_index(ch) {
                // Kerning
                if let Some(prev) = prev_glyph
                    && let Some(kern) = face
                        .tables()
                        .kern
                        .and_then(|k| k.subtables.into_iter().next())
                        .and_then(|st| st.glyphs_kerning(prev, glyph_id))
                {
                    width += kern as f64 * scale;
                }

                if let Some(adv) = face.glyph_hor_advance(glyph_id) {
                    width += adv as f64 * scale;
                }
                prev_glyph = Some(glyph_id);
            }
        }

        TextMetrics {
            width,
            height: ascent - descent,
            ascent,
            descent,
        }
    }

    /// Render text onto a pixel buffer.
    pub fn render_text(
        &self,
        buffer: &mut PixelBuffer,
        text: &str,
        x: f64,
        y: f64,
        size_px: f64,
        color: Color,
    ) {
        let face = self.face();
        let units_per_em = face.units_per_em() as f64;
        let scale = size_px / units_per_em;

        let mut pen_x = x;
        let mut prev_glyph: Option<ttf_parser::GlyphId> = None;

        for ch in text.chars() {
            if let Some(glyph_id) = face.glyph_index(ch) {
                // Kerning
                if let Some(prev) = prev_glyph
                    && let Some(kern) = face
                        .tables()
                        .kern
                        .and_then(|k| k.subtables.into_iter().next())
                        .and_then(|st| st.glyphs_kerning(prev, glyph_id))
                {
                    pen_x += kern as f64 * scale;
                }

                if let Some(glyph) = self.rasterize_glyph(ch, size_px) {
                    blit_glyph(buffer, &glyph, pen_x, y, color);
                    pen_x += glyph.advance;
                } else if let Some(adv) = face.glyph_hor_advance(glyph_id) {
                    pen_x += adv as f64 * scale;
                }
                prev_glyph = Some(glyph_id);
            }
        }
    }

    /// Render one character where a curved label put it: centred on its point
    /// and turned to its angle.
    pub fn render_placed_char(
        &self,
        buffer: &mut PixelBuffer,
        placed: &PlacedChar,
        size_px: f64,
        color: Color,
    ) {
        let Some(glyph) = self.rasterize_glyph(placed.ch, size_px) else {
            return;
        };
        let (cx, cy) = (placed.x, placed.y);
        let (sin_a, cos_a) = placed.angle.sin_cos();
        let half_width = glyph.width as f64 / 2.0;
        let half_height = glyph.height as f64 / 2.0;

        // Walk the rotated bounding box and sample the glyph, so no destination
        // pixel is skipped the way a forward rotation would skip it.
        let extent = (half_width * cos_a.abs() + half_height * sin_a.abs())
            .max(half_width * sin_a.abs() + half_height * cos_a.abs())
            .ceil();
        let x_start = (cx - extent).floor() as i32;
        let x_end = (cx + extent).ceil() as i32;
        let y_start = (cy - extent).floor() as i32;
        let y_end = (cy + extent).ceil() as i32;

        for py in y_start..=y_end {
            for px in x_start..=x_end {
                let dx = px as f64 - cx;
                let dy = py as f64 - cy;
                let local_x = dx * cos_a + dy * sin_a + half_width;
                let local_y = -dx * sin_a + dy * cos_a + half_height;
                if local_x < 0.0 || local_y < 0.0 {
                    continue;
                }
                let (gx, gy) = (local_x as u32, local_y as u32);
                if gx >= glyph.width || gy >= glyph.height {
                    continue;
                }
                let coverage = glyph.coverage[(gy * glyph.width + gx) as usize];
                blend_pixel(buffer, px, py, color, coverage);
            }
        }
    }
}

/// Blit a rasterized glyph onto the buffer with alpha blending.
fn blit_glyph(buffer: &mut PixelBuffer, glyph: &RasterGlyph, pen_x: f64, pen_y: f64, color: Color) {
    let base_x = pen_x as i32 + glyph.bearing_x;
    let base_y = pen_y as i32 - glyph.bearing_y;

    for gy in 0..glyph.height {
        for gx in 0..glyph.width {
            let coverage = glyph.coverage[(gy * glyph.width + gx) as usize];
            blend_pixel(
                buffer,
                base_x + gx as i32,
                base_y + gy as i32,
                color,
                coverage,
            );
        }
    }
}

/// Alpha-blend one coverage sample of `color` into the buffer.
fn blend_pixel(buffer: &mut PixelBuffer, x: i32, y: i32, color: Color, coverage: u8) {
    if x < 0 || y < 0 || x >= buffer.width as i32 || y >= buffer.height as i32 {
        return;
    }
    let alpha = (coverage as u32 * color.a as u32) / 255;
    if alpha == 0 {
        return;
    }

    let idx = ((y as u32 * buffer.width + x as u32) * 4) as usize;
    let inv_a = 255 - alpha;
    buffer.data[idx] = ((color.r as u32 * alpha + buffer.data[idx] as u32 * inv_a) / 255) as u8;
    buffer.data[idx + 1] =
        ((color.g as u32 * alpha + buffer.data[idx + 1] as u32 * inv_a) / 255) as u8;
    buffer.data[idx + 2] =
        ((color.b as u32 * alpha + buffer.data[idx + 2] as u32 * inv_a) / 255) as u8;
    buffer.data[idx + 3] = (alpha + buffer.data[idx + 3] as u32 * inv_a / 255).min(255) as u8;
}

/// Simple glyph rasterizer using scanline coverage.
struct GlyphRasterizer {
    width: u32,
    height: u32,
    offset_x: f64,
    offset_y: f64,
    scale: f64,
    // Winding number coverage buffer
    coverage: Vec<f64>,
    // Current path point
    cursor_x: f64,
    cursor_y: f64,
}

impl GlyphRasterizer {
    fn new(width: u32, height: u32, offset_x: f64, offset_y: f64, scale: f64) -> Self {
        Self {
            width,
            height,
            offset_x,
            offset_y,
            scale,
            coverage: vec![0.0; (width * height) as usize],
            cursor_x: 0.0,
            cursor_y: 0.0,
        }
    }

    fn finish(self) -> Vec<u8> {
        // Convert winding-rule coverage to alpha values
        // Simple approach: accumulate coverage per scanline
        let mut result = vec![0u8; (self.width * self.height) as usize];
        for y in 0..self.height {
            let mut accum = 0.0;
            for x in 0..self.width {
                let idx = (y * self.width + x) as usize;
                accum += self.coverage[idx];
                result[idx] = (accum.abs().min(1.0) * 255.0) as u8;
            }
        }
        result
    }

    fn transform_x(&self, x: f64) -> f64 {
        x * self.scale - self.offset_x
    }

    fn transform_y(&self, y: f64) -> f64 {
        // Font coordinates have Y up; we need Y down
        (self.height as f64) - (y * self.scale - self.offset_y)
    }

    fn add_line_segment(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        // Rasterize a line using coverage-based approach
        let dy = y1 - y0;
        if dy.abs() < 0.001 {
            return;
        }

        let (y_start, y_end, x_at_start, x_at_end, sign) = if dy > 0.0 {
            (y0, y1, x0, x1, 1.0)
        } else {
            (y1, y0, x1, x0, -1.0)
        };

        let y_min = y_start.floor().max(0.0) as u32;
        let y_max = (y_end.ceil() as u32).min(self.height);
        let dx = x_at_end - x_at_start;
        let total_dy = y_end - y_start;

        for y in y_min..y_max {
            let yf = y as f64;
            let t0 = ((yf - y_start) / total_dy).clamp(0.0, 1.0);
            let t1 = ((yf + 1.0 - y_start) / total_dy).clamp(0.0, 1.0);
            let x_at_t0 = x_at_start + dx * t0;
            let x_at_t1 = x_at_start + dx * t1;
            let x_mid = (x_at_t0 + x_at_t1) * 0.5;

            let x_pixel = x_mid.floor().max(0.0) as u32;
            if x_pixel < self.width {
                let idx = (y * self.width + x_pixel) as usize;
                let local_coverage = (t1 - t0) * total_dy;
                self.coverage[idx] += sign * local_coverage;
            }
        }
    }
}

impl ttf_parser::OutlineBuilder for GlyphRasterizer {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cursor_x = self.transform_x(x as f64);
        self.cursor_y = self.transform_y(y as f64);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let nx = self.transform_x(x as f64);
        let ny = self.transform_y(y as f64);
        self.add_line_segment(self.cursor_x, self.cursor_y, nx, ny);
        self.cursor_x = nx;
        self.cursor_y = ny;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // Approximate quadratic bezier with line segments
        let cx = self.transform_x(x1 as f64);
        let cy = self.transform_y(y1 as f64);
        let ex = self.transform_x(x as f64);
        let ey = self.transform_y(y as f64);
        let sx = self.cursor_x;
        let sy = self.cursor_y;

        let steps = 8;
        let mut prev_x = sx;
        let mut prev_y = sy;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let mt = 1.0 - t;
            let nx = mt * mt * sx + 2.0 * mt * t * cx + t * t * ex;
            let ny = mt * mt * sy + 2.0 * mt * t * cy + t * t * ey;
            self.add_line_segment(prev_x, prev_y, nx, ny);
            prev_x = nx;
            prev_y = ny;
        }
        self.cursor_x = ex;
        self.cursor_y = ey;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        // Approximate cubic bezier with line segments
        let c1x = self.transform_x(x1 as f64);
        let c1y = self.transform_y(y1 as f64);
        let c2x = self.transform_x(x2 as f64);
        let c2y = self.transform_y(y2 as f64);
        let ex = self.transform_x(x as f64);
        let ey = self.transform_y(y as f64);
        let sx = self.cursor_x;
        let sy = self.cursor_y;

        let steps = 12;
        let mut prev_x = sx;
        let mut prev_y = sy;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let mt = 1.0 - t;
            let nx = mt * mt * mt * sx
                + 3.0 * mt * mt * t * c1x
                + 3.0 * mt * t * t * c2x
                + t * t * t * ex;
            let ny = mt * mt * mt * sy
                + 3.0 * mt * mt * t * c1y
                + 3.0 * mt * t * t * c2y
                + t * t * t * ey;
            self.add_line_segment(prev_x, prev_y, nx, ny);
            prev_x = nx;
            prev_y = ny;
        }
        self.cursor_x = ex;
        self.cursor_y = ey;
    }

    fn close(&mut self) {
        // Close handled implicitly by the winding rule
    }
}

/// A face from a system font path, for tests. jung embeds no font, so a test
/// that needs one skips when the machine has none.
#[cfg(test)]
pub(crate) fn load_test_font() -> Option<FontFace> {
    let paths = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
        "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "C:/Windows/Fonts/arial.ttf",
    ];
    for path in &paths {
        if let Ok(data) = std::fs::read(path)
            && let Some(face) = FontFace::from_bytes(data)
        {
            return Some(face);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_system_font() {
        if load_test_font().is_none() {
            eprintln!("skipping: no system font found");
        }
    }

    #[test]
    fn rasterize_glyph_a() {
        let Some(face) = load_test_font() else {
            eprintln!("skipping: no system font");
            return;
        };
        let glyph = face.rasterize_glyph('A', 24.0).unwrap();
        assert!(glyph.width > 0);
        assert!(glyph.height > 0);
        assert!(glyph.advance > 0.0);
        // Should have non-zero coverage
        let filled = glyph.coverage.iter().filter(|&&c| c > 0).count();
        assert!(filled > 10);
    }

    #[test]
    fn measure_text_hello() {
        let Some(face) = load_test_font() else {
            eprintln!("skipping: no system font");
            return;
        };
        let metrics = face.measure_text("Hello", 16.0);
        assert!(metrics.width > 20.0);
        assert!(metrics.height > 10.0);
        assert!(metrics.ascent > 0.0);
    }

    #[test]
    fn render_text_on_buffer() {
        let Some(face) = load_test_font() else {
            eprintln!("skipping: no system font");
            return;
        };
        let mut buffer = PixelBuffer::new(200, 50);
        face.render_text(&mut buffer, "Test", 10.0, 30.0, 20.0, Color::rgb(0, 0, 0));
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 20);
    }

    #[test]
    fn empty_font_set_has_no_face() {
        let fonts = FontSet::new();
        assert!(fonts.is_empty());
        assert!(fonts.get(None).is_none());
        assert!(fonts.get(Some("DejaVu Sans")).is_none());
    }

    #[test]
    fn font_set_falls_back_to_the_first_family() {
        let Some(face) = load_test_font() else {
            eprintln!("skipping: no system font");
            return;
        };
        let mut fonts = FontSet::new();
        fonts.insert("Test Sans", face);
        assert!(fonts.get(Some("Test Sans")).is_some());
        assert!(fonts.get(Some("Nothing Like It")).is_some());
        assert!(fonts.get(None).is_some());
    }

    #[test]
    fn rotated_char_fills_pixels_at_every_angle() {
        let Some(face) = load_test_font() else {
            eprintln!("skipping: no system font");
            return;
        };
        for angle in [0.0, 0.5, 1.0, 2.5, -0.7] {
            let mut buffer = PixelBuffer::new(64, 64);
            let placed = PlacedChar {
                ch: 'M',
                x: 32.0,
                y: 32.0,
                angle,
            };
            face.render_placed_char(&mut buffer, &placed, 24.0, Color::rgb(0, 0, 0));
            let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
            assert!(filled > 20, "angle {angle} filled only {filled} pixels");
        }
    }

    #[test]
    fn missing_glyph_returns_none() {
        let Some(face) = load_test_font() else {
            eprintln!("skipping: no system font");
            return;
        };
        // Very obscure character unlikely to be in DejaVu
        let result = face.rasterize_glyph('\u{FFFF}', 16.0);
        // May or may not be None depending on font, just verify no crash
        let _ = result;
    }
}
