//! pins compatibility with the mapbox gl model jung consumes: every layer this crate
//! emits has to survive jung-style's parser, and the expressions have to evaluate
//!
//! jung-style reads icon-image, icon-size, icon-rotate, icon-anchor and icon-offset, but
//! it has no fill-pattern property, so a picture fill layer is only checked for parsing
//! and for its image name being a valid expression, not for reaching jung's model

mod fixtures;

use fixtures::source;
use jung_esri::{Geometry, Translation, translate};
use jung_style::{Color, EvalContext, PropertyValue, StyleValue, parse_expression, parse_style};
use serde_json::{Value, json};
use std::collections::HashMap;

fn all() -> Vec<(&'static str, Value, Geometry)> {
    vec![
        ("simple_point", fixtures::simple_point(), Geometry::Point),
        (
            "unique_value_polygon",
            fixtures::unique_value_polygon(),
            Geometry::Polygon,
        ),
        (
            "class_breaks_line",
            fixtures::class_breaks_line(),
            Geometry::Line,
        ),
        ("labeled_point", fixtures::labeled_point(), Geometry::Point),
        ("picture_point", fixtures::picture_point(), Geometry::Point),
        (
            "picture_unique_value_point",
            fixtures::picture_unique_value_point(),
            Geometry::Point,
        ),
        (
            "picture_fill_polygon",
            fixtures::picture_fill_polygon(),
            Geometry::Polygon,
        ),
    ]
}

fn style_of(out: &Translation) -> String {
    json!({ "name": "esri", "layers": out.layers }).to_string()
}

#[test]
fn jung_style_parses_every_emitted_layer() {
    for (name, drawing_info, geometry) in all() {
        let out = translate(&drawing_info, &source(), geometry);
        assert!(!out.layers.is_empty(), "{name} emitted no layers");
        let style = parse_style(&style_of(&out)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(style.layers.len(), out.layers.len(), "{name}");
        for (parsed, emitted) in style.layers.iter().zip(&out.layers) {
            assert_eq!(json!(parsed.id), emitted["id"], "{name}");
        }
    }
}

#[test]
fn every_emitted_expression_parses() {
    for (name, drawing_info, geometry) in all() {
        let out = translate(&drawing_info, &source(), geometry);
        for layer in &out.layers {
            for group in ["paint", "layout"] {
                let Some(props) = layer[group].as_object() else {
                    continue;
                };
                for (key, value) in props {
                    match value {
                        // dasharray is a plain number array, not an expression
                        Value::Array(_) if key == "line-dasharray" => {}
                        Value::Array(_) => assert!(
                            parse_expression(value).is_some(),
                            "{name}: {group}.{key} is not a jung expression"
                        ),
                        Value::String(s) if key.contains("color") => assert!(
                            jung_style::parse_css_color_pub(s).is_some(),
                            "{name}: {group}.{key} is not a jung color"
                        ),
                        _ => {}
                    }
                }
            }
        }
    }
}

#[test]
fn simple_marker_reaches_jungs_model() {
    let out = translate(&fixtures::simple_point(), &source(), Geometry::Point);
    let style = parse_style(&style_of(&out)).unwrap();
    let layer = &style.layers[0];
    assert_eq!(
        layer.fill_color,
        Some(StyleValue::Literal(Color::rgb(227, 139, 79)))
    );
    assert_eq!(layer.point_radius, Some(StyleValue::Literal(6.0)));
}

#[test]
fn unique_value_fill_color_evaluates_per_feature() {
    let out = translate(
        &fixtures::unique_value_polygon(),
        &source(),
        Geometry::Polygon,
    );
    let style = parse_style(&style_of(&out)).unwrap();
    let fill_color = style.layers[0].fill_color.clone().unwrap();

    let commercial = props(&[("LANDUSE", PropertyValue::String("commercial".into()))]);
    assert_eq!(
        fill_color.resolve(&ctx(&commercial, "Polygon")),
        Some(Color::rgb(0, 92, 230))
    );
    let unmatched = props(&[("LANDUSE", PropertyValue::String("farmland".into()))]);
    assert_eq!(
        fill_color.resolve(&ctx(&unmatched, "Polygon")),
        Some(Color::rgb(200, 200, 200))
    );
}

#[test]
fn class_breaks_width_evaluates_per_feature() {
    let out = translate(&fixtures::class_breaks_line(), &source(), Geometry::Line);
    let style = parse_style(&style_of(&out)).unwrap();
    let width = style.layers[0].stroke_width.clone().unwrap();

    for (aadt, expected) in [(-1.0, 0.67), (0.0, 1.33), (2500.0, 2.67), (99000.0, 5.33)] {
        let feature = props(&[("AADT", PropertyValue::Number(aadt))]);
        assert_eq!(
            width.resolve(&ctx(&feature, "LineString")),
            Some(expected),
            "aadt {aadt}"
        );
    }
}

#[test]
fn label_text_field_evaluates_per_feature() {
    let out = translate(&fixtures::labeled_point(), &source(), Geometry::Point);
    let style = parse_style(&style_of(&out)).unwrap();
    let label = &style.layers[1];
    let feature = props(&[("NAME", PropertyValue::String("Kirkfield".into()))]);
    assert_eq!(
        label
            .text_field
            .clone()
            .unwrap()
            .resolve(&ctx(&feature, "Point")),
        Some("Kirkfield".to_string())
    );
    assert_eq!(label.font_size, Some(StyleValue::Literal(12.0)));
    assert_eq!(
        label.text_color,
        Some(StyleValue::Literal(Color::rgb(39, 39, 39)))
    );
}

#[test]
fn picture_marker_reaches_jungs_model() {
    let out = translate(&fixtures::picture_point(), &source(), Geometry::Point);
    let style = parse_style(&style_of(&out)).unwrap();
    let layer = &style.layers[0];
    assert_eq!(
        layer.icon_image,
        Some(StyleValue::Literal("wells-image".to_string()))
    );
    assert_eq!(layer.icon_rotate, Some(StyleValue::Literal(270.0)));
    // the bitmap is registered at its own size, so the layer never scales it
    assert_eq!(layer.icon_size, None);
    assert!(out.images.contains_key("wells-image"));
}

#[test]
fn picture_icon_image_evaluates_per_feature() {
    let out = translate(
        &fixtures::picture_unique_value_point(),
        &source(),
        Geometry::Point,
    );
    let style = parse_style(&style_of(&out)).unwrap();
    let icon = style.layers[1].icon_image.clone().unwrap();

    let well = props(&[("KIND", PropertyValue::String("well".into()))]);
    assert_eq!(
        icon.resolve(&ctx(&well, "Point")),
        Some("wells-image-0".to_string())
    );
    // the vector branch draws no icon, the circle layer paints it instead
    let dry = props(&[("KIND", PropertyValue::String("dry".into()))]);
    assert_eq!(icon.resolve(&ctx(&dry, "Point")), Some(String::new()));
}

fn props(entries: &[(&str, PropertyValue)]) -> HashMap<String, PropertyValue> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn ctx<'a>(
    properties: &'a HashMap<String, PropertyValue>,
    geometry_type: &'a str,
) -> EvalContext<'a> {
    EvalContext {
        properties,
        zoom: 10.0,
        geometry_type,
    }
}
