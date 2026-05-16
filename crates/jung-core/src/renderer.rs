use crate::geometry::{Feature, Geometry, Point};
use crate::line::{LineParams, render_line};
use crate::marker::{SpriteAtlas, blit_icon};
use crate::polygon::render_polygon;
use jung_style::{Color, EvalContext, Layer, Style, StyleValue};
use thiserror::Error;

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

/// The main renderer. Takes a style and features, produces pixel output.
pub struct Renderer {
    pub width: u32,
    pub height: u32,
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions { width, height });
        }
        Ok(Self { width, height })
    }

    /// Render features according to the given style within the bounding box.
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

        Ok(buffer)
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
            let scale = resolve_f32(&layer.icon_size, ctx).unwrap_or(1.0) as f64;
            blit_icon(buffer, icon, px, py, scale);
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
    use jung_style::{LineCap, LineJoin, StyleValue};
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
}
