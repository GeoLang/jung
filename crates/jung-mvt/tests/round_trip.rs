use jung_mvt::{
    AttributeValue, DEFAULT_EXTENT, MvtFeature, MvtGeometry, MvtLayer, MvtPolygon, MvtValue,
    TileGeometry, TilePolygon, VectorLayer, decode_tile, encode_tile,
};
use std::collections::HashMap;

// a bbox the size of the extent makes tile x equal source x and tile y equal 4096 minus source y
const BBOX: [f64; 4] = [0.0, 0.0, 4096.0, 4096.0];

fn feature(geometry: MvtGeometry, properties: &[(&str, MvtValue)]) -> MvtFeature {
    MvtFeature {
        id: Some(1),
        geometry,
        properties: properties
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect(),
    }
}

fn polygon(exterior: &[[f64; 2]], holes: &[&[[f64; 2]]]) -> MvtPolygon {
    MvtPolygon {
        exterior: exterior.to_vec(),
        holes: holes.iter().map(|hole| hole.to_vec()).collect(),
    }
}

fn reversed(ring: &[[f64; 2]]) -> Vec<[f64; 2]> {
    ring.iter().rev().copied().collect()
}

fn rfc_7946_polygons() -> Vec<MvtPolygon> {
    // exteriors run counterclockwise and holes clockwise with y pointing north
    vec![
        polygon(
            &[
                [0.0, 2096.0],
                [2000.0, 2096.0],
                [2000.0, 4096.0],
                [0.0, 4096.0],
                [0.0, 2096.0],
            ],
            &[&[
                [500.0, 3596.0],
                [1500.0, 3596.0],
                [1500.0, 2596.0],
                [500.0, 2596.0],
                [500.0, 3596.0],
            ]],
        ),
        // quantizes to a single point, so it and its hole are dropped
        polygon(
            &[
                [10.2, 10.2],
                [10.4, 10.2],
                [10.4, 10.4],
                [10.2, 10.4],
                [10.2, 10.2],
            ],
            &[&[
                [3000.0, 100.0],
                [3100.0, 100.0],
                [3100.0, 200.0],
                [3000.0, 200.0],
                [3000.0, 100.0],
            ]],
        ),
        polygon(
            &[
                [2500.0, 2096.0],
                [3500.0, 2096.0],
                [3500.0, 3096.0],
                [2500.0, 3096.0],
                [2500.0, 2096.0],
            ],
            &[],
        ),
        // three points on one line enclose no area, so it is dropped
        polygon(
            &[
                [100.0, 100.0],
                [200.0, 100.0],
                [300.0, 100.0],
                [100.0, 100.0],
            ],
            &[],
        ),
    ]
}

fn opposite_winding_polygons() -> Vec<MvtPolygon> {
    rfc_7946_polygons()
        .iter()
        .map(|p| MvtPolygon {
            exterior: reversed(&p.exterior),
            holes: p.holes.iter().map(|hole| reversed(hole)).collect(),
        })
        .collect()
}

fn encoded_and_decoded_layer() -> VectorLayer {
    let layer = MvtLayer {
        name: "water".to_string(),
        extent: DEFAULT_EXTENT,
        features: vec![
            feature(
                MvtGeometry::Point(vec![[100.0, 3996.0], [200.0, 3946.0]]),
                &[
                    ("name", MvtValue::String("spring".into())),
                    ("depth", MvtValue::Float(1.5)),
                    ("flow", MvtValue::Double(0.25)),
                    ("level", MvtValue::Int(-7)),
                    ("visits", MvtValue::UInt(9)),
                    ("public", MvtValue::Bool(true)),
                ],
            ),
            feature(
                MvtGeometry::LineString(vec![vec![
                    [0.0, 4096.0],
                    [1000.0, 4096.0],
                    [1000.0, 3096.0],
                ]]),
                &[("name", MvtValue::String("brook".into()))],
            ),
            feature(
                MvtGeometry::Polygon(rfc_7946_polygons()),
                &[("name", MvtValue::String("pond".into()))],
            ),
            feature(
                MvtGeometry::Polygon(opposite_winding_polygons()),
                &[("name", MvtValue::String("lake".into()))],
            ),
        ],
    };

    let mut tile = decode_tile(&encode_tile(&[layer], BBOX)).unwrap();
    assert_eq!(tile.layers.len(), 1);
    tile.layers.remove(0)
}

#[test]
fn test_decoded_geometry_is_the_encoded_geometry_in_tile_units() {
    let layer = encoded_and_decoded_layer();
    assert_eq!(layer.name, "water");
    assert_eq!(layer.extent, DEFAULT_EXTENT);
    let polygons = TileGeometry::Polygons(vec![
        TilePolygon {
            exterior: vec![
                [0.0, 2000.0],
                [0.0, 0.0],
                [2000.0, 0.0],
                [2000.0, 2000.0],
                [0.0, 2000.0],
            ],
            holes: vec![vec![
                [500.0, 500.0],
                [500.0, 1500.0],
                [1500.0, 1500.0],
                [1500.0, 500.0],
                [500.0, 500.0],
            ]],
        },
        TilePolygon {
            exterior: vec![
                [2500.0, 2000.0],
                [2500.0, 1000.0],
                [3500.0, 1000.0],
                [3500.0, 2000.0],
                [2500.0, 2000.0],
            ],
            holes: Vec::new(),
        },
    ]);
    let geometries: Vec<&TileGeometry> = layer.features.iter().map(|f| &f.geometry).collect();
    assert_eq!(
        geometries,
        [
            &TileGeometry::Points(vec![[100.0, 100.0], [200.0, 150.0]]),
            &TileGeometry::Lines(vec![vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0]]]),
            &polygons,
            &polygons,
        ]
    );
}

#[test]
fn test_decoded_properties_are_the_encoded_properties() {
    let layer = encoded_and_decoded_layer();
    let expected: HashMap<String, AttributeValue> = [
        ("name", AttributeValue::String("spring".into())),
        ("depth", AttributeValue::Number(1.5)),
        ("flow", AttributeValue::Number(0.25)),
        ("level", AttributeValue::Integer(-7)),
        ("visits", AttributeValue::Integer(9)),
        ("public", AttributeValue::Boolean(true)),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect();
    assert_eq!(layer.features[0].attributes, expected);

    let names: Vec<Option<&AttributeValue>> = layer.features[1..]
        .iter()
        .map(|f| f.attributes.get("name"))
        .collect();
    assert_eq!(
        names,
        [
            Some(&AttributeValue::String("brook".into())),
            Some(&AttributeValue::String("pond".into())),
            Some(&AttributeValue::String("lake".into())),
        ]
    );
}
