//! Label engine: text rendering and collision detection for map labels.
//!
//! Provides a built-in 5x7 bitmap font, text layout, halo rendering,
//! and label placement with collision avoidance.

use crate::renderer::PixelBuffer;
use jung_style::Color;

/// Built-in 5x7 bitmap font covering ASCII printable range (32..127).
/// Each glyph is stored as 7 rows of 5 bits (packed in a u8 per row).
const FONT_WIDTH: u32 = 5;
const FONT_HEIGHT: u32 = 7;

/// Get glyph bitmap for an ASCII character. Returns 7 bytes, each with 5 MSBs
/// representing a row.
fn glyph(ch: char) -> [u8; 7] {
    let idx = ch as usize;
    if !(32..=126).contains(&idx) {
        return [0; 7]; // non-printable → blank
    }
    FONT_DATA[idx - 32]
}

/// Label placement anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    Center,
    Top,
    Bottom,
    Left,
    Right,
}

/// Text halo parameters.
#[derive(Debug, Clone)]
pub struct Halo {
    pub color: Color,
    pub width: u32,
}

/// Parameters for rendering a label.
#[derive(Debug, Clone)]
pub struct LabelParams {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub color: Color,
    pub size: f64, // scale factor (1.0 = native 5x7)
    pub anchor: Anchor,
    pub halo: Option<Halo>,
    pub max_width: Option<f64>, // wrap text if wider than this
    pub priority: i32,          // higher = more important
}

/// An axis-aligned bounding box for collision detection.
#[derive(Debug, Clone, Copy)]
struct LabelBox {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl LabelBox {
    fn intersects(&self, other: &LabelBox) -> bool {
        self.x1 < other.x2 && self.x2 > other.x1 && self.y1 < other.y2 && self.y2 > other.y1
    }
}

/// The label engine manages placement and rendering of text labels.
pub struct LabelEngine {
    placed: Vec<LabelBox>,
}

impl LabelEngine {
    pub fn new() -> Self {
        Self { placed: vec![] }
    }

    /// Reset for a new frame.
    pub fn clear(&mut self) {
        self.placed.clear();
    }

    /// Attempt to place and render a label. Returns true if placed (no collision).
    pub fn place_label(&mut self, buffer: &mut PixelBuffer, params: &LabelParams) -> bool {
        let lines = self.wrap_text(&params.text, params.size, params.max_width);
        let bbox = self.compute_bbox(&lines, params);

        // Collision check
        if self.placed.iter().any(|p| p.intersects(&bbox)) {
            return false;
        }

        // Render halo first (if specified)
        if let Some(halo) = &params.halo {
            for hw in 1..=halo.width {
                for (line_idx, line) in lines.iter().enumerate() {
                    let (lx, ly) = self.line_origin(params, &lines, line_idx);
                    for dx in -(hw as i32)..=(hw as i32) {
                        for dy in -(hw as i32)..=(hw as i32) {
                            render_text_line(
                                buffer,
                                line,
                                lx + dx as f64,
                                ly + dy as f64,
                                params.size,
                                halo.color,
                            );
                        }
                    }
                }
            }
        }

        // Render text
        for (line_idx, line) in lines.iter().enumerate() {
            let (lx, ly) = self.line_origin(params, &lines, line_idx);
            render_text_line(buffer, line, lx, ly, params.size, params.color);
        }

        self.placed.push(bbox);
        true
    }

    /// Word-wrap text into lines.
    fn wrap_text(&self, text: &str, size: f64, max_width: Option<f64>) -> Vec<String> {
        let max_w = match max_width {
            Some(w) => w,
            None => return vec![text.to_string()],
        };

        let char_w = FONT_WIDTH as f64 * size;
        let max_chars = (max_w / char_w).floor() as usize;
        if max_chars == 0 {
            return vec![text.to_string()];
        }

        let words: Vec<&str> = text.split_whitespace().collect();
        let mut lines = Vec::new();
        let mut current_line = String::new();

        for word in words {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= max_chars {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    /// Compute the bounding box for a multi-line label.
    fn compute_bbox(&self, lines: &[String], params: &LabelParams) -> LabelBox {
        let char_w = FONT_WIDTH as f64 * params.size;
        let char_h = FONT_HEIGHT as f64 * params.size;
        let line_spacing = char_h + 1.0;

        let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
        let total_w = max_line_len as f64 * char_w;
        let total_h = lines.len() as f64 * line_spacing;

        let (ox, oy) = match params.anchor {
            Anchor::Center => (params.x - total_w / 2.0, params.y - total_h / 2.0),
            Anchor::Top => (params.x - total_w / 2.0, params.y),
            Anchor::Bottom => (params.x - total_w / 2.0, params.y - total_h),
            Anchor::Left => (params.x, params.y - total_h / 2.0),
            Anchor::Right => (params.x - total_w, params.y - total_h / 2.0),
        };

        // Add padding for halo
        let pad = params.halo.as_ref().map_or(0.0, |h| h.width as f64 + 1.0);

        LabelBox {
            x1: ox - pad,
            y1: oy - pad,
            x2: ox + total_w + pad,
            y2: oy + total_h + pad,
        }
    }

    /// Get the pixel origin (top-left) of a specific line.
    fn line_origin(&self, params: &LabelParams, lines: &[String], line_idx: usize) -> (f64, f64) {
        let char_w = FONT_WIDTH as f64 * params.size;
        let char_h = FONT_HEIGHT as f64 * params.size;
        let line_spacing = char_h + 1.0;

        let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
        let total_w = max_line_len as f64 * char_w;
        let total_h = lines.len() as f64 * line_spacing;

        let (base_x, base_y) = match params.anchor {
            Anchor::Center => (params.x - total_w / 2.0, params.y - total_h / 2.0),
            Anchor::Top => (params.x - total_w / 2.0, params.y),
            Anchor::Bottom => (params.x - total_w / 2.0, params.y - total_h),
            Anchor::Left => (params.x, params.y - total_h / 2.0),
            Anchor::Right => (params.x - total_w, params.y - total_h / 2.0),
        };

        // Center each line horizontally within the block
        let this_line_w = lines[line_idx].len() as f64 * char_w;
        let x_offset = (total_w - this_line_w) / 2.0;

        (base_x + x_offset, base_y + line_idx as f64 * line_spacing)
    }
}

impl Default for LabelEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Render a single line of text onto the buffer at (x, y) top-left.
fn render_text_line(buffer: &mut PixelBuffer, text: &str, x: f64, y: f64, size: f64, color: Color) {
    let char_w = FONT_WIDTH as f64 * size;

    for (i, ch) in text.chars().enumerate() {
        let gx = x + i as f64 * char_w;
        render_glyph(buffer, ch, gx, y, size, color);
    }
}

/// Render a single glyph at (x, y) with given scale.
fn render_glyph(buffer: &mut PixelBuffer, ch: char, x: f64, y: f64, size: f64, color: Color) {
    let bitmap = glyph(ch);

    for row in 0..FONT_HEIGHT {
        let bits = bitmap[row as usize];
        for col in 0..FONT_WIDTH {
            if bits & (0x80 >> col) != 0 {
                // Scale: fill a size×size block for each pixel
                let px_start = (x + col as f64 * size) as i32;
                let py_start = (y + row as f64 * size) as i32;
                let px_end = (x + (col + 1) as f64 * size) as i32;
                let py_end = (y + (row + 1) as f64 * size) as i32;

                for py in py_start..py_end {
                    for px in px_start..px_end {
                        if px >= 0
                            && py >= 0
                            && (px as u32) < buffer.width
                            && (py as u32) < buffer.height
                        {
                            let idx = ((py as u32 * buffer.width + px as u32) * 4) as usize;
                            // Alpha composite
                            let sa = color.a as u32;
                            let da = buffer.data[idx + 3] as u32;
                            let out_a = sa + da * (255 - sa) / 255;
                            if out_a > 0 {
                                let sr = color.r as u32;
                                let sg = color.g as u32;
                                let sb = color.b as u32;
                                let dr = buffer.data[idx] as u32;
                                let dg = buffer.data[idx + 1] as u32;
                                let db = buffer.data[idx + 2] as u32;
                                buffer.data[idx] =
                                    ((sr * sa + dr * da * (255 - sa) / 255) / out_a).min(255) as u8;
                                buffer.data[idx + 1] =
                                    ((sg * sa + dg * da * (255 - sa) / 255) / out_a).min(255) as u8;
                                buffer.data[idx + 2] =
                                    ((sb * sa + db * da * (255 - sa) / 255) / out_a).min(255) as u8;
                                buffer.data[idx + 3] = out_a.min(255) as u8;
                            }
                        }
                    }
                }
            }
        }
    }
}

// 5x7 bitmap font data for ASCII 32-126
// Each character is 7 rows, each row has 5 bits (MSB-aligned in a u8).
#[rustfmt::skip]
const FONT_DATA: [[u8; 7]; 95] = [
    // 32: ' ' (space)
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    // 33: '!'
    [0x20, 0x20, 0x20, 0x20, 0x20, 0x00, 0x20],
    // 34: '"'
    [0x50, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00],
    // 35: '#'
    [0x50, 0xF8, 0x50, 0x50, 0x50, 0xF8, 0x50],
    // 36: '$'
    [0x20, 0x78, 0xA0, 0x70, 0x28, 0xF0, 0x20],
    // 37: '%'
    [0xC8, 0xC8, 0x10, 0x20, 0x40, 0x98, 0x98],
    // 38: '&'
    [0x40, 0xA0, 0xA0, 0x40, 0xA8, 0x90, 0x68],
    // 39: '\''
    [0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00],
    // 40: '('
    [0x10, 0x20, 0x40, 0x40, 0x40, 0x20, 0x10],
    // 41: ')'
    [0x40, 0x20, 0x10, 0x10, 0x10, 0x20, 0x40],
    // 42: '*'
    [0x00, 0x20, 0xA8, 0x70, 0xA8, 0x20, 0x00],
    // 43: '+'
    [0x00, 0x20, 0x20, 0xF8, 0x20, 0x20, 0x00],
    // 44: ','
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x40],
    // 45: '-'
    [0x00, 0x00, 0x00, 0xF8, 0x00, 0x00, 0x00],
    // 46: '.'
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20],
    // 47: '/'
    [0x08, 0x08, 0x10, 0x20, 0x40, 0x80, 0x80],
    // 48: '0'
    [0x70, 0x88, 0x98, 0xA8, 0xC8, 0x88, 0x70],
    // 49: '1'
    [0x20, 0x60, 0x20, 0x20, 0x20, 0x20, 0x70],
    // 50: '2'
    [0x70, 0x88, 0x08, 0x10, 0x20, 0x40, 0xF8],
    // 51: '3'
    [0x70, 0x88, 0x08, 0x30, 0x08, 0x88, 0x70],
    // 52: '4'
    [0x10, 0x30, 0x50, 0x90, 0xF8, 0x10, 0x10],
    // 53: '5'
    [0xF8, 0x80, 0xF0, 0x08, 0x08, 0x88, 0x70],
    // 54: '6'
    [0x30, 0x40, 0x80, 0xF0, 0x88, 0x88, 0x70],
    // 55: '7'
    [0xF8, 0x08, 0x10, 0x20, 0x40, 0x40, 0x40],
    // 56: '8'
    [0x70, 0x88, 0x88, 0x70, 0x88, 0x88, 0x70],
    // 57: '9'
    [0x70, 0x88, 0x88, 0x78, 0x08, 0x10, 0x60],
    // 58: ':'
    [0x00, 0x00, 0x20, 0x00, 0x00, 0x20, 0x00],
    // 59: ';'
    [0x00, 0x00, 0x20, 0x00, 0x00, 0x20, 0x40],
    // 60: '<'
    [0x08, 0x10, 0x20, 0x40, 0x20, 0x10, 0x08],
    // 61: '='
    [0x00, 0x00, 0xF8, 0x00, 0xF8, 0x00, 0x00],
    // 62: '>'
    [0x80, 0x40, 0x20, 0x10, 0x20, 0x40, 0x80],
    // 63: '?'
    [0x70, 0x88, 0x08, 0x10, 0x20, 0x00, 0x20],
    // 64: '@'
    [0x70, 0x88, 0xB8, 0xA8, 0xB8, 0x80, 0x70],
    // 65: 'A'
    [0x70, 0x88, 0x88, 0xF8, 0x88, 0x88, 0x88],
    // 66: 'B'
    [0xF0, 0x88, 0x88, 0xF0, 0x88, 0x88, 0xF0],
    // 67: 'C'
    [0x70, 0x88, 0x80, 0x80, 0x80, 0x88, 0x70],
    // 68: 'D'
    [0xF0, 0x88, 0x88, 0x88, 0x88, 0x88, 0xF0],
    // 69: 'E'
    [0xF8, 0x80, 0x80, 0xF0, 0x80, 0x80, 0xF8],
    // 70: 'F'
    [0xF8, 0x80, 0x80, 0xF0, 0x80, 0x80, 0x80],
    // 71: 'G'
    [0x70, 0x88, 0x80, 0xB8, 0x88, 0x88, 0x70],
    // 72: 'H'
    [0x88, 0x88, 0x88, 0xF8, 0x88, 0x88, 0x88],
    // 73: 'I'
    [0x70, 0x20, 0x20, 0x20, 0x20, 0x20, 0x70],
    // 74: 'J'
    [0x38, 0x10, 0x10, 0x10, 0x10, 0x90, 0x60],
    // 75: 'K'
    [0x88, 0x90, 0xA0, 0xC0, 0xA0, 0x90, 0x88],
    // 76: 'L'
    [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0xF8],
    // 77: 'M'
    [0x88, 0xD8, 0xA8, 0xA8, 0x88, 0x88, 0x88],
    // 78: 'N'
    [0x88, 0xC8, 0xA8, 0x98, 0x88, 0x88, 0x88],
    // 79: 'O'
    [0x70, 0x88, 0x88, 0x88, 0x88, 0x88, 0x70],
    // 80: 'P'
    [0xF0, 0x88, 0x88, 0xF0, 0x80, 0x80, 0x80],
    // 81: 'Q'
    [0x70, 0x88, 0x88, 0x88, 0xA8, 0x90, 0x68],
    // 82: 'R'
    [0xF0, 0x88, 0x88, 0xF0, 0xA0, 0x90, 0x88],
    // 83: 'S'
    [0x70, 0x88, 0x80, 0x70, 0x08, 0x88, 0x70],
    // 84: 'T'
    [0xF8, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20],
    // 85: 'U'
    [0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x70],
    // 86: 'V'
    [0x88, 0x88, 0x88, 0x88, 0x50, 0x50, 0x20],
    // 87: 'W'
    [0x88, 0x88, 0x88, 0xA8, 0xA8, 0xD8, 0x88],
    // 88: 'X'
    [0x88, 0x88, 0x50, 0x20, 0x50, 0x88, 0x88],
    // 89: 'Y'
    [0x88, 0x88, 0x50, 0x20, 0x20, 0x20, 0x20],
    // 90: 'Z'
    [0xF8, 0x08, 0x10, 0x20, 0x40, 0x80, 0xF8],
    // 91: '['
    [0x70, 0x40, 0x40, 0x40, 0x40, 0x40, 0x70],
    // 92: '\\'
    [0x80, 0x80, 0x40, 0x20, 0x10, 0x08, 0x08],
    // 93: ']'
    [0x70, 0x10, 0x10, 0x10, 0x10, 0x10, 0x70],
    // 94: '^'
    [0x20, 0x50, 0x88, 0x00, 0x00, 0x00, 0x00],
    // 95: '_'
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF8],
    // 96: '`'
    [0x40, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00],
    // 97: 'a'
    [0x00, 0x00, 0x70, 0x08, 0x78, 0x88, 0x78],
    // 98: 'b'
    [0x80, 0x80, 0xF0, 0x88, 0x88, 0x88, 0xF0],
    // 99: 'c'
    [0x00, 0x00, 0x70, 0x80, 0x80, 0x80, 0x70],
    // 100: 'd'
    [0x08, 0x08, 0x78, 0x88, 0x88, 0x88, 0x78],
    // 101: 'e'
    [0x00, 0x00, 0x70, 0x88, 0xF8, 0x80, 0x70],
    // 102: 'f'
    [0x30, 0x48, 0x40, 0xE0, 0x40, 0x40, 0x40],
    // 103: 'g'
    [0x00, 0x00, 0x78, 0x88, 0x78, 0x08, 0x70],
    // 104: 'h'
    [0x80, 0x80, 0xF0, 0x88, 0x88, 0x88, 0x88],
    // 105: 'i'
    [0x20, 0x00, 0x60, 0x20, 0x20, 0x20, 0x70],
    // 106: 'j'
    [0x10, 0x00, 0x30, 0x10, 0x10, 0x90, 0x60],
    // 107: 'k'
    [0x80, 0x80, 0x90, 0xA0, 0xC0, 0xA0, 0x90],
    // 108: 'l'
    [0x60, 0x20, 0x20, 0x20, 0x20, 0x20, 0x70],
    // 109: 'm'
    [0x00, 0x00, 0xD0, 0xA8, 0xA8, 0xA8, 0xA8],
    // 110: 'n'
    [0x00, 0x00, 0xF0, 0x88, 0x88, 0x88, 0x88],
    // 111: 'o'
    [0x00, 0x00, 0x70, 0x88, 0x88, 0x88, 0x70],
    // 112: 'p'
    [0x00, 0x00, 0xF0, 0x88, 0xF0, 0x80, 0x80],
    // 113: 'q'
    [0x00, 0x00, 0x78, 0x88, 0x78, 0x08, 0x08],
    // 114: 'r'
    [0x00, 0x00, 0xB0, 0xC8, 0x80, 0x80, 0x80],
    // 115: 's'
    [0x00, 0x00, 0x78, 0x80, 0x70, 0x08, 0xF0],
    // 116: 't'
    [0x40, 0x40, 0xE0, 0x40, 0x40, 0x48, 0x30],
    // 117: 'u'
    [0x00, 0x00, 0x88, 0x88, 0x88, 0x88, 0x78],
    // 118: 'v'
    [0x00, 0x00, 0x88, 0x88, 0x88, 0x50, 0x20],
    // 119: 'w'
    [0x00, 0x00, 0x88, 0x88, 0xA8, 0xA8, 0x50],
    // 120: 'x'
    [0x00, 0x00, 0x88, 0x50, 0x20, 0x50, 0x88],
    // 121: 'y'
    [0x00, 0x00, 0x88, 0x88, 0x78, 0x08, 0x70],
    // 122: 'z'
    [0x00, 0x00, 0xF8, 0x10, 0x20, 0x40, 0xF8],
    // 123: '{'
    [0x10, 0x20, 0x20, 0x40, 0x20, 0x20, 0x10],
    // 124: '|'
    [0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20],
    // 125: '}'
    [0x40, 0x20, 0x20, 0x10, 0x20, 0x20, 0x40],
    // 126: '~'
    [0x00, 0x00, 0x40, 0xA8, 0x10, 0x00, 0x00],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_a() {
        let g = glyph('A');
        // Row 0 of 'A' should be 0x70 = 01110000
        assert_eq!(g[0], 0x70);
    }

    #[test]
    fn render_single_char() {
        let mut buffer = PixelBuffer::new(32, 32);
        render_text_line(&mut buffer, "A", 5.0, 5.0, 2.0, Color::rgb(255, 0, 0));
        // Some pixels should be non-zero
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 0, "Expected some pixels to be filled");
    }

    #[test]
    fn label_collision() {
        let mut engine = LabelEngine::new();
        let mut buffer = PixelBuffer::new(128, 128);

        let params1 = LabelParams {
            text: "Hello".to_string(),
            x: 64.0,
            y: 64.0,
            color: Color::rgb(0, 0, 0),
            size: 2.0,
            anchor: Anchor::Center,
            halo: None,
            max_width: None,
            priority: 0,
        };
        // First label should be placed
        assert!(engine.place_label(&mut buffer, &params1));

        // Same position → collision
        let params2 = LabelParams {
            text: "World".to_string(),
            x: 64.0,
            y: 64.0,
            color: Color::rgb(0, 0, 0),
            size: 2.0,
            anchor: Anchor::Center,
            halo: None,
            max_width: None,
            priority: 0,
        };
        assert!(!engine.place_label(&mut buffer, &params2));
    }

    #[test]
    fn label_no_collision_far_apart() {
        let mut engine = LabelEngine::new();
        let mut buffer = PixelBuffer::new(256, 256);

        let p1 = LabelParams {
            text: "A".to_string(),
            x: 20.0,
            y: 20.0,
            color: Color::rgb(0, 0, 0),
            size: 1.0,
            anchor: Anchor::Center,
            halo: None,
            max_width: None,
            priority: 0,
        };
        assert!(engine.place_label(&mut buffer, &p1));

        let p2 = LabelParams {
            text: "B".to_string(),
            x: 200.0,
            y: 200.0,
            color: Color::rgb(0, 0, 0),
            size: 1.0,
            anchor: Anchor::Center,
            halo: None,
            max_width: None,
            priority: 0,
        };
        assert!(engine.place_label(&mut buffer, &p2));
    }

    #[test]
    fn word_wrap() {
        let engine = LabelEngine::new();
        // At size 1.0, each char is 5px wide. max_width=25 → 5 chars per line
        let lines = engine.wrap_text("Hello World Test", 1.0, Some(25.0));
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Hello");
        assert_eq!(lines[1], "World");
        assert_eq!(lines[2], "Test");
    }

    #[test]
    fn halo_rendering() {
        let mut engine = LabelEngine::new();
        let mut buffer = PixelBuffer::new(64, 64);

        let params = LabelParams {
            text: "X".to_string(),
            x: 32.0,
            y: 32.0,
            color: Color::rgb(255, 255, 255),
            size: 2.0,
            anchor: Anchor::Center,
            halo: Some(Halo {
                color: Color::rgb(0, 0, 0),
                width: 1,
            }),
            max_width: None,
            priority: 0,
        };
        assert!(engine.place_label(&mut buffer, &params));
        // Should have both black (halo) and white (text) pixels
        let white = buffer
            .data
            .chunks(4)
            .filter(|px| px[0] == 255 && px[1] == 255 && px[2] == 255 && px[3] == 255)
            .count();
        let black = buffer
            .data
            .chunks(4)
            .filter(|px| px[0] == 0 && px[1] == 0 && px[2] == 0 && px[3] == 255)
            .count();
        assert!(white > 0, "Expected white text pixels");
        assert!(black > 0, "Expected black halo pixels");
    }

    #[test]
    fn clear_resets_collisions() {
        let mut engine = LabelEngine::new();
        let mut buffer = PixelBuffer::new(64, 64);

        let params = LabelParams {
            text: "Hi".to_string(),
            x: 32.0,
            y: 32.0,
            color: Color::rgb(0, 0, 0),
            size: 1.0,
            anchor: Anchor::Center,
            halo: None,
            max_width: None,
            priority: 0,
        };
        engine.place_label(&mut buffer, &params);
        engine.clear();
        // After clear, same position should work again
        assert!(engine.place_label(&mut buffer, &params));
    }
}
