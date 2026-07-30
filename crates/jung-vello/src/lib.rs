//! # jung-vello
//!
//! GPU-accelerated rendering backend for Jung using [Vello](https://github.com/linebender/vello).
//!
//! Converts styled geospatial features into a `vello::Scene` which can be
//! rendered at high performance via wgpu compute shaders.
//!
//! # Usage
//!
//! ```ignore
//! use jung_vello::SceneBuilder;
//! use jung_core::renderer::BBox;
//!
//! let builder = SceneBuilder::new(512, 512, bbox);
//! let scene = builder.build(&style, &features);
//! // Render `scene` with your wgpu device via vello::Renderer
//! ```

use jung_core::geometry::{Feature, Geometry, Point};
use jung_core::renderer::BBox;
use jung_style::{Color, EvalContext, Layer, Style, StyleValue};
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Circle, Stroke};
use vello::peniko::Fill;

/// Builds a Vello scene from styled geospatial features.
pub struct SceneBuilder {
    width: u32,
    height: u32,
    bbox: BBox,
}

impl SceneBuilder {
    /// Create a new scene builder with output dimensions and map extent.
    pub fn new(width: u32, height: u32, bbox: BBox) -> Self {
        Self {
            width,
            height,
            bbox,
        }
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
