//! # jung-vello
//!
//! GPU-accelerated rendering backend for Jung using [Vello](https://github.com/linebender/vello).
//!
//! Converts styled geospatial features into a `vello::Scene` which can be
//! rendered at high performance via wgpu compute shaders.
//!
//! Point, line and polygon geometry, plus a style's `text-*` properties once
//! the caller supplies a font with [`SceneBuilder::with_font`].
//!
//! # Usage
//!
//! ```ignore
//! use jung_vello::SceneBuilder;
//! use jung_core::renderer::BBox;
//!
//! let builder = SceneBuilder::new(512, 512, bbox)
//!     .with_font(std::fs::read("DejaVuSans.ttf").unwrap())
//!     .unwrap();
//! let scene = builder.build(&style, &features);
//! // Render `scene` with your wgpu device via vello::Renderer
//! ```

use jung_core::geometry::{Feature, Geometry, Point};
use jung_core::ogc::polygon_centroid;
use jung_core::renderer::{BBox, DEFAULT_TEXT_COLOR, DEFAULT_TEXT_SIZE_PX, expand_property_tokens};
use jung_style::{Color, EvalContext, Layer, Style, StyleValue};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use vello::kurbo::{Affine, BezPath, Circle, Stroke};
use vello::peniko::{Blob, Fill, FontData};
use vello::{Glyph, Scene};

/// jung takes one face per font file, never a later member of a collection.
const FONT_COLLECTION_INDEX: u32 = 0;

/// Builds a Vello scene from styled geospatial features.
pub struct SceneBuilder {
    width: u32,
    height: u32,
    bbox: BBox,
    font: Option<FontData>,
}

impl SceneBuilder {
    /// Create a new scene builder with output dimensions and map extent.
    pub fn new(width: u32, height: u32, bbox: BBox) -> Self {
        Self {
            width,
            height,
            bbox,
            font: None,
        }
    }

    /// Give the builder the font labels draw with, as TTF/OTF bytes. jung
    /// embeds none, and there is no discovery or fallback: this one face draws
    /// every text layer whatever family `text-font` names. `None` when the
    /// bytes are not a font.
    pub fn with_font(mut self, font_bytes: Vec<u8>) -> Option<Self> {
        let font = FontData::new(Blob::from(font_bytes), FONT_COLLECTION_INDEX);
        FontRef::from_index(font.data.as_ref(), font.index).ok()?;
        self.font = Some(font);
        Some(self)
    }

    /// Build a complete Vello scene from a style and feature set.
    pub fn build(&self, style: &Style, features: &[Feature]) -> Scene {
        let mut scene = Scene::new();

        for layer in &style.layers {
            for feature in features {
                self.render_feature(&mut scene, layer, feature);
            }
        }

        scene
    }

    /// Build a scene from features using a single layer.
    pub fn build_layer(&self, layer: &Layer, features: &[Feature]) -> Scene {
        let mut scene = Scene::new();
        for feature in features {
            self.render_feature(&mut scene, layer, feature);
        }
        scene
    }

    fn render_feature(&self, scene: &mut Scene, layer: &Layer, feature: &Feature) {
        let ctx = EvalContext {
            properties: &feature.properties,
            zoom: 10.0,
            geometry_type: geometry_type_str(&feature.geometry),
        };

        match &feature.geometry {
            Geometry::Point(p) => self.render_point(scene, layer, &ctx, *p),
            Geometry::MultiPoint(pts) => {
                for p in pts {
                    self.render_point(scene, layer, &ctx, *p);
                }
            }
            Geometry::LineString(pts) => self.render_line(scene, layer, &ctx, pts),
            Geometry::MultiLineString(lines) => {
                for line in lines {
                    self.render_line(scene, layer, &ctx, line);
                }
            }
            Geometry::Polygon { exterior, holes } => {
                self.render_polygon(scene, layer, &ctx, exterior, holes);
            }
            Geometry::MultiPolygon(polys) => {
                for poly in polys {
                    self.render_polygon(scene, layer, &ctx, &poly.exterior, &poly.holes);
                }
            }
        }

        self.render_label(scene, layer, &ctx, feature);
    }

    fn render_label(&self, scene: &mut Scene, layer: &Layer, ctx: &EvalContext, feature: &Feature) {
        let Some(font) = self.font.as_ref() else {
            return;
        };
        let Some(template) = layer.text_field.as_ref().and_then(|sv| sv.resolve(ctx)) else {
            return;
        };
        let text = expand_property_tokens(&template, &feature.properties);
        if text.is_empty() {
            return;
        }
        let size = resolve_f32(&layer.font_size, ctx).unwrap_or(DEFAULT_TEXT_SIZE_PX);
        if size <= 0.0 {
            return;
        }
        let Ok(font_ref) = FontRef::from_index(font.data.as_ref(), font.index) else {
            return;
        };
        let color = layer
            .text_color
            .as_ref()
            .and_then(|sv| sv.resolve(ctx))
            .unwrap_or(DEFAULT_TEXT_COLOR);

        for anchor in label_anchors(&feature.geometry) {
            let (screen_x, screen_y) = self.map_to_screen(&anchor);
            let glyphs = layout_glyphs(&font_ref, &text, size, screen_x, screen_y);
            if glyphs.is_empty() {
                continue;
            }
            scene
                .draw_glyphs(font)
                .font_size(size)
                .brush(to_vello_color(color))
                .draw(Fill::NonZero, glyphs.into_iter());
        }
    }

    fn render_point(&self, scene: &mut Scene, layer: &Layer, ctx: &EvalContext, point: Point) {
        let (sx, sy) = self.map_to_screen(&point);
        let radius = resolve_f32(&layer.point_radius, ctx).unwrap_or(5.0) as f64;

        let fill_color = layer
            .fill_color
            .as_ref()
            .and_then(|sv| sv.resolve(ctx))
            .unwrap_or(Color::rgb(0, 0, 0));

        let circle = Circle::new((sx, sy), radius);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            to_vello_color(fill_color),
            None,
            &circle,
        );
    }

    fn render_line(&self, scene: &mut Scene, layer: &Layer, ctx: &EvalContext, points: &[Point]) {
        if points.len() < 2 {
            return;
        }

        let path = self.points_to_path(points);

        let stroke_color = layer
            .stroke_color
            .as_ref()
            .and_then(|sv| sv.resolve(ctx))
            .or_else(|| layer.fill_color.as_ref().and_then(|sv| sv.resolve(ctx)))
            .unwrap_or(Color::rgb(0, 0, 0));

        let width = resolve_f32(&layer.stroke_width, ctx).unwrap_or(1.0) as f64;
        let stroke = Stroke::new(width);

        scene.stroke(
            &stroke,
            Affine::IDENTITY,
            to_vello_color(stroke_color),
            None,
            &path,
        );
    }

    fn render_polygon(
        &self,
        scene: &mut Scene,
        layer: &Layer,
        ctx: &EvalContext,
        exterior: &[Point],
        holes: &[Vec<Point>],
    ) {
        if exterior.len() < 3 {
            return;
        }

        let mut path = BezPath::new();

        // Exterior ring
        let screen_pts: Vec<(f64, f64)> = exterior.iter().map(|p| self.map_to_screen(p)).collect();
        if let Some(first) = screen_pts.first() {
            path.move_to(vello::kurbo::Point::new(first.0, first.1));
            for pt in &screen_pts[1..] {
                path.line_to(vello::kurbo::Point::new(pt.0, pt.1));
            }
            path.close_path();
        }

        // Holes (winding reversed automatically by even-odd rule)
        for hole in holes {
            let hole_pts: Vec<(f64, f64)> = hole.iter().map(|p| self.map_to_screen(p)).collect();
            if let Some(first) = hole_pts.first() {
                path.move_to(vello::kurbo::Point::new(first.0, first.1));
                for pt in &hole_pts[1..] {
                    path.line_to(vello::kurbo::Point::new(pt.0, pt.1));
                }
                path.close_path();
            }
        }

        // Fill
        let fill_color = layer
            .fill_color
            .as_ref()
            .and_then(|sv| sv.resolve(ctx))
            .unwrap_or(Color::rgba(0, 0, 0, 0));

        if fill_color.a > 0 {
            scene.fill(
                Fill::EvenOdd,
                Affine::IDENTITY,
                to_vello_color(fill_color),
                None,
                &path,
            );
        }

        // Stroke
        if let Some(stroke_color) = layer.stroke_color.as_ref().and_then(|sv| sv.resolve(ctx)) {
            let width = resolve_f32(&layer.stroke_width, ctx).unwrap_or(1.0) as f64;
            if width > 0.0 && stroke_color.a > 0 {
                let stroke = Stroke::new(width);
                scene.stroke(
                    &stroke,
                    Affine::IDENTITY,
                    to_vello_color(stroke_color),
                    None,
                    &path,
                );
            }
        }
    }

    fn map_to_screen(&self, p: &Point) -> (f64, f64) {
        let x = (p.x - self.bbox.min_x) / (self.bbox.max_x - self.bbox.min_x) * self.width as f64;
        let y = (self.bbox.max_y - p.y) / (self.bbox.max_y - self.bbox.min_y) * self.height as f64;
        (x, y)
    }

    fn points_to_path(&self, points: &[Point]) -> BezPath {
        let mut path = BezPath::new();
        let screen_pts: Vec<(f64, f64)> = points.iter().map(|p| self.map_to_screen(p)).collect();
        if let Some(first) = screen_pts.first() {
            path.move_to(vello::kurbo::Point::new(first.0, first.1));
            for pt in &screen_pts[1..] {
                path.line_to(vello::kurbo::Point::new(pt.0, pt.1));
            }
        }
        path
    }
}

/// Where a feature's labels sit. A line gets none: text that follows a line is
/// the CPU renderer's job.
fn label_anchors(geometry: &Geometry) -> Vec<Point> {
    match geometry {
        Geometry::Point(point) => vec![*point],
        Geometry::MultiPoint(points) => points.clone(),
        Geometry::Polygon { exterior, .. } => vec![polygon_centroid(exterior)],
        Geometry::MultiPolygon(polygons) => polygons
            .iter()
            .map(|polygon| polygon_centroid(&polygon.exterior))
            .collect(),
        Geometry::LineString(_) | Geometry::MultiLineString(_) => Vec::new(),
    }
}

/// Lay one line of text out centred on the anchor, the way `jung_core` places
/// point labels. Characters the font cannot map are dropped.
fn layout_glyphs(
    font: &FontRef<'_>,
    text: &str,
    size_px: f32,
    anchor_x: f64,
    anchor_y: f64,
) -> Vec<Glyph> {
    let size = Size::new(size_px);
    let charmap = font.charmap();
    let glyph_metrics = font.glyph_metrics(size, LocationRef::default());
    let glyph_ids: Vec<GlyphId> = text.chars().filter_map(|ch| charmap.map(ch)).collect();

    let text_width: f32 = glyph_ids
        .iter()
        .filter_map(|id| glyph_metrics.advance_width(*id))
        .sum();
    let font_metrics = font.metrics(size, LocationRef::default());
    // skrifa's descent is negative
    let baseline_y = anchor_y as f32 + (font_metrics.ascent + font_metrics.descent) / 2.0;
    let mut pen_x = anchor_x as f32 - text_width / 2.0;

    glyph_ids
        .into_iter()
        .map(|id| {
            let glyph = Glyph {
                id: id.to_u32(),
                x: pen_x,
                y: baseline_y,
            };
            pen_x += glyph_metrics.advance_width(id).unwrap_or_default();
            glyph
        })
        .collect()
}

fn to_vello_color(c: Color) -> vello::peniko::Color {
    vello::peniko::Color::new([
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    ])
}

fn resolve_f32(val: &Option<StyleValue<f32>>, ctx: &EvalContext) -> Option<f32> {
    val.as_ref().and_then(|sv| sv.resolve(ctx))
}

fn geometry_type_str(geom: &Geometry) -> &'static str {
    match geom {
        Geometry::Point(_) | Geometry::MultiPoint(_) => "Point",
        Geometry::LineString(_) | Geometry::MultiLineString(_) => "LineString",
        Geometry::Polygon { .. } | Geometry::MultiPolygon(_) => "Polygon",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jung_core::geometry::Feature;
    use jung_style::{IconAnchor, LineCap, LineJoin};
    use skrifa::outline::{DrawSettings, OutlinePen};
    use std::collections::HashMap;
    use std::ops::Range;
    use vello::kurbo::Shape;

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
            icon_rotate: None,
            icon_anchor: IconAnchor::Center,
            icon_offset: None,
            font_family: None,
            font_size: None,
            text_field: None,
            text_color: None,
        }
    }

    #[test]
    fn build_empty_scene() {
        let builder = SceneBuilder::new(256, 256, test_bbox());
        let style = Style {
            name: "test".to_string(),
            layers: vec![],
        };
        let scene = builder.build(&style, &[]);
        // Scene is valid (no panic)
        let _ = scene;
    }

    #[test]
    fn build_point_scene() {
        let builder = SceneBuilder::new(256, 256, test_bbox());
        let layer = make_layer(Color::rgb(255, 0, 0));
        let features = vec![Feature {
            geometry: Geometry::Point(Point { x: 0.5, y: 0.5 }),
            properties: HashMap::new(),
        }];
        let scene = builder.build_layer(&layer, &features);
        let _ = scene;
    }

    #[test]
    fn build_line_scene() {
        let builder = SceneBuilder::new(256, 256, test_bbox());
        let mut layer = make_layer(Color::rgba(0, 0, 0, 0));
        layer.stroke_color = Some(StyleValue::Literal(Color::rgb(0, 0, 255)));
        layer.stroke_width = Some(StyleValue::Literal(2.0));
        let features = vec![Feature {
            geometry: Geometry::LineString(vec![
                Point { x: 0.1, y: 0.1 },
                Point { x: 0.9, y: 0.9 },
            ]),
            properties: HashMap::new(),
        }];
        let scene = builder.build_layer(&layer, &features);
        let _ = scene;
    }

    #[test]
    fn build_polygon_scene() {
        let builder = SceneBuilder::new(256, 256, test_bbox());
        let layer = make_layer(Color::rgb(0, 255, 0));
        let features = vec![Feature {
            geometry: Geometry::Polygon {
                exterior: vec![
                    Point { x: 0.2, y: 0.2 },
                    Point { x: 0.8, y: 0.2 },
                    Point { x: 0.8, y: 0.8 },
                    Point { x: 0.2, y: 0.8 },
                    Point { x: 0.2, y: 0.2 },
                ],
                holes: vec![],
            },
            properties: HashMap::new(),
        }];
        let scene = builder.build_layer(&layer, &features);
        let _ = scene;
    }

    #[test]
    fn map_to_screen_center() {
        let builder = SceneBuilder::new(100, 100, test_bbox());
        let (x, y) = builder.map_to_screen(&Point { x: 0.5, y: 0.5 });
        assert!((x - 50.0).abs() < 0.01);
        assert!((y - 50.0).abs() < 0.01);
    }

    /// Caladea Regular, Apache-2.0, shipped so the label test never depends on
    /// a system font.
    const FIXTURE_FONT_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/Caladea-Regular.ttf"
    );
    const LABEL_RASTER_SIZE: u32 = 128;
    /// Opaque `#00ff00` in vello's draw data: premultiplied rgba packed with
    /// red in the low byte.
    const OPAQUE_GREEN_RGBA: u32 = 0xff00_ff00;
    /// The label centres on a feature at the middle of the raster, so every
    /// glyph pixel lands in this box and none outside it.
    const LABEL_PIXELS_X: Range<u32> = 20..110;
    const LABEL_PIXELS_Y: Range<u32> = 52..78;
    const MINIMUM_LABEL_PIXELS: usize = 100;
    const STRAY_PIXELS_REPORTED: usize = 8;

    /// Vello rasterizes glyphs on the GPU, so the test walks the outlines of the
    /// glyph run the scene encoded and marks the pixels they cover.
    #[derive(Default)]
    struct GlyphOutline {
        path: BezPath,
        origin: (f64, f64),
    }

    impl GlyphOutline {
        fn raster_point(&self, x: f32, y: f32) -> vello::kurbo::Point {
            vello::kurbo::Point::new(self.origin.0 + x as f64, self.origin.1 - y as f64)
        }
    }

    impl OutlinePen for GlyphOutline {
        fn move_to(&mut self, x: f32, y: f32) {
            let point = self.raster_point(x, y);
            self.path.move_to(point);
        }

        fn line_to(&mut self, x: f32, y: f32) {
            let point = self.raster_point(x, y);
            self.path.line_to(point);
        }

        fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
            let control = self.raster_point(cx, cy);
            let point = self.raster_point(x, y);
            self.path.quad_to(control, point);
        }

        fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
            let first = self.raster_point(cx0, cy0);
            let second = self.raster_point(cx1, cy1);
            let point = self.raster_point(x, y);
            self.path.curve_to(first, second, point);
        }

        fn close(&mut self) {
            self.path.close_path();
        }
    }

    fn glyph_pixels(scene: &Scene, raster_size: u32) -> Vec<(u32, u32)> {
        let mut outline = GlyphOutline::default();
        let encoding = scene.encoding();
        for run in &encoding.resources.glyph_runs {
            let font = FontRef::from_index(run.font.data.as_ref(), run.font.index).unwrap();
            let outline_glyphs = font.outline_glyphs();
            for glyph in &encoding.resources.glyphs[run.glyphs.clone()] {
                let Some(outline_glyph) = outline_glyphs.get(GlyphId::new(glyph.id)) else {
                    continue;
                };
                outline.origin = (glyph.x as f64, glyph.y as f64);
                outline_glyph
                    .draw(
                        DrawSettings::unhinted(Size::new(run.font_size), LocationRef::default()),
                        &mut outline,
                    )
                    .unwrap();
            }
        }

        (0..raster_size)
            .flat_map(|y| (0..raster_size).map(move |x| (x, y)))
            .filter(|(x, y)| {
                outline
                    .path
                    .contains(vello::kurbo::Point::new(*x as f64 + 0.5, *y as f64 + 0.5))
            })
            .collect()
    }

    #[test]
    fn text_layer_draws_glyphs_around_the_feature() {
        let builder = SceneBuilder::new(LABEL_RASTER_SIZE, LABEL_RASTER_SIZE, test_bbox())
            .with_font(std::fs::read(FIXTURE_FONT_PATH).unwrap())
            .expect("the fixture font parses");
        let style = jung_style::parse_style(
            r##"{"layers": [{
                "id": "labels",
                "paint": { "text-color": "#00ff00" },
                "layout": { "text-field": "{name}", "text-size": 16.0 }
            }]}"##,
        )
        .unwrap();
        let mut properties = HashMap::new();
        properties.insert(
            "name".to_string(),
            jung_style::PropertyValue::String("Springfield".to_string()),
        );
        let features = vec![Feature {
            geometry: Geometry::Point(Point { x: 0.5, y: 0.5 }),
            properties,
        }];

        let scene = builder.build(&style, &features);
        assert!(
            scene.encoding().draw_data.contains(&OPAQUE_GREEN_RGBA),
            "the layer's text-color reaches the scene"
        );

        let label_pixels = glyph_pixels(&scene, LABEL_RASTER_SIZE);
        assert!(
            label_pixels.len() > MINIMUM_LABEL_PIXELS,
            "expected glyph pixels, got {}",
            label_pixels.len()
        );
        let stray: Vec<(u32, u32)> = label_pixels
            .into_iter()
            .filter(|(x, y)| !LABEL_PIXELS_X.contains(x) || !LABEL_PIXELS_Y.contains(y))
            .take(STRAY_PIXELS_REPORTED)
            .collect();
        assert!(
            stray.is_empty(),
            "glyph pixels outside the label: {stray:?}"
        );
    }

    #[test]
    fn no_font_means_no_glyphs() {
        let builder = SceneBuilder::new(LABEL_RASTER_SIZE, LABEL_RASTER_SIZE, test_bbox());
        let mut layer = make_layer(Color::rgb(255, 0, 0));
        layer.text_field = Some(StyleValue::Literal("Springfield".to_string()));
        let features = vec![Feature {
            geometry: Geometry::Point(Point { x: 0.5, y: 0.5 }),
            properties: HashMap::new(),
        }];
        let scene = builder.build_layer(&layer, &features);
        assert!(scene.encoding().resources.glyph_runs.is_empty());
    }

    #[test]
    fn color_conversion() {
        let c = to_vello_color(Color::rgba(255, 128, 0, 200));
        let components = c.to_rgba8();
        assert_eq!(components.r, 255);
        assert!((components.g as i32 - 128).abs() <= 1);
        assert_eq!(components.b, 0);
        assert!((components.a as i32 - 200).abs() <= 1);
    }
}
