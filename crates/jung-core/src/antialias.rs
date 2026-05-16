//! Anti-aliased rendering primitives.
//!
//! Provides line, circle, and polygon rendering with subpixel anti-aliasing
//! using coverage-based compositing. Replaces the aliased primitives in the
//! core renderer when quality is preferred over speed.

use crate::renderer::PixelBuffer;
use jung_style::Color;

/// Anti-aliased line rendering using Xiaolin Wu's algorithm.
pub fn draw_line_aa(
    buffer: &mut PixelBuffer,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    width: f64,
    color: Color,
) {
    if width <= 1.0 {
        draw_line_wu(buffer, x0, y0, x1, y1, color);
    } else {
        draw_line_thick_aa(buffer, x0, y0, x1, y1, width, color);
    }
}

/// Anti-aliased circle rendering.
pub fn draw_circle_aa(buffer: &mut PixelBuffer, cx: f64, cy: f64, radius: f64, color: Color) {
    let r2_outer = (radius + 0.5) * (radius + 0.5);
    let r2_inner = (radius - 0.5).max(0.0) * (radius - 0.5).max(0.0);

    let min_x = (cx - radius - 1.0).floor().max(0.0) as i32;
    let max_x = (cx + radius + 1.0).ceil().min(buffer.width as f64 - 1.0) as i32;
    let min_y = (cy - radius - 1.0).floor().max(0.0) as i32;
    let max_y = (cy + radius + 1.0).ceil().min(buffer.height as f64 - 1.0) as i32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            let dist2 = dx * dx + dy * dy;

            let alpha = if dist2 <= r2_inner {
                1.0
            } else if dist2 <= r2_outer {
                let dist = dist2.sqrt();
                (radius + 0.5 - dist).clamp(0.0, 1.0)
            } else {
                continue;
            };

            let coverage = (alpha * color.a as f64) as u8;
            blend_pixel(buffer, x as u32, y as u32, color, coverage);
        }
    }
}

/// Anti-aliased filled circle.
pub fn fill_circle_aa(buffer: &mut PixelBuffer, cx: f64, cy: f64, radius: f64, fill: Color) {
    draw_circle_aa(buffer, cx, cy, radius, fill);
}

/// Anti-aliased polygon fill using scanline with subpixel coverage.
pub fn fill_polygon_aa(buffer: &mut PixelBuffer, vertices: &[(f64, f64)], color: Color) {
    if vertices.len() < 3 {
        return;
    }

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
    let max_y = max_y.min(buffer.height as i32 - 1);

    let n = vertices.len();
    let subsamples = 4; // 4x vertical supersampling

    for y in min_y..=max_y {
        for sub in 0..subsamples {
            let scan_y = y as f64 + (sub as f64 + 0.5) / subsamples as f64;
            let mut intersections = Vec::new();

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
                    let x_start = pair[0];
                    let x_end = pair[1];

                    let ix_start = x_start.floor() as i32;
                    let ix_end = x_end.ceil() as i32;

                    for x in ix_start.max(0)..=ix_end.min(buffer.width as i32 - 1) {
                        let xf = x as f64;
                        // Compute coverage for this pixel at this subsample
                        let coverage = if xf + 1.0 <= x_start || xf >= x_end {
                            0.0
                        } else if xf >= x_start && xf + 1.0 <= x_end {
                            1.0
                        } else if xf < x_start {
                            (xf + 1.0 - x_start).clamp(0.0, 1.0)
                        } else {
                            (x_end - xf).clamp(0.0, 1.0)
                        };

                        let alpha = (coverage * color.a as f64 / subsamples as f64) as u8;
                        if alpha > 0 {
                            blend_pixel(buffer, x as u32, y as u32, color, alpha);
                        }
                    }
                }
            }
        }
    }
}

/// Xiaolin Wu's line algorithm for 1px anti-aliased lines.
fn draw_line_wu(
    buffer: &mut PixelBuffer,
    mut x0: f64,
    mut y0: f64,
    mut x1: f64,
    mut y1: f64,
    color: Color,
) {
    let steep = (y1 - y0).abs() > (x1 - x0).abs();

    if steep {
        std::mem::swap(&mut x0, &mut y0);
        std::mem::swap(&mut x1, &mut y1);
    }
    if x0 > x1 {
        std::mem::swap(&mut x0, &mut x1);
        std::mem::swap(&mut y0, &mut y1);
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    let gradient = if dx.abs() < 0.001 { 1.0 } else { dy / dx };

    // First endpoint
    let xend = x0.round();
    let yend = y0 + gradient * (xend - x0);
    let xgap = rfpart(x0 + 0.5);
    let xpxl1 = xend as i32;
    let ypxl1 = yend.floor() as i32;

    if steep {
        plot(
            buffer,
            ypxl1,
            xpxl1,
            (rfpart(yend) * xgap * 255.0) as u8,
            color,
        );
        plot(
            buffer,
            ypxl1 + 1,
            xpxl1,
            (fpart(yend) * xgap * 255.0) as u8,
            color,
        );
    } else {
        plot(
            buffer,
            xpxl1,
            ypxl1,
            (rfpart(yend) * xgap * 255.0) as u8,
            color,
        );
        plot(
            buffer,
            xpxl1,
            ypxl1 + 1,
            (fpart(yend) * xgap * 255.0) as u8,
            color,
        );
    }

    let mut intery = yend + gradient;

    // Second endpoint
    let xend2 = x1.round();
    let yend2 = y1 + gradient * (xend2 - x1);
    let xgap2 = fpart(x1 + 0.5);
    let xpxl2 = xend2 as i32;
    let ypxl2 = yend2.floor() as i32;

    if steep {
        plot(
            buffer,
            ypxl2,
            xpxl2,
            (rfpart(yend2) * xgap2 * 255.0) as u8,
            color,
        );
        plot(
            buffer,
            ypxl2 + 1,
            xpxl2,
            (fpart(yend2) * xgap2 * 255.0) as u8,
            color,
        );
    } else {
        plot(
            buffer,
            xpxl2,
            ypxl2,
            (rfpart(yend2) * xgap2 * 255.0) as u8,
            color,
        );
        plot(
            buffer,
            xpxl2,
            ypxl2 + 1,
            (fpart(yend2) * xgap2 * 255.0) as u8,
            color,
        );
    }

    // Main loop
    for x in (xpxl1 + 1)..xpxl2 {
        let y = intery.floor() as i32;
        if steep {
            plot(buffer, y, x, (rfpart(intery) * 255.0) as u8, color);
            plot(buffer, y + 1, x, (fpart(intery) * 255.0) as u8, color);
        } else {
            plot(buffer, x, y, (rfpart(intery) * 255.0) as u8, color);
            plot(buffer, x, y + 1, (fpart(intery) * 255.0) as u8, color);
        }
        intery += gradient;
    }
}

/// Thick anti-aliased line using distance-based coverage.
fn draw_line_thick_aa(
    buffer: &mut PixelBuffer,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    width: f64,
    color: Color,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }

    let half_w = width / 2.0;
    let nx = -dy / len; // normal x
    let ny = dx / len; // normal y

    let min_x = x0.min(x1) - half_w - 1.0;
    let max_x = x0.max(x1) + half_w + 1.0;
    let min_y = y0.min(y1) - half_w - 1.0;
    let max_y = y0.max(y1) + half_w + 1.0;

    let ix_min = min_x.floor().max(0.0) as i32;
    let ix_max = max_x.ceil().min(buffer.width as f64 - 1.0) as i32;
    let iy_min = min_y.floor().max(0.0) as i32;
    let iy_max = max_y.ceil().min(buffer.height as f64 - 1.0) as i32;

    for y in iy_min..=iy_max {
        for x in ix_min..=ix_max {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;

            // Project point onto line segment
            let t = ((px - x0) * dx + (py - y0) * dy) / (len * len);
            let t_clamped = t.clamp(0.0, 1.0);
            let closest_x = x0 + t_clamped * dx;
            let closest_y = y0 + t_clamped * dy;

            let dist = ((px - closest_x).powi(2) + (py - closest_y).powi(2)).sqrt();

            if dist <= half_w + 0.5 {
                let alpha = (half_w + 0.5 - dist).clamp(0.0, 1.0);
                let coverage = (alpha * color.a as f64) as u8;
                if coverage > 0 {
                    blend_pixel(buffer, x as u32, y as u32, color, coverage);
                }
            }
        }
    }

    // discard unused normal (kept for reference)
    let _ = (nx, ny);
}

fn fpart(x: f64) -> f64 {
    x - x.floor()
}

fn rfpart(x: f64) -> f64 {
    1.0 - fpart(x)
}

fn plot(buffer: &mut PixelBuffer, x: i32, y: i32, coverage: u8, color: Color) {
    if x < 0 || y < 0 || x >= buffer.width as i32 || y >= buffer.height as i32 {
        return;
    }
    blend_pixel(buffer, x as u32, y as u32, color, coverage);
}

fn blend_pixel(buffer: &mut PixelBuffer, x: u32, y: u32, color: Color, alpha: u8) {
    if x >= buffer.width || y >= buffer.height {
        return;
    }
    let idx = ((y * buffer.width + x) * 4) as usize;
    let sa = alpha as u32;
    let inv_a = 255 - sa;

    buffer.data[idx] = ((color.r as u32 * sa + buffer.data[idx] as u32 * inv_a) / 255) as u8;
    buffer.data[idx + 1] =
        ((color.g as u32 * sa + buffer.data[idx + 1] as u32 * inv_a) / 255) as u8;
    buffer.data[idx + 2] =
        ((color.b as u32 * sa + buffer.data[idx + 2] as u32 * inv_a) / 255) as u8;
    buffer.data[idx + 3] = (sa + buffer.data[idx + 3] as u32 * inv_a / 255).min(255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aa_line_draws_pixels() {
        let mut buffer = PixelBuffer::new(100, 100);
        draw_line_aa(
            &mut buffer,
            10.0,
            10.0,
            90.0,
            90.0,
            1.0,
            Color::rgb(255, 0, 0),
        );
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 50);
    }

    #[test]
    fn aa_line_has_gradient_alpha() {
        let mut buffer = PixelBuffer::new(100, 100);
        draw_line_aa(
            &mut buffer,
            10.0,
            10.0,
            90.0,
            50.0,
            1.0,
            Color::rgb(255, 0, 0),
        );
        // Should have pixels with partial alpha (anti-aliased edges)
        let partial = buffer
            .data
            .chunks(4)
            .filter(|px| px[3] > 0 && px[3] < 255)
            .count();
        assert!(partial > 0);
    }

    #[test]
    fn aa_thick_line() {
        let mut buffer = PixelBuffer::new(100, 100);
        draw_line_aa(
            &mut buffer,
            10.0,
            50.0,
            90.0,
            50.0,
            5.0,
            Color::rgb(0, 0, 255),
        );
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        // Thick line should cover more pixels
        assert!(filled > 200);
    }

    #[test]
    fn aa_circle_smooth_edges() {
        let mut buffer = PixelBuffer::new(64, 64);
        draw_circle_aa(&mut buffer, 32.0, 32.0, 15.0, Color::rgb(0, 255, 0));
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 200);
        // Should have anti-aliased edge pixels
        let partial = buffer
            .data
            .chunks(4)
            .filter(|px| px[3] > 0 && px[3] < 255)
            .count();
        assert!(partial > 10);
    }

    #[test]
    fn aa_polygon_fill() {
        let mut buffer = PixelBuffer::new(100, 100);
        let vertices = vec![(20.0, 20.0), (80.0, 20.0), (80.0, 80.0), (20.0, 80.0)];
        fill_polygon_aa(&mut buffer, &vertices, Color::rgba(255, 0, 0, 200));
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        // ~60x60 = 3600 pixels
        assert!(filled > 3000);
    }

    #[test]
    fn aa_polygon_triangle_coverage() {
        let mut buffer = PixelBuffer::new(100, 100);
        let vertices = vec![(50.0, 10.0), (90.0, 90.0), (10.0, 90.0)];
        fill_polygon_aa(&mut buffer, &vertices, Color::rgb(0, 0, 255));
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 1000);
    }

    #[test]
    fn blend_pixel_accumulates() {
        let mut buffer = PixelBuffer::new(10, 10);
        blend_pixel(&mut buffer, 5, 5, Color::rgb(255, 0, 0), 128);
        let idx = (5 * 10 + 5) * 4;
        assert!(buffer.data[idx] > 100); // R
        assert!(buffer.data[idx + 3] > 0); // A

        // Blend again - should accumulate
        blend_pixel(&mut buffer, 5, 5, Color::rgb(255, 0, 0), 128);
        assert!(buffer.data[idx + 3] > 128);
    }
}
