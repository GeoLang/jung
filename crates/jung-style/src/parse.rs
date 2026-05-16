use serde::Deserialize;
use thiserror::Error;

/// Errors from style parsing.
#[derive(Debug, Error)]
pub enum StyleError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("missing required field: {0}")]
    MissingField(String),
}

/// An RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// A parsed style definition.
#[derive(Debug, Clone)]
pub struct Style {
    pub name: String,
    pub layers: Vec<Layer>,
}

/// Line cap style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

use crate::expr::StyleValue;

/// Line join style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// A single symbology layer.
#[derive(Debug, Clone)]
pub struct Layer {
    pub id: String,
    pub source: Option<String>,
    pub fill_color: Option<StyleValue<Color>>,
    pub stroke_color: Option<StyleValue<Color>>,
    pub stroke_width: Option<StyleValue<f32>>,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub line_dasharray: Option<Vec<f32>>,
    pub line_offset: Option<StyleValue<f32>>,
    pub line_opacity: Option<StyleValue<f32>>,
    pub point_radius: Option<StyleValue<f32>>,
    pub icon_image: Option<StyleValue<String>>,
    pub icon_size: Option<StyleValue<f32>>,
    pub font_family: Option<String>,
    pub font_size: Option<StyleValue<f32>>,
    pub text_field: Option<StyleValue<String>>,
    pub text_color: Option<StyleValue<Color>>,
}

/// Intermediate JSON representation for deserialization.
#[derive(Deserialize)]
struct StyleJson {
    name: Option<String>,
    layers: Vec<LayerJson>,
}

#[derive(Deserialize)]
struct LayerJson {
    id: String,
    source: Option<String>,
    #[serde(default)]
    paint: PaintJson,
    #[serde(default)]
    layout: LayoutJson,
}

#[derive(Deserialize, Default)]
struct PaintJson {
    #[serde(rename = "fill-color")]
    fill_color: Option<serde_json::Value>,
    #[serde(rename = "line-color")]
    line_color: Option<serde_json::Value>,
    #[serde(rename = "line-width")]
    line_width: Option<serde_json::Value>,
    #[serde(rename = "line-dasharray")]
    line_dasharray: Option<Vec<f32>>,
    #[serde(rename = "line-offset")]
    line_offset: Option<serde_json::Value>,
    #[serde(rename = "line-opacity")]
    line_opacity: Option<serde_json::Value>,
    #[serde(rename = "circle-radius")]
    circle_radius: Option<serde_json::Value>,
    #[serde(rename = "circle-color")]
    circle_color: Option<serde_json::Value>,
    #[serde(rename = "text-color")]
    text_color: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct LayoutJson {
    #[serde(rename = "text-field")]
    text_field: Option<serde_json::Value>,
    #[serde(rename = "text-font")]
    text_font: Option<Vec<String>>,
    #[serde(rename = "text-size")]
    text_size: Option<serde_json::Value>,
    #[serde(rename = "line-cap")]
    line_cap: Option<String>,
    #[serde(rename = "line-join")]
    line_join: Option<String>,
    #[serde(rename = "icon-image")]
    icon_image: Option<serde_json::Value>,
    #[serde(rename = "icon-size")]
    icon_size: Option<serde_json::Value>,
}

/// Parse a JSON style string into a `Style`.
pub fn parse_style(json: &str) -> Result<Style, StyleError> {
    let raw: StyleJson = serde_json::from_str(json)?;

    let layers = raw
        .layers
        .into_iter()
        .map(|l| Layer {
            id: l.id,
            source: l.source,
            fill_color: parse_color_value(&l.paint.fill_color)
                .or_else(|| parse_color_value(&l.paint.circle_color)),
            stroke_color: parse_color_value(&l.paint.line_color),
            stroke_width: parse_number_value(&l.paint.line_width),
            line_cap: l
                .layout
                .line_cap
                .as_deref()
                .map(parse_line_cap)
                .unwrap_or_default(),
            line_join: l
                .layout
                .line_join
                .as_deref()
                .map(parse_line_join)
                .unwrap_or_default(),
            line_dasharray: l.paint.line_dasharray,
            line_offset: parse_number_value(&l.paint.line_offset),
            line_opacity: parse_number_value(&l.paint.line_opacity),
            point_radius: parse_number_value(&l.paint.circle_radius),
            icon_image: parse_string_value(&l.layout.icon_image),
            icon_size: parse_number_value(&l.layout.icon_size),
            font_family: l.layout.text_font.as_ref().and_then(|f| f.first().cloned()),
            font_size: parse_number_value(&l.layout.text_size),
            text_field: parse_string_value(&l.layout.text_field),
            text_color: parse_color_value(&l.paint.text_color),
        })
        .collect();

    Ok(Style {
        name: raw.name.unwrap_or_else(|| "untitled".to_string()),
        layers,
    })
}

/// Parse a JSON value into a StyleValue<Color> — either a literal color string or an expression.
fn parse_color_value(value: &Option<serde_json::Value>) -> Option<StyleValue<Color>> {
    let val = value.as_ref()?;
    match val {
        serde_json::Value::String(s) => parse_css_color(s).map(StyleValue::Literal),
        serde_json::Value::Array(_) => {
            crate::expr::parse_expression(val).map(StyleValue::Expression)
        }
        _ => None,
    }
}

/// Parse a JSON value into a StyleValue<f32> — either a literal number or an expression.
fn parse_number_value(value: &Option<serde_json::Value>) -> Option<StyleValue<f32>> {
    let val = value.as_ref()?;
    match val {
        serde_json::Value::Number(n) => n.as_f64().map(|v| StyleValue::Literal(v as f32)),
        serde_json::Value::Array(_) => {
            crate::expr::parse_expression(val).map(StyleValue::Expression)
        }
        _ => None,
    }
}

/// Parse a JSON value into a StyleValue<String> — either a literal string or an expression.
fn parse_string_value(value: &Option<serde_json::Value>) -> Option<StyleValue<String>> {
    let val = value.as_ref()?;
    match val {
        serde_json::Value::String(s) => Some(StyleValue::Literal(s.clone())),
        serde_json::Value::Array(_) => {
            crate::expr::parse_expression(val).map(StyleValue::Expression)
        }
        _ => None,
    }
}

fn parse_line_cap(s: &str) -> LineCap {
    match s {
        "round" => LineCap::Round,
        "square" => LineCap::Square,
        _ => LineCap::Butt,
    }
}

fn parse_line_join(s: &str) -> LineJoin {
    match s {
        "round" => LineJoin::Round,
        "bevel" => LineJoin::Bevel,
        _ => LineJoin::Miter,
    }
}

/// Parse a CSS color string (#rgb, #rrggbb, #rrggbbaa, or named colors).
fn parse_css_color(s: &str) -> Option<Color> {
    let s = s.trim();

    // Named colors
    match s.to_lowercase().as_str() {
        "red" => return Some(Color::rgb(255, 0, 0)),
        "green" => return Some(Color::rgb(0, 128, 0)),
        "blue" => return Some(Color::rgb(0, 0, 255)),
        "white" => return Some(Color::rgb(255, 255, 255)),
        "black" => return Some(Color::rgb(0, 0, 0)),
        "yellow" => return Some(Color::rgb(255, 255, 0)),
        "cyan" => return Some(Color::rgb(0, 255, 255)),
        "magenta" => return Some(Color::rgb(255, 0, 255)),
        "transparent" => return Some(Color::rgba(0, 0, 0, 0)),
        _ => {}
    }

    // Hex colors
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    // rgba(r, g, b, a) and rgb(r, g, b)
    if let Some(inner) = s.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 4 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            let a = (parts[3].trim().parse::<f32>().ok()? * 255.0) as u8;
            return Some(Color::rgba(r, g, b, a));
        }
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some(Color::rgb(r, g, b));
        }
    }

    None
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some(Color::rgb(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::rgb(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Color::rgba(r, g, b, a))
        }
        _ => None,
    }
}

/// Public accessor for CSS color parsing (used by the expression module).
pub fn parse_css_color_pub(s: &str) -> Option<Color> {
    parse_css_color(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::StyleValue;

    #[test]
    fn parse_hex_colors() {
        assert_eq!(parse_css_color("#ff0000"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(
            parse_css_color("#00ff0080"),
            Some(Color::rgba(0, 255, 0, 128))
        );
        assert_eq!(parse_css_color("#fff"), Some(Color::rgb(255, 255, 255)));
    }

    #[test]
    fn parse_named_colors() {
        assert_eq!(parse_css_color("red"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(
            parse_css_color("transparent"),
            Some(Color::rgba(0, 0, 0, 0))
        );
    }

    #[test]
    fn parse_rgb_rgba() {
        assert_eq!(
            parse_css_color("rgb(128, 64, 32)"),
            Some(Color::rgb(128, 64, 32))
        );
        assert_eq!(
            parse_css_color("rgba(255, 0, 0, 0.5)"),
            Some(Color::rgba(255, 0, 0, 127))
        );
    }

    #[test]
    fn parse_minimal_style() {
        let json = r##"{
            "name": "test-style",
            "layers": [
                {
                    "id": "points",
                    "source": "my-source",
                    "paint": {
                        "circle-color": "#ff0000",
                        "circle-radius": 5.0
                    },
                    "layout": {}
                }
            ]
        }"##;
        let style = parse_style(json).unwrap();
        assert_eq!(style.name, "test-style");
        assert_eq!(style.layers.len(), 1);
        assert_eq!(style.layers[0].id, "points");
        assert_eq!(
            style.layers[0].fill_color,
            Some(StyleValue::Literal(Color::rgb(255, 0, 0)))
        );
        assert_eq!(style.layers[0].point_radius, Some(StyleValue::Literal(5.0)));
    }

    #[test]
    fn parse_multi_layer_style() {
        let json = r##"{
            "layers": [
                {
                    "id": "fill-layer",
                    "paint": { "fill-color": "blue" }
                },
                {
                    "id": "line-layer",
                    "paint": { "line-color": "#00ff00", "line-width": 2.0 }
                },
                {
                    "id": "label-layer",
                    "paint": { "text-color": "white" },
                    "layout": { "text-field": "{name}", "text-size": 14.0 }
                }
            ]
        }"##;
        let style = parse_style(json).unwrap();
        assert_eq!(style.name, "untitled");
        assert_eq!(style.layers.len(), 3);
        assert_eq!(
            style.layers[1].stroke_color,
            Some(StyleValue::Literal(Color::rgb(0, 255, 0)))
        );
        assert_eq!(style.layers[1].stroke_width, Some(StyleValue::Literal(2.0)));
        assert_eq!(
            style.layers[2].text_field,
            Some(StyleValue::Literal("{name}".to_string()))
        );
        assert_eq!(style.layers[2].font_size, Some(StyleValue::Literal(14.0)));
    }

    #[test]
    fn invalid_json() {
        assert!(parse_style("not json").is_err());
    }
}
