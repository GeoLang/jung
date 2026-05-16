//! Maritime symbology: S-52 presentation library and S-57 ENC support.
//!
//! Implements chart symbol rendering conforming to IHO S-52 display standards
//! for Electronic Navigational Charts (ENCs) encoded in S-57 format.

use crate::geometry::Point;
use crate::renderer::{BBox, PixelBuffer};
use jung_style::Color;

/// S-57 object classes (feature types in navigational charts).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum S57ObjectClass {
    /// Buoy (lateral, cardinal, etc.)
    Buoy,
    /// Light (lighthouse, sector light)
    Light,
    /// Depth contour
    DepthContour,
    /// Depth area
    DepthArea,
    /// Land area
    LandArea,
    /// Coastline
    Coastline,
    /// Fairway
    Fairway,
    /// Anchorage area
    AnchorageArea,
    /// Restricted area
    RestrictedArea,
    /// Traffic separation scheme
    TrafficSeparation,
    /// Wreck
    Wreck,
    /// Rock (submerged, awash)
    Rock,
    /// Obstruction
    Obstruction,
    /// Sounding (depth value)
    Sounding,
    /// Navigation aid
    NavAid,
}

/// S-52 display categories.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayCategory {
    /// Always shown (base display)
    DisplayBase,
    /// Standard display
    Standard,
    /// Other (user selectable)
    Other,
    /// Mariners' choice
    MarinersStandard,
}

/// S-52 symbol type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SymbolType {
    Point,
    Line,
    Area,
    Text,
}

/// An S-52 color token (IHO color palette).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum S52Color {
    /// Deep water (blue)
    DEPDW,
    /// Medium depth water
    DEPMD,
    /// Shallow water
    DEPVS,
    /// Very shallow / drying
    DEPIT,
    /// Land (buff/tan)
    LANDA,
    /// Land (green)
    LANDG,
    /// Buoy/beacon
    CHMGD,
    /// Danger
    DNGHL,
    /// Caution area
    CHMGF,
    /// Traffic
    TRFCD,
    /// Restricted area
    RESBL,
    /// Background
    NODTA,
}

impl S52Color {
    /// Convert to an actual RGBA color (day palette).
    pub fn to_color(self) -> Color {
        match self {
            S52Color::DEPDW => Color::rgb(180, 210, 240), // deep water
            S52Color::DEPMD => Color::rgb(200, 225, 245), // medium depth
            S52Color::DEPVS => Color::rgb(180, 230, 230), // shallow
            S52Color::DEPIT => Color::rgb(140, 200, 180), // drying/intertidal
            S52Color::LANDA => Color::rgb(230, 210, 170), // land (buff)
            S52Color::LANDG => Color::rgb(180, 210, 160), // land (green)
            S52Color::CHMGD => Color::rgb(200, 50, 150),  // buoy/beacon magenta
            S52Color::DNGHL => Color::rgb(255, 0, 0),     // danger
            S52Color::CHMGF => Color::rgb(180, 100, 200), // caution
            S52Color::TRFCD => Color::rgb(200, 50, 200),  // traffic
            S52Color::RESBL => Color::rgb(100, 100, 200), // restricted
            S52Color::NODTA => Color::rgb(200, 200, 200), // no data
        }
    }

    /// Night palette (reduced brightness).
    pub fn to_color_night(self) -> Color {
        let day = self.to_color();
        Color::rgb(day.r / 3, day.g / 3, day.b / 3)
    }

    /// Dusk palette (intermediate).
    pub fn to_color_dusk(self) -> Color {
        let day = self.to_color();
        Color::rgb(
            (day.r as u16 * 2 / 3) as u8,
            (day.g as u16 * 2 / 3) as u8,
            (day.b as u16 * 2 / 3) as u8,
        )
    }
}

/// Color palette mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaletteMode {
    Day,
    Dusk,
    Night,
}

/// Depth zone classification per S-52.
#[derive(Debug, Clone, Copy)]
pub struct DepthZones {
    pub safety_contour: f64,
    pub shallow_contour: f64,
    pub deep_contour: f64,
}

impl Default for DepthZones {
    fn default() -> Self {
        Self {
            safety_contour: 30.0,
            shallow_contour: 5.0,
            deep_contour: 30.0,
        }
    }
}

impl DepthZones {
    /// Get the S-52 color for a depth value.
    pub fn color_for_depth(&self, depth: f64) -> S52Color {
        if depth < 0.0 {
            S52Color::DEPIT // drying/intertidal
        } else if depth < self.shallow_contour {
            S52Color::DEPVS // very shallow
        } else if depth < self.safety_contour {
            S52Color::DEPMD // medium
        } else {
            S52Color::DEPDW // deep
        }
    }
}

/// S-52 chart rendering parameters.
#[derive(Debug, Clone)]
pub struct ChartParams {
    pub palette: PaletteMode,
    pub depth_zones: DepthZones,
    pub safety_depth: f64,
    pub show_soundings: bool,
    pub display_category: DisplayCategory,
    pub symbol_scale: f64,
}

impl Default for ChartParams {
    fn default() -> Self {
        Self {
            palette: PaletteMode::Day,
            depth_zones: DepthZones::default(),
            safety_depth: 30.0,
            show_soundings: true,
            display_category: DisplayCategory::Standard,
            symbol_scale: 1.0,
        }
    }
}

/// Render a depth area (colored by depth zone).
pub fn render_depth_area(
    buffer: &mut PixelBuffer,
    polygon: &[Point],
    bbox: &BBox,
    depth: f64,
    params: &ChartParams,
) {
    let s52_color = params.depth_zones.color_for_depth(depth);
    let color = match params.palette {
        PaletteMode::Day => s52_color.to_color(),
        PaletteMode::Dusk => s52_color.to_color_dusk(),
        PaletteMode::Night => s52_color.to_color_night(),
    };

    // Use scanline fill
    let w = buffer.width;
    let h = buffer.height;
    let screen: Vec<(f64, f64)> = polygon
        .iter()
        .map(|p| {
            let x = (p.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * w as f64;
            let y = (bbox.max_y - p.y) / (bbox.max_y - bbox.min_y) * h as f64;
            (x, y)
        })
        .collect();

    scanline_fill(buffer, &screen, color);
}

/// Render a sounding (depth value as text).
pub fn render_sounding(
    buffer: &mut PixelBuffer,
    position: &Point,
    depth: f64,
    bbox: &BBox,
    params: &ChartParams,
) {
    if !params.show_soundings {
        return;
    }

    let w = buffer.width;
    let h = buffer.height;
    let px = (position.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * w as f64;
    let py = (bbox.max_y - position.y) / (bbox.max_y - bbox.min_y) * h as f64;

    // Color: depths below safety in danger color
    let color = if depth < params.safety_depth {
        S52Color::DNGHL.to_color()
    } else {
        Color::rgb(60, 60, 60) // normal sounding color
    };

    // Simple dot at sounding location
    let r = (2.0 * params.symbol_scale) as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                let x = px as i32 + dx;
                let y = py as i32 + dy;
                if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                    let idx = ((y as u32 * w + x as u32) * 4) as usize;
                    buffer.data[idx] = color.r;
                    buffer.data[idx + 1] = color.g;
                    buffer.data[idx + 2] = color.b;
                    buffer.data[idx + 3] = 255;
                }
            }
        }
    }
}

/// Render a buoy symbol.
pub fn render_buoy(buffer: &mut PixelBuffer, position: &Point, bbox: &BBox, params: &ChartParams) {
    let w = buffer.width;
    let h = buffer.height;
    let px = (position.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * w as f64;
    let py = (bbox.max_y - position.y) / (bbox.max_y - bbox.min_y) * h as f64;

    let color = match params.palette {
        PaletteMode::Day => S52Color::CHMGD.to_color(),
        PaletteMode::Dusk => S52Color::CHMGD.to_color_dusk(),
        PaletteMode::Night => S52Color::CHMGD.to_color_night(),
    };

    // Diamond shape for buoy
    let r = (4.0 * params.symbol_scale) as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx.unsigned_abs() as i32 + dy.unsigned_abs() as i32 <= r {
                let x = px as i32 + dx;
                let y = py as i32 + dy;
                if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                    let idx = ((y as u32 * w + x as u32) * 4) as usize;
                    buffer.data[idx] = color.r;
                    buffer.data[idx + 1] = color.g;
                    buffer.data[idx + 2] = color.b;
                    buffer.data[idx + 3] = 255;
                }
            }
        }
    }
}

fn scanline_fill(buffer: &mut PixelBuffer, vertices: &[(f64, f64)], color: Color) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_zone_classification() {
        let zones = DepthZones::default();
        assert_eq!(zones.color_for_depth(-1.0), S52Color::DEPIT);
        assert_eq!(zones.color_for_depth(3.0), S52Color::DEPVS);
        assert_eq!(zones.color_for_depth(15.0), S52Color::DEPMD);
        assert_eq!(zones.color_for_depth(50.0), S52Color::DEPDW);
    }

    #[test]
    fn palette_modes() {
        let c = S52Color::DEPDW;
        let day = c.to_color();
        let night = c.to_color_night();
        let dusk = c.to_color_dusk();
        // Night should be darker
        assert!(night.r < day.r);
        assert!(night.g < day.g);
        // Dusk intermediate
        assert!(dusk.r < day.r);
        assert!(dusk.r > night.r);
    }

    #[test]
    fn render_depth_area_fills() {
        let mut buffer = PixelBuffer::new(64, 64);
        let polygon = vec![
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
        render_depth_area(&mut buffer, &polygon, &bbox, 15.0, &ChartParams::default());
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 100);
    }

    #[test]
    fn render_sounding_dot() {
        let mut buffer = PixelBuffer::new(64, 64);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        render_sounding(
            &mut buffer,
            &Point { x: 0.5, y: 0.5 },
            10.0,
            &bbox,
            &ChartParams::default(),
        );
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 0);
    }

    #[test]
    fn render_buoy_symbol() {
        let mut buffer = PixelBuffer::new(64, 64);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        render_buoy(
            &mut buffer,
            &Point { x: 0.5, y: 0.5 },
            &bbox,
            &ChartParams::default(),
        );
        let filled = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 0);
    }

    #[test]
    fn dangerous_sounding_red() {
        let mut buffer = PixelBuffer::new(64, 64);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        render_sounding(
            &mut buffer,
            &Point { x: 0.5, y: 0.5 },
            2.0, // below safety depth
            &bbox,
            &ChartParams::default(),
        );
        // Should have red pixels
        let center_idx = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(buffer.data[center_idx], 255); // R = danger color
    }

    #[test]
    fn display_category_default() {
        let params = ChartParams::default();
        assert_eq!(params.display_category, DisplayCategory::Standard);
        assert_eq!(params.palette, PaletteMode::Day);
    }
}
