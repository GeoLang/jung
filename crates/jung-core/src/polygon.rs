use crate::geometry::Point;
use crate::line::{LineParams, render_line};
use crate::renderer::{BBox, PixelBuffer};
use jung_style::{Color, EvalContext, Layer, LineCap, StyleValue};

/// Render a filled polygon (exterior ring with optional holes).
#[allow(clippy::too_many_arguments)]
pub fn render_polygon(
    buffer: &mut PixelBuffer,
    exterior: &[Point],
    holes: &[Vec<Point>],
    bbox: &BBox,
    canvas_width: u32,
    canvas_height: u32,
    layer: &Layer,
    ctx: &EvalContext,
) {
    if exterior.len() < 3 {
        return;
    }

    let fill_color = layer
        .fill_color
        .as_ref()
        .and_then(|sv| sv.resolve(ctx))
        .unwrap_or(Color::rgba(0, 0, 0, 0));

    // Convert to screen coordinates
    let ext_screen: Vec<(f64, f64)> = exterior
        .iter()
        .map(|p| map_to_screen(p, bbox, canvas_width, canvas_height))
        .collect();

    let holes_screen: Vec<Vec<(f64, f64)>> = holes
        .iter()
        .map(|hole| {
            hole.iter()
                .map(|p| map_to_screen(p, bbox, canvas_width, canvas_height))
                .collect()
        })
        .collect();

    // Fill using even-odd scanline
    if fill_color.a > 0 {
        scanline_fill(buffer, &ext_screen, &holes_screen, fill_color);
    }

    // Stroke the outline
    if let Some(stroke_color) = layer.stroke_color.as_ref().and_then(|sv| sv.resolve(ctx)) {
        let width = resolve_f32(&layer.stroke_width, ctx).unwrap_or(1.0);
        if width > 0.0 && stroke_color.a > 0 {
            let params = LineParams {
                color: stroke_color,
                width,
                cap: LineCap::Butt,
                join: layer.line_join,
                dasharray: layer.line_dasharray.clone(),
                offset: 0.0,
                opacity: resolve_f32(&layer.line_opacity, ctx).unwrap_or(1.0),
            };
            render_line(buffer, exterior, bbox, canvas_width, canvas_height, &params);

            // Stroke holes too
            for hole in holes {
                render_line(buffer, hole, bbox, canvas_width, canvas_height, &params);
            }
        }
    }
}

fn resolve_f32(val: &Option<StyleValue<f32>>, ctx: &EvalContext) -> Option<f32> {
    val.as_ref().and_then(|sv| sv.resolve(ctx))
}

fn map_to_screen(p: &Point, bbox: &BBox, width: u32, height: u32) -> (f64, f64) {
    let x = (p.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * width as f64;
    let y = (bbox.max_y - p.y) / (bbox.max_y - bbox.min_y) * height as f64;
    (x, y)
}

/// Scanline fill using even-odd rule, supporting holes.
fn scanline_fill(
    buffer: &mut PixelBuffer,
    exterior: &[(f64, f64)],
    holes: &[Vec<(f64, f64)>],
    color: Color,
) {
    // Collect all edges from exterior and holes
    let mut edges: Vec<Edge> = Vec::new();
    collect_edges(exterior, &mut edges);
    for hole in holes {
        collect_edges(hole, &mut edges);
    }

    if edges.is_empty() {
        return;
    }

    // Find vertical bounds
    let min_y = edges.iter().map(|e| e.y_min).fold(f64::INFINITY, f64::min);
    let max_y = edges
        .iter()
        .map(|e| e.y_max)
        .fold(f64::NEG_INFINITY, f64::max);

    let y_start = (min_y.floor() as i32).max(0);
    let y_end = (max_y.ceil() as i32).min(buffer.height as i32 - 1);

    for y in y_start..=y_end {
        let fy = y as f64 + 0.5;

        // Find all x-intersections with edges at this scanline
        let mut intersections: Vec<f64> = Vec::new();
        for edge in &edges {
            if fy >= edge.y_min && fy < edge.y_max {
                let x = edge.x_at_y(fy);
                intersections.push(x);
            }
        }

        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Even-odd fill: fill between pairs of intersections
        let mut i = 0;
        while i + 1 < intersections.len() {
            let x_start = (intersections[i].ceil() as i32).max(0);
            let x_end = (intersections[i + 1].floor() as i32).min(buffer.width as i32 - 1);

            for x in x_start..=x_end {
                blend_pixel(buffer, x as u32, y as u32, color);
            }
            i += 2;
        }
    }
}

struct Edge {
    y_min: f64,
    y_max: f64,
    x_at_ymin: f64,
    inv_slope: f64, // dx/dy
}

impl Edge {
    fn x_at_y(&self, y: f64) -> f64 {
        self.x_at_ymin + (y - self.y_min) * self.inv_slope
    }
}

fn collect_edges(ring: &[(f64, f64)], edges: &mut Vec<Edge>) {
    let n = ring.len();
    if n < 3 {
        return;
    }
    for i in 0..n {
        let (x0, y0) = ring[i];
        let (x1, y1) = ring[(i + 1) % n];

        // Skip horizontal edges
        if (y1 - y0).abs() < 1e-10 {
            continue;
        }

        let (y_min, y_max, x_at_ymin) = if y0 < y1 { (y0, y1, x0) } else { (y1, y0, x1) };

        let inv_slope = (x1 - x0) / (y1 - y0);

        edges.push(Edge {
            y_min,
            y_max,
            x_at_ymin,
            inv_slope,
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use jung_style::{LineCap, LineJoin, StyleValue};
    use std::collections::HashMap;

    fn test_bbox() -> BBox {
        BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        }
    }

    fn make_layer(fill: Color) -> Layer {
        Layer {
            id: "test".to_string(),
            source: None,
            fill_color: Some(StyleValue::Literal(fill)),
            stroke_color: None,
            stroke_width: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            line_dasharray: None,
            line_offset: None,
            line_opacity: None,
            point_radius: None,
            icon_image: None,
            icon_size: None,
            font_family: None,
            font_size: None,
            text_field: None,
            text_color: None,
        }
    }

    fn make_ctx() -> EvalContext<'static> {
        static EMPTY: std::sync::LazyLock<HashMap<String, jung_style::PropertyValue>> =
            std::sync::LazyLock::new(HashMap::new);
        EvalContext {
            properties: &EMPTY,
            zoom: 10.0,
            geometry_type: "Polygon",
        }
    }

    #[test]
    fn fill_simple_square() {
        let mut buffer = PixelBuffer::new(100, 100);
        let exterior = vec![
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.8, y: 0.2 },
            Point { x: 0.8, y: 0.8 },
            Point { x: 0.2, y: 0.8 },
            Point { x: 0.2, y: 0.2 },
        ];
        let layer = make_layer(Color::rgb(255, 0, 0));
        let ctx = make_ctx();
        render_polygon(
            &mut buffer,
            &exterior,
            &[],
            &test_bbox(),
            100,
            100,
            &layer,
            &ctx,
        );
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        // ~60x60 = 3600 pixels
        assert!(filled > 3000);
    }

    #[test]
    fn polygon_with_hole() {
        let mut buffer = PixelBuffer::new(100, 100);
        let exterior = vec![
            Point { x: 0.1, y: 0.1 },
            Point { x: 0.9, y: 0.1 },
            Point { x: 0.9, y: 0.9 },
            Point { x: 0.1, y: 0.9 },
            Point { x: 0.1, y: 0.1 },
        ];
        let hole = vec![
            Point { x: 0.4, y: 0.4 },
            Point { x: 0.6, y: 0.4 },
            Point { x: 0.6, y: 0.6 },
            Point { x: 0.4, y: 0.6 },
            Point { x: 0.4, y: 0.4 },
        ];
        let layer = make_layer(Color::rgb(0, 0, 255));
        let ctx = make_ctx();
        render_polygon(
            &mut buffer,
            &exterior,
            &[hole],
            &test_bbox(),
            100,
            100,
            &layer,
            &ctx,
        );
        // Center should be empty (hole)
        let center_idx = (50 * 100 + 50) * 4;
        assert_eq!(buffer.data[center_idx + 3], 0);
        // Outer area should be filled
        let corner_idx = (20 * 100 + 20) * 4;
        assert!(buffer.data[corner_idx + 3] > 0);
    }

    #[test]
    fn degenerate_polygon_no_crash() {
        let mut buffer = PixelBuffer::new(50, 50);
        let exterior = vec![Point { x: 0.5, y: 0.5 }, Point { x: 0.6, y: 0.6 }];
        let layer = make_layer(Color::rgb(0, 255, 0));
        let ctx = make_ctx();
        render_polygon(
            &mut buffer,
            &exterior,
            &[],
            &test_bbox(),
            50,
            50,
            &layer,
            &ctx,
        );
        // No crash, no fill for <3 vertices
    }

    #[test]
    fn polygon_with_stroke() {
        let mut buffer = PixelBuffer::new(100, 100);
        let exterior = vec![
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.8, y: 0.2 },
            Point { x: 0.8, y: 0.8 },
            Point { x: 0.2, y: 0.8 },
            Point { x: 0.2, y: 0.2 },
        ];
        let mut layer = make_layer(Color::rgba(0, 0, 0, 0)); // No fill
        layer.stroke_color = Some(StyleValue::Literal(Color::rgb(255, 0, 0)));
        layer.stroke_width = Some(StyleValue::Literal(2.0));
        let ctx = make_ctx();
        render_polygon(
            &mut buffer,
            &exterior,
            &[],
            &test_bbox(),
            100,
            100,
            &layer,
            &ctx,
        );
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 100); // stroke pixels
    }

    #[test]
    fn polygon_semi_transparent() {
        let mut buffer = PixelBuffer::new(100, 100);
        let exterior = vec![
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.8, y: 0.2 },
            Point { x: 0.8, y: 0.8 },
            Point { x: 0.2, y: 0.8 },
            Point { x: 0.2, y: 0.2 },
        ];
        let layer = make_layer(Color::rgba(255, 0, 0, 128));
        let ctx = make_ctx();
        render_polygon(
            &mut buffer,
            &exterior,
            &[],
            &test_bbox(),
            100,
            100,
            &layer,
            &ctx,
        );
        // Should have semi-transparent pixels
        let semi = buffer
            .data
            .chunks(4)
            .filter(|px| px[3] > 50 && px[3] < 200)
            .count();
        assert!(semi > 1000);
    }
}
