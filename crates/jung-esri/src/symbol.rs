use crate::convert::{color, loss, number, px, text};
use crate::{Geometry, Loss};
use serde_json::Value;

/// a translated line, used for line symbols and for outlines
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Stroke {
    pub color: String,
    pub width: f64,
    pub dash: Option<Vec<f64>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Marker {
    pub color: String,
    pub radius: f64,
    pub stroke: Option<Stroke>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fill {
    /// None when the symbol has no fill, the outline still applies
    pub color: Option<String>,
    pub stroke: Option<Stroke>,
}

/// what a single esri symbol paints
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Paint {
    Marker(Marker),
    Line(Stroke),
    Fill(Fill),
    /// nothing to draw, either refused or an esri null style
    Nothing,
}

impl Paint {
    pub(crate) fn marker(&self) -> Option<&Marker> {
        match self {
            Paint::Marker(m) => Some(m),
            _ => None,
        }
    }

    pub(crate) fn line(&self) -> Option<&Stroke> {
        match self {
            Paint::Line(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn fill(&self) -> Option<&Fill> {
        match self {
            Paint::Fill(f) => Some(f),
            _ => None,
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Paint::Marker(_) => "esriSMS",
            Paint::Line(_) => "esriSLS",
            Paint::Fill(_) => "esriSFS",
            Paint::Nothing => "none",
        }
    }
}

/// parse a symbol that is allowed to be absent, absence is not a loss
pub(crate) fn parse_optional(
    symbol: Option<&Value>,
    path: &str,
    geometry: Geometry,
    losses: &mut Vec<Loss>,
) -> Paint {
    match symbol.filter(|s| !s.is_null()) {
        Some(_) => parse(symbol, path, geometry, losses),
        None => Paint::Nothing,
    }
}

/// parse a required symbol into the paint it produces on a `geometry` layer
pub(crate) fn parse(
    symbol: Option<&Value>,
    path: &str,
    geometry: Geometry,
    losses: &mut Vec<Loss>,
) -> Paint {
    let paint = parse_kind(symbol, path, losses);
    let matches_geometry = match geometry {
        Geometry::Point => paint.marker().is_some(),
        Geometry::Line => paint.line().is_some(),
        Geometry::Polygon => paint.fill().is_some(),
    };
    if !matches_geometry && paint != Paint::Nothing {
        loss(
            losses,
            format!("{path}.type: {}", paint.kind()),
            format!("symbol does not fit a {geometry:?} layer, drawn invisible"),
        );
    }
    paint
}

fn parse_kind(symbol: Option<&Value>, path: &str, losses: &mut Vec<Loss>) -> Paint {
    let Some(sym) = symbol.filter(|s| !s.is_null()) else {
        loss(losses, path, "symbol missing");
        return Paint::Nothing;
    };
    if !sym.is_object() {
        loss(losses, path, "symbol must be a JSON object");
        return Paint::Nothing;
    }
    let Some(kind) = text(sym, "type") else {
        loss(losses, path, "symbol type missing");
        return Paint::Nothing;
    };
    match kind {
        "esriSMS" => marker(sym, path, losses),
        "esriSLS" => match line(sym, path, losses) {
            Some(stroke) => Paint::Line(stroke),
            None => Paint::Nothing,
        },
        "esriSFS" => fill(sym, path, losses),
        "esriPMS" => {
            loss(
                losses,
                format!("{path}.type: esriPMS"),
                "picture marker symbols need a sprite image, not translated",
            );
            Paint::Nothing
        }
        "esriPFS" => {
            loss(
                losses,
                format!("{path}.type: esriPFS"),
                "picture fill symbols need a sprite image, not translated",
            );
            Paint::Nothing
        }
        "esriTS" => {
            loss(
                losses,
                format!("{path}.type: esriTS"),
                "text symbols are only translated from labelingInfo",
            );
            Paint::Nothing
        }
        other => {
            loss(
                losses,
                format!("{path}.type: {other}"),
                "unknown symbol type",
            );
            Paint::Nothing
        }
    }
}

fn marker(sym: &Value, path: &str, losses: &mut Vec<Loss>) -> Paint {
    let style = match text(sym, "style") {
        Some(s) => s,
        None => {
            loss(
                losses,
                format!("{path}.style"),
                "marker style missing, assumed a circle",
            );
            "esriSMSCircle"
        }
    };
    match style {
        "esriSMSCircle" => {}
        "esriSMSSquare" | "esriSMSDiamond" | "esriSMSCross" | "esriSMSX" | "esriSMSTriangle" => {
            loss(
                losses,
                format!("{path}.style: {style}"),
                "shape approximated by a circle, mapbox circles have no shape without a sprite",
            )
        }
        other => loss(
            losses,
            format!("{path}.style: {other}"),
            "unknown marker style, approximated by a circle",
        ),
    }

    // esri size is the marker diameter in points
    let size = match number(sym, "size") {
        Some(s) => s,
        None => {
            loss(
                losses,
                format!("{path}.size"),
                "marker size missing, assumed 8 points",
            );
            8.0
        }
    };

    Paint::Marker(Marker {
        color: color(sym.get("color"), &format!("{path}.color"), losses),
        radius: px(size / 2.0),
        stroke: outline(sym, path, losses),
    })
}

fn outline(sym: &Value, path: &str, losses: &mut Vec<Loss>) -> Option<Stroke> {
    let o = sym.get("outline").filter(|v| !v.is_null())?;
    line(o, &format!("{path}.outline"), losses)
}

fn line(sym: &Value, path: &str, losses: &mut Vec<Loss>) -> Option<Stroke> {
    if !sym.is_object() {
        loss(losses, path, "line symbol must be a JSON object");
        return None;
    }
    if let Some(kind) = text(sym, "type")
        && kind != "esriSLS"
    {
        loss(
            losses,
            format!("{path}.type: {kind}"),
            "expected an esriSLS line symbol",
        );
        return None;
    }
    let style = text(sym, "style").unwrap_or("esriSLSSolid");
    let dash = match line_style(style) {
        LineStyle::Solid => None,
        LineStyle::Dash(d) => Some(d),
        LineStyle::Null => {
            loss(
                losses,
                format!("{path}.style: esriSLSNull"),
                "null line style draws nothing, no line layer emitted",
            );
            return None;
        }
        LineStyle::Unknown => {
            loss(
                losses,
                format!("{path}.style: {style}"),
                "unknown line style, drawn solid",
            );
            None
        }
    };
    let width = match number(sym, "width") {
        Some(w) => px(w),
        None => {
            loss(
                losses,
                format!("{path}.width"),
                "line width missing, assumed 1 point",
            );
            px(1.0)
        }
    };
    Some(Stroke {
        color: color(sym.get("color"), &format!("{path}.color"), losses),
        width,
        dash,
    })
}

fn fill(sym: &Value, path: &str, losses: &mut Vec<Loss>) -> Paint {
    let stroke = outline(sym, path, losses);
    let style = match text(sym, "style") {
        Some(s) => s,
        None => {
            loss(
                losses,
                format!("{path}.style"),
                "fill style missing, assumed a solid fill",
            );
            "esriSFSSolid"
        }
    };
    let color_path = format!("{path}.color");
    let fill_color = match fill_style(style) {
        FillStyle::Solid => Some(color(sym.get("color"), &color_path, losses)),
        FillStyle::Null => None,
        FillStyle::Pattern => {
            loss(
                losses,
                format!("{path}.style: {style}"),
                "hatch fill pattern drawn as a solid fill",
            );
            Some(color(sym.get("color"), &color_path, losses))
        }
        FillStyle::Unknown => {
            loss(
                losses,
                format!("{path}.style: {style}"),
                "unknown fill style, drawn as a solid fill",
            );
            Some(color(sym.get("color"), &color_path, losses))
        }
    };
    Paint::Fill(Fill {
        color: fill_color,
        stroke,
    })
}

enum LineStyle {
    Solid,
    /// dash pattern in multiples of the line width, the mapbox line-dasharray unit
    Dash(Vec<f64>),
    Null,
    Unknown,
}

fn line_style(style: &str) -> LineStyle {
    let pattern: &[f64] = match style {
        "esriSLSSolid" => return LineStyle::Solid,
        "esriSLSNull" => return LineStyle::Null,
        "esriSLSDash" => &[4.0, 3.0],
        "esriSLSDot" => &[1.0, 3.0],
        "esriSLSDashDot" => &[4.0, 3.0, 1.0, 3.0],
        "esriSLSDashDotDot" => &[4.0, 3.0, 1.0, 3.0, 1.0, 3.0],
        "esriSLSLongDash" => &[8.0, 3.0],
        "esriSLSLongDashDot" => &[8.0, 3.0, 1.0, 3.0],
        "esriSLSShortDash" => &[2.0, 2.0],
        "esriSLSShortDot" => &[1.0, 2.0],
        "esriSLSShortDashDot" => &[2.0, 2.0, 1.0, 2.0],
        "esriSLSShortDashDotDot" => &[2.0, 2.0, 1.0, 2.0, 1.0, 2.0],
        _ => return LineStyle::Unknown,
    };
    LineStyle::Dash(pattern.to_vec())
}

enum FillStyle {
    Solid,
    Null,
    Pattern,
    Unknown,
}

fn fill_style(style: &str) -> FillStyle {
    match style {
        "esriSFSSolid" => FillStyle::Solid,
        "esriSFSNull" => FillStyle::Null,
        "esriSFSBackwardDiagonal"
        | "esriSFSCross"
        | "esriSFSDiagonalCross"
        | "esriSFSForwardDiagonal"
        | "esriSFSHorizontal"
        | "esriSFSVertical" => FillStyle::Pattern,
        _ => FillStyle::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn circle_marker_with_outline() {
        let mut losses = Vec::new();
        let sym = json!({
            "type": "esriSMS",
            "style": "esriSMSCircle",
            "color": [255, 0, 0, 255],
            "size": 12,
            "outline": { "type": "esriSLS", "style": "esriSLSSolid", "color": [0, 0, 0, 255], "width": 1.5 }
        });
        let paint = parse(Some(&sym), "renderer.symbol", Geometry::Point, &mut losses);
        assert_eq!(
            paint,
            Paint::Marker(Marker {
                color: "rgba(255,0,0,1)".to_string(),
                radius: 8.0,
                stroke: Some(Stroke {
                    color: "rgba(0,0,0,1)".to_string(),
                    width: 2.0,
                    dash: None,
                }),
            })
        );
        assert!(losses.is_empty());
    }

    #[test]
    fn cross_marker_is_approximated() {
        let mut losses = Vec::new();
        let sym = json!({ "type": "esriSMS", "style": "esriSMSCross", "color": [0, 0, 0, 255], "size": 6 });
        assert!(
            parse(Some(&sym), "renderer.symbol", Geometry::Point, &mut losses)
                .marker()
                .is_some()
        );
        assert_eq!(losses.len(), 1);
        assert_eq!(losses[0].path, "renderer.symbol.style: esriSMSCross");
    }

    #[test]
    fn dashed_line() {
        let mut losses = Vec::new();
        let sym = json!({ "type": "esriSLS", "style": "esriSLSDashDot", "color": [10, 20, 30, 255], "width": 3 });
        assert_eq!(
            parse(Some(&sym), "renderer.symbol", Geometry::Line, &mut losses),
            Paint::Line(Stroke {
                color: "rgba(10,20,30,1)".to_string(),
                width: 4.0,
                dash: Some(vec![4.0, 3.0, 1.0, 3.0]),
            })
        );
        assert!(losses.is_empty());
    }

    #[test]
    fn null_line_draws_nothing() {
        let mut losses = Vec::new();
        let sym = json!({ "type": "esriSLS", "style": "esriSLSNull", "width": 1 });
        assert_eq!(
            parse(Some(&sym), "renderer.symbol", Geometry::Line, &mut losses),
            Paint::Nothing
        );
        assert_eq!(losses[0].path, "renderer.symbol.style: esriSLSNull");
    }

    #[test]
    fn hatch_fill_is_solid_with_loss() {
        let mut losses = Vec::new();
        let sym = json!({
            "type": "esriSFS",
            "style": "esriSFSCross",
            "color": [1, 2, 3, 100],
            "outline": { "type": "esriSLS", "style": "esriSLSSolid", "color": [0, 0, 0, 255], "width": 1 }
        });
        let paint = parse(
            Some(&sym),
            "renderer.symbol",
            Geometry::Polygon,
            &mut losses,
        );
        let fill = paint.fill().unwrap();
        assert_eq!(fill.color.as_deref(), Some("rgba(1,2,3,0.392)"));
        assert!(fill.stroke.is_some());
        assert_eq!(losses[0].path, "renderer.symbol.style: esriSFSCross");
    }

    #[test]
    fn null_fill_keeps_the_outline() {
        let mut losses = Vec::new();
        let sym = json!({
            "type": "esriSFS",
            "style": "esriSFSNull",
            "color": null,
            "outline": { "type": "esriSLS", "style": "esriSLSSolid", "color": [0, 0, 0, 255], "width": 2 }
        });
        let paint = parse(
            Some(&sym),
            "renderer.symbol",
            Geometry::Polygon,
            &mut losses,
        );
        let fill = paint.fill().unwrap();
        assert_eq!(fill.color, None);
        assert_eq!(fill.stroke.as_ref().unwrap().width, 2.67);
        assert!(losses.is_empty());
    }

    #[test]
    fn picture_symbols_are_refused() {
        let mut losses = Vec::new();
        let sym = json!({ "type": "esriPMS", "url": "abc", "width": 10, "height": 10 });
        assert_eq!(
            parse(Some(&sym), "renderer.symbol", Geometry::Point, &mut losses),
            Paint::Nothing
        );
        assert_eq!(losses[0].path, "renderer.symbol.type: esriPMS");
    }

    #[test]
    fn unknown_symbol_type_is_refused_by_name() {
        let mut losses = Vec::new();
        let sym = json!({ "type": "CIMSymbol" });
        assert_eq!(
            parse(Some(&sym), "renderer.symbol", Geometry::Point, &mut losses),
            Paint::Nothing
        );
        assert_eq!(losses[0].path, "renderer.symbol.type: CIMSymbol");
    }

    #[test]
    fn missing_sizes_fall_back_with_losses() {
        let mut losses = Vec::new();
        let sym = json!({ "type": "esriSMS", "color": [0, 0, 0, 255] });
        let paint = parse(Some(&sym), "renderer.symbol", Geometry::Point, &mut losses);
        assert_eq!(paint.marker().unwrap().radius, 5.33);
        assert_eq!(losses.len(), 2);
    }
}
