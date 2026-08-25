//! GeoJSON geometry parsing (RFC 7946).

use crate::geometry::{Geometry, Point, PolygonGeom};
use crate::ogc::WktError;
use serde_json::Value;

/// RFC 7946 requires at least two positions in a LineString.
const MINIMUM_LINE_POSITIONS: usize = 2;

/// RFC 7946 requires at least four positions in a linear ring, the last
/// repeating the first.
const MINIMUM_RING_POSITIONS: usize = 4;

/// Parse one GeoJSON geometry object into geometries.
///
/// Every type but GeometryCollection yields exactly one geometry. A
/// GeometryCollection yields one per member, flattened recursively, so a caller
/// gets a flat list whatever it was given.
pub fn parse_geojson_geometry(value: &Value) -> Result<Vec<Geometry>, WktError> {
    let geometry_type = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| WktError::InvalidGeometry("geometry missing type".to_string()))?;

    if geometry_type == "GeometryCollection" {
        let members = value
            .get("geometries")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                WktError::InvalidGeometry("GeometryCollection missing geometries".to_string())
            })?;
        let mut geometries = Vec::new();
        for member in members {
            geometries.extend(parse_geojson_geometry(member)?);
        }
        return Ok(geometries);
    }

    let coordinates = value
        .get("coordinates")
        .ok_or_else(|| WktError::InvalidGeometry(format!("{geometry_type} missing coordinates")))?;

    let geometry = match geometry_type {
        "Point" => Geometry::Point(parse_position(coordinates)?),
        "MultiPoint" => Geometry::MultiPoint(parse_positions(coordinates)?),
        "LineString" => Geometry::LineString(parse_line(coordinates)?),
        "MultiLineString" => Geometry::MultiLineString(parse_each(coordinates, parse_line)?),
        "Polygon" => {
            let polygon = parse_polygon(coordinates)?;
            Geometry::Polygon {
                exterior: polygon.exterior,
                holes: polygon.holes,
            }
        }
        "MultiPolygon" => Geometry::MultiPolygon(parse_each(coordinates, parse_polygon)?),
        other => return Err(WktError::UnsupportedType(other.to_string())),
    };

    Ok(vec![geometry])
}

fn parse_each<T>(
    value: &Value,
    parse_member: fn(&Value) -> Result<T, WktError>,
) -> Result<Vec<T>, WktError> {
    let members = value.as_array().ok_or(WktError::InvalidCoordinates)?;
    members.iter().map(parse_member).collect()
}

fn parse_position(value: &Value) -> Result<Point, WktError> {
    let ordinates = value.as_array().ok_or(WktError::InvalidCoordinates)?;
    if ordinates.len() < 2 {
        return Err(WktError::InvalidCoordinates);
    }
    let x = ordinates[0].as_f64().ok_or(WktError::InvalidCoordinates)?;
    let y = ordinates[1].as_f64().ok_or(WktError::InvalidCoordinates)?;
    Ok(Point { x, y })
}

fn parse_positions(value: &Value) -> Result<Vec<Point>, WktError> {
    parse_each(value, parse_position)
}

fn parse_line(value: &Value) -> Result<Vec<Point>, WktError> {
    let positions = parse_positions(value)?;
    if positions.len() < MINIMUM_LINE_POSITIONS {
        return Err(WktError::InvalidGeometry(format!(
            "LineString needs at least {MINIMUM_LINE_POSITIONS} positions"
        )));
    }
    Ok(positions)
}

fn parse_ring(value: &Value) -> Result<Vec<Point>, WktError> {
    let positions = parse_positions(value)?;
    if positions.len() < MINIMUM_RING_POSITIONS {
        return Err(WktError::InvalidGeometry(format!(
            "Polygon ring needs at least {MINIMUM_RING_POSITIONS} positions"
        )));
    }
    Ok(positions)
}

fn parse_polygon(value: &Value) -> Result<PolygonGeom, WktError> {
    let rings = value.as_array().ok_or(WktError::InvalidCoordinates)?;
    let (exterior, holes) = rings
        .split_first()
        .ok_or_else(|| WktError::InvalidGeometry("Polygon has no rings".to_string()))?;
    Ok(PolygonGeom {
        exterior: parse_ring(exterior)?,
        holes: holes
            .iter()
            .map(parse_ring)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_one(value: serde_json::Value) -> Geometry {
        let mut geometries = parse_geojson_geometry(&value).unwrap();
        assert_eq!(geometries.len(), 1);
        geometries.remove(0)
    }

    #[test]
    fn point() {
        let geometry = parse_one(json!({"type": "Point", "coordinates": [1.5, 2.5]}));
        assert_eq!(geometry, Geometry::Point(Point { x: 1.5, y: 2.5 }));
    }

    #[test]
    fn a_third_ordinate_is_ignored() {
        let geometry = parse_one(json!({"type": "Point", "coordinates": [1.0, 2.0, 300.0]}));
        assert_eq!(geometry, Geometry::Point(Point { x: 1.0, y: 2.0 }));
    }

    #[test]
    fn linestring() {
        let geometry = parse_one(json!({"type": "LineString", "coordinates": [[0, 0], [1, 1]]}));
        assert_eq!(
            geometry,
            Geometry::LineString(vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }])
        );
    }

    #[test]
    fn multipoint() {
        let geometry = parse_one(json!({"type": "MultiPoint", "coordinates": [[0, 0], [1, 1]]}));
        assert_eq!(
            geometry,
            Geometry::MultiPoint(vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }])
        );
    }

    #[test]
    fn multilinestring() {
        let geometry = parse_one(json!({
            "type": "MultiLineString",
            "coordinates": [[[0, 0], [1, 1]], [[2, 2], [3, 3], [4, 4]]]
        }));
        match geometry {
            Geometry::MultiLineString(lines) => {
                assert_eq!(lines.len(), 2);
                assert_eq!(lines[1].len(), 3);
            }
            other => panic!("expected MultiLineString, got {other:?}"),
        }
    }

    #[test]
    fn the_first_polygon_ring_is_the_exterior_and_the_rest_are_holes() {
        let geometry = parse_one(json!({
            "type": "Polygon",
            "coordinates": [
                [[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]],
                [[2, 2], [4, 2], [4, 4], [2, 2]],
                [[6, 6], [8, 6], [8, 8], [6, 6]]
            ]
        }));
        match geometry {
            Geometry::Polygon { exterior, holes } => {
                assert_eq!(exterior[1], Point { x: 10.0, y: 0.0 });
                assert_eq!(holes.len(), 2);
                assert_eq!(holes[0][0], Point { x: 2.0, y: 2.0 });
                assert_eq!(holes[1][0], Point { x: 6.0, y: 6.0 });
            }
            other => panic!("expected Polygon, got {other:?}"),
        }
    }

    #[test]
    fn multipolygon() {
        let geometry = parse_one(json!({
            "type": "MultiPolygon",
            "coordinates": [
                [[[0, 0], [1, 0], [1, 1], [0, 0]]],
                [[[5, 5], [6, 5], [6, 6], [5, 5]], [[5, 5], [5, 6], [6, 6], [5, 5]]]
            ]
        }));
        match geometry {
            Geometry::MultiPolygon(polygons) => {
                assert_eq!(polygons.len(), 2);
                assert!(polygons[0].holes.is_empty());
                assert_eq!(polygons[1].holes.len(), 1);
            }
            other => panic!("expected MultiPolygon, got {other:?}"),
        }
    }

    #[test]
    fn a_geometry_collection_flattens_to_one_geometry_per_member() {
        let geometries = parse_geojson_geometry(&json!({
            "type": "GeometryCollection",
            "geometries": [
                {"type": "Point", "coordinates": [0, 0]},
                {"type": "GeometryCollection", "geometries": [
                    {"type": "LineString", "coordinates": [[0, 0], [1, 1]]},
                    {"type": "Point", "coordinates": [2, 2]}
                ]}
            ]
        }))
        .unwrap();
        assert_eq!(geometries.len(), 3);
        assert_eq!(geometries[0], Geometry::Point(Point { x: 0.0, y: 0.0 }));
        assert!(matches!(geometries[1], Geometry::LineString(_)));
        assert_eq!(geometries[2], Geometry::Point(Point { x: 2.0, y: 2.0 }));
    }

    #[test]
    fn an_unknown_type_is_named_in_the_error() {
        let error =
            parse_geojson_geometry(&json!({"type": "Circle", "coordinates": [0, 0]})).unwrap_err();
        assert!(error.to_string().contains("Circle"), "got {error}");
    }

    #[test]
    fn a_position_with_one_ordinate_is_an_error() {
        let error = parse_geojson_geometry(&json!({"type": "Point", "coordinates": [1]}));
        assert!(matches!(error, Err(WktError::InvalidCoordinates)));
    }

    #[test]
    fn a_non_numeric_ordinate_is_an_error() {
        let error = parse_geojson_geometry(&json!({"type": "Point", "coordinates": ["1", "2"]}));
        assert!(matches!(error, Err(WktError::InvalidCoordinates)));
    }

    #[test]
    fn a_one_position_linestring_is_an_error_naming_the_type() {
        let error = parse_geojson_geometry(&json!({"type": "LineString", "coordinates": [[0, 0]]}))
            .unwrap_err();
        assert!(error.to_string().contains("LineString"), "got {error}");
    }

    #[test]
    fn a_three_position_ring_is_an_error_naming_the_type() {
        let error = parse_geojson_geometry(&json!({
            "type": "Polygon",
            "coordinates": [[[0, 0], [1, 0], [0, 0]]]
        }))
        .unwrap_err();
        assert!(error.to_string().contains("Polygon ring"), "got {error}");
    }

    #[test]
    fn a_polygon_with_no_rings_is_an_error() {
        let error =
            parse_geojson_geometry(&json!({"type": "Polygon", "coordinates": []})).unwrap_err();
        assert!(error.to_string().contains("no rings"), "got {error}");
    }

    #[test]
    fn a_geometry_without_a_type_is_an_error() {
        let error = parse_geojson_geometry(&json!({"coordinates": [0, 0]})).unwrap_err();
        assert!(error.to_string().contains("missing type"), "got {error}");
    }

    #[test]
    fn a_geometry_without_coordinates_is_an_error_naming_the_type() {
        let error = parse_geojson_geometry(&json!({"type": "LineString"})).unwrap_err();
        assert!(error.to_string().contains("LineString"), "got {error}");
    }
}
