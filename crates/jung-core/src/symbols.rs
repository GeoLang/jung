//! Built-in symbol library.
//!
//! Provides a collection of pre-defined vector symbols (markers, patterns,
//! shields) that can be rendered at any size without sprites.
//! Each symbol is defined as vector draw commands for resolution independence.

use crate::renderer::PixelBuffer;
use jung_style::Color;

/// A vector symbol definition.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub commands: Vec<DrawCommand>,
    /// Logical width of the symbol (coordinate space).
    pub width: f64,
    /// Logical height of the symbol (coordinate space).
    pub height: f64,
}

/// Drawing commands for vector symbols.
#[derive(Debug, Clone)]
pub enum DrawCommand {
    /// Filled circle at (cx, cy) with radius r.
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        color: Color,
    },
    /// Filled rectangle.
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: Color,
    },
    /// Filled triangle.
    Triangle {
        points: [(f64, f64); 3],
        color: Color,
    },
    /// Line segment.
    Line {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        color: Color,
        width: f64,
    },
    /// Filled polygon.
    Polygon {
        points: Vec<(f64, f64)>,
        color: Color,
    },
    /// Arc (partial circle outline).
    Arc {
        cx: f64,
        cy: f64,
        r: f64,
        start: f64,
        end: f64,
        color: Color,
        width: f64,
    },
}

/// The built-in symbol catalogue.
pub struct SymbolLibrary {
    symbols: Vec<Symbol>,
}

impl Default for SymbolLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolLibrary {
    /// Create the library with all built-in symbols.
    pub fn new() -> Self {
        let symbols = vec![
            // Map markers
            pin_marker(),
            flag_marker(),
            crosshair(),
            // Transportation
            airport(),
            parking(),
            fuel_station(),
            // Points of interest
            hospital(),
            restaurant(),
            information(),
            // Nature
            tree(),
            mountain_peak(),
            water_drop(),
            // Shields (road shields)
            highway_shield(),
            state_route_shield(),
            // Hazards
            warning_triangle(),
            radiation(),
        ];

        Self { symbols }
    }

    /// Get a symbol by name.
    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.name == name)
    }

    /// List all available symbol names.
    pub fn names(&self) -> Vec<&str> {
        self.symbols.iter().map(|s| s.name.as_str()).collect()
    }

    /// Render a symbol onto a buffer at given position and size.
    pub fn render(
        &self,
        buffer: &mut PixelBuffer,
        name: &str,
        cx: f64,
        cy: f64,
        size: f64,
    ) -> bool {
        let Some(symbol) = self.get(name) else {
            return false;
        };
        render_symbol(buffer, symbol, cx, cy, size);
        true
    }
}

/// Render a symbol centered at (cx, cy) with given pixel size.
pub fn render_symbol(buffer: &mut PixelBuffer, symbol: &Symbol, cx: f64, cy: f64, size: f64) {
    let scale_x = size / symbol.width;
    let scale_y = size / symbol.height;
    let origin_x = cx - size / 2.0;
    let origin_y = cy - size / 2.0;

    for cmd in &symbol.commands {
        match cmd {
            DrawCommand::Circle {
                cx: scx,
                cy: scy,
                r,
                color,
            } => {
                let px = origin_x + scx * scale_x;
                let py = origin_y + scy * scale_y;
                let pr = r * scale_x.min(scale_y);
                fill_circle(buffer, px, py, pr, *color);
            }
            DrawCommand::Rect { x, y, w, h, color } => {
                let px = (origin_x + x * scale_x) as i32;
                let py = (origin_y + y * scale_y) as i32;
                let pw = (w * scale_x) as i32;
                let ph = (h * scale_y) as i32;
                fill_rect(buffer, px, py, pw, ph, *color);
            }
            DrawCommand::Triangle { points, color } => {
                let pts: Vec<(f64, f64)> = points
                    .iter()
                    .map(|(x, y)| (origin_x + x * scale_x, origin_y + y * scale_y))
                    .collect();
                fill_triangle(buffer, &pts, *color);
            }
            DrawCommand::Line {
                x0,
                y0,
                x1,
                y1,
                color,
                width,
            } => {
                let px0 = origin_x + x0 * scale_x;
                let py0 = origin_y + y0 * scale_y;
                let px1 = origin_x + x1 * scale_x;
                let py1 = origin_y + y1 * scale_y;
                draw_line(
                    buffer,
                    px0,
                    py0,
                    px1,
                    py1,
                    *width * scale_x.min(scale_y),
                    *color,
                );
            }
            DrawCommand::Polygon { points, color } => {
                let pts: Vec<(f64, f64)> = points
                    .iter()
                    .map(|(x, y)| (origin_x + x * scale_x, origin_y + y * scale_y))
                    .collect();
                fill_poly(buffer, &pts, *color);
            }
            DrawCommand::Arc {
                cx: acx,
                cy: acy,
                r,
                start,
                end,
                color,
                width,
            } => {
                let px = origin_x + acx * scale_x;
                let py = origin_y + acy * scale_y;
                let pr = r * scale_x.min(scale_y);
                draw_arc(
                    buffer,
                    &ArcParams {
                        cx: px,
                        cy: py,
                        r: pr,
                        start: *start,
                        end: *end,
                        width: *width * scale_x.min(scale_y),
                        color: *color,
                    },
                );
            }
        }
    }
}

// --- Symbol definitions ---

fn pin_marker() -> Symbol {
    Symbol {
        name: "pin".to_string(),
        width: 24.0,
        height: 36.0,
        commands: vec![
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 10.0,
                color: Color::rgb(220, 50, 50),
            },
            DrawCommand::Triangle {
                points: [(6.0, 20.0), (18.0, 20.0), (12.0, 35.0)],
                color: Color::rgb(220, 50, 50),
            },
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 4.0,
                color: Color::rgb(255, 255, 255),
            },
        ],
    }
}

fn flag_marker() -> Symbol {
    Symbol {
        name: "flag".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Line {
                x0: 4.0,
                y0: 2.0,
                x1: 4.0,
                y1: 22.0,
                color: Color::rgb(60, 60, 60),
                width: 2.0,
            },
            DrawCommand::Polygon {
                points: vec![(4.0, 2.0), (20.0, 5.0), (20.0, 12.0), (4.0, 9.0)],
                color: Color::rgb(220, 50, 50),
            },
        ],
    }
}

fn crosshair() -> Symbol {
    Symbol {
        name: "crosshair".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 8.0,
                color: Color::rgba(0, 0, 0, 0),
            },
            DrawCommand::Line {
                x0: 12.0,
                y0: 2.0,
                x1: 12.0,
                y1: 8.0,
                color: Color::rgb(0, 0, 0),
                width: 1.5,
            },
            DrawCommand::Line {
                x0: 12.0,
                y0: 16.0,
                x1: 12.0,
                y1: 22.0,
                color: Color::rgb(0, 0, 0),
                width: 1.5,
            },
            DrawCommand::Line {
                x0: 2.0,
                y0: 12.0,
                x1: 8.0,
                y1: 12.0,
                color: Color::rgb(0, 0, 0),
                width: 1.5,
            },
            DrawCommand::Line {
                x0: 16.0,
                y0: 12.0,
                x1: 22.0,
                y1: 12.0,
                color: Color::rgb(0, 0, 0),
                width: 1.5,
            },
        ],
    }
}

fn airport() -> Symbol {
    Symbol {
        name: "airport".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 11.0,
                color: Color::rgb(50, 100, 200),
            },
            // Simplified plane shape
            DrawCommand::Rect {
                x: 10.0,
                y: 4.0,
                w: 4.0,
                h: 16.0,
                color: Color::rgb(255, 255, 255),
            },
            DrawCommand::Rect {
                x: 4.0,
                y: 9.0,
                w: 16.0,
                h: 3.0,
                color: Color::rgb(255, 255, 255),
            },
            DrawCommand::Rect {
                x: 8.0,
                y: 17.0,
                w: 8.0,
                h: 2.0,
                color: Color::rgb(255, 255, 255),
            },
        ],
    }
}

fn parking() -> Symbol {
    Symbol {
        name: "parking".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Rect {
                x: 2.0,
                y: 2.0,
                w: 20.0,
                h: 20.0,
                color: Color::rgb(0, 100, 200),
            },
            // Letter P (simplified)
            DrawCommand::Rect {
                x: 8.0,
                y: 5.0,
                w: 2.0,
                h: 14.0,
                color: Color::rgb(255, 255, 255),
            },
            DrawCommand::Rect {
                x: 8.0,
                y: 5.0,
                w: 8.0,
                h: 2.0,
                color: Color::rgb(255, 255, 255),
            },
            DrawCommand::Rect {
                x: 14.0,
                y: 5.0,
                w: 2.0,
                h: 7.0,
                color: Color::rgb(255, 255, 255),
            },
            DrawCommand::Rect {
                x: 8.0,
                y: 10.0,
                w: 8.0,
                h: 2.0,
                color: Color::rgb(255, 255, 255),
            },
        ],
    }
}

fn fuel_station() -> Symbol {
    Symbol {
        name: "fuel".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Rect {
                x: 4.0,
                y: 6.0,
                w: 12.0,
                h: 14.0,
                color: Color::rgb(60, 60, 60),
            },
            DrawCommand::Rect {
                x: 6.0,
                y: 8.0,
                w: 8.0,
                h: 5.0,
                color: Color::rgb(200, 200, 50),
            },
            DrawCommand::Line {
                x0: 18.0,
                y0: 4.0,
                x1: 18.0,
                y1: 16.0,
                color: Color::rgb(60, 60, 60),
                width: 2.0,
            },
            DrawCommand::Line {
                x0: 16.0,
                y0: 16.0,
                x1: 18.0,
                y1: 16.0,
                color: Color::rgb(60, 60, 60),
                width: 2.0,
            },
        ],
    }
}

fn hospital() -> Symbol {
    Symbol {
        name: "hospital".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Rect {
                x: 2.0,
                y: 2.0,
                w: 20.0,
                h: 20.0,
                color: Color::rgb(255, 255, 255),
            },
            DrawCommand::Rect {
                x: 10.0,
                y: 5.0,
                w: 4.0,
                h: 14.0,
                color: Color::rgb(220, 0, 0),
            },
            DrawCommand::Rect {
                x: 5.0,
                y: 10.0,
                w: 14.0,
                h: 4.0,
                color: Color::rgb(220, 0, 0),
            },
        ],
    }
}

fn restaurant() -> Symbol {
    Symbol {
        name: "restaurant".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 11.0,
                color: Color::rgb(180, 60, 30),
            },
            // Fork and knife (simplified)
            DrawCommand::Line {
                x0: 9.0,
                y0: 5.0,
                x1: 9.0,
                y1: 19.0,
                color: Color::rgb(255, 255, 255),
                width: 1.5,
            },
            DrawCommand::Line {
                x0: 15.0,
                y0: 5.0,
                x1: 15.0,
                y1: 19.0,
                color: Color::rgb(255, 255, 255),
                width: 1.5,
            },
            DrawCommand::Line {
                x0: 7.0,
                y0: 5.0,
                x1: 7.0,
                y1: 10.0,
                color: Color::rgb(255, 255, 255),
                width: 1.0,
            },
            DrawCommand::Line {
                x0: 11.0,
                y0: 5.0,
                x1: 11.0,
                y1: 10.0,
                color: Color::rgb(255, 255, 255),
                width: 1.0,
            },
        ],
    }
}

fn information() -> Symbol {
    Symbol {
        name: "info".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 11.0,
                color: Color::rgb(50, 130, 200),
            },
            DrawCommand::Circle {
                cx: 12.0,
                cy: 7.0,
                r: 2.0,
                color: Color::rgb(255, 255, 255),
            },
            DrawCommand::Rect {
                x: 10.5,
                y: 10.0,
                w: 3.0,
                h: 9.0,
                color: Color::rgb(255, 255, 255),
            },
        ],
    }
}

fn tree() -> Symbol {
    Symbol {
        name: "tree".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Rect {
                x: 10.0,
                y: 16.0,
                w: 4.0,
                h: 6.0,
                color: Color::rgb(139, 90, 43),
            },
            DrawCommand::Triangle {
                points: [(12.0, 2.0), (4.0, 16.0), (20.0, 16.0)],
                color: Color::rgb(34, 139, 34),
            },
        ],
    }
}

fn mountain_peak() -> Symbol {
    Symbol {
        name: "peak".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Triangle {
                points: [(12.0, 3.0), (3.0, 21.0), (21.0, 21.0)],
                color: Color::rgb(139, 90, 43),
            },
            DrawCommand::Triangle {
                points: [(12.0, 3.0), (9.0, 9.0), (15.0, 9.0)],
                color: Color::rgb(255, 255, 255),
            },
        ],
    }
}

fn water_drop() -> Symbol {
    Symbol {
        name: "water".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Circle {
                cx: 12.0,
                cy: 15.0,
                r: 7.0,
                color: Color::rgb(50, 130, 220),
            },
            DrawCommand::Triangle {
                points: [(12.0, 3.0), (6.0, 12.0), (18.0, 12.0)],
                color: Color::rgb(50, 130, 220),
            },
        ],
    }
}

fn highway_shield() -> Symbol {
    Symbol {
        name: "highway-shield".to_string(),
        width: 24.0,
        height: 20.0,
        commands: vec![
            // Interstate shield shape (simplified as pentagon)
            DrawCommand::Polygon {
                points: vec![
                    (2.0, 2.0),
                    (22.0, 2.0),
                    (22.0, 14.0),
                    (12.0, 19.0),
                    (2.0, 14.0),
                ],
                color: Color::rgb(0, 80, 160),
            },
            DrawCommand::Polygon {
                points: vec![
                    (4.0, 4.0),
                    (20.0, 4.0),
                    (20.0, 12.0),
                    (12.0, 16.0),
                    (4.0, 12.0),
                ],
                color: Color::rgb(220, 0, 0),
            },
        ],
    }
}

fn state_route_shield() -> Symbol {
    Symbol {
        name: "route-shield".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 11.0,
                color: Color::rgb(255, 255, 255),
            },
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 9.5,
                color: Color::rgb(0, 0, 0),
            },
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 8.5,
                color: Color::rgb(255, 255, 255),
            },
        ],
    }
}

fn warning_triangle() -> Symbol {
    Symbol {
        name: "warning".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Triangle {
                points: [(12.0, 2.0), (2.0, 22.0), (22.0, 22.0)],
                color: Color::rgb(255, 200, 0),
            },
            DrawCommand::Triangle {
                points: [(12.0, 5.0), (5.0, 20.0), (19.0, 20.0)],
                color: Color::rgb(255, 220, 50),
            },
            DrawCommand::Rect {
                x: 11.0,
                y: 9.0,
                w: 2.0,
                h: 6.0,
                color: Color::rgb(0, 0, 0),
            },
            DrawCommand::Circle {
                cx: 12.0,
                cy: 17.5,
                r: 1.2,
                color: Color::rgb(0, 0, 0),
            },
        ],
    }
}

fn radiation() -> Symbol {
    Symbol {
        name: "radiation".to_string(),
        width: 24.0,
        height: 24.0,
        commands: vec![
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 11.0,
                color: Color::rgb(255, 220, 0),
            },
            DrawCommand::Circle {
                cx: 12.0,
                cy: 12.0,
                r: 3.0,
                color: Color::rgb(0, 0, 0),
            },
            // Three blades (simplified as triangles)
            DrawCommand::Triangle {
                points: [(12.0, 2.0), (9.0, 9.0), (15.0, 9.0)],
                color: Color::rgb(0, 0, 0),
            },
            DrawCommand::Triangle {
                points: [(4.0, 17.0), (9.0, 13.0), (12.0, 17.0)],
                color: Color::rgb(0, 0, 0),
            },
            DrawCommand::Triangle {
                points: [(20.0, 17.0), (15.0, 13.0), (12.0, 17.0)],
                color: Color::rgb(0, 0, 0),
            },
        ],
    }
}

// --- Rendering helpers ---

fn fill_circle(buffer: &mut PixelBuffer, cx: f64, cy: f64, r: f64, color: Color) {
    if color.a == 0 {
        return;
    }
    let r2 = r * r;
    let min_x = (cx - r).floor().max(0.0) as i32;
    let max_x = (cx + r).ceil().min(buffer.width as f64 - 1.0) as i32;
    let min_y = (cy - r).floor().max(0.0) as i32;
    let max_y = (cy + r).ceil().min(buffer.height as f64 - 1.0) as i32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                set_pixel(buffer, x as u32, y as u32, color);
            }
        }
    }
}

fn fill_rect(buffer: &mut PixelBuffer, x: i32, y: i32, w: i32, h: i32, color: Color) {
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px >= 0 && py >= 0 && (px as u32) < buffer.width && (py as u32) < buffer.height {
                set_pixel(buffer, px as u32, py as u32, color);
            }
        }
    }
}

fn fill_triangle(buffer: &mut PixelBuffer, pts: &[(f64, f64)], color: Color) {
    if pts.len() < 3 {
        return;
    }
    fill_poly(buffer, &pts[..3], color);
}

fn fill_poly(buffer: &mut PixelBuffer, vertices: &[(f64, f64)], color: Color) {
    if vertices.is_empty() {
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

    for y in min_y..=max_y {
        let scan_y = y as f64 + 0.5;
        let mut xs = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            let (x0, y0) = vertices[i];
            let (x1, y1) = vertices[j];
            if (y0 <= scan_y && y1 > scan_y) || (y1 <= scan_y && y0 > scan_y) {
                let t = (scan_y - y0) / (y1 - y0);
                xs.push(x0 + t * (x1 - x0));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.chunks(2) {
            if pair.len() == 2 {
                let x_start = pair[0].ceil().max(0.0) as i32;
                let x_end = pair[1].floor().min(buffer.width as f64 - 1.0) as i32;
                for x in x_start..=x_end {
                    set_pixel(buffer, x as u32, y as u32, color);
                }
            }
        }
    }
}

fn draw_line(
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
    let steps = (len * 2.0).ceil() as i32;
    let half_w = width / 2.0;

    for s in 0..=steps {
        let t = s as f64 / steps as f64;
        let cx = x0 + dx * t;
        let cy = y0 + dy * t;
        let hw = half_w.ceil() as i32;
        for py in -hw..=hw {
            for px in -hw..=hw {
                if (px * px + py * py) as f64 <= half_w * half_w + 0.5 {
                    let x = cx as i32 + px;
                    let y = cy as i32 + py;
                    if x >= 0 && y >= 0 && (x as u32) < buffer.width && (y as u32) < buffer.height {
                        set_pixel(buffer, x as u32, y as u32, color);
                    }
                }
            }
        }
    }
}

struct ArcParams {
    cx: f64,
    cy: f64,
    r: f64,
    start: f64,
    end: f64,
    width: f64,
    color: Color,
}

fn draw_arc(buffer: &mut PixelBuffer, params: &ArcParams) {
    let ArcParams {
        cx,
        cy,
        r,
        start,
        end,
        width,
        color,
    } = *params;
    let steps = (r * (end - start)).ceil().max(8.0) as i32;
    for s in 0..=steps {
        let angle = start + (end - start) * s as f64 / steps as f64;
        let x = cx + angle.cos() * r;
        let y = cy + angle.sin() * r;
        let hw = (width / 2.0).ceil() as i32;
        for py in -hw..=hw {
            for px in -hw..=hw {
                let xi = x as i32 + px;
                let yi = y as i32 + py;
                if xi >= 0 && yi >= 0 && (xi as u32) < buffer.width && (yi as u32) < buffer.height {
                    set_pixel(buffer, xi as u32, yi as u32, color);
                }
            }
        }
    }
}

fn set_pixel(buffer: &mut PixelBuffer, x: u32, y: u32, color: Color) {
    if x >= buffer.width || y >= buffer.height {
        return;
    }
    let idx = ((y * buffer.width + x) * 4) as usize;
    let sa = color.a as u32;
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
    fn library_has_symbols() {
        let lib = SymbolLibrary::new();
        let names = lib.names();
        assert!(names.len() >= 14);
        assert!(names.contains(&"pin"));
        assert!(names.contains(&"airport"));
        assert!(names.contains(&"hospital"));
        assert!(names.contains(&"tree"));
        assert!(names.contains(&"warning"));
    }

    #[test]
    fn render_pin_marker() {
        let lib = SymbolLibrary::new();
        let mut buffer = PixelBuffer::new(64, 64);
        assert!(lib.render(&mut buffer, "pin", 32.0, 32.0, 32.0));
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 100);
    }

    #[test]
    fn render_hospital() {
        let lib = SymbolLibrary::new();
        let mut buffer = PixelBuffer::new(48, 48);
        assert!(lib.render(&mut buffer, "hospital", 24.0, 24.0, 40.0));
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 500);
    }

    #[test]
    fn render_unknown_returns_false() {
        let lib = SymbolLibrary::new();
        let mut buffer = PixelBuffer::new(32, 32);
        assert!(!lib.render(&mut buffer, "nonexistent", 16.0, 16.0, 24.0));
    }

    #[test]
    fn render_warning_triangle() {
        let lib = SymbolLibrary::new();
        let mut buffer = PixelBuffer::new(48, 48);
        assert!(lib.render(&mut buffer, "warning", 24.0, 24.0, 40.0));
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 200);
    }

    #[test]
    fn all_symbols_render_without_panic() {
        let lib = SymbolLibrary::new();
        for name in lib.names() {
            let mut buffer = PixelBuffer::new(32, 32);
            lib.render(&mut buffer, name, 16.0, 16.0, 24.0);
        }
    }

    #[test]
    fn symbol_at_different_sizes() {
        let lib = SymbolLibrary::new();
        let mut small = PixelBuffer::new(64, 64);
        let mut large = PixelBuffer::new(64, 64);
        lib.render(&mut small, "pin", 32.0, 32.0, 16.0);
        lib.render(&mut large, "pin", 32.0, 32.0, 48.0);
        let small_filled = small.data.chunks(4).filter(|px| px[3] > 0).count();
        let large_filled = large.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(large_filled > small_filled);
    }
}
