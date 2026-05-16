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
    fn feature_construction() {
        let feature = Feature {
            geometry: Geometry::Point(Point { x: 0.0, y: 0.0 }),
            properties: std::collections::HashMap::new(),
        };
        assert_eq!(feature.geometry, Geometry::Point(Point { x: 0.0, y: 0.0 }));
    }
}
