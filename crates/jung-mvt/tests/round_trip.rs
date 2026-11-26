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

fn encoded(features: Vec<MvtFeature>) -> Vec<u8> {
    let layer = MvtLayer {
        name: "edges".to_string(),
        extent: DEFAULT_EXTENT,
        features,
    };
    encode_tile(&[layer], BBOX)
}

fn round_trip(features: Vec<MvtFeature>) -> VectorLayer {
    let mut tile = decode_tile(&encoded(features)).unwrap();
    tile.layers.remove(0)
}

#[test]
fn test_fractional_coordinates_round_to_the_nearest_tile_unit() {
    let layer = round_trip(vec![feature(
        MvtGeometry::Point(vec![[10.7, 4096.0 - 20.6], [-0.7, 4096.0 + 1.6]]),
        &[],
    )]);
    assert_eq!(
        layer.features[0].geometry,
        TileGeometry::Points(vec![[11.0, 21.0], [-1.0, -2.0]])
    );
}

#[test]
fn test_line_points_that_quantize_together_are_written_once() {
    let name = [("name", MvtValue::String("brook".into()))];
    let with_repeats = feature(
        MvtGeometry::LineString(vec![
            vec![[0.0, 4096.0], [0.2, 4096.0], [1000.0, 4096.0]],
            // both points land on tile unit (5, 4091)
            vec![[5.2, 5.2], [5.4, 5.3]],
        ]),
        &name,
    );
    let without = feature(
        MvtGeometry::LineString(vec![vec![[0.0, 4096.0], [1000.0, 4096.0]]]),
        &name,
    );
    assert_eq!(encoded(vec![with_repeats]), encoded(vec![without]));
}

#[test]
fn test_features_left_without_geometry_are_not_written() {
    let brook = feature(
        MvtGeometry::LineString(vec![vec![[0.0, 4096.0], [1000.0, 4096.0]]]),
        &[("name", MvtValue::String("brook".into()))],
    );
    let features = vec![
        feature(
            MvtGeometry::Point(Vec::new()),
            &[("empty", MvtValue::Bool(true))],
        ),
        brook.clone(),
        feature(
            MvtGeometry::LineString(vec![vec![[5.2, 5.2], [5.4, 5.3]]]),
            &[("short", MvtValue::Int(2))],
        ),
        feature(
            MvtGeometry::Polygon(vec![rfc_7946_polygons().swap_remove(1)]),
            &[("tiny", MvtValue::Double(0.5))],
        ),
    ];
    assert_eq!(encoded(features), encoded(vec![brook]));
}

#[test]
fn test_geometry_past_the_coordinate_limit_is_clipped() {
    // mvt 2.1 caps geometry deltas at 2^31 - 1
    let coordinate_limit = 2f32.powi(30);
    let far = 1e10;
    let layer = round_trip(vec![
        feature(
            MvtGeometry::Point(vec![[100.0, 3996.0], [far, 3996.0]]),
            &[],
        ),
        // in tile units the line runs along y = 2048 + x / 2
        feature(
            MvtGeometry::LineString(vec![vec![
                [-far, 2048.0 + far / 2.0],
                [far, 2048.0 - far / 2.0],
            ]]),
            &[],
        ),
        feature(
            MvtGeometry::Polygon(vec![polygon(
                &[
                    [-far, -far],
                    [far, -far],
                    [far, far],
                    [-far, far],
                    [-far, -far],
                ],
                &[],
            )]),
            &[],
        ),
    ]);

    assert_eq!(
        layer.features[0].geometry,
        TileGeometry::Points(vec![[100.0, 100.0]])
    );

    let TileGeometry::Lines(lines) = &layer.features[1].geometry else {
        panic!("not a line: {:?}", layer.features[1].geometry);
    };
    assert_eq!(lines.len(), 1, "{lines:?}");
    let [start, end] = lines[0][..] else {
        panic!("not two points: {lines:?}");
    };
    assert!(start[0] < 0.0 && end[0] > 4096.0, "{lines:?}");
    for point in [start, end] {
        assert!(point[0].abs() <= coordinate_limit, "{lines:?}");
        // decoded coordinates are f32, which steps by 128 near 2^30
        assert!(
            (point[1] - 2048.0 - point[0] / 2.0).abs() < 256.0,
            "{lines:?}"
        );
    }

    let TileGeometry::Polygons(polygons) = &layer.features[2].geometry else {
        panic!("not a polygon: {:?}", layer.features[2].geometry);
    };
    assert_eq!(polygons.len(), 1, "{polygons:?}");
    let exterior = &polygons[0].exterior;
    assert!(
        exterior
            .iter()
            .flatten()
            .all(|coordinate| coordinate.abs() <= coordinate_limit),
        "{exterior:?}"
    );
    for axis in 0..2 {
        let coordinates = || exterior.iter().map(|vertex| vertex[axis]);
        assert!(
            coordinates().fold(f32::INFINITY, f32::min) < 0.0,
            "{exterior:?}"
        );
        assert!(
            coordinates().fold(f32::NEG_INFINITY, f32::max) > 4096.0,
            "{exterior:?}"
        );
    }
}
