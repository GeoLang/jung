//! Three layers over one tile, encoded by the mapbox-vector-tile Python
//! reference implementation, so the decoder is checked against bytes it did
//! not produce.

use jung_mvt::{AttributeValue, TileGeometry, decode_tile};

const SAMPLE_TILE: &[u8] = include_bytes!("fixtures/sample.mvt");

#[test]
fn test_decodes_layers_geometry_and_attributes() {
    let tile = decode_tile(SAMPLE_TILE).unwrap();
    let names: Vec<&str> = tile.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["places", "roads", "water"]);

    let places = tile.layer("places").unwrap();
    assert_eq!(places.extent, 4096);
    assert_eq!(
        places.features[0].geometry,
        TileGeometry::Points(vec![[25.0, 17.0]])
    );
    assert_eq!(
        places.features[0].attributes.get("name"),
        Some(&AttributeValue::String("London".into()))
    );
    assert_eq!(
        places.features[0].attributes.get("population"),
        Some(&AttributeValue::Integer(8_982_000))
    );

    assert_eq!(
        tile.layer("roads").unwrap().features[0].geometry,
        TileGeometry::Lines(vec![vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]]])
    );
    assert_eq!(
        tile.layer("water").unwrap().features[0]
            .attributes
            .get("depth"),
        Some(&AttributeValue::Number(3.5))
    );
}

/// The encoder winds the outer ring one way and the hole the other, which is
/// the only thing separating a hole from a second polygon.
#[test]
fn test_decodes_a_polygon_hole_by_winding() {
    let tile = decode_tile(SAMPLE_TILE).unwrap();
    let TileGeometry::Polygons(polygons) = &tile.layer("water").unwrap().features[0].geometry
    else {
        panic!("expected polygons");
    };
    assert_eq!(polygons.len(), 1);
    assert_eq!(polygons[0].holes.len(), 1);
    assert_eq!(polygons[0].exterior.first(), polygons[0].exterior.last());
}
