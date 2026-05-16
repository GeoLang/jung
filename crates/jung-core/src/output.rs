//! Print-quality and vector output support.
//!
//! Provides high-DPI rendering and SVG vector export for print-quality
//! cartographic output. Supports configurable DPI scaling, map elements
//! (scale bars, north arrows, legends), and SVG serialization.

use crate::geometry::Point;
use crate::renderer::{BBox, PixelBuffer};
use jung_style::Color;

/// Print output parameters.
#[derive(Debug, Clone)]
pub struct PrintParams {
    /// Output DPI (dots per inch). 72 = screen, 300 = print, 600 = high-quality.
    pub dpi: f64,
    /// Paper width in millimeters.
    pub width_mm: f64,
    /// Paper height in millimeters.
    pub height_mm: f64,
    /// Margin in millimeters.
    pub margin_mm: f64,
    /// Whether to include a scale bar.
    pub scale_bar: bool,
    /// Whether to include a north arrow.
    pub north_arrow: bool,
    /// Title text (empty = no title).
    pub title: String,
}

impl Default for PrintParams {
    fn default() -> Self {
        Self {
            dpi: 300.0,
            width_mm: 210.0,  // A4 width
            height_mm: 297.0, // A4 height
            margin_mm: 10.0,
            scale_bar: true,
            north_arrow: true,
            title: String::new(),
        }
    }
}

impl PrintParams {
    /// Compute the pixel dimensions for this print configuration.
    pub fn pixel_dimensions(&self) -> (u32, u32) {
        let w = (self.width_mm / 25.4 * self.dpi) as u32;
        let h = (self.height_mm / 25.4 * self.dpi) as u32;
        (w, h)
    }

    /// Compute the map area pixel dimensions (excluding margins).
    pub fn map_area_pixels(&self) -> (u32, u32) {
        let margin_px = (self.margin_mm / 25.4 * self.dpi) as u32;
        let (w, h) = self.pixel_dimensions();
        (w - 2 * margin_px, h - 2 * margin_px)
    }

    /// Get the DPI scale factor relative to 96 DPI (standard screen).
    pub fn scale_factor(&self) -> f64 {
        self.dpi / 96.0
    }
}

/// Create a high-DPI pixel buffer suitable for print output.
pub fn create_print_buffer(params: &PrintParams) -> PixelBuffer {
    let (w, h) = params.pixel_dimensions();
    let mut buffer = PixelBuffer::new(w, h);
    // Fill with white background
    for chunk in buffer.data.chunks_mut(4) {
        chunk[0] = 255;
        chunk[1] = 255;
        chunk[2] = 255;
        chunk[3] = 255;
    }
    buffer
}

/// SVG document builder for vector output.
#[derive(Debug, Clone)]
pub struct SvgDocument {
    pub width: f64,
    pub height: f64,
    pub elements: Vec<SvgElement>,
    pub view_box: Option<(f64, f64, f64, f64)>,
}

/// An SVG element.
#[derive(Debug, Clone)]
pub enum SvgElement {
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: String,
        stroke: String,
        stroke_width: f64,
    },
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fill: String,
        stroke: String,
        stroke_width: f64,
    },
    Path {
        d: String,
        fill: String,
        stroke: String,
        stroke_width: f64,
    },
    Text {
        x: f64,
        y: f64,
        content: String,
        font_size: f64,
        fill: String,
        anchor: String,
    },
    Polygon {
        points: String,
        fill: String,
        stroke: String,
        stroke_width: f64,
    },
    Group {
        transform: Option<String>,
        children: Vec<SvgElement>,
    },
}

impl SvgDocument {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            elements: Vec::new(),
            view_box: None,
        }
    }

    /// Set the SVG viewBox attribute.
    pub fn set_view_box(&mut self, min_x: f64, min_y: f64, w: f64, h: f64) {
        self.view_box = Some((min_x, min_y, w, h));
    }

    /// Add a circle element.
    pub fn add_circle(&mut self, cx: f64, cy: f64, r: f64, fill: &str, stroke: &str, sw: f64) {
        self.elements.push(SvgElement::Circle {
            cx,
            cy,
            r,
            fill: fill.to_string(),
            stroke: stroke.to_string(),
            stroke_width: sw,
        });
    }

    /// Add a polyline/path from points.
    pub fn add_polyline(&mut self, points: &[Point], bbox: &BBox, stroke: &str, sw: f64) {
        if points.len() < 2 {
            return;
        }
        let mut d = String::new();
        for (i, p) in points.iter().enumerate() {
            let x = (p.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * self.width;
            let y = (bbox.max_y - p.y) / (bbox.max_y - bbox.min_y) * self.height;
            if i == 0 {
                d.push_str(&format!("M {x:.2} {y:.2}"));
            } else {
                d.push_str(&format!(" L {x:.2} {y:.2}"));
            }
        }
        self.elements.push(SvgElement::Path {
            d,
            fill: "none".to_string(),
            stroke: stroke.to_string(),
            stroke_width: sw,
        });
    }

    /// Add a filled polygon.
    pub fn add_polygon(
        &mut self,
        points: &[Point],
        bbox: &BBox,
        fill: &str,
        stroke: &str,
        sw: f64,
    ) {
        if points.is_empty() {
            return;
        }
        let pts: Vec<String> = points
            .iter()
            .map(|p| {
                let x = (p.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * self.width;
                let y = (bbox.max_y - p.y) / (bbox.max_y - bbox.min_y) * self.height;
                format!("{x:.2},{y:.2}")
            })
            .collect();
        self.elements.push(SvgElement::Polygon {
            points: pts.join(" "),
            fill: fill.to_string(),
            stroke: stroke.to_string(),
            stroke_width: sw,
        });
    }

    /// Add text.
    pub fn add_text(&mut self, x: f64, y: f64, content: &str, font_size: f64, fill: &str) {
        self.elements.push(SvgElement::Text {
            x,
            y,
            content: content.to_string(),
            font_size,
            fill: fill.to_string(),
            anchor: "start".to_string(),
        });
    }

    /// Serialize to SVG XML string.
    pub fn to_svg(&self) -> String {
        let mut svg = String::new();
        svg.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        svg.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"",
            self.width, self.height
        ));
        if let Some((vx, vy, vw, vh)) = self.view_box {
            svg.push_str(&format!(" viewBox=\"{vx} {vy} {vw} {vh}\""));
        }
        svg.push_str(">\n");

        for elem in &self.elements {
            svg.push_str(&render_element(elem, 1));
        }

        svg.push_str("</svg>\n");
        svg
    }
}

/// Render a scale bar onto a pixel buffer.
pub fn render_scale_bar(buffer: &mut PixelBuffer, x: u32, y: u32, length_px: u32, color: Color) {
    let bar_height = 4u32;
    for dy in 0..bar_height {
        for dx in 0..length_px {
            let px = x + dx;
            let py = y + dy;
            if px < buffer.width && py < buffer.height {
                let idx = ((py * buffer.width + px) * 4) as usize;
                buffer.data[idx] = color.r;
                buffer.data[idx + 1] = color.g;
                buffer.data[idx + 2] = color.b;
                buffer.data[idx + 3] = 255;
            }
        }
    }
    // End ticks
    for dy in 0..8u32 {
        let py = y.saturating_sub(2) + dy;
        if py < buffer.height {
            // Left tick
            if x < buffer.width {
                let idx = ((py * buffer.width + x) * 4) as usize;
                buffer.data[idx] = color.r;
                buffer.data[idx + 1] = color.g;
                buffer.data[idx + 2] = color.b;
                buffer.data[idx + 3] = 255;
            }
            // Right tick
            let rx = x + length_px - 1;
            if rx < buffer.width {
                let idx = ((py * buffer.width + rx) * 4) as usize;
                buffer.data[idx] = color.r;
                buffer.data[idx + 1] = color.g;
                buffer.data[idx + 2] = color.b;
                buffer.data[idx + 3] = 255;
            }
        }
    }
}

/// Render a simple north arrow.
pub fn render_north_arrow(buffer: &mut PixelBuffer, cx: u32, cy: u32, size: u32) {
    let half = size / 2;
    let color = Color::rgb(0, 0, 0);

    // Triangle pointing up
    for dy in 0..size {
        let y = cy - half + dy;
        let progress = dy as f64 / size as f64;
        let half_width = (progress * half as f64) as u32;
        for dx in cx.saturating_sub(half_width)..=(cx + half_width).min(buffer.width - 1) {
            if y < buffer.height {
                let idx = ((y * buffer.width + dx) * 4) as usize;
                buffer.data[idx] = color.r;
                buffer.data[idx + 1] = color.g;
                buffer.data[idx + 2] = color.b;
                buffer.data[idx + 3] = 255;
            }
        }
    }
}

fn render_element(elem: &SvgElement, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    match elem {
        SvgElement::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
        } => {
            format!(
                "{pad}<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" fill=\"{fill}\" \
                 stroke=\"{stroke}\" stroke-width=\"{stroke_width}\"/>\n"
            )
        }
        SvgElement::Rect {
            x,
            y,
            width,
            height,
            fill,
            stroke,
            stroke_width,
        } => {
            format!(
                "{pad}<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" \
                 fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{stroke_width}\"/>\n"
            )
        }
        SvgElement::Path {
            d,
            fill,
            stroke,
            stroke_width,
        } => {
            format!(
                "{pad}<path d=\"{d}\" fill=\"{fill}\" stroke=\"{stroke}\" \
                 stroke-width=\"{stroke_width}\"/>\n"
            )
        }
        SvgElement::Text {
            x,
            y,
            content,
            font_size,
            fill,
            anchor,
        } => {
            let escaped = xml_escape(content);
            format!(
                "{pad}<text x=\"{x}\" y=\"{y}\" font-size=\"{font_size}\" \
                 fill=\"{fill}\" text-anchor=\"{anchor}\">{escaped}</text>\n"
            )
        }
        SvgElement::Polygon {
            points,
            fill,
            stroke,
            stroke_width,
        } => {
            format!(
                "{pad}<polygon points=\"{points}\" fill=\"{fill}\" \
                 stroke=\"{stroke}\" stroke-width=\"{stroke_width}\"/>\n"
            )
        }
        SvgElement::Group {
            transform,
            children,
        } => {
            let mut s = if let Some(t) = transform {
                format!("{pad}<g transform=\"{t}\">\n")
            } else {
                format!("{pad}<g>\n")
            };
            for child in children {
                s.push_str(&render_element(child, indent + 1));
            }
            s.push_str(&format!("{pad}</g>\n"));
            s
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_params_pixel_dimensions() {
        let params = PrintParams::default(); // A4 300dpi
        let (w, h) = params.pixel_dimensions();
        assert_eq!(w, 2480); // 210/25.4 * 300 ≈ 2480
        assert_eq!(h, 3507); // 297/25.4 * 300 ≈ 3507
    }

    #[test]
    fn print_params_scale_factor() {
        let params = PrintParams {
            dpi: 300.0,
            ..Default::default()
        };
        assert!((params.scale_factor() - 3.125).abs() < 0.01);
    }

    #[test]
    fn svg_circle() {
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.add_circle(50.0, 50.0, 10.0, "red", "black", 1.0);
        let svg = doc.to_svg();
        assert!(svg.contains("<circle"));
        assert!(svg.contains("cx=\"50\""));
        assert!(svg.contains("fill=\"red\""));
    }

    #[test]
    fn svg_path() {
        let mut doc = SvgDocument::new(100.0, 100.0);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let points = vec![Point { x: 0.0, y: 0.5 }, Point { x: 1.0, y: 0.5 }];
        doc.add_polyline(&points, &bbox, "blue", 2.0);
        let svg = doc.to_svg();
        assert!(svg.contains("<path"));
        assert!(svg.contains("stroke=\"blue\""));
    }

    #[test]
    fn svg_text_escaping() {
        let mut doc = SvgDocument::new(100.0, 100.0);
        doc.add_text(10.0, 20.0, "A < B & C > D", 12.0, "black");
        let svg = doc.to_svg();
        assert!(svg.contains("A &lt; B &amp; C &gt; D"));
    }

    #[test]
    fn create_print_buffer_white() {
        let params = PrintParams {
            dpi: 72.0,
            width_mm: 50.0,
            height_mm: 50.0,
            ..Default::default()
        };
        let buffer = create_print_buffer(&params);
        // All pixels should be white
        assert!(buffer.data.chunks(4).all(|px| px[0] == 255 && px[3] == 255));
    }

    #[test]
    fn scale_bar_renders() {
        let mut buffer = PixelBuffer::new(100, 100);
        render_scale_bar(&mut buffer, 10, 90, 50, Color::rgb(0, 0, 0));
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 100);
    }

    #[test]
    fn north_arrow_renders() {
        let mut buffer = PixelBuffer::new(64, 64);
        render_north_arrow(&mut buffer, 32, 32, 20);
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 20);
    }

    #[test]
    fn svg_polygon() {
        let mut doc = SvgDocument::new(200.0, 200.0);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let points = vec![
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.8, y: 0.2 },
            Point { x: 0.5, y: 0.8 },
        ];
        doc.add_polygon(&points, &bbox, "#ff0000", "black", 1.0);
        let svg = doc.to_svg();
        assert!(svg.contains("<polygon"));
        assert!(svg.contains("fill=\"#ff0000\""));
    }
}
