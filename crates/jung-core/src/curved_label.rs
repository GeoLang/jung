//! Curved label placement along line geometries.
//!
//! Places text labels along polylines, following the curvature of the line.
//! Characters are individually rotated to follow the line direction.

use crate::geometry::Point;
use crate::renderer::BBox;
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
