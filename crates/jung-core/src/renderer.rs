use crate::curved_label::{CurvedLabelParams, PlacedChar, place_curved_label, to_screen_coords};
use crate::geometry::{Feature, Geometry, Point};
use crate::label_priority::{LabelCandidate, LabelPriority, PriorityLabelEngine};
use crate::line::{LineParams, render_line};
use crate::marker::{IconPlacement, SpriteAtlas, blit_icon_placed};
use crate::polygon::render_polygon;
use crate::text::{FontFace, FontSet};
use jung_style::{Color, EvalContext, Layer, PropertyValue, Style, StyleValue};
use std::collections::HashMap;
use thiserror::Error;

/// Mapbox's default `text-size`, used when a text layer names none.
pub const DEFAULT_TEXT_SIZE_PX: f32 = 16.0;

/// Mapbox's default `text-color`, used when a text layer names none.
pub const DEFAULT_TEXT_COLOR: Color = Color::rgb(0, 0, 0);

/// Type size is the only importance signal a Mapbox style carries, so bigger
/// text outranks smaller text when two labels want the same pixels.
const TEXT_SIZE_PRIORITIES: [(f64, LabelPriority); 4] = [
    (24.0, LabelPriority::Critical),
    (18.0, LabelPriority::High),
    (14.0, LabelPriority::Medium),
    (10.0, LabelPriority::Low),
];

/// How far deconfliction may move a label before a run fitted to a line counts
/// as displaced.
const MOVED_LABEL_TOLERANCE_PX: f64 = 0.5;

/// Errors that can occur during rendering.
#[derive(Debug, Error)]
pub enum RenderError {
    #[error("canvas dimensions must be positive: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("no layers to render")]
    NoLayers,
}

/// Bounding box in map coordinates.
#[derive(Debug, Clone, Copy)]
pub struct BBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

/// An RGBA pixel buffer (row-major, 4 bytes per pixel).
#[derive(Debug, Clone)]
pub struct PixelBuffer {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl PixelBuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width * height * 4) as usize],
        }
    }

    fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.data[idx] = color.r;
        self.data[idx + 1] = color.g;
        self.data[idx + 2] = color.b;
        self.data[idx + 3] = color.a;
    }
}

/// Text a render placed, in canvas pixels.
#[derive(Debug, Clone)]
pub struct LabelPlacement {
    pub text: String,
    /// Family the layer's `text-font` named, if any.
    pub font_family: Option<String>,
    pub size: f64,
    pub color: Color,
    pub geometry: LabelGeometry,
}

/// Where a placed label sits.
#[derive(Debug, Clone)]
pub enum LabelGeometry {
    /// Baseline origin of straight text.
    Baseline { x: f64, y: f64 },
    /// One entry per character, following a line feature.
    AlongLine(Vec<PlacedChar>),
}

/// The main renderer. Takes a style and features, produces pixel output.
pub struct Renderer {
    pub width: u32,
    pub height: u32,
    fonts: FontSet,
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions { width, height });
        }
        Ok(Self {
            width,
            height,
            fonts: FontSet::new(),
        })
    }

    /// Give the renderer the faces label text is rasterized with. jung embeds no
    /// font, so without this call a style's text layers draw nothing.
    pub fn with_fonts(mut self, fonts: FontSet) -> Self {
        self.fonts = fonts;
        self
    }

    /// Ids of the style's text layers this renderer has no font for, so a caller
    /// can report labels it is about to skip.
    pub fn text_layers_without_font<'a>(&self, style: &'a Style) -> Vec<&'a str> {
        style
            .layers
            .iter()
            .filter(|layer| {
                layer.text_field.is_some() && self.fonts.get(layer.font_family.as_deref()).is_none()
            })
            .map(|layer| layer.id.as_str())
            .collect()
    }

    /// Render features according to the given style within the bounding box.
    ///
    /// Text layers draw only when the renderer has a font, see `with_fonts`, and
    /// labels land over all geometry whatever order their layer sits in.
    pub fn render(
        &self,
        style: &Style,
        features: &[Feature],
        bbox: &BBox,
    ) -> Result<PixelBuffer, RenderError> {
        self.render_at_zoom(style, features, bbox, 0.0)
    }

    /// Render features with a sprite atlas for icon rendering.
    pub fn render_with_sprites(
        &self,
        style: &Style,
        features: &[Feature],
        bbox: &BBox,
        zoom: f64,
        sprites: &SpriteAtlas,
    ) -> Result<PixelBuffer, RenderError> {
        if style.layers.is_empty() {
            return Err(RenderError::NoLayers);
        }

        let mut buffer = PixelBuffer::new(self.width, self.height);

        for layer in &style.layers {
            for feature in features {
                self.render_feature_impl(&mut buffer, layer, feature, bbox, zoom, Some(sprites));
            }
        }
        self.draw_labels(&mut buffer, &self.place_labels(style, features, bbox, zoom));

        Ok(buffer)
    }

    /// Render features at a specific zoom level (enables zoom-dependent expressions).
    pub fn render_at_zoom(
        &self,
        style: &Style,
        features: &[Feature],
        bbox: &BBox,
        zoom: f64,
    ) -> Result<PixelBuffer, RenderError> {
        if style.layers.is_empty() {
            return Err(RenderError::NoLayers);
        }

        let mut buffer = PixelBuffer::new(self.width, self.height);

        for layer in &style.layers {
            for feature in features {
                self.render_feature_impl(&mut buffer, layer, feature, bbox, zoom, None);
            }
        }
        self.draw_labels(&mut buffer, &self.place_labels(style, features, bbox, zoom));

        Ok(buffer)
    }

    /// Place every label the style's text layers ask for, deconflicted by
    /// priority. Layers whose font is missing contribute nothing.
    pub fn place_labels(
        &self,
        style: &Style,
        features: &[Feature],
        bbox: &BBox,
        zoom: f64,
    ) -> Vec<LabelPlacement> {
        let mut collector = LabelCollector::default();

        for layer in &style.layers {
            let Some(text_field) = layer.text_field.as_ref() else {
                continue;
            };
            let Some(font) = self.fonts.get(layer.font_family.as_deref()) else {
                continue;
            };

            for feature in features {
                let ctx = make_eval_context(feature, zoom);
                let Some(template) = text_field.resolve(&ctx) else {
                    continue;
                };
                let text = expand_property_tokens(&template, &feature.properties);
                if text.is_empty() {
                    continue;
                }

                let size =
                    resolve_f32(&layer.font_size, &ctx).unwrap_or(DEFAULT_TEXT_SIZE_PX) as f64;
                if size <= 0.0 {
                    continue;
                }
                let text_style = TextStyle {
                    font,
                    family: layer.font_family.as_deref(),
                    size,
                    color: resolve_color(&layer.text_color, &ctx).unwrap_or(DEFAULT_TEXT_COLOR),
                    priority: priority_for_text_size(size),
                };

                match &feature.geometry {
                    Geometry::Point(pt) => {
                        collector.push_point(self.to_screen(pt, bbox), &text, &text_style);
                    }
                    Geometry::MultiPoint(pts) => {
                        for pt in pts {
                            collector.push_point(self.to_screen(pt, bbox), &text, &text_style);
                        }
                    }
                    Geometry::LineString(pts) => {
                        let screen = to_screen_coords(pts, bbox, self.width, self.height);
                        collector.push_line(&screen, &text, &text_style);
                    }
                    Geometry::MultiLineString(lines) => {
                        for line in lines {
                            let screen = to_screen_coords(line, bbox, self.width, self.height);
                            collector.push_line(&screen, &text, &text_style);
                        }
                    }
                    // labelling a polygon needs an interior point, which nothing here computes
                    Geometry::Polygon { .. } | Geometry::MultiPolygon(_) => {}
                }
            }
        }

        collector.resolve(self.width as f64, self.height as f64)
    }

    fn draw_labels(&self, buffer: &mut PixelBuffer, placements: &[LabelPlacement]) {
        for placement in placements {
            let Some(font) = self.fonts.get(placement.font_family.as_deref()) else {
                continue;
            };
            match &placement.geometry {
                LabelGeometry::Baseline { x, y } => font.render_text(
                    buffer,
                    &placement.text,
                    *x,
                    *y,
                    placement.size,
                    placement.color,
                ),
                LabelGeometry::AlongLine(chars) => {
                    for placed in chars {
                        font.render_placed_char(buffer, placed, placement.size, placement.color);
                    }
                }
            }
        }
    }

    fn to_screen(&self, pt: &Point, bbox: &BBox) -> (f64, f64) {
        (self.map_x(pt.x, bbox), self.map_y(pt.y, bbox))
    }

    fn render_feature_impl(
        &self,
        buffer: &mut PixelBuffer,
        layer: &Layer,
        feature: &Feature,
        bbox: &BBox,
        zoom: f64,
        sprites: Option<&SpriteAtlas>,
    ) {
        let ctx = make_eval_context(feature, zoom);

        match &feature.geometry {
            Geometry::Point(pt) => self.render_point(buffer, layer, pt, bbox, &ctx, sprites),
            Geometry::MultiPoint(pts) => {
                for pt in pts {
                    self.render_point(buffer, layer, pt, bbox, &ctx, sprites);
                }
            }
            Geometry::LineString(pts) => {
                self.render_linestring(buffer, layer, pts, bbox, &ctx);
            }
            Geometry::MultiLineString(lines) => {
                for line in lines {
                    self.render_linestring(buffer, layer, line, bbox, &ctx);
                }
            }
            Geometry::Polygon { exterior, holes } => {
                render_polygon(
                    buffer,
                    exterior,
                    holes,
                    bbox,
                    self.width,
                    self.height,
                    layer,
                    &ctx,
                );
            }
            Geometry::MultiPolygon(polys) => {
                for poly in polys {
                    render_polygon(
                        buffer,
                        &poly.exterior,
                        &poly.holes,
                        bbox,
                        self.width,
                        self.height,
                        layer,
                        &ctx,
                    );
                }
            }
        }
    }

    fn render_linestring(
        &self,
        buffer: &mut PixelBuffer,
        layer: &Layer,
        points: &[Point],
        bbox: &BBox,
        ctx: &EvalContext,
    ) {
        let params = LineParams {
            color: resolve_color(&layer.stroke_color, ctx).unwrap_or(Color::rgb(0, 0, 0)),
            width: resolve_f32(&layer.stroke_width, ctx).unwrap_or(1.0),
            cap: layer.line_cap,
            join: layer.line_join,
            dasharray: layer.line_dasharray.clone(),
            offset: resolve_f32(&layer.line_offset, ctx).unwrap_or(0.0),
            opacity: resolve_f32(&layer.line_opacity, ctx).unwrap_or(1.0),
        };
        render_line(buffer, points, bbox, self.width, self.height, &params);
    }

    fn render_point(
        &self,
        buffer: &mut PixelBuffer,
        layer: &Layer,
        pt: &Point,
        bbox: &BBox,
        ctx: &EvalContext,
        sprites: Option<&SpriteAtlas>,
    ) {
        let px = self.map_x(pt.x, bbox);
        let py = self.map_y(pt.y, bbox);

        // Try icon rendering first
        if let Some(icon) = layer
            .icon_image
            .as_ref()
            .and_then(|sv| sv.resolve(ctx))
            .and_then(|name| sprites.and_then(|atlas| atlas.get(&name)))
        {
            let placement = IconPlacement {
                anchor: layer.icon_anchor,
                offset: layer
                    .icon_offset
                    .map(|[ox, oy]| [ox as f64, oy as f64])
                    .unwrap_or([0.0, 0.0]),
                rotation_deg: resolve_f32(&layer.icon_rotate, ctx).unwrap_or(0.0) as f64,
                scale: resolve_f32(&layer.icon_size, ctx).unwrap_or(1.0) as f64,
            };
            blit_icon_placed(buffer, icon, px, py, &placement);
            return;
        }

        // Fallback: simple circle rasterization
        let radius = resolve_f32(&layer.point_radius, ctx).unwrap_or(4.0) as i32;
        let color = resolve_color(&layer.fill_color, ctx).unwrap_or(Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        });

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius {
                    let x = px as i32 + dx;
                    let y = py as i32 + dy;
                    if x >= 0 && y >= 0 {
                        buffer.set_pixel(x as u32, y as u32, color);
                    }
                }
            }
        }
    }

    fn map_x(&self, x: f64, bbox: &BBox) -> f64 {
        (x - bbox.min_x) / (bbox.max_x - bbox.min_x) * self.width as f64
    }

    fn map_y(&self, y: f64, bbox: &BBox) -> f64 {
        (bbox.max_y - y) / (bbox.max_y - bbox.min_y) * self.height as f64
    }
}

/// The text properties one layer resolved to for one feature.
struct TextStyle<'a> {
    font: &'a FontFace,
    family: Option<&'a str>,
    size: f64,
    color: Color,
    priority: LabelPriority,
}

/// What a candidate needs to draw itself, held until deconfliction says whether
/// it survived.
struct PendingLabel {
    text: String,
    font_family: Option<String>,
    size: f64,
    color: Color,
    shape: PendingShape,
}

enum PendingShape {
    /// Baseline offset from the top of the candidate box.
    Straight { ascent: f64 },
    /// Characters already fitted to a line.
    AlongLine {
        chars: Vec<PlacedChar>,
        requested_x: f64,
        requested_y: f64,
    },
}

/// Gathers the labels a style asks for, then hands them to the priority engine.
#[derive(Default)]
struct LabelCollector {
    candidates: Vec<LabelCandidate>,
    pending: Vec<Option<PendingLabel>>,
}

impl LabelCollector {
    fn push_point(&mut self, screen: (f64, f64), text: &str, style: &TextStyle) {
        let metrics = style.font.measure_text(text, style.size);
        if metrics.width <= 0.0 {
            return;
        }
        let (x, y) = screen;
        self.push(
            LabelCandidate {
                id: self.pending.len(),
                text: text.to_string(),
                x: x - metrics.width / 2.0,
                y: y - metrics.height / 2.0,
                width: metrics.width,
                height: metrics.height,
                priority: style.priority,
                rotation: 0.0,
                anchor_x: x,
                anchor_y: y,
            },
            style,
            PendingShape::Straight {
                ascent: metrics.ascent,
            },
        );
    }

    fn push_line(&mut self, screen_points: &[(f64, f64)], text: &str, style: &TextStyle) {
        let char_widths: Vec<f64> = text
            .chars()
            .map(|ch| style.font.measure_text(&ch.to_string(), style.size).width)
            .collect();
        let params = CurvedLabelParams {
            font_size: style.size,
            ..Default::default()
        };
        let Some(chars) = place_curved_label(screen_points, text, &char_widths, &params) else {
            return;
        };

        // box the fitted run so the collision grid sees all of it
        let half = style.size / 2.0;
        let min_x = chars.iter().map(|c| c.x).fold(f64::INFINITY, f64::min) - half;
        let max_x = chars.iter().map(|c| c.x).fold(f64::NEG_INFINITY, f64::max) + half;
        let min_y = chars.iter().map(|c| c.y).fold(f64::INFINITY, f64::min) - half;
        let max_y = chars.iter().map(|c| c.y).fold(f64::NEG_INFINITY, f64::max) + half;

        self.push(
            LabelCandidate {
                id: self.pending.len(),
                text: text.to_string(),
                x: min_x,
                y: min_y,
                width: max_x - min_x,
                height: max_y - min_y,
                priority: style.priority,
                rotation: chars.first().map_or(0.0, |c| c.angle),
                anchor_x: min_x,
                anchor_y: min_y,
            },
            style,
            PendingShape::AlongLine {
                chars,
                requested_x: min_x,
                requested_y: min_y,
            },
        );
    }

    fn push(&mut self, candidate: LabelCandidate, style: &TextStyle, shape: PendingShape) {
        self.pending.push(Some(PendingLabel {
            text: candidate.text.clone(),
            font_family: style.family.map(str::to_string),
            size: style.size,
            color: style.color,
            shape,
        }));
        self.candidates.push(candidate);
    }

    fn resolve(mut self, canvas_width: f64, canvas_height: f64) -> Vec<LabelPlacement> {
        let placed = PriorityLabelEngine::new(canvas_width, canvas_height).place(&self.candidates);
        let mut placements = Vec::with_capacity(placed.len());

        for label in placed {
            let Some(pending) = self.pending.get_mut(label.id).and_then(Option::take) else {
                continue;
            };
            let geometry = match pending.shape {
                PendingShape::Straight { ascent } => LabelGeometry::Baseline {
                    x: label.x,
                    y: label.y + ascent,
                },
                PendingShape::AlongLine {
                    chars,
                    requested_x,
                    requested_y,
                } => {
                    // a run moved off its line says nothing about the line, so drop it
                    if (label.x - requested_x).abs() > MOVED_LABEL_TOLERANCE_PX
                        || (label.y - requested_y).abs() > MOVED_LABEL_TOLERANCE_PX
                    {
                        continue;
                    }
                    LabelGeometry::AlongLine(chars)
                }
            };
            placements.push(LabelPlacement {
                text: pending.text,
                font_family: pending.font_family,
                size: pending.size,
                color: pending.color,
                geometry,
            });
        }

        placements
    }
}

fn priority_for_text_size(size: f64) -> LabelPriority {
    TEXT_SIZE_PRIORITIES
        .iter()
        .find(|(min_size, _)| size >= *min_size)
        .map_or(LabelPriority::Optional, |(_, priority)| *priority)
}

/// Substitute `{property}` tokens in a `text-field` value. An unknown property
/// leaves nothing behind, as in Mapbox GL.
pub fn expand_property_tokens(
    template: &str,
    properties: &HashMap<String, PropertyValue>,
) -> String {
    if !template.contains('{') {
        return template.to_string();
    }

    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 1..];
        match after_open.find('}') {
            Some(close) => {
                if let Some(value) = properties.get(&after_open[..close]) {
                    out.push_str(&property_to_string(value));
                }
                rest = &after_open[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

fn property_to_string(value: &PropertyValue) -> String {
    match value {
        PropertyValue::String(s) => s.clone(),
        PropertyValue::Number(n) => n.to_string(),
        PropertyValue::Integer(i) => i.to_string(),
        PropertyValue::Boolean(b) => b.to_string(),
        PropertyValue::Null => String::new(),
    }
}

/// Create an EvalContext from a Feature for expression evaluation.
fn make_eval_context<'a>(feature: &'a Feature, zoom: f64) -> EvalContext<'a> {
    let geom_type = match &feature.geometry {
        Geometry::Point(_) | Geometry::MultiPoint(_) => "Point",
        Geometry::LineString(_) | Geometry::MultiLineString(_) => "LineString",
        Geometry::Polygon { .. } | Geometry::MultiPolygon(_) => "Polygon",
    };
    EvalContext {
        properties: &feature.properties,
        zoom,
        geometry_type: geom_type,
    }
}

fn resolve_color(val: &Option<StyleValue<Color>>, ctx: &EvalContext) -> Option<Color> {
    val.as_ref().and_then(|sv| sv.resolve(ctx))
}

fn resolve_f32(val: &Option<StyleValue<f32>>, ctx: &EvalContext) -> Option<f32> {
    val.as_ref().and_then(|sv| sv.resolve(ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jung_style::{IconAnchor, LineCap, LineJoin, StyleValue};
    use std::collections::HashMap;

    fn test_style() -> Style {
        Style {
            name: "test".to_string(),
            layers: vec![Layer {
                id: "points".to_string(),
                source: None,
                fill_color: Some(StyleValue::Literal(Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                })),
                stroke_color: None,
                stroke_width: None,
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                line_dasharray: None,
                line_offset: None,
                line_opacity: None,
                point_radius: Some(StyleValue::Literal(3.0)),
                icon_image: None,
                icon_size: None,
                icon_rotate: None,
                icon_anchor: IconAnchor::Center,
                icon_offset: None,
                font_family: None,
                font_size: None,
                text_field: None,
                text_color: None,
            }],
        }
    }

    #[test]
    fn render_empty_features() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = test_style();
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let result = renderer.render(&style, &[], &bbox).unwrap();
        assert_eq!(result.width, 256);
        assert_eq!(result.height, 256);
        // All pixels should be transparent black
        assert!(result.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn render_single_point() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = test_style();
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let features = vec![Feature {
            geometry: Geometry::Point(Point { x: 0.5, y: 0.5 }),
            properties: HashMap::new(),
        }];
        let result = renderer.render(&style, &features, &bbox).unwrap();
        // Center pixel should be red
        let cx = 128u32;
        let cy = 128u32;
        let idx = ((cy * 256 + cx) * 4) as usize;
        assert_eq!(result.data[idx], 255); // R
        assert_eq!(result.data[idx + 1], 0); // G
        assert_eq!(result.data[idx + 2], 0); // B
        assert_eq!(result.data[idx + 3], 255); // A
    }

    #[test]
    fn invalid_dimensions() {
        assert!(Renderer::new(0, 256).is_err());
        assert!(Renderer::new(256, 0).is_err());
    }

    #[test]
    fn no_layers_error() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = Style {
            name: "empty".to_string(),
            layers: vec![],
        };
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        assert!(renderer.render(&style, &[], &bbox).is_err());
    }

    fn line_style(width: f32) -> Style {
        Style {
            name: "line-test".to_string(),
            layers: vec![Layer {
                id: "lines".to_string(),
                source: None,
                fill_color: None,
                stroke_color: Some(StyleValue::Literal(Color::rgb(0, 255, 0))),
                stroke_width: Some(StyleValue::Literal(width)),
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                line_dasharray: None,
                line_offset: None,
                line_opacity: None,
                point_radius: None,
                icon_image: None,
                icon_size: None,
                icon_rotate: None,
                icon_anchor: IconAnchor::Center,
                icon_offset: None,
                font_family: None,
                font_size: None,
                text_field: None,
                text_color: None,
            }],
        }
    }

    #[test]
    fn render_horizontal_line() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = line_style(3.0);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let features = vec![Feature {
            geometry: Geometry::LineString(vec![
                Point { x: 0.1, y: 0.5 },
                Point { x: 0.9, y: 0.5 },
            ]),
            properties: HashMap::new(),
        }];
        let result = renderer.render(&style, &features, &bbox).unwrap();
        // Center of line should be green
        let cx = 128u32;
        let cy = 128u32;
        let idx = ((cy * 256 + cx) * 4) as usize;
        assert_eq!(result.data[idx], 0); // R
        assert_eq!(result.data[idx + 1], 255); // G
        assert_eq!(result.data[idx + 2], 0); // B
        assert_eq!(result.data[idx + 3], 255); // A
    }

    #[test]
    fn render_diagonal_line() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = line_style(4.0);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let features = vec![Feature {
            geometry: Geometry::LineString(vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 1.0, y: 1.0 },
            ]),
            properties: HashMap::new(),
        }];
        let result = renderer.render(&style, &features, &bbox).unwrap();
        // Middle of the line (center of canvas) should have some green pixels
        let cx = 128u32;
        let cy = 128u32;
        let idx = ((cy * 256 + cx) * 4) as usize;
        assert_eq!(result.data[idx + 1], 255); // G channel
        assert_eq!(result.data[idx + 3], 255); // A channel
    }

    #[test]
    fn render_multilinestring() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = line_style(2.0);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let features = vec![Feature {
            geometry: Geometry::MultiLineString(vec![
                vec![Point { x: 0.1, y: 0.5 }, Point { x: 0.9, y: 0.5 }],
                vec![Point { x: 0.5, y: 0.1 }, Point { x: 0.5, y: 0.9 }],
            ]),
            properties: HashMap::new(),
        }];
        let result = renderer.render(&style, &features, &bbox).unwrap();
        // Intersection at center should be green
        let cx = 128u32;
        let cy = 128u32;
        let idx = ((cy * 256 + cx) * 4) as usize;
        assert_eq!(result.data[idx + 1], 255); // G
    }

    #[test]
    fn line_not_rendered_outside_bbox() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = line_style(2.0);
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        // Line completely outside the bbox
        let features = vec![Feature {
            geometry: Geometry::LineString(vec![
                Point { x: 2.0, y: 2.0 },
                Point { x: 3.0, y: 3.0 },
            ]),
            properties: HashMap::new(),
        }];
        let result = renderer.render(&style, &features, &bbox).unwrap();
        // All pixels should be transparent
        assert!(result.data.iter().all(|&b| b == 0));
    }

    fn polygon_style() -> Style {
        Style {
            name: "poly-test".to_string(),
            layers: vec![Layer {
                id: "polygons".to_string(),
                source: None,
                fill_color: Some(StyleValue::Literal(Color::rgb(0, 0, 255))),
                stroke_color: Some(StyleValue::Literal(Color::rgb(255, 255, 0))),
                stroke_width: Some(StyleValue::Literal(2.0)),
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                line_dasharray: None,
                line_offset: None,
                line_opacity: None,
                point_radius: None,
                icon_image: None,
                icon_size: None,
                icon_rotate: None,
                icon_anchor: IconAnchor::Center,
                icon_offset: None,
                font_family: None,
                font_size: None,
                text_field: None,
                text_color: None,
            }],
        }
    }

    #[test]
    fn render_filled_polygon() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = polygon_style();
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        // A square polygon covering the center
        let features = vec![Feature {
            geometry: Geometry::Polygon {
                exterior: vec![
                    Point { x: 0.25, y: 0.25 },
                    Point { x: 0.75, y: 0.25 },
                    Point { x: 0.75, y: 0.75 },
                    Point { x: 0.25, y: 0.75 },
                    Point { x: 0.25, y: 0.25 },
                ],
                holes: vec![],
            },
            properties: HashMap::new(),
        }];
        let result = renderer.render(&style, &features, &bbox).unwrap();
        // Center should be blue (fill)
        let cx = 128u32;
        let cy = 128u32;
        let idx = ((cy * 256 + cx) * 4) as usize;
        assert_eq!(result.data[idx], 0); // R
        assert_eq!(result.data[idx + 1], 0); // G
        assert_eq!(result.data[idx + 2], 255); // B
        assert_eq!(result.data[idx + 3], 255); // A
    }

    #[test]
    fn render_polygon_with_hole() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = Style {
            name: "hole-test".to_string(),
            layers: vec![Layer {
                id: "polygons".to_string(),
                source: None,
                fill_color: Some(StyleValue::Literal(Color::rgb(255, 0, 0))),
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
                icon_rotate: None,
                icon_anchor: IconAnchor::Center,
                icon_offset: None,
                font_family: None,
                font_size: None,
                text_field: None,
                text_color: None,
            }],
        };
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        // Outer square with inner hole at center
        let features = vec![Feature {
            geometry: Geometry::Polygon {
                exterior: vec![
                    Point { x: 0.1, y: 0.1 },
                    Point { x: 0.9, y: 0.1 },
                    Point { x: 0.9, y: 0.9 },
                    Point { x: 0.1, y: 0.9 },
                    Point { x: 0.1, y: 0.1 },
                ],
                holes: vec![vec![
                    Point { x: 0.4, y: 0.4 },
                    Point { x: 0.6, y: 0.4 },
                    Point { x: 0.6, y: 0.6 },
                    Point { x: 0.4, y: 0.6 },
                    Point { x: 0.4, y: 0.4 },
                ]],
            },
            properties: HashMap::new(),
        }];
        let result = renderer.render(&style, &features, &bbox).unwrap();
        // Center (inside hole) should be transparent
        let cx = 128u32;
        let cy = 128u32;
        let idx = ((cy * 256 + cx) * 4) as usize;
        assert_eq!(result.data[idx + 3], 0); // A = 0 (hole)

        // Point between exterior and hole should be red
        let ox = 50u32;
        let oy = 128u32;
        let idx2 = ((oy * 256 + ox) * 4) as usize;
        assert_eq!(result.data[idx2], 255); // R
        assert_eq!(result.data[idx2 + 3], 255); // A
    }

    #[test]
    fn render_multipolygon() {
        let renderer = Renderer::new(256, 256).unwrap();
        let style = polygon_style();
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        use crate::geometry::PolygonGeom;
        let features = vec![Feature {
            geometry: Geometry::MultiPolygon(vec![
                PolygonGeom {
                    exterior: vec![
                        Point { x: 0.1, y: 0.1 },
                        Point { x: 0.4, y: 0.1 },
                        Point { x: 0.4, y: 0.4 },
                        Point { x: 0.1, y: 0.4 },
                        Point { x: 0.1, y: 0.1 },
                    ],
                    holes: vec![],
                },
                PolygonGeom {
                    exterior: vec![
                        Point { x: 0.6, y: 0.6 },
                        Point { x: 0.9, y: 0.6 },
                        Point { x: 0.9, y: 0.9 },
                        Point { x: 0.6, y: 0.9 },
                        Point { x: 0.6, y: 0.6 },
                    ],
                    holes: vec![],
                },
            ]),
            properties: HashMap::new(),
        }];
        let result = renderer.render(&style, &features, &bbox).unwrap();
        // Both polygons should have blue fill at their centers
        // First polygon center approx (64, 192) — y is flipped
        let idx1 = ((192u32 * 256 + 64) * 4) as usize;
        assert_eq!(result.data[idx1 + 2], 255); // B
        assert_eq!(result.data[idx1 + 3], 255); // A
    }

    #[test]
    fn data_driven_fill_color() {
        use jung_style::{ExprValue, Expression, PropertyValue};

        let renderer = Renderer::new(256, 256).unwrap();
        // Layer with expression: ["case", ["==", ["get","type"], "park"], "green", "red"]
        let style = Style {
            name: "data-driven".to_string(),
            layers: vec![Layer {
                id: "polygons".to_string(),
                source: None,
                fill_color: Some(StyleValue::Expression(Expression::Case(
                    vec![(
                        Expression::Eq(
                            Box::new(Expression::Get("type".to_string())),
                            Box::new(Expression::Literal(ExprValue::String("park".to_string()))),
                        ),
                        Expression::Literal(ExprValue::Color(Color::rgb(0, 128, 0))),
                    )],
                    Box::new(Expression::Literal(ExprValue::Color(Color::rgb(255, 0, 0)))),
                ))),
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
                icon_rotate: None,
                icon_anchor: IconAnchor::Center,
                icon_offset: None,
                font_family: None,
                font_size: None,
                text_field: None,
                text_color: None,
            }],
        };
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };

        // Feature with type=park → should be green
        let mut park_props = HashMap::new();
        park_props.insert(
            "type".to_string(),
            PropertyValue::String("park".to_string()),
        );
        // Feature with type=building → should be red (fallback)
        let mut bldg_props = HashMap::new();
        bldg_props.insert(
            "type".to_string(),
            PropertyValue::String("building".to_string()),
        );

        let features = vec![
            Feature {
                geometry: Geometry::Polygon {
                    exterior: vec![
                        Point { x: 0.1, y: 0.1 },
                        Point { x: 0.4, y: 0.1 },
                        Point { x: 0.4, y: 0.4 },
                        Point { x: 0.1, y: 0.4 },
                        Point { x: 0.1, y: 0.1 },
                    ],
                    holes: vec![],
                },
                properties: park_props,
            },
            Feature {
                geometry: Geometry::Polygon {
                    exterior: vec![
                        Point { x: 0.6, y: 0.6 },
                        Point { x: 0.9, y: 0.6 },
                        Point { x: 0.9, y: 0.9 },
                        Point { x: 0.6, y: 0.9 },
                        Point { x: 0.6, y: 0.6 },
                    ],
                    holes: vec![],
                },
                properties: bldg_props,
            },
        ];

        let result = renderer.render(&style, &features, &bbox).unwrap();

        // Park polygon center (approx pixel 64, 192 with y-flip) → green
        let park_idx = ((192u32 * 256 + 64) * 4) as usize;
        assert_eq!(result.data[park_idx], 0); // R
        assert_eq!(result.data[park_idx + 1], 128); // G
        assert_eq!(result.data[park_idx + 2], 0); // B
        assert_eq!(result.data[park_idx + 3], 255); // A

        // Building polygon center (approx pixel 192, 64 with y-flip) → red
        let bldg_idx = ((64u32 * 256 + 192) * 4) as usize;
        assert_eq!(result.data[bldg_idx], 255); // R
        assert_eq!(result.data[bldg_idx + 1], 0); // G
        assert_eq!(result.data[bldg_idx + 2], 0); // B
        assert_eq!(result.data[bldg_idx + 3], 255); // A
    }

    #[test]
    fn zoom_dependent_line_width() {
        use jung_style::{ExprValue, Expression, Interpolation};

        let renderer = Renderer::new(256, 256).unwrap();
        // interpolate: linear zoom, stops at z5=1px, z15=10px
        let style = Style {
            name: "zoom-style".to_string(),
            layers: vec![Layer {
                id: "lines".to_string(),
                source: None,
                fill_color: None,
                stroke_color: Some(StyleValue::Literal(Color::rgb(255, 0, 0))),
                stroke_width: Some(StyleValue::Expression(Expression::Interpolate {
                    interpolation: Interpolation::Linear,
                    input: Box::new(Expression::Zoom),
                    stops: vec![
                        (5.0, Expression::Literal(ExprValue::Number(1.0))),
                        (15.0, Expression::Literal(ExprValue::Number(10.0))),
                    ],
                })),
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                line_dasharray: None,
                line_offset: None,
                line_opacity: None,
                point_radius: None,
                icon_image: None,
                icon_size: None,
                icon_rotate: None,
                icon_anchor: IconAnchor::Center,
                icon_offset: None,
                font_family: None,
                font_size: None,
                text_field: None,
                text_color: None,
            }],
        };
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let features = vec![Feature {
            geometry: Geometry::LineString(vec![
                Point { x: 0.1, y: 0.5 },
                Point { x: 0.9, y: 0.5 },
            ]),
            properties: HashMap::new(),
        }];

        // At zoom 5, width=1 → thin line (few pixels around center)
        let r5 = renderer
            .render_at_zoom(&style, &features, &bbox, 5.0)
            .unwrap();
        // At zoom 15, width=10 → thick line (many pixels around center)
        let r15 = renderer
            .render_at_zoom(&style, &features, &bbox, 15.0)
            .unwrap();

        // Count non-transparent pixels in each
        let count5 = r5.data.chunks(4).filter(|px| px[3] > 0).count();
        let count15 = r15.data.chunks(4).filter(|px| px[3] > 0).count();

        // z15 line should be substantially thicker (more pixels)
        assert!(
            count15 > count5 * 3,
            "z15 pixels ({count15}) should be >>  z5 pixels ({count5})"
        );
    }

    #[test]
    fn render_icon_from_sprite() {
        use crate::marker::{Icon, SpriteAtlas};

        let renderer = Renderer::new(64, 64).unwrap();
        let mut atlas = SpriteAtlas::new();
        atlas.insert("pin", Icon::square(6, 0, 0, 255, 255));

        let style = Style {
            name: "icon-test".to_string(),
            layers: vec![Layer {
                id: "icons".to_string(),
                source: None,
                fill_color: None,
                stroke_color: None,
                stroke_width: None,
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                line_dasharray: None,
                line_offset: None,
                line_opacity: None,
                point_radius: None,
                icon_image: Some(StyleValue::Literal("pin".to_string())),
                icon_size: Some(StyleValue::Literal(1.0)),
                icon_rotate: None,
                icon_anchor: IconAnchor::Center,
                icon_offset: None,
                font_family: None,
                font_size: None,
                text_field: None,
                text_color: None,
            }],
        };
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let features = vec![Feature {
            geometry: Geometry::Point(Point { x: 0.5, y: 0.5 }),
            properties: HashMap::new(),
        }];

        let result = renderer
            .render_with_sprites(&style, &features, &bbox, 0.0, &atlas)
            .unwrap();

        // Center pixel should be blue (from the 6x6 square icon)
        let cx = 32u32;
        let cy = 32u32;
        let idx = ((cy * 64 + cx) * 4) as usize;
        assert_eq!(result.data[idx], 0); // R
        assert_eq!(result.data[idx + 1], 0); // G
        assert_eq!(result.data[idx + 2], 255); // B
        assert_eq!(result.data[idx + 3], 255); // A
    }

    #[test]
    fn render_icon_with_anchor_offset_and_rotation() {
        use crate::marker::{Icon, IconPlacement, SpriteAtlas, blit_icon_placed};

        let renderer = Renderer::new(64, 64).unwrap();
        let mut atlas = SpriteAtlas::new();
        // asymmetric icon so rotation is observable
        let mut data = vec![0u8; 6 * 6 * 4];
        for (px, py) in [(0usize, 0usize), (1, 0), (2, 0), (0, 1)] {
            let i = (py * 6 + px) * 4;
            data[i + 2] = 255;
            data[i + 3] = 255;
        }
        let icon = Icon::new(6, 6, data).unwrap();
        atlas.insert("pin", icon.clone());

        let style = Style {
            name: "icon-placement".to_string(),
            layers: vec![Layer {
                id: "icons".to_string(),
                source: None,
                fill_color: None,
                stroke_color: None,
                stroke_width: None,
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                line_dasharray: None,
                line_offset: None,
                line_opacity: None,
                point_radius: None,
                icon_image: Some(StyleValue::Literal("pin".to_string())),
                icon_size: Some(StyleValue::Literal(1.0)),
                icon_rotate: Some(StyleValue::Literal(90.0)),
                icon_anchor: IconAnchor::TopLeft,
                icon_offset: Some([4.0, -3.0]),
                font_family: None,
                font_size: None,
                text_field: None,
                text_color: None,
            }],
        };
        let bbox = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let features = vec![Feature {
            geometry: Geometry::Point(Point { x: 0.5, y: 0.5 }),
            properties: HashMap::new(),
        }];

        let result = renderer
            .render_with_sprites(&style, &features, &bbox, 0.0, &atlas)
            .unwrap();

        let mut expected = PixelBuffer::new(64, 64);
        blit_icon_placed(
            &mut expected,
            &icon,
            32.0,
            32.0,
            &IconPlacement {
                anchor: IconAnchor::TopLeft,
                offset: [4.0, -3.0],
                rotation_deg: 90.0,
                scale: 1.0,
            },
        );
        assert_eq!(result.data, expected.data);

        // and it differs from the unplaced default
        let mut plain = PixelBuffer::new(64, 64);
        blit_icon_placed(&mut plain, &icon, 32.0, 32.0, &IconPlacement::default());
        assert_ne!(result.data, plain.data);
    }

    // ── labels ─────────────────────────────────────────────────────────────

    const TEST_FAMILY: &str = "Test Sans";

    fn unit_bbox() -> BBox {
        BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        }
    }

    /// A renderer carrying a system font under `TEST_FAMILY`, or `None` on a
    /// machine with no font to load.
    fn labelling_renderer(width: u32, height: u32) -> Option<Renderer> {
        let face = crate::text::load_test_font()?;
        let mut fonts = FontSet::new();
        fonts.insert(TEST_FAMILY, face);
        Some(Renderer::new(width, height).unwrap().with_fonts(fonts))
    }

    fn named_point(name: &str, x: f64, y: f64) -> Feature {
        let mut properties = HashMap::new();
        properties.insert("name".to_string(), PropertyValue::String(name.to_string()));
        Feature {
            geometry: Geometry::Point(Point { x, y }),
            properties,
        }
    }

    #[test]
    fn text_layer_draws_label_pixels_at_the_feature() {
        let Some(renderer) = labelling_renderer(256, 256) else {
            eprintln!("skipping: no system font");
            return;
        };
        let style = jung_style::parse_style(
            r##"{"layers": [{
                "id": "labels",
                "paint": { "text-color": "#00ff00" },
                "layout": { "text-field": "{name}", "text-size": 20.0, "text-font": ["Test Sans"] }
            }]}"##,
        )
        .unwrap();
        let features = vec![named_point("Springfield", 0.5, 0.5)];

        let placements = renderer.place_labels(&style, &features, &unit_bbox(), 0.0);
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].text, "Springfield");
        assert_eq!(placements[0].color, Color::rgb(0, 255, 0));
        let LabelGeometry::Baseline { x, y } = placements[0].geometry else {
            panic!("a point label places straight");
        };
        assert!(x < 128.0 && x > 20.0, "label starts left of the point: {x}");
        assert!((y - 128.0).abs() < 20.0, "baseline near the point: {y}");

        let result = renderer.render(&style, &features, &unit_bbox()).unwrap();
        let green: Vec<(u32, u32)> = result
            .data
            .chunks(4)
            .enumerate()
            .filter(|(_, px)| px[1] == 255 && px[0] == 0 && px[3] > 0)
            .map(|(i, _)| (i as u32 % 256, i as u32 / 256))
            .collect();
        assert!(
            green.len() > 20,
            "expected text pixels, got {}",
            green.len()
        );
        assert!(
            green
                .iter()
                .all(|(px, py)| (40..220).contains(px) && (100..160).contains(py)),
            "text pixels should sit around the feature"
        );
    }

    #[test]
    fn bigger_text_wins_a_collision() {
        // 200x40 leaves room for one large label at the centre and nowhere for a
        // second one to move to
        let Some(renderer) = labelling_renderer(200, 40) else {
            eprintln!("skipping: no system font");
            return;
        };
        let minor_layer = r##"{
            "id": "minor",
            "layout": { "text-field": "Bakersfield", "text-size": 18.0, "text-font": ["Test Sans"] }
        }"##;
        let major_layer = r##"{
            "id": "major",
            "layout": { "text-field": "Metropolis", "text-size": 24.0, "text-font": ["Test Sans"] }
        }"##;
        let features = vec![named_point("ignored", 0.5, 0.5)];

        let alone = jung_style::parse_style(&format!(r#"{{"layers": [{minor_layer}]}}"#)).unwrap();
        let placed_alone = renderer.place_labels(&alone, &features, &unit_bbox(), 0.0);
        assert_eq!(
            placed_alone.len(),
            1,
            "the smaller label fits when nothing competes"
        );

        let both =
            jung_style::parse_style(&format!(r#"{{"layers": [{minor_layer}, {major_layer}]}}"#))
                .unwrap();
        let placements = renderer.place_labels(&both, &features, &unit_bbox(), 0.0);
        let texts: Vec<&str> = placements.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, vec!["Metropolis"]);
    }

    #[test]
    fn line_feature_label_follows_the_line() {
        let Some(renderer) = labelling_renderer(400, 400) else {
            eprintln!("skipping: no system font");
            return;
        };
        let style = jung_style::parse_style(
            r##"{"layers": [{
                "id": "rivers",
                "layout": { "text-field": "{name}", "text-size": 16.0, "text-font": ["Test Sans"] }
            }]}"##,
        )
        .unwrap();
        let mut properties = HashMap::new();
        properties.insert(
            "name".to_string(),
            PropertyValue::String("Rio Bravo".to_string()),
        );
        // screen-space diagonal from (40, 360) to (360, 40), so y = 400 - x
        let features = vec![Feature {
            geometry: Geometry::LineString(vec![
                Point { x: 0.1, y: 0.1 },
                Point { x: 0.9, y: 0.9 },
            ]),
            properties,
        }];

        let placements = renderer.place_labels(&style, &features, &unit_bbox(), 0.0);
        assert_eq!(placements.len(), 1);
        let LabelGeometry::AlongLine(chars) = &placements[0].geometry else {
            panic!("a line label follows the geometry");
        };
        assert_eq!(chars.len(), "Rio Bravo".chars().count());
        for placed in chars {
            assert!(
                (placed.y - (400.0 - placed.x)).abs() < 2.0,
                "character {} off the line at ({}, {})",
                placed.ch,
                placed.x,
                placed.y
            );
            assert!(
                (placed.angle + std::f64::consts::FRAC_PI_4).abs() < 0.05,
                "character {} not turned with the line: {}",
                placed.ch,
                placed.angle
            );
        }
    }

    #[test]
    fn text_layer_without_a_font_renders_no_labels() {
        let renderer = Renderer::new(128, 128).unwrap();
        let with_text = jung_style::parse_style(
            r##"{"layers": [{
                "id": "labels",
                "paint": { "circle-color": "#0000ff", "circle-radius": 3.0, "text-color": "#00ff00" },
                "layout": { "text-field": "{name}", "text-size": 20.0 }
            }]}"##,
        )
        .unwrap();
        let without_text = jung_style::parse_style(
            r##"{"layers": [{
                "id": "labels",
                "paint": { "circle-color": "#0000ff", "circle-radius": 3.0 }
            }]}"##,
        )
        .unwrap();
        let features = vec![named_point("Springfield", 0.5, 0.5)];

        assert_eq!(
            renderer.text_layers_without_font(&with_text),
            vec!["labels"]
        );
        assert!(
            renderer
                .place_labels(&with_text, &features, &unit_bbox(), 0.0)
                .is_empty()
        );

        let labelled = renderer
            .render(&with_text, &features, &unit_bbox())
            .unwrap();
        let plain = renderer
            .render(&without_text, &features, &unit_bbox())
            .unwrap();
        assert_eq!(labelled.data, plain.data, "no font, so no label pixels");
    }

    #[test]
    fn label_text_expands_property_tokens() {
        let mut properties = HashMap::new();
        properties.insert(
            "name".to_string(),
            PropertyValue::String("Oslo".to_string()),
        );
        properties.insert("population".to_string(), PropertyValue::Integer(709_037));

        assert_eq!(
            expand_property_tokens("{name} ({population})", &properties),
            "Oslo (709037)"
        );
        assert_eq!(expand_property_tokens("{missing}", &properties), "");
        assert_eq!(expand_property_tokens("plain", &properties), "plain");
        assert_eq!(
            expand_property_tokens("{unclosed", &properties),
            "{unclosed"
        );
    }

    #[test]
    fn text_size_sets_label_priority() {
        assert_eq!(priority_for_text_size(30.0), LabelPriority::Critical);
        assert_eq!(priority_for_text_size(18.0), LabelPriority::High);
        assert_eq!(priority_for_text_size(16.0), LabelPriority::Medium);
        assert_eq!(priority_for_text_size(10.0), LabelPriority::Low);
        assert_eq!(priority_for_text_size(6.0), LabelPriority::Optional);
    }
}
