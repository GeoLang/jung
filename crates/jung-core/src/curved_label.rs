//! Curved label placement along line geometries.
//!
//! Places text labels along polylines, following the curvature of the line.
//! Characters are individually rotated to follow the line direction.

use crate::geometry::Point;
use crate::renderer::{BBox, PixelBuffer};
use jung_style::Color;

/// Parameters for curved label placement.
#[derive(Debug, Clone)]
pub struct CurvedLabelParams {
    /// Font size in pixels.
    pub font_size: f64,
    /// Text color.
    pub color: Color,
    /// Halo color (outline around text).
    pub halo_color: Option<Color>,
    /// Halo width in pixels.
    pub halo_width: f64,
    /// Minimum spacing between repeated labels (pixels).
    pub repeat_distance: f64,
    /// Maximum angle change between consecutive characters (radians).
    pub max_angle_delta: f64,
    /// Offset from line center (positive = left of direction).
    pub offset: f64,
}

impl Default for CurvedLabelParams {
    fn default() -> Self {
        Self {
            font_size: 12.0,
            color: Color::rgb(0, 0, 0),
            halo_color: Some(Color::rgba(255, 255, 255, 200)),
            halo_width: 2.0,
            repeat_distance: 250.0,
            max_angle_delta: 0.4, // ~23 degrees
            offset: 0.0,
        }
    }
}

/// A positioned character along a curve.
#[derive(Debug, Clone)]
pub struct PlacedChar {
    /// Character to render.
    pub ch: char,
    /// Center x position (screen pixels).
    pub x: f64,
    /// Center y position (screen pixels).
    pub y: f64,
    /// Rotation angle (radians).
    pub angle: f64,
}

/// Compute the total length of a polyline in screen space.
pub fn polyline_length(screen_points: &[(f64, f64)]) -> f64 {
    screen_points
        .windows(2)
        .map(|w| {
            let dx = w[1].0 - w[0].0;
            let dy = w[1].1 - w[0].1;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

/// Get a point and angle at a given distance along a polyline.
fn point_at_distance(points: &[(f64, f64)], distance: f64) -> Option<(f64, f64, f64)> {
    let mut remaining = distance;
    for window in points.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];
        let dx = x1 - x0;
        let dy = y1 - y0;
        let seg_len = (dx * dx + dy * dy).sqrt();

        if remaining <= seg_len {
            let t = remaining / seg_len;
            let x = x0 + dx * t;
            let y = y0 + dy * t;
            let angle = dy.atan2(dx);
            return Some((x, y, angle));
        }
        remaining -= seg_len;
    }
    None
}

/// Place characters along a polyline.
/// Returns None if the line is too short or too curved.
pub fn place_curved_label(
    screen_points: &[(f64, f64)],
    text: &str,
    char_widths: &[f64],
    params: &CurvedLabelParams,
) -> Option<Vec<PlacedChar>> {
    if text.is_empty() || screen_points.len() < 2 {
        return None;
    }

    let total_length = polyline_length(screen_points);
    let text_width: f64 = char_widths.iter().sum();

    if text_width > total_length * 0.8 {
        return None; // Text too long for line
    }

    // Center the text along the line
    let start_offset = (total_length - text_width) / 2.0;
    let mut placed = Vec::with_capacity(text.len());
    let mut distance = start_offset;

    let mut prev_angle: Option<f64> = None;

    for (ch, &char_width) in text.chars().zip(char_widths.iter()) {
        distance += char_width / 2.0;

        let (x, y, angle) = point_at_distance(screen_points, distance)?;

        // Check angle delta
        if let Some(prev) = prev_angle {
            let delta = (angle - prev).abs();
            let delta = if delta > std::f64::consts::PI {
                2.0 * std::f64::consts::PI - delta
            } else {
                delta
            };
            if delta > params.max_angle_delta {
                return None; // Too curved
            }
        }

        // Apply offset perpendicular to line direction
        let (final_x, final_y) = if params.offset.abs() > 0.01 {
            let nx = -angle.sin() * params.offset;
            let ny = angle.cos() * params.offset;
            (x + nx, y + ny)
        } else {
            (x, y)
        };

        placed.push(PlacedChar {
            ch,
            x: final_x,
            y: final_y,
            angle,
        });

        prev_angle = Some(angle);
        distance += char_width / 2.0;
    }

    Some(placed)
}

/// Render placed characters onto a buffer using the built-in bitmap font.
/// For proper rendering, use `text::FontFace::render_text` with rotation.
pub fn render_curved_label_bitmap(
    buffer: &mut PixelBuffer,
    placed: &[PlacedChar],
    params: &CurvedLabelParams,
) {
    for pc in placed {
        render_rotated_char(buffer, pc.ch, pc.x, pc.y, pc.angle, params);
    }
}

/// Convert a polyline from geo coordinates to screen space.
pub fn to_screen_coords(points: &[Point], bbox: &BBox, width: u32, height: u32) -> Vec<(f64, f64)> {
    points
        .iter()
        .map(|p| {
            let x = (p.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * width as f64;
            let y = (bbox.max_y - p.y) / (bbox.max_y - bbox.min_y) * height as f64;
            (x, y)
        })
        .collect()
}

fn render_rotated_char(
    buffer: &mut PixelBuffer,
    ch: char,
    cx: f64,
    cy: f64,
    angle: f64,
    params: &CurvedLabelParams,
) {
    // Use the 5x7 bitmap font from the label module, rotated
    let glyph = get_bitmap_glyph(ch);
    let scale = params.font_size / 7.0; // 7 pixels = font height
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // Render halo first
    if let Some(halo_color) = params.halo_color {
        let hw = params.halo_width;
        for offx in [-hw, 0.0, hw] {
            for ofy in [-hw, 0.0, hw] {
                if offx == 0.0 && ofy == 0.0 {
                    continue;
                }
                render_glyph_rotated(
                    buffer,
                    &glyph,
                    &GlyphTransform {
                        cx: cx + offx,
                        cy: cy + ofy,
                        cos_a,
                        sin_a,
                        scale,
                        color: halo_color,
                    },
                );
            }
        }
    }

    // Render character
    render_glyph_rotated(
        buffer,
        &glyph,
        &GlyphTransform {
            cx,
            cy,
            cos_a,
            sin_a,
            scale,
            color: params.color,
        },
    );
}

struct GlyphTransform {
    cx: f64,
    cy: f64,
    cos_a: f64,
    sin_a: f64,
    scale: f64,
    color: Color,
}

fn render_glyph_rotated(buffer: &mut PixelBuffer, glyph: &[u8; 7], transform: &GlyphTransform) {
    let GlyphTransform {
        cx,
        cy,
        cos_a,
        sin_a,
        scale,
        color,
    } = *transform;
    for (row, glyph_row) in glyph.iter().enumerate() {
        for col in 0..5 {
            if glyph_row & (1 << (4 - col)) != 0 {
                // Transform pixel position relative to glyph center
                let lx = (col as f64 - 2.0) * scale;
                let ly = (row as f64 - 3.0) * scale;

                let px = cx + lx * cos_a - ly * sin_a;
                let py = cy + lx * sin_a + ly * cos_a;

                let ix = px as i32;
                let iy = py as i32;
                if ix >= 0 && iy >= 0 && (ix as u32) < buffer.width && (iy as u32) < buffer.height {
                    let idx = ((iy as u32 * buffer.width + ix as u32) * 4) as usize;
                    buffer.data[idx] = color.r;
                    buffer.data[idx + 1] = color.g;
                    buffer.data[idx + 2] = color.b;
                    buffer.data[idx + 3] = color.a;
                }
            }
        }
    }
}

/// Get the 5x7 bitmap glyph for a character.
fn get_bitmap_glyph(ch: char) -> [u8; 7] {
    // Subset of ASCII 32-126 from label module
    let idx = ch as u32;
    if !(32..=126).contains(&idx) {
        return [0; 7]; // blank
    }
    FONT_GLYPHS[(idx - 32) as usize]
}

/// Minimal 5x7 bitmap font (same as label.rs but duplicated here to avoid coupling).
static FONT_GLYPHS: [[u8; 7]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // space
    [0x04, 0x04, 0x04, 0x04, 0x00, 0x04, 0x00], // !
    [0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00], // "
    [0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x00, 0x00], // #
    [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04], // $
    [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03], // %
    [0x08, 0x14, 0x14, 0x08, 0x15, 0x12, 0x0D], // &
    [0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00], // '
    [0x02, 0x04, 0x04, 0x04, 0x04, 0x04, 0x02], // (
    [0x08, 0x04, 0x04, 0x04, 0x04, 0x04, 0x08], // )
    [0x00, 0x04, 0x15, 0x0E, 0x15, 0x04, 0x00], // *
    [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00], // +
    [0x00, 0x00, 0x00, 0x00, 0x04, 0x04, 0x08], // ,
    [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00], // -
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00], // .
    [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10], // /
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E], // 0
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E], // 1
    [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F], // 2
    [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E], // 3
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02], // 4
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E], // 5
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E], // 6
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08], // 7
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E], // 8
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C], // 9
    [0x00, 0x04, 0x00, 0x00, 0x04, 0x00, 0x00], // :
    [0x00, 0x04, 0x00, 0x00, 0x04, 0x04, 0x08], // ;
    [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02], // <
    [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00], // =
    [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08], // >
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04], // ?
    [0x0E, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0E], // @
    [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11], // A
    [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E], // B
    [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E], // C
    [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E], // D
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F], // E
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10], // F
    [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0E], // G
    [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11], // H
    [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E], // I
    [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C], // J
    [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11], // K
    [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F], // L
    [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11], // M
    [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11], // N
    [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E], // O
    [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10], // P
    [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D], // Q
    [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11], // R
    [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E], // S
    [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // T
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E], // U
    [0x11, 0x11, 0x11, 0x0A, 0x0A, 0x04, 0x04], // V
    [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11], // W
    [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11], // X
    [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04], // Y
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F], // Z
    [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E], // [
    [0x10, 0x10, 0x08, 0x04, 0x02, 0x01, 0x01], // backslash
    [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E], // ]
    [0x04, 0x0A, 0x11, 0x00, 0x00, 0x00, 0x00], // ^
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F], // _
    [0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00], // `
    [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F], // a
    [0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x1E], // b
    [0x00, 0x00, 0x0E, 0x11, 0x10, 0x11, 0x0E], // c
    [0x01, 0x01, 0x0F, 0x11, 0x11, 0x11, 0x0F], // d
    [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E], // e
    [0x06, 0x08, 0x1E, 0x08, 0x08, 0x08, 0x08], // f
    [0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x0E], // g
    [0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x11], // h
    [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E], // i
    [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0C], // j
    [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12], // k
    [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E], // l
    [0x00, 0x00, 0x1A, 0x15, 0x15, 0x15, 0x15], // m
    [0x00, 0x00, 0x1E, 0x11, 0x11, 0x11, 0x11], // n
    [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E], // o
    [0x00, 0x00, 0x1E, 0x11, 0x1E, 0x10, 0x10], // p
    [0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x01], // q
    [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10], // r
    [0x00, 0x00, 0x0E, 0x10, 0x0E, 0x01, 0x1E], // s
    [0x08, 0x08, 0x1E, 0x08, 0x08, 0x09, 0x06], // t
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D], // u
    [0x00, 0x00, 0x11, 0x11, 0x0A, 0x0A, 0x04], // v
    [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0A], // w
    [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11], // x
    [0x00, 0x00, 0x11, 0x0A, 0x04, 0x08, 0x10], // y
    [0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F], // z
    [0x02, 0x04, 0x04, 0x08, 0x04, 0x04, 0x02], // {
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // |
    [0x08, 0x04, 0x04, 0x02, 0x04, 0x04, 0x08], // }
    [0x00, 0x00, 0x08, 0x15, 0x02, 0x00, 0x00], // ~
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polyline_length_simple() {
        let pts = vec![(0.0, 0.0), (3.0, 4.0)]; // 3-4-5 triangle
        assert!((polyline_length(&pts) - 5.0).abs() < 0.001);
    }

    #[test]
    fn place_label_on_straight_line() {
        let pts = vec![(0.0, 50.0), (200.0, 50.0)];
        let text = "TEST";
        let widths = vec![8.0; 4]; // 4 chars, each 8px wide
        let params = CurvedLabelParams::default();
        let placed = place_curved_label(&pts, text, &widths, &params).unwrap();
        assert_eq!(placed.len(), 4);
        // All characters should be at y=50, angle ≈ 0
        for pc in &placed {
            assert!((pc.y - 50.0).abs() < 1.0);
            assert!(pc.angle.abs() < 0.01);
        }
    }

    #[test]
    fn rejects_too_short_line() {
        let pts = vec![(0.0, 0.0), (10.0, 0.0)]; // 10px line
        let text = "LONG LABEL TEXT";
        let widths = vec![8.0; 15]; // 120px of text
        let params = CurvedLabelParams::default();
        assert!(place_curved_label(&pts, text, &widths, &params).is_none());
    }

    #[test]
    fn rejects_sharp_curve() {
        // 90-degree turn
        let pts = vec![(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)];
        let text = "AB";
        let widths = vec![40.0, 40.0]; // Force chars across the turn
        let params = CurvedLabelParams {
            max_angle_delta: 0.1, // Very strict angle limit
            ..Default::default()
        };
        let result = place_curved_label(&pts, text, &widths, &params);
        assert!(result.is_none());
    }

    #[test]
    fn gentle_curve_accepted() {
        // Gentle arc
        let pts: Vec<(f64, f64)> = (0..20)
            .map(|i| {
                let x = i as f64 * 10.0;
                let y = 50.0 + (i as f64 * 0.1).sin() * 5.0;
                (x, y)
            })
            .collect();
        let text = "River";
        let widths = vec![8.0; 5];
        let params = CurvedLabelParams::default();
        let placed = place_curved_label(&pts, text, &widths, &params);
        assert!(placed.is_some());
    }

    #[test]
    fn render_curved_label_on_buffer() {
        let mut buffer = PixelBuffer::new(200, 100);
        let pts = vec![(10.0, 50.0), (190.0, 50.0)];
        let text = "Hello";
        let widths = vec![8.0; 5];
        let params = CurvedLabelParams::default();
        let placed = place_curved_label(&pts, text, &widths, &params).unwrap();
        render_curved_label_bitmap(&mut buffer, &placed, &params);
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 10);
    }

    #[test]
    fn to_screen_coords_conversion() {
        let points = vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }];
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let screen = to_screen_coords(&points, &bbox, 100, 100);
        assert!((screen[0].0 - 0.0).abs() < 0.001);
        assert!((screen[0].1 - 100.0).abs() < 0.001);
        assert!((screen[1].0 - 100.0).abs() < 0.001);
        assert!((screen[1].1 - 0.0).abs() < 0.001);
    }
}
