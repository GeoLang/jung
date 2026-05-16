//! 3D extrusion: renders polygons with height as pseudo-3D buildings.
//!
//! Uses orthographic projection with configurable light direction to render
//! extruded polygon "buildings" with top faces, walls, and shadows.

use crate::geometry::Point;
use crate::renderer::{BBox, PixelBuffer};
use jung_style::Color;

/// Parameters for 3D extrusion rendering.
#[derive(Debug, Clone)]
pub struct ExtrusionParams {
    /// Base height (ground level, in map units).
    pub base_height: f64,
    /// Extrusion height (building height, in map units).
    pub height: f64,
    /// Light direction as angle in radians (0 = right, π/2 = up).
    pub light_angle: f64,
    /// Light intensity (0.0..1.0).
    pub light_intensity: f64,
    /// Color of the top face.
    pub top_color: Color,
    /// Color of the walls (will be shaded).
    pub wall_color: Color,
    /// Pixel offset per unit height for the "3D" perspective effect.
    pub height_scale: f64,
}

impl Default for ExtrusionParams {
    fn default() -> Self {
        Self {
            base_height: 0.0,
            height: 10.0,
            light_angle: std::f64::consts::FRAC_PI_4, // 45 degrees
            light_intensity: 0.6,
            top_color: Color::rgb(180, 180, 200),
            wall_color: Color::rgb(140, 140, 160),
            height_scale: 0.5,
        }
    }
}

/// Render an extruded polygon (pseudo-3D building).
pub fn render_extrusion(
    buffer: &mut PixelBuffer,
    exterior: &[Point],
    bbox: &BBox,
    params: &ExtrusionParams,
) {
    if exterior.len() < 3 {
        return;
    }

    let w = buffer.width;
    let h = buffer.height;

    // Compute pixel offset for height
    let dx_offset = params.light_angle.cos() * params.height * params.height_scale;
    let dy_offset = -(params.light_angle.sin() * params.height * params.height_scale);

    // Convert to screen coordinates
    let base_screen: Vec<(f64, f64)> = exterior
        .iter()
        .map(|p| map_to_screen(p, bbox, w, h))
        .collect();

    let top_screen: Vec<(f64, f64)> = base_screen
        .iter()
        .map(|(x, y)| (x + dx_offset, y + dy_offset))
        .collect();

    // Render walls (back-to-front for painter's algorithm)
    render_walls(buffer, &base_screen, &top_screen, params);

    // Render top face
    scanline_fill_polygon(buffer, &top_screen, params.top_color);
}

/// Render the wall segments of the extrusion.
fn render_walls(
    buffer: &mut PixelBuffer,
    base: &[(f64, f64)],
    top: &[(f64, f64)],
    params: &ExtrusionParams,
) {
    let n = base.len();
    if n < 2 {
        return;
    }

    // Sort edges by average Y (render back walls first)
    let mut edges: Vec<usize> = (0..n - 1).collect();
    edges.sort_by(|&a, &b| {
        let ay = (base[a].1 + base[a + 1].1) / 2.0;
        let by = (base[b].1 + base[b + 1].1) / 2.0;
        ay.partial_cmp(&by).unwrap()
    });

    for &i in &edges {
        let j = i + 1;
        // Wall quad: base[i], base[j], top[j], top[i]
        let quad = [base[i], base[j], top[j], top[i]];

        // Compute wall normal for shading
        let edge_dx = base[j].0 - base[i].0;
        let edge_dy = base[j].1 - base[i].1;
        let normal_angle = edge_dy.atan2(-edge_dx);
        let light_dot = (normal_angle - params.light_angle).cos();
        let shade = 0.4 + 0.6 * (light_dot * params.light_intensity).clamp(0.0, 1.0);

        let wall_color = Color::rgb(
            (params.wall_color.r as f64 * shade) as u8,
            (params.wall_color.g as f64 * shade) as u8,
            (params.wall_color.b as f64 * shade) as u8,
        );

        scanline_fill_polygon(buffer, &quad, wall_color);
    }
}

/// Simple scanline polygon fill (convex or concave).
fn scanline_fill_polygon(buffer: &mut PixelBuffer, vertices: &[(f64, f64)], color: Color) {
    if vertices.is_empty() {
        return;
    }

    let w = buffer.width as i32;
    let h = buffer.height as i32;

    let min_y = vertices
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::MAX, f64::min)
        .floor() as i32;
    let max_y = vertices
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::MIN, f64::max)
        .ceil() as i32;

    let min_y = min_y.max(0);
    let max_y = max_y.min(h - 1);

    let n = vertices.len();
    for y in min_y..=max_y {
        let mut intersections = Vec::new();
        let scan_y = y as f64 + 0.5;

        for i in 0..n {
            let j = (i + 1) % n;
            let (x0, y0) = vertices[i];
            let (x1, y1) = vertices[j];

            if (y0 <= scan_y && y1 > scan_y) || (y1 <= scan_y && y0 > scan_y) {
                let t = (scan_y - y0) / (y1 - y0);
                intersections.push(x0 + t * (x1 - x0));
            }
        }

        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap());

        for pair in intersections.chunks(2) {
            if pair.len() == 2 {
                let x_start = (pair[0].ceil() as i32).max(0);
                let x_end = (pair[1].floor() as i32).min(w - 1);
                for x in x_start..=x_end {
                    let idx = ((y * w + x) * 4) as usize;
                    buffer.data[idx] = color.r;
                    buffer.data[idx + 1] = color.g;
                    buffer.data[idx + 2] = color.b;
                    buffer.data[idx + 3] = color.a;
                }
            }
        }
    }
}

fn map_to_screen(p: &Point, bbox: &BBox, width: u32, height: u32) -> (f64, f64) {
    let x = (p.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * width as f64;
    let y = (bbox.max_y - p.y) / (bbox.max_y - bbox.min_y) * height as f64;
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_extrusion() {
        let mut buffer = PixelBuffer::new(128, 128);
        let exterior = vec![
            Point { x: 0.3, y: 0.3 },
            Point { x: 0.7, y: 0.3 },
            Point { x: 0.7, y: 0.7 },
            Point { x: 0.3, y: 0.7 },
            Point { x: 0.3, y: 0.3 },
        ];
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let params = ExtrusionParams {
            height: 5.0,
            height_scale: 2.0,
            ..Default::default()
        };
        render_extrusion(&mut buffer, &exterior, &bbox, &params);

        // Should have non-zero pixels (something was drawn)
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 100, "Expected filled pixels, got {filled}");
    }

    #[test]
    fn extrusion_height_affects_offset() {
        let mut buf1 = PixelBuffer::new(128, 128);
        let mut buf2 = PixelBuffer::new(128, 128);
        let exterior = vec![
            Point { x: 0.3, y: 0.3 },
            Point { x: 0.7, y: 0.3 },
            Point { x: 0.7, y: 0.7 },
            Point { x: 0.3, y: 0.7 },
            Point { x: 0.3, y: 0.3 },
        ];
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };

        let p1 = ExtrusionParams {
            height: 2.0,
            ..Default::default()
        };
        let p2 = ExtrusionParams {
            height: 10.0,
            ..Default::default()
        };

        render_extrusion(&mut buf1, &exterior, &bbox, &p1);
        render_extrusion(&mut buf2, &exterior, &bbox, &p2);

        // Taller building should cover more pixels
        let count1 = buf1.data.chunks(4).filter(|px| px[3] > 0).count();
        let count2 = buf2.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(
            count2 > count1,
            "Taller building ({count2}) should have more pixels than short ({count1})"
        );
    }

    #[test]
    fn empty_polygon_no_crash() {
        let mut buffer = PixelBuffer::new(64, 64);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        render_extrusion(&mut buffer, &[], &bbox, &ExtrusionParams::default());
        render_extrusion(
            &mut buffer,
            &[Point { x: 0.5, y: 0.5 }],
            &bbox,
            &ExtrusionParams::default(),
        );
        // Should not crash
    }

    #[test]
    fn wall_shading_varies() {
        let mut buffer = PixelBuffer::new(128, 128);
        let exterior = vec![
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.8, y: 0.2 },
            Point { x: 0.8, y: 0.8 },
            Point { x: 0.2, y: 0.8 },
            Point { x: 0.2, y: 0.2 },
        ];
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let params = ExtrusionParams {
            height: 8.0,
            height_scale: 2.0,
            light_angle: 0.0, // light from right
            light_intensity: 1.0,
            ..Default::default()
        };
        render_extrusion(&mut buffer, &exterior, &bbox, &params);

        // Collect unique colors to verify shading variation
        let mut unique_colors: std::collections::HashSet<(u8, u8, u8)> =
            std::collections::HashSet::new();
        for chunk in buffer.data.chunks(4) {
            if chunk[3] > 0 {
                unique_colors.insert((chunk[0], chunk[1], chunk[2]));
            }
        }
        // Should have multiple shades (walls + top)
        assert!(
            unique_colors.len() >= 2,
            "Expected multiple shades, got {}",
            unique_colors.len()
        );
    }
}
