//! Mapbox Vector Tile (MVT) decoding.
//!
//! Decodes tiles with [`jung_mvt`] and normalises their tile units into the
//! engine's coordinate space: 0 to 1 across the tile with y increasing
//! northward.

use crate::geometry::{Feature, Geometry, Point, PolygonGeom};
use jung_mvt::{AttributeValue, TileGeometry};
use jung_style::PropertyValue;

pub use jung_mvt::Error as MvtError;

/// A decoded vector tile.
#[derive(Debug, Clone)]
pub struct VectorTile {
    pub layers: Vec<TileLayer>,
}

/// A layer within a vector tile.
#[derive(Debug, Clone)]
pub struct TileLayer {
    pub name: String,
    pub extent: u32,
    pub features: Vec<Feature>,
}

/// Decode a vector tile from raw protobuf bytes.
pub fn decode_tile(data: &[u8]) -> Result<VectorTile, MvtError> {
    let tile = jung_mvt::decode_tile(data)?;
    Ok(VectorTile {
        layers: tile.layers.into_iter().map(convert_layer).collect(),
    })
}

fn convert_layer(layer: jung_mvt::VectorLayer) -> TileLayer {
    let jung_mvt::VectorLayer {
        name,
        extent,
        features,
    } = layer;

    let features = features
        .into_iter()
        .map(|feature| Feature {
            geometry: convert_geometry(feature.geometry, extent),
            properties: feature
                .attributes
                .into_iter()
                .map(|(key, value)| (key, convert_value(value)))
                .collect(),
        })
        .collect();

    TileLayer {
        name,
        extent,
        features,
    }
}

fn convert_geometry(geometry: TileGeometry, extent: u32) -> Geometry {
    match geometry {
        TileGeometry::Points(points) => {
            let points = convert_ring(points, extent);
            match points[..] {
                [point] => Geometry::Point(point),
                _ => Geometry::MultiPoint(points),
            }
        }
        TileGeometry::Lines(lines) => {
            let mut lines: Vec<Vec<Point>> = lines
                .into_iter()
                .map(|line| convert_ring(line, extent))
                .collect();
            match lines.len() {
                1 => Geometry::LineString(lines.remove(0)),
                _ => Geometry::MultiLineString(lines),
            }
        }
        TileGeometry::Polygons(polygons) => {
            let mut polygons: Vec<PolygonGeom> = polygons
                .into_iter()
                .map(|polygon| PolygonGeom {
                    exterior: convert_ring(polygon.exterior, extent),
                    holes: polygon
                        .holes
                        .into_iter()
                        .map(|hole| convert_ring(hole, extent))
                        .collect(),
                })
                .collect();
            match polygons.len() {
                1 => {
                    let PolygonGeom { exterior, holes } = polygons.remove(0);
                    Geometry::Polygon { exterior, holes }
                }
                _ => Geometry::MultiPolygon(polygons),
            }
        }
    }
}

fn convert_ring(ring: Vec<[f32; 2]>, extent: u32) -> Vec<Point> {
    let extent = f64::from(extent);
    ring.into_iter()
        .map(|[x, y]| Point {
            x: f64::from(x) / extent,
            y: 1.0 - f64::from(y) / extent,
        })
        .collect()
}

fn convert_value(value: AttributeValue) -> PropertyValue {
    match value {
        AttributeValue::String(text) => PropertyValue::String(text),
        AttributeValue::Number(number) => PropertyValue::Number(number),
        AttributeValue::Integer(number) => PropertyValue::Integer(number),
        AttributeValue::Boolean(flag) => PropertyValue::Boolean(flag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jung_mvt::TilePolygon;

    const EXTENT: u32 = 4096;

    fn ring(points: &[(f32, f32)]) -> Vec<[f32; 2]> {
        points.iter().map(|&(x, y)| [x, y]).collect()
    }

    #[test]
    fn point_is_normalised_with_y_flipped() {
        let geometry = convert_geometry(TileGeometry::Points(ring(&[(1024.0, 1024.0)])), EXTENT);
        assert_eq!(geometry, Geometry::Point(Point { x: 0.25, y: 0.75 }));
    }

    #[test]
    fn several_points_become_a_multipoint() {
        let geometry = convert_geometry(
            TileGeometry::Points(ring(&[(0.0, 0.0), (4096.0, 4096.0)])),
            EXTENT,
        );
        assert_eq!(
            geometry,
            Geometry::MultiPoint(vec![Point { x: 0.0, y: 1.0 }, Point { x: 1.0, y: 0.0 },])
        );
    }

    #[test]
    fn one_line_is_a_linestring_and_two_are_a_multilinestring() {
        let line = ring(&[(0.0, 0.0), (2048.0, 0.0)]);
        let single = convert_geometry(TileGeometry::Lines(vec![line.clone()]), EXTENT);
        assert_eq!(
            single,
            Geometry::LineString(vec![Point { x: 0.0, y: 1.0 }, Point { x: 0.5, y: 1.0 },])
        );

        let pair = convert_geometry(TileGeometry::Lines(vec![line.clone(), line]), EXTENT);
        assert!(matches!(pair, Geometry::MultiLineString(lines) if lines.len() == 2));
    }

    #[test]
    fn polygon_holes_are_normalised_too() {
        let geometry = convert_geometry(
            TileGeometry::Polygons(vec![TilePolygon {
                exterior: ring(&[(0.0, 0.0), (4096.0, 0.0), (4096.0, 4096.0), (0.0, 0.0)]),
                holes: vec![ring(&[
                    (1024.0, 1024.0),
                    (2048.0, 1024.0),
                    (2048.0, 2048.0),
                    (1024.0, 1024.0),
                ])],
            }]),
            EXTENT,
        );
        let Geometry::Polygon { exterior, holes } = geometry else {
            panic!("expected a polygon");
        };
        assert_eq!(exterior[0], Point { x: 0.0, y: 1.0 });
        assert_eq!(holes.len(), 1);
        assert_eq!(holes[0][0], Point { x: 0.25, y: 0.75 });
    }

    /// Two exterior rings are two polygons, not one polygon with a hole.
    #[test]
    fn two_polygons_become_a_multipolygon() {
        let part = |offset: f32| TilePolygon {
            exterior: ring(&[
                (offset, offset),
                (offset + 100.0, offset),
                (offset + 100.0, offset + 100.0),
                (offset, offset),
            ]),
            holes: Vec::new(),
        };
        let geometry = convert_geometry(
            TileGeometry::Polygons(vec![part(0.0), part(1000.0)]),
            EXTENT,
        );
        let Geometry::MultiPolygon(polygons) = geometry else {
            panic!("expected a multipolygon");
        };
        assert_eq!(polygons.len(), 2);
        assert!(polygons.iter().all(|polygon| polygon.holes.is_empty()));
    }

    #[test]
    fn decode_minimal_tile() {
        let tile = decode_tile(&build_test_tile()).unwrap();
        assert_eq!(tile.layers.len(), 1);
        assert_eq!(tile.layers[0].name, "test");
        assert_eq!(tile.layers[0].extent, EXTENT);
        assert_eq!(tile.layers[0].features.len(), 1);

        let feature = &tile.layers[0].features[0];
        assert_eq!(
            feature.geometry,
            Geometry::Point(Point {
                x: 10.0 / 4096.0,
                y: 1.0 - 10.0 / 4096.0,
            })
        );
        assert_eq!(
            feature.properties.get("name"),
            Some(&PropertyValue::String("hello".to_string()))
        );
    }

    /// Build a minimal valid MVT tile with one layer, one point feature.
    fn build_test_tile() -> Vec<u8> {
        let mut tile = Vec::new();

        // Build layer
        let mut layer = vec![
            0x0A, // tag: field 1, wire type 2 (name)
            4,    // length
            b't', b'e', b's', b't', 0x1A, // tag: field 3, wire type 2 (keys)
            4,    // length
            b'n', b'a', b'm', b'e',
        ];

        // field 4: values = string "hello"
        let value = vec![
            0x0A, // field 1, wire type 2 (string_value)
            5, b'h', b'e', b'l', b'l', b'o',
        ];
        layer.push(0x22); // tag: field 4, wire type 2
        layer.push(value.len() as u8);
        layer.extend_from_slice(&value);

        // field 2: feature
        let mut feature = vec![
            0x12, // field 2, wire type 2 (tags packed)
            2,    // length
            0,    // key index 0
            0,    // value index 0
            0x18, // field 3, wire type 0 (type)
            1,    // POINT
        ];

        // geometry: MoveTo(1) x=10 y=10
        // zigzag(10) = 20, which fits in one varint byte
        let mut geom = Vec::new();
        encode_varint(&mut geom, 9); // MoveTo(1): (1 << 3) | 1 = 9
        encode_varint(&mut geom, 20); // zigzag(10) = 20
        encode_varint(&mut geom, 20); // zigzag(10) = 20
        feature.push(0x22); // field 4, wire type 2
        encode_varint(&mut feature, geom.len() as u64);
        feature.extend_from_slice(&geom);

        layer.push(0x12); // tag: field 2, wire type 2
        encode_varint(&mut layer, feature.len() as u64);
        layer.extend_from_slice(&feature);

        // field 5: extent = 4096
        layer.push(0x28); // field 5, wire type 0
        encode_varint(&mut layer, 4096);

        // Tile: field 3 = layer
        tile.push(0x1A); // tag: field 3, wire type 2
        encode_varint(&mut tile, layer.len() as u64);
        tile.extend_from_slice(&layer);

        tile
    }

    fn encode_varint(buf: &mut Vec<u8>, mut value: u64) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                buf.push(byte);
                break;
            }
            buf.push(byte | 0x80);
        }
    }
}
