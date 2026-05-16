use crate::geometry::Point;
use crate::renderer::{BBox, PixelBuffer};
use jung_style::{Color, LineCap, LineJoin};

/// Parameters for line rendering.
pub struct LineParams {
    pub color: Color,
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub dasharray: Option<Vec<f32>>,
    pub offset: f32,
    pub opacity: f32,
}

impl Default for LineParams {
    fn default() -> Self {
        Self {
            color: Color::rgb(0, 0, 0),
            width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            dasharray: None,
            offset: 0.0,
            opacity: 1.0,
        }
    }
}

/// Render a polyline onto the pixel buffer.
pub fn render_line(
    buffer: &mut PixelBuffer,
    points: &[Point],
    bbox: &BBox,
    canvas_width: u32,
    canvas_height: u32,
    params: &LineParams,
) {
    if points.len() < 2 {
        return;
    }

    let screen_pts: Vec<(f64, f64)> = points
        .iter()
        .map(|p| map_to_screen(p, bbox, canvas_width, canvas_height))
        .collect();

    let color = apply_opacity(params.color, params.opacity);

    // Apply line offset if non-zero
    let screen_pts = if params.offset.abs() > f32::EPSILON {
        offset_polyline(&screen_pts, params.offset as f64)
    } else {
        screen_pts
    };

    match &params.dasharray {
        Some(pattern) if !pattern.is_empty() => {
            render_dashed_line(
                buffer,
                &screen_pts,
                color,
                params.width,
                pattern,
                params.cap,
            );
        }
        _ => {
            render_solid_line(
                buffer,
                &screen_pts,
                color,
                params.width,
                params.cap,
                params.join,
            );
        }
    }
}

fn map_to_screen(p: &Point, bbox: &BBox, width: u32, height: u32) -> (f64, f64) {
    let x = (p.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * width as f64;
    let y = (bbox.max_y - p.y) / (bbox.max_y - bbox.min_y) * height as f64;
    (x, y)
}

fn apply_opacity(color: Color, opacity: f32) -> Color {
    let a = (color.a as f32 * opacity.clamp(0.0, 1.0)) as u8;
    Color::rgba(color.r, color.g, color.b, a)
}

fn render_solid_line(
    buffer: &mut PixelBuffer,
    points: &[(f64, f64)],
    color: Color,
    width: f32,
    cap: LineCap,
    join: LineJoin,
) {
    let half_w = width / 2.0;

    for i in 0..points.len() - 1 {
        let (x0, y0) = points[i];
        let (x1, y1) = points[i + 1];

        draw_thick_segment(buffer, x0, y0, x1, y1, half_w, color);

        // Draw joins at interior vertices
        if i > 0 {
            let (xp, yp) = points[i - 1];
            draw_join(
                buffer,
                &JoinArgs {
                    xp,
                    yp,
                    xc: x0,
                    yc: y0,
                    xn: x1,
                    yn: y1,
                    half_width: half_w,
                    color,
                    join,
                },
            );
        }
    }

    // Draw caps at endpoints
    if points.len() >= 2 {
        let (x0, y0) = points[0];
        let (x1, y1) = points[1];
        draw_cap(
            buffer,
            &CapArgs {
                x0,
                y0,
                x1,
                y1,
                half_width: half_w,
                color,
                cap,
                is_start: true,
            },
        );

        let n = points.len();
        let (x0, y0) = points[n - 2];
        let (x1, y1) = points[n - 1];
        draw_cap(
            buffer,
            &CapArgs {
                x0,
                y0,
                x1,
                y1,
                half_width: half_w,
                color,
                cap,
                is_start: false,
            },
        );
    }
}

fn render_dashed_line(
    buffer: &mut PixelBuffer,
    points: &[(f64, f64)],
    color: Color,
    width: f32,
    pattern: &[f32],
    cap: LineCap,
) {
    if pattern.is_empty() {
        return;
    }

    let half_w = width / 2.0;
    let mut pattern_idx = 0;
    let mut remaining_in_segment = pattern[0] as f64;
    let mut drawing = true; // first entry is always a dash

    for i in 0..points.len() - 1 {
        let (mut cx, mut cy) = points[i];
        let (ex, ey) = points[i + 1];

        let dx = ex - cx;
        let dy = ey - cy;
        let seg_len = (dx * dx + dy * dy).sqrt();
        if seg_len < 1e-10 {
            continue;
        }

        let ux = dx / seg_len;
        let uy = dy / seg_len;
        let mut consumed = 0.0;

        while consumed < seg_len {
            let step = remaining_in_segment.min(seg_len - consumed);
            let nx = cx + ux * step;
            let ny = cy + uy * step;

            if drawing {
                draw_thick_segment(buffer, cx, cy, nx, ny, half_w, color);
                if cap != LineCap::Butt {
                    draw_cap(
                        buffer,
                        &CapArgs {
                            x0: cx,
                            y0: cy,
                            x1: nx,
                            y1: ny,
                            half_width: half_w,
                            color,
                            cap,
                            is_start: true,
                        },
                    );
                    draw_cap(
                        buffer,
                        &CapArgs {
                            x0: cx,
                            y0: cy,
                            x1: nx,
                            y1: ny,
                            half_width: half_w,
                            color,
                            cap,
                            is_start: false,
                        },
                    );
                }
            }

            consumed += step;
            remaining_in_segment -= step;
            cx = nx;
            cy = ny;

            if remaining_in_segment < 1e-10 {
                drawing = !drawing;
                pattern_idx = (pattern_idx + 1) % pattern.len();
                remaining_in_segment = pattern[pattern_idx] as f64;
            }
        }
    }
}

/// Draw a thick line segment using perpendicular offset and scanline fill.
fn draw_thick_segment(
    buffer: &mut PixelBuffer,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    half_width: f32,
    color: Color,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-10 {
        return;
    }

    // Perpendicular unit vector
    let px = -dy / len;
    let py = dx / len;
    let hw = half_width as f64;

    // Four corners of the thick line quad
    let corners = [
        (x0 + px * hw, y0 + py * hw),
        (x0 - px * hw, y0 - py * hw),
        (x1 - px * hw, y1 - py * hw),
        (x1 + px * hw, y1 + py * hw),
    ];

    fill_convex_quad(buffer, &corners, color);
}

/// Fill a convex quadrilateral by scanline.
fn fill_convex_quad(buffer: &mut PixelBuffer, corners: &[(f64, f64); 4], color: Color) {
    let min_y = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|c| c.1)
        .fold(f64::NEG_INFINITY, f64::max);

    let y_start = (min_y.floor() as i32).max(0);
    let y_end = (max_y.ceil() as i32).min(buffer.height as i32 - 1);

    for y in y_start..=y_end {
        let fy = y as f64 + 0.5;
        let mut x_min = f64::INFINITY;
        let mut x_max = f64::NEG_INFINITY;

        // Find intersections with all 4 edges
        for i in 0..4 {
            let j = (i + 1) % 4;
            let (x0, y0) = corners[i];
            let (x1, y1) = corners[j];

            if (y0 <= fy && fy < y1) || (y1 <= fy && fy < y0) {
                let t = (fy - y0) / (y1 - y0);
                let x = x0 + t * (x1 - x0);
                x_min = x_min.min(x);
                x_max = x_max.max(x);
            }
        }

        if x_min <= x_max {
            let xs = (x_min.floor() as i32).max(0);
            let xe = (x_max.ceil() as i32).min(buffer.width as i32 - 1);
            for x in xs..=xe {
                blend_pixel(buffer, x as u32, y as u32, color);
            }
        }
    }
}

struct CapArgs {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    half_width: f32,
    color: Color,
    cap: LineCap,
    is_start: bool,
}

fn draw_cap(buffer: &mut PixelBuffer, args: &CapArgs) {
    let hw = args.half_width as f64;
    match args.cap {
        LineCap::Butt => {} // nothing extra
        LineCap::Round => {
            let (cx, cy) = if args.is_start {
                (args.x0, args.y0)
            } else {
                (args.x1, args.y1)
            };
            fill_circle(buffer, cx, cy, hw, args.color);
        }
        LineCap::Square => {
            let dx = args.x1 - args.x0;
            let dy = args.y1 - args.y0;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-10 {
                return;
            }
            let ux = dx / len;
            let uy = dy / len;
            let px = -uy;
            let py = ux;

            let (bx, by) = if args.is_start {
                (args.x0, args.y0)
            } else {
                (args.x1, args.y1)
            };
            let ext = if args.is_start { -1.0 } else { 1.0 };

            let corners = [
                (bx + px * hw, by + py * hw),
                (bx - px * hw, by - py * hw),
                (bx - px * hw + ux * hw * ext, by - py * hw + uy * hw * ext),
                (bx + px * hw + ux * hw * ext, by + py * hw + uy * hw * ext),
            ];
            fill_convex_quad(buffer, &corners, args.color);
        }
    }
}

struct JoinArgs {
    xp: f64,
    yp: f64,
    xc: f64,
    yc: f64,
    xn: f64,
    yn: f64,
    half_width: f32,
    color: Color,
    join: LineJoin,
}

fn draw_join(buffer: &mut PixelBuffer, args: &JoinArgs) {
    let hw = args.half_width as f64;
    match args.join {
        LineJoin::Round => {
            fill_circle(buffer, args.xc, args.yc, hw, args.color);
        }
        LineJoin::Bevel => {
            let (p1x, p1y) = perp_offset(args.xp, args.yp, args.xc, args.yc);
            let (p2x, p2y) = perp_offset(args.xc, args.yc, args.xn, args.yn);

            let corners = [
                (args.xc, args.yc),
                (args.xc + p1x * hw, args.yc + p1y * hw),
                (args.xc + p2x * hw, args.yc + p2y * hw),
                (args.xc, args.yc),
            ];
            fill_convex_quad(buffer, &corners, args.color);

            let corners2 = [
                (args.xc, args.yc),
                (args.xc - p1x * hw, args.yc - p1y * hw),
                (args.xc - p2x * hw, args.yc - p2y * hw),
                (args.xc, args.yc),
            ];
            fill_convex_quad(buffer, &corners2, args.color);
        }
        LineJoin::Miter => {
            fill_circle(buffer, args.xc, args.yc, hw, args.color);
        }
    }
}

fn perp_offset(x0: f64, y0: f64, x1: f64, y1: f64) -> (f64, f64) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-10 {
        return (0.0, 0.0);
    }
    (-dy / len, dx / len)
}

fn fill_circle(buffer: &mut PixelBuffer, cx: f64, cy: f64, radius: f64, color: Color) {
    let r = radius.ceil() as i32;
    let r_sq = radius * radius;

    for dy in -r..=r {
        for dx in -r..=r {
            let dist_sq = (dx * dx + dy * dy) as f64;
            if dist_sq <= r_sq {
                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if px >= 0 && py >= 0 {
                    blend_pixel(buffer, px as u32, py as u32, color);
                }
            }
        }
    }
}

fn blend_pixel(buffer: &mut PixelBuffer, x: u32, y: u32, color: Color) {
    if x >= buffer.width || y >= buffer.height {
        return;
    }
    let idx = ((y * buffer.width + x) * 4) as usize;
    let src_a = color.a as f32 / 255.0;
    let dst_a = buffer.data[idx + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);

    if out_a > 0.0 {
        buffer.data[idx] = ((color.r as f32 * src_a
            + buffer.data[idx] as f32 * dst_a * (1.0 - src_a))
            / out_a) as u8;
        buffer.data[idx + 1] = ((color.g as f32 * src_a
            + buffer.data[idx + 1] as f32 * dst_a * (1.0 - src_a))
            / out_a) as u8;
        buffer.data[idx + 2] = ((color.b as f32 * src_a
            + buffer.data[idx + 2] as f32 * dst_a * (1.0 - src_a))
            / out_a) as u8;
        buffer.data[idx + 3] = (out_a * 255.0) as u8;
    }
}

/// Offset a polyline by a perpendicular distance (positive = left, negative = right).
fn offset_polyline(points: &[(f64, f64)], offset: f64) -> Vec<(f64, f64)> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let mut result = Vec::with_capacity(points.len());

    for i in 0..points.len() {
        let (px, py) = if i == 0 {
            // Use first segment direction
            let dx = points[1].0 - points[0].0;
            let dy = points[1].1 - points[0].1;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-10 {
                (0.0, 0.0)
            } else {
                (-dy / len, dx / len)
            }
        } else if i == points.len() - 1 {
            // Use last segment direction
            let dx = points[i].0 - points[i - 1].0;
            let dy = points[i].1 - points[i - 1].1;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-10 {
                (0.0, 0.0)
            } else {
                (-dy / len, dx / len)
            }
        } else {
            // Average of both segment directions
            let dx1 = points[i].0 - points[i - 1].0;
            let dy1 = points[i].1 - points[i - 1].1;
            let len1 = (dx1 * dx1 + dy1 * dy1).sqrt();
            let dx2 = points[i + 1].0 - points[i].0;
            let dy2 = points[i + 1].1 - points[i].1;
            let len2 = (dx2 * dx2 + dy2 * dy2).sqrt();

            if len1 < 1e-10 || len2 < 1e-10 {
                (0.0, 0.0)
            } else {
                let nx = (-dy1 / len1 + -dy2 / len2) / 2.0;
                let ny = (dx1 / len1 + dx2 / len2) / 2.0;
                let nlen = (nx * nx + ny * ny).sqrt();
                if nlen < 1e-10 {
                    (0.0, 0.0)
                } else {
                    (nx / nlen, ny / nlen)
                }
            }
        };

        result.push((points[i].0 + px * offset, points[i].1 + py * offset));
    }

    result
}
