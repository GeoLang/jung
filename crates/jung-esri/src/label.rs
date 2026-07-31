use crate::convert::{color, loss, number, px, text};
use crate::{Geometry, LayerBuilder, Loss, Source};
use serde_json::{Value, json};

/// esri point placements as mapbox text-anchor, the anchor is the part of the
/// label that sits on the point so "above" anchors at the label bottom
const POINT_ANCHORS: &[(&str, &str)] = &[
    ("esriServerPointLabelPlacementAboveCenter", "bottom"),
    ("esriServerPointLabelPlacementAboveLeft", "bottom-right"),
    ("esriServerPointLabelPlacementAboveRight", "bottom-left"),
    ("esriServerPointLabelPlacementBelowCenter", "top"),
    ("esriServerPointLabelPlacementBelowLeft", "top-right"),
    ("esriServerPointLabelPlacementBelowRight", "top-left"),
    ("esriServerPointLabelPlacementCenterCenter", "center"),
    ("esriServerPointLabelPlacementCenterLeft", "right"),
    ("esriServerPointLabelPlacementCenterRight", "left"),
];

const LINE_ANCHORS: &[(&str, &str)] = &[
    ("esriServerLinePlacementAboveAlong", "bottom"),
    ("esriServerLinePlacementBelowAlong", "top"),
    ("esriServerLinePlacementCenterAlong", "center"),
];

/// translate the first labelingInfo class into a symbol layer
pub(crate) fn translate(
    drawing_info: &Value,
    src: &Source,
    geometry: Geometry,
    opacity: Option<f64>,
    losses: &mut Vec<Loss>,
) -> Option<Value> {
    let info = drawing_info.get("labelingInfo").filter(|v| !v.is_null())?;
    if drawing_info.get("showLabels") == Some(&Value::Bool(false)) {
        return None;
    }
    let Some(classes) = info.as_array() else {
        loss(
            losses,
            "labelingInfo",
            "expected an array of labeling classes",
        );
        return None;
    };
    let class = classes.first()?;
    if classes.len() > 1 {
        let skipped: Vec<String> = classes[1..]
            .iter()
            .map(|c| {
                text(c, "labelExpression")
                    .unwrap_or("<no labelExpression>")
                    .to_string()
            })
            .collect();
        loss(
            losses,
            format!("labelingInfo[1..{}]", classes.len()),
            format!(
                "only the first labeling class is translated, skipped: {}",
                skipped.join(", ")
            ),
        );
    }

    let field = text_field(class, losses)?;
    let mut layer = LayerBuilder::new(format!("{}-label", src.prefix()), "symbol", src);
    layer.layout("text-field", field);

    if number(class, "minScale").is_some_and(|s| s != 0.0)
        || number(class, "maxScale").is_some_and(|s| s != 0.0)
    {
        loss(
            losses,
            "labelingInfo[0].minScale/maxScale",
            "label scale limits are not translated to zoom limits",
        );
    }
    if class.get("where").is_some_and(|w| !w.is_null()) {
        loss(
            losses,
            "labelingInfo[0].where",
            "label class where clause is not translated to a filter",
        );
    }

    symbol(class.get("symbol"), &mut layer, losses);
    placement(class, geometry, &mut layer, losses);
    if let Some(o) = opacity {
        layer.paint("text-opacity", json!(o));
    }
    Some(layer.build())
}

fn text_field(class: &Value, losses: &mut Vec<Loss>) -> Option<Value> {
    if let Some(expr) = text(class, "labelExpression") {
        match parse_expression(expr) {
            Some(v) => return Some(v),
            None => {
                loss(
                    losses,
                    format!("labelingInfo[0].labelExpression: {expr}"),
                    "not a plain [field] expression, no label layer emitted",
                );
                return None;
            }
        }
    }
    if let Some(info) = class.get("labelExpressionInfo").filter(|v| !v.is_null()) {
        let expr = text(info, "expression").unwrap_or("<empty>");
        loss(
            losses,
            format!("labelingInfo[0].labelExpressionInfo.expression: {expr}"),
            "arcade label expressions are not translated",
        );
        return None;
    }
    loss(
        losses,
        "labelingInfo[0].labelExpression",
        "no label expression, no label layer emitted",
    );
    None
}

/// "[NAME]" and plain mixes of [field] tokens with literal text, anything with
/// vbscript or arcade syntax is refused
fn parse_expression(expr: &str) -> Option<Value> {
    let mut parts: Vec<Value> = Vec::new();
    let mut rest = expr;
    while let Some(open) = rest.find('[') {
        let (literal, tail) = rest.split_at(open);
        if !literal.is_empty() {
            parts.push(json!(plain_literal(literal)?));
        }
        let close = tail.find(']')?;
        let field = &tail[1..close];
        if field.is_empty() || field.contains(['[', '"', '\'']) {
            return None;
        }
        parts.push(json!(["to-string", ["get", field]]));
        rest = &tail[close + 1..];
    }
    if !rest.is_empty() {
        parts.push(json!(plain_literal(rest)?));
    }
    match parts.len() {
        0 => None,
        1 => Some(parts.remove(0)),
        _ => {
            let mut arr = vec![json!("concat")];
            arr.append(&mut parts);
            Some(Value::Array(arr))
        }
    }
}

/// literal text between field tokens, refused when it carries expression syntax
fn plain_literal(s: &str) -> Option<&str> {
    if s.contains(['&', '$', '"', '\'', '+', '(', ')', ']', '\n', '\r'])
        || s.to_uppercase().contains("CONCAT")
    {
        return None;
    }
    Some(s)
}

fn symbol(sym: Option<&Value>, layer: &mut LayerBuilder, losses: &mut Vec<Loss>) {
    let Some(sym) = sym.filter(|s| !s.is_null()) else {
        loss(
            losses,
            "labelingInfo[0].symbol",
            "label symbol missing, label drawn with mapbox defaults",
        );
        return;
    };
    match text(sym, "type") {
        Some("esriTS") => {}
        Some(other) => {
            loss(
                losses,
                format!("labelingInfo[0].symbol.type: {other}"),
                "expected an esriTS text symbol, label drawn with mapbox defaults",
            );
            return;
        }
        None => {
            loss(
                losses,
                "labelingInfo[0].symbol.type",
                "label symbol type missing, label drawn with mapbox defaults",
            );
            return;
        }
    }

    if sym.get("color").is_some_and(|c| !c.is_null()) {
        let text_color = color(
            sym.get("color"),
            "labelingInfo[0].symbol.color",
            &mut *losses,
        );
        layer.paint("text-color", json!(text_color));
    }

    // size lives on the font block, older services put it on the symbol
    let font = sym.get("font");
    let size = font.and_then(|f| number(f, "size")).or(number(sym, "size"));
    if let Some(size) = size {
        layer.layout("text-size", json!(px(size)));
    }

    if sym.get("haloColor").is_some_and(|c| !c.is_null()) {
        let halo = color(
            sym.get("haloColor"),
            "labelingInfo[0].symbol.haloColor",
            &mut *losses,
        );
        layer.paint("text-halo-color", json!(halo));
    }
    if let Some(halo_size) = number(sym, "haloSize") {
        layer.paint("text-halo-width", json!(px(halo_size)));
    }

    if let Some(font) = font {
        if let Some(family) = text(font, "family") {
            loss(
                losses,
                format!("labelingInfo[0].symbol.font.family: {family}"),
                "font family is not mapped to a glyph stack",
            );
        }
        for key in ["style", "weight", "decoration"] {
            if let Some(v) = text(font, key).filter(|v| *v != "normal" && *v != "none") {
                loss(
                    losses,
                    format!("labelingInfo[0].symbol.font.{key}: {v}"),
                    "font variant is not translated",
                );
            }
        }
    }
    for key in ["backgroundColor", "borderLineColor"] {
        if sym.get(key).is_some_and(|v| !v.is_null()) {
            loss(
                losses,
                format!("labelingInfo[0].symbol.{key}"),
                "label background and border are not translated",
            );
        }
    }
    if number(sym, "angle").is_some_and(|a| a != 0.0) {
        loss(
            losses,
            "labelingInfo[0].symbol.angle",
            "label rotation is not translated",
        );
    }
}

fn placement(class: &Value, geometry: Geometry, layer: &mut LayerBuilder, losses: &mut Vec<Loss>) {
    if geometry == Geometry::Line {
        layer.layout("symbol-placement", json!("line"));
    }
    let table = match geometry {
        Geometry::Point => POINT_ANCHORS,
        Geometry::Line => LINE_ANCHORS,
        // polygon labels only offer esriServerPolygonPlacementAlwaysHorizontal
        Geometry::Polygon => &[],
    };
    let Some(placement) = text(class, "labelPlacement") else {
        loss(
            losses,
            "labelingInfo[0].labelPlacement",
            "label placement missing, using the mapbox default anchor",
        );
        return;
    };
    if geometry == Geometry::Polygon {
        if placement != "esriServerPolygonPlacementAlwaysHorizontal" {
            loss(
                losses,
                format!("labelingInfo[0].labelPlacement: {placement}"),
                "placement is not a polygon placement, using the mapbox default anchor",
            );
        }
        return;
    }
    match table.iter().find(|(name, _)| *name == placement) {
        Some((_, anchor)) => layer.layout("text-anchor", json!(anchor)),
        None => loss(
            losses,
            format!("labelingInfo[0].labelPlacement: {placement}"),
            "placement is not translated, using the mapbox default anchor",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_field_expression() {
        assert_eq!(
            parse_expression("[NAME]"),
            Some(json!(["to-string", ["get", "NAME"]]))
        );
    }

    #[test]
    fn concatenated_expression() {
        assert_eq!(
            parse_expression("[CITY], [STATE]"),
            Some(json!([
                "concat",
                ["to-string", ["get", "CITY"]],
                ", ",
                ["to-string", ["get", "STATE"]]
            ]))
        );
    }

    #[test]
    fn literal_only_expression() {
        assert_eq!(parse_expression("Site"), Some(json!("Site")));
    }

    #[test]
    fn vbscript_and_arcade_expressions_are_refused() {
        assert_eq!(parse_expression("[A] & \" \" & [B]"), None);
        assert_eq!(parse_expression("[A] CONCAT [B]"), None);
        assert_eq!(parse_expression("$feature.NAME"), None);
        assert_eq!(parse_expression("[UNCLOSED"), None);
        assert_eq!(parse_expression("[]"), None);
    }
}
