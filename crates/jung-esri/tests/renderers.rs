mod fixtures;

use fixtures::source;
use jung_esri::{Geometry, translate};
use serde_json::json;

#[test]
fn simple_marker_renderer() {
    let out = translate(&fixtures::simple_point(), &source(), Geometry::Point);
    assert_eq!(
        out.layers,
        vec![json!({
            "id": "wells-circle",
            "type": "circle",
            "source": "ptolemy",
            "source-layer": "wells",
            "paint": {
                "circle-color": "rgba(227,139,79,1)",
                "circle-radius": 6.0,
                "circle-stroke-color": "rgba(255,255,255,1)",
                "circle-stroke-width": 1.0
            }
        })]
    );
    assert_eq!(out.losses, vec![]);
}

#[test]
fn unique_value_renderer_matches_field1() {
    let out = translate(
        &fixtures::unique_value_polygon(),
        &source(),
        Geometry::Polygon,
    );
    assert_eq!(out.losses, vec![]);
    assert_eq!(out.layers.len(), 2);
    assert_eq!(
        out.layers[0],
        json!({
            "id": "wells-fill",
            "type": "fill",
            "source": "ptolemy",
            "source-layer": "wells",
            "paint": {
                "fill-color": ["match", ["to-string", ["get", "LANDUSE"]],
                    "residential", "rgba(255,170,0,1)",
                    "commercial", "rgba(0,92,230,1)",
                    "rgba(200,200,200,1)"],
                // transparency 25 becomes opacity 0.75
                "fill-opacity": 0.75
            }
        })
    );
    // every branch shares one outline, so it collapses to a literal
    assert_eq!(
        out.layers[1],
        json!({
            "id": "wells-outline",
            "type": "line",
            "source": "ptolemy",
            "source-layer": "wells",
            "paint": {
                "line-color": "rgba(110,110,110,1)",
                "line-width": 0.53,
                "line-opacity": 0.75
            }
        })
    );
}

#[test]
fn class_breaks_renderer_steps_the_field() {
    let out = translate(&fixtures::class_breaks_line(), &source(), Geometry::Line);
    assert_eq!(
        out.layers,
        vec![json!({
            "id": "wells-line",
            "type": "line",
            "source": "ptolemy",
            "source-layer": "wells",
            "paint": {
                "line-color": ["step", ["get", "AADT"],
                    "rgba(190,190,190,1)",
                    0.0, "rgba(255,235,175,1)",
                    1000.0, "rgba(255,170,0,1)",
                    5000.0, "rgba(230,0,0,1)"],
                "line-width": ["step", ["get", "AADT"], 0.67, 0.0, 1.33, 1000.0, 2.67, 5000.0, 5.33]
            }
        })]
    );
    // a step has no upper bound, so the top class keeps painting above its max
    assert_eq!(out.losses.len(), 1);
    assert_eq!(
        out.losses[0].path,
        "renderer.classBreakInfos[2].classMaxValue: 25000"
    );
}

#[test]
fn labeling_info_becomes_a_symbol_layer() {
    let out = translate(&fixtures::labeled_point(), &source(), Geometry::Point);
    assert_eq!(out.losses, vec![]);
    assert_eq!(
        out.layers[1],
        json!({
            "id": "wells-label",
            "type": "symbol",
            "source": "ptolemy",
            "source-layer": "wells",
            "layout": {
                "text-field": ["to-string", ["get", "NAME"]],
                "text-size": 12.0,
                "text-anchor": "bottom-left"
            },
            "paint": {
                "text-color": "rgba(39,39,39,1)",
                "text-halo-color": "rgba(255,255,255,1)",
                "text-halo-width": 2.0
            }
        })
    );
}

#[test]
fn label_fields_are_concatenated() {
    let drawing_info = json!({
        "renderer": { "type": "simple", "symbol": { "type": "esriSLS", "style": "esriSLSSolid", "color": [0, 0, 0, 255], "width": 1 } },
        "labelingInfo": [{
            "labelExpression": "[ROUTE] - [NAME]",
            "labelPlacement": "esriServerLinePlacementCenterAlong",
            "symbol": { "type": "esriTS", "color": [0, 0, 0, 255], "font": { "size": 8, "family": "Arial" } }
        }]
    });
    let out = translate(&drawing_info, &source(), Geometry::Line);
    let layout = &out.layers[1]["layout"];
    assert_eq!(
        layout["text-field"],
        json!([
            "concat",
            ["to-string", ["get", "ROUTE"]],
            " - ",
            ["to-string", ["get", "NAME"]]
        ])
    );
    assert_eq!(layout["symbol-placement"], json!("line"));
    assert_eq!(layout["text-anchor"], json!("center"));
    // the font family has no glyph stack on the mapbox side
    assert_eq!(
        out.losses[0].path,
        "labelingInfo[0].symbol.font.family: Arial"
    );
}

#[test]
fn extra_labeling_classes_are_listed_as_a_loss() {
    let drawing_info = json!({
        "renderer": { "type": "simple", "symbol": { "type": "esriSMS", "style": "esriSMSCircle", "color": [0, 0, 0, 255], "size": 4 } },
        "labelingInfo": [
            { "labelExpression": "[NAME]", "labelPlacement": "esriServerPointLabelPlacementCenterCenter", "symbol": { "type": "esriTS", "color": [0, 0, 0, 255] } },
            { "labelExpression": "[CODE]", "labelPlacement": "esriServerPointLabelPlacementCenterCenter", "symbol": { "type": "esriTS", "color": [0, 0, 0, 255] } },
            { "labelPlacement": "esriServerPointLabelPlacementCenterCenter" }
        ]
    });
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert_eq!(out.layers.len(), 2);
    assert_eq!(out.losses.len(), 1);
    assert_eq!(out.losses[0].path, "labelingInfo[1..3]");
    assert_eq!(
        out.losses[0].reason,
        "only the first labeling class is translated, skipped: [CODE], <no labelExpression>"
    );
}

#[test]
fn arcade_labels_are_refused() {
    let drawing_info = json!({
        "renderer": { "type": "simple", "symbol": { "type": "esriSMS", "style": "esriSMSCircle", "color": [0, 0, 0, 255], "size": 4 } },
        "labelingInfo": [{
            "labelExpressionInfo": { "expression": "$feature.NAME + \" well\"" },
            "labelPlacement": "esriServerPointLabelPlacementCenterCenter",
            "symbol": { "type": "esriTS", "color": [0, 0, 0, 255] }
        }]
    });
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert_eq!(out.layers.len(), 1);
    assert_eq!(
        out.losses[0].path,
        "labelingInfo[0].labelExpressionInfo.expression: $feature.NAME + \" well\""
    );
}

#[test]
fn show_labels_false_drops_the_label_layer() {
    let mut drawing_info = fixtures::labeled_point();
    drawing_info["showLabels"] = json!(false);
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert_eq!(out.layers.len(), 1);
    assert_eq!(out.losses, vec![]);
}

#[test]
fn null_line_style_emits_no_layer() {
    let drawing_info = json!({
        "renderer": { "type": "simple", "symbol": { "type": "esriSLS", "style": "esriSLSNull", "color": [0, 0, 0, 0], "width": 1 } }
    });
    let out = translate(&drawing_info, &source(), Geometry::Line);
    assert!(out.layers.is_empty());
    assert_eq!(out.losses[0].path, "renderer.symbol.style: esriSLSNull");
}

#[test]
fn picture_marker_becomes_a_symbol_layer() {
    let out = translate(&fixtures::picture_point(), &source(), Geometry::Point);
    assert_eq!(out.losses, vec![]);
    assert_eq!(
        out.layers,
        vec![json!({
            "id": "wells-icon",
            "type": "symbol",
            "source": "ptolemy",
            "source-layer": "wells",
            "layout": {
                "icon-image": "wells-image",
                "icon-allow-overlap": true,
                // esri turns 90 degrees counter clockwise, mapbox clockwise
                "icon-rotate": 270.0
            }
        })]
    );
    assert_eq!(out.images.len(), 1);
    let image = &out.images["wells-image"];
    assert_eq!(
        image.data_uri,
        format!("data:image/png;base64,{}", fixtures::PNG)
    );
    // 18 points at 96 dpi, the size the consumer registers the bitmap at
    assert_eq!((image.width, image.height), (24.0, 24.0));
}

#[test]
fn images_serialize_under_their_names() {
    let out = translate(&fixtures::picture_point(), &source(), Geometry::Point);
    assert_eq!(
        serde_json::to_value(&out.images).unwrap(),
        json!({
            "wells-image": {
                "data_uri": format!("data:image/png;base64,{}", fixtures::PNG),
                "width": 24.0,
                "height": 24.0
            }
        })
    );
}

#[test]
fn mixed_picture_and_vector_branches_get_a_layer_each() {
    let out = translate(
        &fixtures::picture_unique_value_point(),
        &source(),
        Geometry::Point,
    );
    assert_eq!(out.losses, vec![]);
    assert_eq!(out.layers.len(), 2);
    // the circle layer paints the picture branch away
    assert_eq!(
        out.layers[0]["paint"],
        json!({
            "circle-color": ["match", ["to-string", ["get", "KIND"]],
                "well", "rgba(0,0,0,0)",
                "dry", "rgba(0,0,0,1)",
                "rgba(120,120,120,1)"],
            "circle-radius": ["match", ["to-string", ["get", "KIND"]],
                "well", 0.0,
                "dry", 6.0,
                4.0]
        })
    );
    // and the icon layer draws nothing for the vector branches
    assert_eq!(
        out.layers[1],
        json!({
            "id": "wells-icon",
            "type": "symbol",
            "source": "ptolemy",
            "source-layer": "wells",
            "layout": {
                "icon-image": ["match", ["to-string", ["get", "KIND"]],
                    "well", "wells-image-0",
                    "dry", "",
                    ""],
                "icon-allow-overlap": true
            }
        })
    );
    assert_eq!(out.images.keys().collect::<Vec<_>>(), vec!["wells-image-0"]);
    assert_eq!(out.images["wells-image-0"].width, 16.0);
}

#[test]
fn picture_fill_becomes_a_fill_pattern() {
    let out = translate(
        &fixtures::picture_fill_polygon(),
        &source(),
        Geometry::Polygon,
    );
    assert_eq!(out.losses, vec![]);
    assert_eq!(
        out.layers,
        vec![json!({
            "id": "wells-pattern",
            "type": "fill",
            "source": "ptolemy",
            "source-layer": "wells",
            "paint": {
                "fill-pattern": "wells-image",
                "fill-opacity": 0.8
            }
        })]
    );
    assert_eq!(out.images["wells-image"].height, 32.0);
}

#[test]
fn a_picture_symbol_with_only_a_url_is_a_loss() {
    let drawing_info = json!({
        "renderer": {
            "type": "simple",
            "symbol": { "type": "esriPMS", "url": "https://example.com/3f.png", "width": 12, "height": 12 }
        }
    });
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert!(out.layers.is_empty());
    assert!(out.images.is_empty());
    assert_eq!(
        out.losses[0].path,
        "renderer.symbol.url: https://example.com/3f.png"
    );
}

#[test]
fn malformed_image_data_reaches_the_consumer_verbatim() {
    let drawing_info = json!({
        "renderer": {
            "type": "simple",
            "symbol": { "type": "esriPMS", "imageData": "!!!not base64", "contentType": "image/png", "width": 6, "height": 6 }
        }
    });
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert_eq!(out.losses, vec![]);
    assert_eq!(
        out.images["wells-image"].data_uri,
        "data:image/png;base64,!!!not base64"
    );
}

#[test]
fn class_breaks_step_over_image_names() {
    let drawing_info = json!({
        "renderer": {
            "type": "classBreaks",
            "field": "DEPTH",
            "minValue": 0,
            "classBreakInfos": [
                { "classMinValue": 0, "symbol": { "type": "esriPMS", "imageData": fixtures::PNG, "contentType": "image/png", "width": 6, "height": 6 } },
                { "classMinValue": 100, "symbol": { "type": "esriPMS", "imageData": fixtures::PNG, "contentType": "image/png", "width": 12, "height": 12 } }
            ]
        }
    });
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert_eq!(out.losses, vec![]);
    assert_eq!(
        out.layers[0]["layout"]["icon-image"],
        json!([
            "step",
            ["get", "DEPTH"],
            "",
            0.0,
            "wells-image-0",
            100.0,
            "wells-image-1"
        ])
    );
    // one image per class, each at its own registered size
    assert_eq!(out.images["wells-image-0"].width, 8.0);
    assert_eq!(out.images["wells-image-1"].width, 16.0);
}

#[test]
fn per_class_dash_patterns_collapse_to_the_first() {
    let drawing_info = json!({
        "renderer": {
            "type": "uniqueValue",
            "field1": "STATUS",
            "uniqueValueInfos": [
                { "value": "built", "symbol": { "type": "esriSLS", "style": "esriSLSSolid", "color": [0, 0, 0, 255], "width": 1 } },
                { "value": "planned", "symbol": { "type": "esriSLS", "style": "esriSLSDash", "color": [0, 0, 0, 255], "width": 1 } }
            ]
        }
    });
    let out = translate(&drawing_info, &source(), Geometry::Line);
    assert_eq!(out.layers.len(), 1);
    assert!(out.layers[0]["paint"].get("line-dasharray").is_none());
    assert_eq!(out.losses[0].path, "renderer.symbol.style");
    assert_eq!(
        out.losses[0].reason,
        "per class dash patterns are not data driven, the first pattern is used"
    );
}

#[test]
fn multi_field_unique_values_join_with_the_delimiter() {
    let drawing_info = json!({
        "renderer": {
            "type": "uniqueValue",
            "field1": "KIND",
            "field2": "STATUS",
            "fieldDelimiter": ", ",
            "defaultSymbol": { "type": "esriSMS", "style": "esriSMSCircle", "color": [0, 0, 0, 255], "size": 4 },
            "uniqueValueInfos": [
                { "value": "well, active", "symbol": { "type": "esriSMS", "style": "esriSMSCircle", "color": [255, 0, 0, 255], "size": 6 } }
            ]
        }
    });
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert_eq!(out.losses, vec![]);
    assert_eq!(
        out.layers[0]["paint"]["circle-color"],
        json!([
            "match",
            [
                "concat",
                ["to-string", ["get", "KIND"]],
                ", ",
                ["to-string", ["get", "STATUS"]]
            ],
            "well, active",
            "rgba(255,0,0,1)",
            "rgba(0,0,0,1)"
        ])
    );
}

#[test]
fn unknown_renderer_type_is_refused_by_name() {
    let drawing_info = json!({ "renderer": { "type": "heatmap", "blurRadius": 10 } });
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert!(out.layers.is_empty());
    assert_eq!(out.losses[0].path, "renderer.type: heatmap");
}

#[test]
fn visual_variables_are_reported() {
    let drawing_info = json!({
        "renderer": {
            "type": "simple",
            "symbol": { "type": "esriSMS", "style": "esriSMSCircle", "color": [0, 0, 0, 255], "size": 4 },
            "visualVariables": [{ "type": "sizeInfo", "field": "POP" }]
        }
    });
    let out = translate(&drawing_info, &source(), Geometry::Point);
    assert_eq!(out.layers.len(), 1);
    assert_eq!(out.losses[0].path, "renderer.visualVariables");
}

#[test]
fn a_symbol_that_does_not_fit_the_geometry_is_reported() {
    let drawing_info = json!({
        "renderer": { "type": "simple", "symbol": { "type": "esriSFS", "style": "esriSFSSolid", "color": [1, 2, 3, 255] } }
    });
    let out = translate(&drawing_info, &source(), Geometry::Line);
    assert!(out.layers.is_empty());
    assert_eq!(out.losses[0].path, "renderer.symbol.type: esriSFS");
}

#[test]
fn layer_ids_fall_back_to_the_source_name() {
    let source = jung_esri::Source {
        source: "wells-geojson".to_string(),
        source_layer: String::new(),
    };
    let out = translate(&fixtures::simple_point(), &source, Geometry::Point);
    assert_eq!(out.layers[0]["id"], json!("wells-geojson-circle"));
    assert!(out.layers[0].get("source-layer").is_none());
}
