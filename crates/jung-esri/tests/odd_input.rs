//! drawingInfo comes off the wire, so every odd shape has to end as a loss or a skip

use jung_esri::{Geometry, Source, translate};
use serde_json::{Value, json};

fn source() -> Source {
    Source {
        source: "src".to_string(),
        source_layer: "layer".to_string(),
    }
}

#[test]
fn empty_object_emits_nothing() {
    let out = translate(&json!({}), &source(), Geometry::Point);
    assert!(out.layers.is_empty());
    assert_eq!(out.losses.len(), 1);
    assert_eq!(out.losses[0].path, "renderer");
}

#[test]
fn non_objects_are_refused() {
    for value in [
        json!(null),
        json!([]),
        json!("drawingInfo"),
        json!(7),
        json!(true),
    ] {
        let out = translate(&value, &source(), Geometry::Polygon);
        assert!(out.layers.is_empty());
        assert_eq!(out.losses[0].path, "drawingInfo");
    }
}

#[test]
fn all_nulls_emit_nothing() {
    let drawing_info = json!({
        "renderer": null,
        "transparency": null,
        "labelingInfo": null,
        "showLabels": null
    });
    let out = translate(&drawing_info, &source(), Geometry::Line);
    assert!(out.layers.is_empty());
    assert_eq!(out.losses.len(), 1);
}

#[test]
fn wrong_types_everywhere_never_panic() {
    let cases = [
        json!({ "renderer": [] }),
        json!({ "renderer": "simple" }),
        json!({ "renderer": { "type": 7 } }),
        json!({ "renderer": { "type": "simple", "symbol": 7 } }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriSMS", "color": "#ff0000", "size": "big" } } }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriSMS", "color": [1, 2, 3, 4, 5], "size": 3 } } }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriSFS", "outline": 3 } } }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriSLS", "style": 9, "width": [] } } }),
        json!({ "renderer": { "type": "uniqueValue" } }),
        json!({ "renderer": { "type": "uniqueValue", "field1": "A", "uniqueValueInfos": {} } }),
        json!({ "renderer": { "type": "uniqueValue", "field1": "A", "uniqueValueInfos": [null, 3, { "value": null }, { "value": [] }] } }),
        json!({ "renderer": { "type": "uniqueValue", "field1": "A", "field3": "C", "uniqueValueInfos": [] } }),
        json!({ "renderer": { "type": "classBreaks" } }),
        json!({ "renderer": { "type": "classBreaks", "field": "A", "classBreakInfos": "three" } }),
        json!({ "renderer": { "type": "classBreaks", "field": "A", "classBreakInfos": [] } }),
        json!({ "renderer": { "type": "classBreaks", "field": "A", "classBreakInfos": [null, null] } }),
        json!({ "renderer": { "type": "simple", "symbol": null }, "transparency": "half" }),
        json!({ "renderer": { "type": "simple", "symbol": null }, "labelingInfo": {} }),
        json!({ "renderer": { "type": "simple", "symbol": null }, "labelingInfo": [] }),
        json!({ "renderer": { "type": "simple", "symbol": null }, "labelingInfo": [null] }),
        json!({ "renderer": { "type": "simple", "symbol": null }, "labelingInfo": [{ "labelExpression": "[UNCLOSED", "symbol": 4 }] }),
        json!({ "renderer": { "type": "simple", "symbol": null }, "labelingInfo": [{ "labelExpression": "[A]", "symbol": { "type": "esriSFS" } }] }),
        json!({ "renderer": { "type": "simple", "symbol": null }, "labelingInfo": [{ "labelExpression": "[A]", "labelPlacement": "esriServerPointLabelPlacementNowhere" }] }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriPMS" } } }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriPFS", "url": "" } } }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriPMS", "imageData": 7, "contentType": [], "width": "x", "height": null } } }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriPMS", "imageData": "iVBOR", "contentType": "image/png" } } }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriPMS", "imageData": "%%%", "contentType": "../../etc/passwd", "width": -3, "height": 1e308, "angle": "spin" } } }),
        json!({ "renderer": { "type": "simple", "symbol": { "type": "esriPFS", "imageData": "iVBOR", "contentType": "image/png", "width": 8, "height": 8, "angle": 45, "outline": {} } } }),
        json!({ "renderer": { "type": "uniqueValue", "field1": "A", "uniqueValueInfos": [
            { "value": "a", "symbol": { "type": "esriPMS", "imageData": "iVBOR", "contentType": "image/png", "width": 0, "height": 0 } },
            { "value": "b", "symbol": { "type": "esriPFS", "imageData": "iVBOR", "contentType": "image/png", "width": 8, "height": 8 } }
        ] } }),
    ];
    for (geometry, name) in [
        (Geometry::Point, "point"),
        (Geometry::Line, "line"),
        (Geometry::Polygon, "polygon"),
    ] {
        for case in &cases {
            let out = translate(case, &source(), geometry);
            assert!(!out.losses.is_empty(), "{name} {case} translated silently");
            for layer in &out.layers {
                assert!(
                    layer["id"].is_string(),
                    "{name} {case} emitted a layer without an id"
                );
            }
            for (image_name, image) in &out.images {
                assert!(
                    image.data_uri.starts_with("data:"),
                    "{name} {case} named {image_name} without a data uri"
                );
                assert!(
                    image.width > 0.0 && image.height > 0.0,
                    "{name} {case} registered {image_name} at a useless size"
                );
            }
        }
    }
}

#[test]
fn transparency_is_clamped() {
    let drawing_info = |transparency: Value| {
        json!({
            "renderer": {
                "type": "simple",
                "symbol": { "type": "esriSFS", "style": "esriSFSSolid", "color": [1, 2, 3, 255] }
            },
            "transparency": transparency
        })
    };
    let out = translate(&drawing_info(json!(150)), &source(), Geometry::Polygon);
    assert_eq!(out.layers[0]["paint"]["fill-opacity"], json!(0.0));

    // at or below zero is fully opaque, so no opacity property at all
    let out = translate(&drawing_info(json!(-20)), &source(), Geometry::Polygon);
    assert!(out.layers[0]["paint"].get("fill-opacity").is_none());

    let out = translate(&drawing_info(json!("half")), &source(), Geometry::Polygon);
    assert_eq!(out.losses[0].path, "transparency");
}

#[test]
fn huge_unique_value_list_is_translated_whole() {
    let infos: Vec<Value> = (0..5000)
        .map(|i| {
            json!({
                "value": format!("v{i}"),
                "symbol": { "type": "esriSMS", "style": "esriSMSCircle", "color": [i % 256, 0, 0, 255], "size": 4 }
            })
        })
        .collect();
    let drawing_info = json!({
        "renderer": {
            "type": "uniqueValue",
            "field1": "KIND",
            "defaultSymbol": { "type": "esriSMS", "style": "esriSMSCircle", "color": [0, 0, 0, 255], "size": 4 },
            "uniqueValueInfos": infos
        }
    });
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert_eq!(out.losses, vec![]);
    let color = out.layers[0]["paint"]["circle-color"].as_array().unwrap();
    // match, input, 5000 value and output pairs, fallback
    assert_eq!(color.len(), 2 + 5000 * 2 + 1);
    // every branch has the same size, so the radius collapses to one literal
    assert_eq!(out.layers[0]["paint"]["circle-radius"], json!(2.67));
}

#[test]
fn huge_class_break_list_is_translated_whole() {
    let infos: Vec<Value> = (0..2000)
        .map(|i| {
            json!({
                "classMinValue": i,
                "classMaxValue": i + 1,
                "symbol": { "type": "esriSLS", "style": "esriSLSSolid", "color": [0, 0, 0, 255], "width": i % 7 }
            })
        })
        .collect();
    let drawing_info = json!({
        "renderer": { "type": "classBreaks", "field": "V", "minValue": 0, "classBreakInfos": infos }
    });
    let out = translate(&drawing_info, &source(), Geometry::Line);
    let width = out.layers[0]["paint"]["line-width"].as_array().unwrap();
    // step, input, default, 2000 threshold and output pairs
    assert_eq!(width.len(), 3 + 2000 * 2);
}

#[test]
fn descending_class_breaks_are_refused() {
    let drawing_info = json!({
        "renderer": {
            "type": "classBreaks",
            "field": "V",
            "minValue": 100,
            "classBreakInfos": [
                { "classMinValue": 100, "classMaxValue": 200, "symbol": { "type": "esriSLS", "color": [0, 0, 0, 255], "width": 1 } },
                { "classMinValue": 10, "classMaxValue": 20, "symbol": { "type": "esriSLS", "color": [0, 0, 0, 255], "width": 2 } }
            ]
        }
    });
    let out = translate(&drawing_info, &source(), Geometry::Line);
    assert!(out.layers.is_empty());
    assert!(
        out.losses
            .iter()
            .any(|l| l.reason.contains("not ascending"))
    );
}

#[test]
fn class_breaks_without_a_minimum_use_the_first_class_as_fallback() {
    let drawing_info = json!({
        "renderer": {
            "type": "classBreaks",
            "field": "V",
            "classBreakInfos": [
                { "classMaxValue": 10, "symbol": { "type": "esriSLS", "style": "esriSLSSolid", "color": [1, 1, 1, 255], "width": 1 } },
                { "classMaxValue": 20, "symbol": { "type": "esriSLS", "style": "esriSLSSolid", "color": [2, 2, 2, 255], "width": 2 } }
            ]
        }
    });
    let out = translate(&drawing_info, &source(), Geometry::Line);
    assert_eq!(
        out.layers[0]["paint"]["line-color"],
        json!(["step", ["get", "V"], "rgba(1,1,1,1)", 10.0, "rgba(2,2,2,1)"])
    );
    assert!(out.losses.iter().any(|l| l.path == "renderer.minValue"));
}
