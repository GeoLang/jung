//! Heatmap renderer: kernel density estimation with configurable color ramps.
//!
//! Produces smooth density visualizations from point data using Gaussian kernels.

use crate::classification::ColorRamp;
use crate::geometry::Point;
use crate::renderer::{BBox, PixelBuffer};
use jung_style::Color;

/// Heatmap rendering parameters.
#[derive(Debug, Clone)]
pub struct HeatmapParams {
    /// Radius of influence for each point (in pixels).
    pub radius: f64,
    /// Intensity multiplier per point.
    pub intensity: f64,
    /// Color ramp for density visualization.
    pub color_ramp: ColorRamp,
    /// Opacity of the heatmap layer (0.0..1.0).
    pub opacity: f64,
}

impl Default for HeatmapParams {
    fn default() -> Self {
        Self {
            radius: 20.0,
            intensity: 1.0,
            color_ramp: default_heatmap_ramp(),
            opacity: 0.8,
        }
    }
}

/// Render a heatmap from point data.
pub fn render_heatmap(
    buffer: &mut PixelBuffer,
    points: &[Point],
    weights: &[f64],
    bbox: &BBox,
    params: &HeatmapParams,
) {
    let w = buffer.width as usize;
    let h = buffer.height as usize;

    // Accumulate density
    let mut density = vec![0.0f64; w * h];

    for (i, pt) in points.iter().enumerate() {
        let weight = weights.get(i).copied().unwrap_or(1.0) * params.intensity;
        let px = (pt.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * w as f64;
        let py = (bbox.max_y - pt.y) / (bbox.max_y - bbox.min_y) * h as f64;

        let r = params.radius;
        let r2 = r * r;
        let x_start_i = (px - r).floor() as i32;
        let x_end_i = (px + r).ceil() as i32;
        let y_start_i = (py - r).floor() as i32;
        let y_end_i = (py + r).ceil() as i32;

        // Clamp to buffer bounds
        let x_start = x_start_i.max(0) as usize;
        let x_end = x_end_i.min(w as i32 - 1);
        let y_start = y_start_i.max(0) as usize;
        let y_end = y_end_i.min(h as i32 - 1);

        if x_end < 0 || y_end < 0 {
            continue;
        }
        let x_end = x_end as usize;
        let y_end = y_end as usize;

        for y in y_start..=y_end {
            for x in x_start..=x_end {
                let dx = x as f64 - px;
                let dy = y as f64 - py;
                let dist2 = dx * dx + dy * dy;
                if dist2 <= r2 {
                    // Gaussian kernel: exp(-dist²/(2σ²)) where σ = radius/3
                    let sigma = r / 3.0;
                    let kernel = (-dist2 / (2.0 * sigma * sigma)).exp();
                    density[y * w + x] += weight * kernel;
                }
            }
        }
    }

    // Find max density for normalization
    let max_density = density.iter().copied().fold(0.0f64, f64::max);
    if max_density <= 0.0 {
        return;
    }

    // Map density to colors
    for y in 0..h {
        for x in 0..w {
            let d = density[y * w + x];
            if d <= 0.0 {
                continue;
            }
            let t = (d / max_density).clamp(0.0, 1.0);
            let color = params.color_ramp.interpolate(t);
            let alpha = (t * params.opacity * 255.0) as u8;

            // Alpha-composite onto buffer
            let idx = (y * w + x) * 4;
            let sa = alpha as u32;
            if sa == 0 {
                continue;
            }
            let da = buffer.data[idx + 3] as u32;
            let out_a = sa + da * (255 - sa) / 255;
            if out_a > 0 {
                let sr = color.r as u32;
                let sg = color.g as u32;
                let sb = color.b as u32;
                let dr = buffer.data[idx] as u32;
                let dg = buffer.data[idx + 1] as u32;
                let db = buffer.data[idx + 2] as u32;
                buffer.data[idx] = ((sr * sa + dr * da * (255 - sa) / 255) / out_a).min(255) as u8;
                buffer.data[idx + 1] =
                    ((sg * sa + dg * da * (255 - sa) / 255) / out_a).min(255) as u8;
                buffer.data[idx + 2] =
                    ((sb * sa + db * da * (255 - sa) / 255) / out_a).min(255) as u8;
                buffer.data[idx + 3] = out_a.min(255) as u8;
            }
        }
    }
}

/// Default heatmap color ramp: transparent → blue → green → yellow → red.
fn default_heatmap_ramp() -> ColorRamp {
    ColorRamp::new(vec![
        Color::rgba(0, 0, 255, 0),     // transparent blue (low)
        Color::rgba(0, 0, 255, 255),   // blue
        Color::rgba(0, 255, 0, 255),   // green
        Color::rgba(255, 255, 0, 255), // yellow
        Color::rgba(255, 0, 0, 255),   // red (high)
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_point_heatmap() {
        let mut buffer = PixelBuffer::new(64, 64);
        let points = vec![Point { x: 0.5, y: 0.5 }];
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let params = HeatmapParams {
            radius: 15.0,
            intensity: 1.0,
            ..Default::default()
        };
        render_heatmap(&mut buffer, &points, &[], &bbox, &params);

        // Center should have highest density (non-zero alpha)
        let center_idx = (32 * 64 + 32) * 4;
        assert!(buffer.data[center_idx + 3] > 0);
    }

    #[test]
    fn empty_points_no_output() {
        let mut buffer = PixelBuffer::new(32, 32);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        render_heatmap(&mut buffer, &[], &[], &bbox, &HeatmapParams::default());
        assert!(buffer.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn multiple_points_accumulate() {
        let mut buffer = PixelBuffer::new(64, 64);
        // Two points near center → higher density
        let points = vec![Point { x: 0.48, y: 0.5 }, Point { x: 0.52, y: 0.5 }];
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let params = HeatmapParams {
            radius: 10.0,
            intensity: 1.0,
            ..Default::default()
        };
        render_heatmap(&mut buffer, &points, &[], &bbox, &params);

        // Center area should be bright
        let center_idx = (32 * 64 + 32) * 4;
        assert!(buffer.data[center_idx + 3] > 100);
    }

    #[test]
    fn weighted_heatmap() {
        let mut buffer1 = PixelBuffer::new(64, 64);
        let mut buffer2 = PixelBuffer::new(64, 64);
        let points = vec![Point { x: 0.5, y: 0.5 }];
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let params = HeatmapParams {
            radius: 15.0,
            intensity: 1.0,
            ..Default::default()
        };

        render_heatmap(&mut buffer1, &points, &[1.0], &bbox, &params);
        render_heatmap(&mut buffer2, &points, &[5.0], &bbox, &params);

        // With single point, both produce same normalized result
        // (max density is just higher), so the visual output should be identical
        // since normalization happens per-frame.
        // This tests that the weight parameter is accepted without error.
        let center1 = (32 * 64 + 32) * 4;
        assert!(buffer1.data[center1 + 3] > 0);
        assert!(buffer2.data[center1 + 3] > 0);
    }

    #[test]
    fn heatmap_respects_bbox() {
        let mut buffer = PixelBuffer::new(64, 64);
        // Point outside bbox
        let points = vec![Point { x: 5.0, y: 5.0 }];
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        render_heatmap(&mut buffer, &points, &[], &bbox, &HeatmapParams::default());
        // Should produce no visible output (point maps far off-screen)
        assert!(buffer.data.iter().all(|&b| b == 0));
    }
}
