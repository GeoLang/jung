//! Topographic symbology: contour lines, hillshading, and terrain rendering.
//!
//! Implements standard topographic map conventions including contour lines,
//! index contours, hillshade (analytical shading), slope/aspect visualization,
//! and hypsometric tinting.

use crate::geometry::Point;
use crate::renderer::{BBox, PixelBuffer};
use jung_style::Color;

/// Contour line type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContourType {
    /// Regular intermediate contour.
    Intermediate,
    /// Index contour (typically every 5th, drawn thicker with labels).
    Index,
    /// Supplementary contour (half-interval, dashed).
    Supplementary,
    /// Depression contour (tick marks pointing downhill).
    Depression,
}

/// Parameters for contour rendering.
#[derive(Debug, Clone)]
pub struct ContourParams {
    /// Contour interval in elevation units.
    pub interval: f64,
    /// Index contour interval (typically 5× the regular interval).
    pub index_interval: f64,
    /// Color for intermediate contours.
    pub color: Color,
    /// Color for index contours.
    pub index_color: Color,
    /// Width of intermediate contours (pixels).
    pub width: f64,
    /// Width of index contours (pixels).
    pub index_width: f64,
}

impl Default for ContourParams {
    fn default() -> Self {
        Self {
            interval: 10.0,
            index_interval: 50.0,
            color: Color::rgba(139, 90, 43, 180), // brown, semi-transparent
            index_color: Color::rgba(120, 70, 30, 220), // darker brown
            width: 0.8,
            index_width: 1.5,
        }
    }
}

/// Render a contour line.
pub fn render_contour(
    buffer: &mut PixelBuffer,
    points: &[Point],
    bbox: &BBox,
    elevation: f64,
    params: &ContourParams,
) {
    let contour_type = classify_contour(elevation, params);
    let (color, width) = match contour_type {
        ContourType::Index => (params.index_color, params.index_width),
        ContourType::Intermediate => (params.color, params.width),
        ContourType::Supplementary => (
            Color::rgba(params.color.r, params.color.g, params.color.b, 100),
            params.width * 0.5,
        ),
        ContourType::Depression => (params.color, params.width),
    };

    // Convert to screen coords and draw
    let screen: Vec<(f64, f64)> = points
        .iter()
        .map(|p| {
            let x = (p.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * buffer.width as f64;
            let y = (bbox.max_y - p.y) / (bbox.max_y - bbox.min_y) * buffer.height as f64;
            (x, y)
        })
        .collect();

    draw_polyline(buffer, &screen, color, width);
}

/// Classify a contour by its elevation relative to intervals.
pub fn classify_contour(elevation: f64, params: &ContourParams) -> ContourType {
    if (elevation % params.index_interval).abs() < 0.01 {
        ContourType::Index
    } else {
        ContourType::Intermediate
    }
}

/// Hillshade parameters.
#[derive(Debug, Clone, Copy)]
pub struct HillshadeParams {
    /// Sun azimuth (degrees, 0=north, clockwise).
    pub azimuth: f64,
    /// Sun altitude (degrees above horizon).
    pub altitude: f64,
    /// Vertical exaggeration factor.
    pub z_factor: f64,
}

impl Default for HillshadeParams {
    fn default() -> Self {
        Self {
            azimuth: 315.0,
            altitude: 45.0,
            z_factor: 1.0,
        }
    }
}

/// Compute analytical hillshade from a DEM (digital elevation model).
/// The DEM is a row-major grid of elevations.
pub fn compute_hillshade(
    dem: &[f64],
    width: usize,
    height: usize,
    cell_size: f64,
    params: &HillshadeParams,
) -> Vec<u8> {
    let mut shade = vec![128u8; width * height];

    let azimuth_rad = (360.0 - params.azimuth + 90.0).to_radians();
    let altitude_rad = params.altitude.to_radians();
    let z = params.z_factor / (8.0 * cell_size);

    for y in 1..height - 1 {
        for x in 1..width - 1 {
            // 3x3 neighborhood
            let a = dem[(y - 1) * width + (x - 1)];
            let b = dem[(y - 1) * width + x];
            let c = dem[(y - 1) * width + (x + 1)];
            let d = dem[y * width + (x - 1)];
            let f = dem[y * width + (x + 1)];
            let g = dem[(y + 1) * width + (x - 1)];
            let h = dem[(y + 1) * width + x];
            let i = dem[(y + 1) * width + (x + 1)];

            // Horn's method for slope and aspect
            let dz_dx = ((c + 2.0 * f + i) - (a + 2.0 * d + g)) * z;
            let dz_dy = ((g + 2.0 * h + i) - (a + 2.0 * b + c)) * z;

            let slope = (dz_dx * dz_dx + dz_dy * dz_dy).sqrt().atan();
            let aspect = if dz_dx.abs() < 1e-10 && dz_dy.abs() < 1e-10 {
                0.0
            } else {
                dz_dy.atan2(-dz_dx)
            };

            let hillshade = (altitude_rad.sin() * slope.cos()
                + altitude_rad.cos() * slope.sin() * (azimuth_rad - aspect).cos())
            .clamp(0.0, 1.0);

            shade[y * width + x] = (hillshade * 255.0) as u8;
        }
    }
    shade
}

/// Apply hillshade as a semi-transparent overlay to a buffer.
pub fn apply_hillshade(buffer: &mut PixelBuffer, hillshade: &[u8], opacity: f64) {
    let w = buffer.width as usize;
    let h = buffer.height as usize;
    let expected = w * h;
    if hillshade.len() < expected {
        return;
    }

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 4;
            let shade = hillshade[y * w + x] as f64 / 255.0;
            let factor = 1.0 - (1.0 - shade) * opacity;

            buffer.data[idx] = (buffer.data[idx] as f64 * factor).clamp(0.0, 255.0) as u8;
            buffer.data[idx + 1] = (buffer.data[idx + 1] as f64 * factor).clamp(0.0, 255.0) as u8;
            buffer.data[idx + 2] = (buffer.data[idx + 2] as f64 * factor).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Hypsometric tinting: map elevation to a color ramp.
pub fn hypsometric_color(elevation: f64, min_elev: f64, max_elev: f64) -> Color {
    let t = ((elevation - min_elev) / (max_elev - min_elev)).clamp(0.0, 1.0);

    // Standard hypsometric scale: green → yellow → brown → white
    if t < 0.25 {
        let s = t / 0.25;
        Color::rgb(
            (40.0 + s * 150.0) as u8,
            (140.0 + s * 70.0) as u8,
            (40.0 + s * 20.0) as u8,
        )
    } else if t < 0.5 {
        let s = (t - 0.25) / 0.25;
        Color::rgb(
            (190.0 + s * 50.0) as u8,
            (210.0 - s * 60.0) as u8,
            (60.0 + s * 20.0) as u8,
        )
    } else if t < 0.75 {
        let s = (t - 0.5) / 0.25;
        Color::rgb(
            (240.0 - s * 40.0) as u8,
            (150.0 - s * 50.0) as u8,
            (80.0 + s * 50.0) as u8,
        )
    } else {
        let s = (t - 0.75) / 0.25;
        Color::rgb(
            (200.0 + s * 55.0) as u8,
            (100.0 + s * 155.0) as u8,
            (130.0 + s * 125.0) as u8,
        )
    }
}

/// Draw a polyline with given width using simple pixel blitting.
fn draw_polyline(buffer: &mut PixelBuffer, points: &[(f64, f64)], color: Color, width: f64) {
    if points.len() < 2 {
        return;
    }
    let half_w = width / 2.0;

    for window in points.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];

        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.001 {
            continue;
        }

        let steps = (len * 2.0).ceil() as i32;
        for s in 0..=steps {
            let t = s as f64 / steps as f64;
            let cx = x0 + dx * t;
            let cy = y0 + dy * t;

            // Draw thick pixel
            let hw = half_w.ceil() as i32;
            for py in -hw..=hw {
                for px in -hw..=hw {
                    if (px * px + py * py) as f64 <= half_w * half_w + 0.5 {
                        let x = (cx as i32 + px) as u32;
                        let y = (cy as i32 + py) as u32;
                        if x < buffer.width && y < buffer.height {
                            let idx = ((y * buffer.width + x) * 4) as usize;
                            // Alpha blend
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contour_classification() {
        let params = ContourParams::default();
        assert_eq!(classify_contour(50.0, &params), ContourType::Index);
        assert_eq!(classify_contour(100.0, &params), ContourType::Index);
        assert_eq!(classify_contour(30.0, &params), ContourType::Intermediate);
    }

    #[test]
    fn render_contour_line() {
        let mut buffer = PixelBuffer::new(64, 64);
        let points = vec![Point { x: 0.1, y: 0.5 }, Point { x: 0.9, y: 0.5 }];
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        render_contour(
            &mut buffer,
            &points,
            &bbox,
            100.0,
            &ContourParams::default(),
        );
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 0);
    }

    #[test]
    fn hillshade_flat_terrain() {
        // Flat terrain → uniform shade
        let dem = vec![100.0; 9]; // 3x3
        let shade = compute_hillshade(&dem, 3, 3, 1.0, &HillshadeParams::default());
        // Center pixel should be ~128 (uniform slope = no shadow)
        assert!(shade[4] > 100); // center of 3x3
    }

    #[test]
    fn hillshade_slope() {
        // East-facing slope
        let mut dem = vec![0.0; 25]; // 5x5
        for y in 0..5 {
            for x in 0..5 {
                dem[y * 5 + x] = x as f64 * 10.0; // rising to the east
            }
        }
        let shade = compute_hillshade(&dem, 5, 5, 1.0, &HillshadeParams::default());
        // With default 315° sun (NW), east slope should be darker
        let center = shade[2 * 5 + 2];
        assert!(center < 200); // not fully lit from this angle
    }

    #[test]
    fn hypsometric_low_is_green() {
        let c = hypsometric_color(0.0, 0.0, 1000.0);
        assert!(c.g > c.r); // greenish at low elevations
    }

    #[test]
    fn hypsometric_high_is_bright() {
        let c = hypsometric_color(1000.0, 0.0, 1000.0);
        // Top of ramp is white-ish
        assert!(c.r > 200);
        assert!(c.g > 200);
    }

    #[test]
    fn apply_hillshade_darkens() {
        let mut buffer = PixelBuffer::new(4, 4);
        // Fill with white
        for chunk in buffer.data.chunks_mut(4) {
            chunk[0] = 255;
            chunk[1] = 255;
            chunk[2] = 255;
            chunk[3] = 255;
        }
        // Shade = 0 (full shadow) everywhere
        let shade = vec![0u8; 16];
        apply_hillshade(&mut buffer, &shade, 0.5);
        // Pixels should be darkened by 50%
        assert!(buffer.data[0] < 200);
        assert!(buffer.data[0] > 100);
    }
}
