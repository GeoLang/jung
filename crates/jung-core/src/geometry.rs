/// A 2D point in map coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Geometry types that can be symbolized.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    Point(Point),
    LineString(Vec<Point>),
    Polygon {
        exterior: Vec<Point>,
        holes: Vec<Vec<Point>>,
    },
    MultiPoint(Vec<Point>),
    MultiLineString(Vec<Vec<Point>>),
    MultiPolygon(Vec<PolygonGeom>),
}

/// A single polygon (exterior ring + holes).
#[derive(Debug, Clone, PartialEq)]
pub struct PolygonGeom {
    pub exterior: Vec<Point>,
    pub holes: Vec<Vec<Point>>,
}

use jung_style::PropertyValue;

/// A geospatial feature with geometry and attributes.
#[derive(Debug, Clone)]
pub struct Feature {
    pub geometry: Geometry,
    pub properties: std::collections::HashMap<String, PropertyValue>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_equality() {
        let p1 = Point { x: 1.0, y: 2.0 };
        let p2 = Point { x: 1.0, y: 2.0 };
        assert_eq!(p1, p2);
    }

    #[test]
    fn point_inequality() {
        let p1 = Point { x: 1.0, y: 2.0 };
        let p2 = Point { x: 1.0, y: 3.0 };
        assert_ne!(p1, p2);
    }

    #[test]
    fn feature_construction() {
        let feature = Feature {
            geometry: Geometry::Point(Point { x: 0.0, y: 0.0 }),
            properties: std::collections::HashMap::new(),
        };
        assert_eq!(feature.geometry, Geometry::Point(Point { x: 0.0, y: 0.0 }));
    }

    #[test]
    fn linestring_geometry() {
        let ls = Geometry::LineString(vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }]);
        match ls {
            Geometry::LineString(pts) => assert_eq!(pts.len(), 2),
            _ => panic!("expected LineString"),
        }
    }

    #[test]
    fn polygon_geometry() {
        let poly = Geometry::Polygon {
            exterior: vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 1.0, y: 0.0 },
                Point { x: 1.0, y: 1.0 },
                Point { x: 0.0, y: 0.0 },
            ],
            holes: vec![],
        };
        match poly {
            Geometry::Polygon { exterior, holes } => {
                assert_eq!(exterior.len(), 4);
                assert!(holes.is_empty());
            }
            _ => panic!("expected Polygon"),
        }
    }

    #[test]
    fn multi_geometries() {
        let mp = Geometry::MultiPoint(vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }]);
        let mls = Geometry::MultiLineString(vec![
            vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 0.0 }],
            vec![Point { x: 0.0, y: 1.0 }, Point { x: 1.0, y: 1.0 }],
        ]);
        match mp {
            Geometry::MultiPoint(pts) => assert_eq!(pts.len(), 2),
            _ => panic!("expected MultiPoint"),
        }
        match mls {
            Geometry::MultiLineString(lines) => assert_eq!(lines.len(), 2),
            _ => panic!("expected MultiLineString"),
        }
    }

    #[test]
    fn feature_with_properties() {
        let mut props = std::collections::HashMap::new();
        props.insert(
            "name".to_string(),
            PropertyValue::String("test".to_string()),
        );
        props.insert("pop".to_string(), PropertyValue::Number(1000.0));
        let feature = Feature {
            geometry: Geometry::Point(Point { x: 0.0, y: 0.0 }),
            properties: props,
        };
        assert_eq!(feature.properties.len(), 2);
    }
}
