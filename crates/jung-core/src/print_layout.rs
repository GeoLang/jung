//! Page layout engine for print-quality cartographic output.
//!
//! Composes map frames, titles, legends, scale bars, north arrows,
//! and attribution text into a page layout. Outputs to SVG for
//! print-ready PDF generation (via external tools like Inkscape/rsvg).

use crate::output::{PrintParams, SvgDocument, SvgElement};
use crate::renderer::BBox;

/// A complete page layout specification.
#[derive(Debug, Clone)]
pub struct PageLayout {
    /// Print parameters (paper size, DPI, margins).
    pub params: PrintParams,
    /// Map frame definition.
    pub map_frame: MapFrame,
    /// Optional title block.
    pub title_block: Option<TitleBlock>,
    /// Optional legend.
    pub legend: Option<Legend>,
    /// Optional attribution/source text.
    pub attribution: Option<String>,
    /// Whether to include a graticule (grid lines).
    pub graticule: bool,
    /// Border style.
    pub border: BorderStyle,
}

/// Map frame within the page layout.
#[derive(Debug, Clone)]
pub struct MapFrame {
    /// Position and size as fraction of printable area [0..1].
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Geographic extent to display.
    pub bbox: BBox,
}

/// Title block with main title and optional subtitle.
#[derive(Debug, Clone)]
pub struct TitleBlock {
    pub title: String,
    pub subtitle: Option<String>,
    /// Position: "top", "bottom".
    pub position: String,
    pub font_size: f64,
}

/// Map legend with symbol entries.
#[derive(Debug, Clone)]
pub struct Legend {
    pub title: Option<String>,
    pub entries: Vec<LegendEntry>,
    /// Position: "bottom-right", "bottom-left", "top-right", "top-left".
    pub position: String,
}

/// A single legend entry.
#[derive(Debug, Clone)]
pub struct LegendEntry {
    pub label: String,
    pub symbol: LegendSymbol,
}

/// Legend symbol types.
#[derive(Debug, Clone)]
pub enum LegendSymbol {
    ColorBox { fill: String, stroke: String },
    Line { color: String, width: f64 },
    Circle { fill: String, radius: f64 },
}

/// Page border style.
#[derive(Debug, Clone)]
pub enum BorderStyle {
    None,
    Simple { width: f64 },
    Neatline { width: f64, gap: f64 },
}

impl Default for PageLayout {
    fn default() -> Self {
        Self {
            params: PrintParams::default(),
            map_frame: MapFrame {
                x: 0.0,
                y: 0.1,
                width: 1.0,
                height: 0.8,
                bbox: BBox {
                    min_x: -180.0,
                    min_y: -90.0,
                    max_x: 180.0,
                    max_y: 90.0,
                },
            },
            title_block: None,
            legend: None,
            attribution: None,
            graticule: false,
            border: BorderStyle::Simple { width: 1.0 },
        }
    }
}

impl PageLayout {
    /// Render the layout to an SVG document.
    pub fn to_svg(&self) -> SvgDocument {
        let (page_w, page_h) = self.params.pixel_dimensions();
        let pw = page_w as f64;
        let ph = page_h as f64;
        let margin = self.params.margin_mm / 25.4 * self.params.dpi;

        let mut doc = SvgDocument::new(pw, ph);

        // White background
        doc.elements.push(SvgElement::Rect {
            x: 0.0,
            y: 0.0,
            width: pw,
            height: ph,
            fill: "white".to_string(),
            stroke: "none".to_string(),
            stroke_width: 0.0,
        });

        // Printable area
        let print_x = margin;
        let print_y = margin;
        let print_w = pw - 2.0 * margin;
        let print_h = ph - 2.0 * margin;

        // Map frame
        let map_x = print_x + self.map_frame.x * print_w;
        let map_y = print_y + self.map_frame.y * print_h;
        let map_w = self.map_frame.width * print_w;
        let map_h = self.map_frame.height * print_h;

        // Map frame border
        doc.elements.push(SvgElement::Rect {
            x: map_x,
            y: map_y,
            width: map_w,
            height: map_h,
            fill: "#f0f0f0".to_string(),
            stroke: "black".to_string(),
            stroke_width: 0.5,
        });

        // Map frame label (placeholder for rendered map content)
        let cx = map_x + map_w / 2.0;
        let cy = map_y + map_h / 2.0;
        doc.elements.push(SvgElement::Text {
            x: cx,
            y: cy,
            content: format!(
                "[Map: {:.1}°, {:.1}° to {:.1}°, {:.1}°]",
                self.map_frame.bbox.min_x,
                self.map_frame.bbox.min_y,
                self.map_frame.bbox.max_x,
                self.map_frame.bbox.max_y
            ),
            font_size: 12.0,
            fill: "#999".to_string(),
            anchor: "middle".to_string(),
        });

        // Graticule
        if self.graticule {
            render_graticule(&mut doc, map_x, map_y, map_w, map_h, &self.map_frame.bbox);
        }

        // Title block
        if let Some(title) = &self.title_block {
            render_title(&mut doc, title, print_x, print_y, print_w, &self.params);
        }

        // Scale bar
        if self.params.scale_bar {
            let sb_x = map_x + 20.0;
            let sb_y = map_y + map_h - 30.0;
            let sb_len = map_w * 0.25;
            render_svg_scale_bar(&mut doc, sb_x, sb_y, sb_len, &self.map_frame.bbox, map_w);
        }

        // North arrow
        if self.params.north_arrow {
            let na_x = map_x + map_w - 40.0;
            let na_y = map_y + 40.0;
            render_svg_north_arrow(&mut doc, na_x, na_y, 30.0);
        }

        // Legend
        if let Some(legend) = &self.legend {
            render_legend(&mut doc, legend, map_x, map_y, map_w, map_h);
        }

        // Attribution
        if let Some(attr) = &self.attribution {
            doc.elements.push(SvgElement::Text {
                x: print_x + print_w,
                y: print_y + print_h - 5.0,
                content: attr.clone(),
                font_size: 8.0,
                fill: "#666".to_string(),
                anchor: "end".to_string(),
            });
        }

        // Page border
        match &self.border {
            BorderStyle::None => {}
            BorderStyle::Simple { width } => {
                doc.elements.push(SvgElement::Rect {
                    x: margin,
                    y: margin,
                    width: print_w,
                    height: print_h,
                    fill: "none".to_string(),
                    stroke: "black".to_string(),
                    stroke_width: *width,
                });
            }
            BorderStyle::Neatline { width, gap } => {
                doc.elements.push(SvgElement::Rect {
                    x: margin,
                    y: margin,
                    width: print_w,
                    height: print_h,
                    fill: "none".to_string(),
                    stroke: "black".to_string(),
                    stroke_width: *width,
                });
                doc.elements.push(SvgElement::Rect {
                    x: margin - gap,
                    y: margin - gap,
                    width: print_w + 2.0 * gap,
                    height: print_h + 2.0 * gap,
                    fill: "none".to_string(),
                    stroke: "black".to_string(),
                    stroke_width: width * 0.5,
                });
            }
        }

        doc
    }
}

fn render_title(
    doc: &mut SvgDocument,
    title: &TitleBlock,
    print_x: f64,
    print_y: f64,
    print_w: f64,
    params: &PrintParams,
) {
    let scale = params.dpi / 96.0;
    let cx = print_x + print_w / 2.0;

    let y = if title.position == "bottom" {
        let margin = params.margin_mm / 25.4 * params.dpi;
        let (_, ph) = params.pixel_dimensions();
        ph as f64 - margin - 10.0
    } else {
        print_y + title.font_size * scale + 5.0
    };

    doc.elements.push(SvgElement::Text {
        x: cx,
        y,
        content: title.title.clone(),
        font_size: title.font_size * scale,
        fill: "black".to_string(),
        anchor: "middle".to_string(),
    });

    if let Some(sub) = &title.subtitle {
        doc.elements.push(SvgElement::Text {
            x: cx,
            y: y + title.font_size * scale * 0.8,
            content: sub.clone(),
            font_size: title.font_size * scale * 0.6,
            fill: "#444".to_string(),
            anchor: "middle".to_string(),
        });
    }
}

fn render_legend(
    doc: &mut SvgDocument,
    legend: &Legend,
    map_x: f64,
    map_y: f64,
    map_w: f64,
    map_h: f64,
) {
    let entry_height = 18.0;
    let padding = 10.0;
    let symbol_size = 12.0;
    let total_h = padding * 2.0
        + legend.entries.len() as f64 * entry_height
        + if legend.title.is_some() {
            entry_height
        } else {
            0.0
        };
    let total_w = 140.0;

    let (lx, ly) = match legend.position.as_str() {
        "top-left" => (map_x + 10.0, map_y + 10.0),
        "top-right" => (map_x + map_w - total_w - 10.0, map_y + 10.0),
        "bottom-left" => (map_x + 10.0, map_y + map_h - total_h - 10.0),
        _ => (
            map_x + map_w - total_w - 10.0,
            map_y + map_h - total_h - 10.0,
        ),
    };

    // Legend background
    doc.elements.push(SvgElement::Rect {
        x: lx,
        y: ly,
        width: total_w,
        height: total_h,
        fill: "white".to_string(),
        stroke: "black".to_string(),
        stroke_width: 0.5,
    });

    let mut cy = ly + padding;

    // Legend title
    if let Some(title) = &legend.title {
        doc.elements.push(SvgElement::Text {
            x: lx + padding,
            y: cy + 12.0,
            content: title.clone(),
            font_size: 11.0,
            fill: "black".to_string(),
            anchor: "start".to_string(),
        });
        cy += entry_height;
    }

    // Legend entries
    for entry in &legend.entries {
        match &entry.symbol {
            LegendSymbol::ColorBox { fill, stroke } => {
                doc.elements.push(SvgElement::Rect {
                    x: lx + padding,
                    y: cy + 2.0,
                    width: symbol_size,
                    height: symbol_size,
                    fill: fill.clone(),
                    stroke: stroke.clone(),
                    stroke_width: 0.5,
                });
            }
            LegendSymbol::Line { color, width } => {
                let d = format!(
                    "M {:.1} {:.1} L {:.1} {:.1}",
                    lx + padding,
                    cy + symbol_size / 2.0 + 2.0,
                    lx + padding + symbol_size,
                    cy + symbol_size / 2.0 + 2.0
                );
                doc.elements.push(SvgElement::Path {
                    d,
                    fill: "none".to_string(),
                    stroke: color.clone(),
                    stroke_width: *width,
                });
            }
            LegendSymbol::Circle { fill, radius } => {
                doc.elements.push(SvgElement::Circle {
                    cx: lx + padding + symbol_size / 2.0,
                    cy: cy + symbol_size / 2.0 + 2.0,
                    r: *radius,
                    fill: fill.clone(),
                    stroke: "black".to_string(),
                    stroke_width: 0.5,
                });
            }
        }

        doc.elements.push(SvgElement::Text {
            x: lx + padding + symbol_size + 6.0,
            y: cy + symbol_size,
            content: entry.label.clone(),
            font_size: 10.0,
            fill: "black".to_string(),
            anchor: "start".to_string(),
        });

        cy += entry_height;
    }
}

fn render_graticule(
    doc: &mut SvgDocument,
    map_x: f64,
    map_y: f64,
    map_w: f64,
    map_h: f64,
    bbox: &BBox,
) {
    let range_x = bbox.max_x - bbox.min_x;
    let range_y = bbox.max_y - bbox.min_y;

    let interval = pick_graticule_interval(range_x.max(range_y));

    // Vertical grid lines
    let start_x = (bbox.min_x / interval).ceil() * interval;
    let mut x = start_x;
    while x < bbox.max_x {
        let px = map_x + (x - bbox.min_x) / range_x * map_w;
        let d = format!("M {px:.1} {map_y:.1} L {px:.1} {:.1}", map_y + map_h);
        doc.elements.push(SvgElement::Path {
            d,
            fill: "none".to_string(),
            stroke: "#ccc".to_string(),
            stroke_width: 0.3,
        });
        // Label
        doc.elements.push(SvgElement::Text {
            x: px,
            y: map_y + map_h + 12.0,
            content: format!("{x:.0}°"),
            font_size: 7.0,
            fill: "#666".to_string(),
            anchor: "middle".to_string(),
        });
        x += interval;
    }

    // Horizontal grid lines
    let start_y = (bbox.min_y / interval).ceil() * interval;
    let mut y = start_y;
    while y < bbox.max_y {
        let py = map_y + (bbox.max_y - y) / range_y * map_h;
        let d = format!("M {map_x:.1} {py:.1} L {:.1} {py:.1}", map_x + map_w);
        doc.elements.push(SvgElement::Path {
            d,
            fill: "none".to_string(),
            stroke: "#ccc".to_string(),
            stroke_width: 0.3,
        });
        doc.elements.push(SvgElement::Text {
            x: map_x - 5.0,
            y: py + 3.0,
            content: format!("{y:.0}°"),
            font_size: 7.0,
            fill: "#666".to_string(),
            anchor: "end".to_string(),
        });
        y += interval;
    }
}

fn pick_graticule_interval(range: f64) -> f64 {
    if range > 180.0 {
        30.0
    } else if range > 90.0 {
        15.0
    } else if range > 30.0 {
        10.0
    } else if range > 10.0 {
        5.0
    } else if range > 5.0 {
        1.0
    } else if range > 1.0 {
        0.5
    } else {
        0.1
    }
}

fn render_svg_scale_bar(
    doc: &mut SvgDocument,
    x: f64,
    y: f64,
    bar_len: f64,
    bbox: &BBox,
    map_width_px: f64,
) {
    let map_units = bbox.max_x - bbox.min_x;
    let units_per_px = map_units / map_width_px;
    let bar_units = bar_len * units_per_px;

    // Round to nice number
    let nice = nice_round(bar_units);
    let nice_px = nice / units_per_px;

    // Bar
    doc.elements.push(SvgElement::Rect {
        x,
        y,
        width: nice_px,
        height: 4.0,
        fill: "black".to_string(),
        stroke: "black".to_string(),
        stroke_width: 0.5,
    });

    // Ticks
    for &frac in &[0.0, 0.5, 1.0] {
        let tx = x + nice_px * frac;
        let d = format!("M {tx:.1} {:.1} L {tx:.1} {:.1}", y - 2.0, y + 6.0);
        doc.elements.push(SvgElement::Path {
            d,
            fill: "none".to_string(),
            stroke: "black".to_string(),
            stroke_width: 0.5,
        });
    }

    // Labels
    doc.elements.push(SvgElement::Text {
        x,
        y: y + 14.0,
        content: "0".to_string(),
        font_size: 7.0,
        fill: "black".to_string(),
        anchor: "middle".to_string(),
    });
    doc.elements.push(SvgElement::Text {
        x: x + nice_px,
        y: y + 14.0,
        content: format_distance(nice),
        font_size: 7.0,
        fill: "black".to_string(),
        anchor: "middle".to_string(),
    });
}

fn nice_round(v: f64) -> f64 {
    let pow = 10.0f64.powf(v.abs().log10().floor());
    let norm = v / pow;
    let nice = if norm < 1.5 {
        1.0
    } else if norm < 3.5 {
        2.0
    } else if norm < 7.5 {
        5.0
    } else {
        10.0
    };
    nice * pow
}

fn format_distance(units: f64) -> String {
    if units >= 1.0 {
        format!("{:.0}°", units)
    } else {
        format!("{:.1}′", units * 60.0)
    }
}

fn render_svg_north_arrow(doc: &mut SvgDocument, cx: f64, cy: f64, size: f64) {
    let half = size / 2.0;
    // Arrow pointing up
    let top = format!("{cx:.1},{:.1}", cy - half);
    let bl = format!("{:.1},{:.1}", cx - half * 0.4, cy + half * 0.3);
    let br = format!("{:.1},{:.1}", cx + half * 0.4, cy + half * 0.3);

    doc.elements.push(SvgElement::Polygon {
        points: format!("{top} {bl} {br}"),
        fill: "black".to_string(),
        stroke: "black".to_string(),
        stroke_width: 0.5,
    });

    // "N" label
    doc.elements.push(SvgElement::Text {
        x: cx,
        y: cy + half * 0.3 + 12.0,
        content: "N".to_string(),
        font_size: 10.0,
        fill: "black".to_string(),
        anchor: "middle".to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_produces_svg() {
        let layout = PageLayout::default();
        let svg = layout.to_svg();
        let output = svg.to_svg();
        assert!(output.contains("<svg"));
        assert!(output.contains("</svg>"));
    }

    #[test]
    fn layout_with_title() {
        let layout = PageLayout {
            title_block: Some(TitleBlock {
                title: "Test Map".to_string(),
                subtitle: Some("A subtitle".to_string()),
                position: "top".to_string(),
                font_size: 18.0,
            }),
            ..Default::default()
        };
        let svg = layout.to_svg();
        let output = svg.to_svg();
        assert!(output.contains("Test Map"));
        assert!(output.contains("A subtitle"));
    }

    #[test]
    fn layout_with_legend() {
        let layout = PageLayout {
            legend: Some(Legend {
                title: Some("Legend".to_string()),
                entries: vec![
                    LegendEntry {
                        label: "Forest".to_string(),
                        symbol: LegendSymbol::ColorBox {
                            fill: "#228B22".to_string(),
                            stroke: "black".to_string(),
                        },
                    },
                    LegendEntry {
                        label: "River".to_string(),
                        symbol: LegendSymbol::Line {
                            color: "#4169E1".to_string(),
                            width: 2.0,
                        },
                    },
                    LegendEntry {
                        label: "City".to_string(),
                        symbol: LegendSymbol::Circle {
                            fill: "red".to_string(),
                            radius: 5.0,
                        },
                    },
                ],
                position: "bottom-right".to_string(),
            }),
            ..Default::default()
        };
        let svg = layout.to_svg();
        let output = svg.to_svg();
        assert!(output.contains("Forest"));
        assert!(output.contains("River"));
        assert!(output.contains("City"));
        assert!(output.contains("<circle"));
    }

    #[test]
    fn layout_with_graticule() {
        let layout = PageLayout {
            graticule: true,
            ..Default::default()
        };
        let svg = layout.to_svg();
        let output = svg.to_svg();
        // Should have grid lines
        assert!(output.contains("°"));
    }

    #[test]
    fn layout_with_attribution() {
        let layout = PageLayout {
            attribution: Some("© OpenStreetMap contributors".to_string()),
            ..Default::default()
        };
        let svg = layout.to_svg();
        let output = svg.to_svg();
        assert!(output.contains("OpenStreetMap"));
    }

    #[test]
    fn neatline_border() {
        let layout = PageLayout {
            border: BorderStyle::Neatline {
                width: 2.0,
                gap: 3.0,
            },
            ..Default::default()
        };
        let svg = layout.to_svg();
        let output = svg.to_svg();
        // Should have at least 2 rect elements for the neatline
        let rect_count = output.matches("<rect").count();
        assert!(rect_count >= 3); // background + map frame + 2 neatline rects
    }

    #[test]
    fn nice_round_values() {
        assert!((nice_round(1.0) - 1.0).abs() < 1e-10);
        assert!((nice_round(3.0) - 2.0).abs() < 1e-10);
        assert!((nice_round(7.0) - 5.0).abs() < 1e-10);
        assert!((nice_round(15.0) - 20.0).abs() < 1e-10);
        assert!((nice_round(0.3) - 0.2).abs() < 1e-10);
    }

    #[test]
    fn graticule_interval_selection() {
        assert!((pick_graticule_interval(360.0) - 30.0).abs() < 1e-10);
        assert!((pick_graticule_interval(20.0) - 5.0).abs() < 1e-10);
        assert!((pick_graticule_interval(3.0) - 0.5).abs() < 1e-10);
    }
}
