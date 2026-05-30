//! Print layout composer — build multi-element map layouts for PDF/SVG export.
//!
//! Supports map frames, legends, scale bars, north arrows, text labels,
//! images, and tables in a page layout for cartographic output.

use serde::{Deserialize, Serialize};

/// A complete print layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub name: String,
    pub page: PageSize,
    pub orientation: Orientation,
    pub elements: Vec<LayoutElement>,
    pub margin: Margin,
}

/// Page size presets or custom dimensions (in mm).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PageSize {
    A4,
    A3,
    A2,
    A1,
    A0,
    Letter,
    Tabloid,
    Custom { width_mm: f64, height_mm: f64 },
}

impl PageSize {
    pub fn dimensions_mm(self) -> (f64, f64) {
        match self {
            Self::A4 => (210.0, 297.0),
            Self::A3 => (297.0, 420.0),
            Self::A2 => (420.0, 594.0),
            Self::A1 => (594.0, 841.0),
            Self::A0 => (841.0, 1189.0),
            Self::Letter => (215.9, 279.4),
            Self::Tabloid => (279.4, 431.8),
            Self::Custom {
                width_mm,
                height_mm,
            } => (width_mm, height_mm),
        }
    }

    pub fn dimensions_px(self, dpi: f64) -> (f64, f64) {
        let (w, h) = self.dimensions_mm();
        (w * dpi / 25.4, h * dpi / 25.4)
    }
}

/// Page orientation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Page margins in mm.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Margin {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

impl Default for Margin {
    fn default() -> Self {
        Self {
            top: 10.0,
            bottom: 10.0,
            left: 10.0,
            right: 10.0,
        }
    }
}

/// A positioned element in the layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutElement {
    pub id: String,
    pub rect: Rect,
    pub rotation: f64,
    pub locked: bool,
    pub visible: bool,
    pub content: ElementContent,
}

/// Position and size in mm from page origin (top-left).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Content types for layout elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElementContent {
    MapFrame(MapFrame),
    Legend(Legend),
    ScaleBar(ScaleBar),
    NorthArrow(NorthArrow),
    TextLabel(TextLabel),
    Image(ImageElement),
    Table(TableElement),
    Shape(ShapeElement),
}

/// Map frame — renders a map view at a specific scale/extent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapFrame {
    pub layers: Vec<String>,
    pub center: [f64; 2],
    pub scale: f64,
    pub crs: String,
    pub grid: Option<MapGrid>,
    pub border: Option<Border>,
}

/// Map grid/graticule overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapGrid {
    pub interval_x: f64,
    pub interval_y: f64,
    pub line_color: String,
    pub line_width: f64,
    pub show_labels: bool,
    pub label_format: GridLabelFormat,
}

/// Grid label format.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GridLabelFormat {
    Decimal,
    DegMinSec,
    Mgrs,
}

/// Border style for elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Border {
    pub color: String,
    pub width: f64,
    pub style: LineStyle,
}

/// Line dash style.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    DashDot,
}

/// Legend element — auto-generated from map layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Legend {
    pub title: Option<String>,
    pub columns: u32,
    pub symbol_width: f64,
    pub symbol_height: f64,
    pub font_size: f64,
    pub background: Option<String>,
    pub border: Option<Border>,
}

/// Scale bar element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleBar {
    pub style: ScaleBarStyle,
    pub units: ScaleBarUnits,
    pub segments: u32,
    pub height_mm: f64,
    pub font_size: f64,
    pub color: String,
    pub linked_map: Option<String>,
}

/// Scale bar visual style.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScaleBarStyle {
    SingleBox,
    DoubleBox,
    Line,
    LineTicks,
    Numeric,
}

/// Scale bar distance units.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScaleBarUnits {
    Meters,
    Kilometers,
    Miles,
    NauticalMiles,
    Feet,
}

/// North arrow element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NorthArrow {
    pub style: NorthArrowStyle,
    pub size_mm: f64,
    pub color: String,
    pub linked_map: Option<String>,
}

/// North arrow visual style.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NorthArrowStyle {
    Simple,
    Compass,
    FancyCompass,
    Arrow,
}

/// Text label with formatting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextLabel {
    pub text: String,
    pub font_family: String,
    pub font_size: f64,
    pub color: String,
    pub bold: bool,
    pub italic: bool,
    pub alignment: TextAlignment,
    pub background: Option<String>,
}

/// Text alignment.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

/// Image element (logo, photo, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageElement {
    pub source: ImageSource,
    pub maintain_aspect_ratio: bool,
}

/// Image source — file path or embedded data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSource {
    File(String),
    Url(String),
    Base64 { data: String, media_type: String },
}

/// Table element for attribute data display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableElement {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub font_size: f64,
    pub header_background: String,
    pub stripe_color: Option<String>,
    pub border: Option<Border>,
}

/// Geometric shapes (frames, decorations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapeElement {
    pub shape_type: ShapeType,
    pub fill: Option<String>,
    pub stroke: Option<Border>,
}

/// Basic shape types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShapeType {
    Rectangle,
    Ellipse,
    Triangle,
}

impl Layout {
    /// Create a new empty layout.
    pub fn new(name: &str, page: PageSize, orientation: Orientation) -> Self {
        Self {
            name: name.to_string(),
            page,
            orientation,
            elements: Vec::new(),
            margin: Margin::default(),
        }
    }

    /// Get effective page dimensions considering orientation.
    pub fn page_dimensions_mm(&self) -> (f64, f64) {
        let (w, h) = self.page.dimensions_mm();
        match self.orientation {
            Orientation::Portrait => (w, h),
            Orientation::Landscape => (h, w),
        }
    }

    /// Add a map frame to the layout.
    pub fn add_map_frame(&mut self, id: &str, rect: Rect, frame: MapFrame) {
        self.elements.push(LayoutElement {
            id: id.to_string(),
            rect,
            rotation: 0.0,
            locked: false,
            visible: true,
            content: ElementContent::MapFrame(frame),
        });
    }

    /// Add a legend linked to a map frame.
    pub fn add_legend(&mut self, id: &str, rect: Rect, legend: Legend) {
        self.elements.push(LayoutElement {
            id: id.to_string(),
            rect,
            rotation: 0.0,
            locked: false,
            visible: true,
            content: ElementContent::Legend(legend),
        });
    }

    /// Add a scale bar.
    pub fn add_scale_bar(&mut self, id: &str, rect: Rect, bar: ScaleBar) {
        self.elements.push(LayoutElement {
            id: id.to_string(),
            rect,
            rotation: 0.0,
            locked: false,
            visible: true,
            content: ElementContent::ScaleBar(bar),
        });
    }

    /// Add a text label.
    pub fn add_text(&mut self, id: &str, rect: Rect, label: TextLabel) {
        self.elements.push(LayoutElement {
            id: id.to_string(),
            rect,
            rotation: 0.0,
            locked: false,
            visible: true,
            content: ElementContent::TextLabel(label),
        });
    }

    /// Calculate the SVG viewBox string for this layout.
    pub fn svg_viewbox(&self, dpi: f64) -> String {
        let (w, h) = self.page_dimensions_mm();
        let px_w = w * dpi / 25.4;
        let px_h = h * dpi / 25.4;
        format!("0 0 {px_w:.0} {px_h:.0}")
    }

    /// Find element by ID.
    pub fn get_element(&self, id: &str) -> Option<&LayoutElement> {
        self.elements.iter().find(|e| e.id == id)
    }

    /// Find element by ID (mutable).
    pub fn get_element_mut(&mut self, id: &str) -> Option<&mut LayoutElement> {
        self.elements.iter_mut().find(|e| e.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_dimensions() {
        assert_eq!(PageSize::A4.dimensions_mm(), (210.0, 297.0));
        assert_eq!(PageSize::A3.dimensions_mm(), (297.0, 420.0));
    }

    #[test]
    fn test_landscape_orientation() {
        let layout = Layout::new("test", PageSize::A4, Orientation::Landscape);
        let (w, h) = layout.page_dimensions_mm();
        assert_eq!(w, 297.0);
        assert_eq!(h, 210.0);
    }

    #[test]
    fn test_add_elements() {
        let mut layout = Layout::new("map1", PageSize::A3, Orientation::Landscape);
        layout.add_map_frame(
            "main_map",
            Rect {
                x: 10.0,
                y: 10.0,
                width: 280.0,
                height: 180.0,
            },
            MapFrame {
                layers: vec!["roads".to_string(), "buildings".to_string()],
                center: [151.2, -33.9],
                scale: 25_000.0,
                crs: "EPSG:3857".to_string(),
                grid: None,
                border: None,
            },
        );
        layout.add_text(
            "title",
            Rect {
                x: 10.0,
                y: 200.0,
                width: 280.0,
                height: 15.0,
            },
            TextLabel {
                text: "Site Plan".to_string(),
                font_family: "Arial".to_string(),
                font_size: 24.0,
                color: "#000000".to_string(),
                bold: true,
                italic: false,
                alignment: TextAlignment::Center,
                background: None,
            },
        );
        assert_eq!(layout.elements.len(), 2);
        assert!(layout.get_element("main_map").is_some());
    }

    #[test]
    fn test_svg_viewbox() {
        let layout = Layout::new("test", PageSize::A4, Orientation::Portrait);
        let vb = layout.svg_viewbox(300.0);
        // 210mm * 300/25.4 ≈ 2480, 297mm * 300/25.4 ≈ 3508
        assert!(vb.contains("2480"));
        assert!(vb.contains("3508"));
    }
}
