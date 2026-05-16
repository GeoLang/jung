//! OGC Styled Layer Descriptor (SLD) and Symbology Encoding (SE) import/export.
//!
//! Parses SLD 1.1 / SE 1.1 XML into Jung style rules, and exports
//! Jung styles back to SLD/SE XML for interoperability.

use crate::rules::{RuleValue, StyleRule};
use jung_style::{ExprValue, Expression};
use std::collections::HashMap;

/// Errors during SLD parsing.
#[derive(Debug, thiserror::Error)]
pub enum SldError {
    #[error("XML parse error: {0}")]
    XmlParse(String),
    #[error("unsupported SLD element: {0}")]
    Unsupported(String),
    #[error("missing required element: {0}")]
    MissingElement(String),
}

/// An SLD document containing named layers.
#[derive(Debug, Clone)]
pub struct SldDocument {
    pub version: String,
    pub named_layers: Vec<NamedLayer>,
}

/// A named layer in the SLD document.
#[derive(Debug, Clone)]
pub struct NamedLayer {
    pub name: String,
    pub styles: Vec<SldStyle>,
}

/// A style within an SLD layer.
#[derive(Debug, Clone)]
pub struct SldStyle {
    pub name: String,
    pub rules: Vec<SldRule>,
}

/// A symbolizer rule parsed from SLD.
#[derive(Debug, Clone)]
pub struct SldRule {
    pub name: Option<String>,
    pub min_scale: Option<f64>,
    pub max_scale: Option<f64>,
    pub filter: Option<SldFilter>,
    pub symbolizers: Vec<Symbolizer>,
}

/// SLD filter expression.
#[derive(Debug, Clone)]
pub enum SldFilter {
    PropertyIsEqualTo { property: String, literal: String },
    PropertyIsGreaterThan { property: String, literal: String },
    PropertyIsLessThan { property: String, literal: String },
    And(Vec<SldFilter>),
    Or(Vec<SldFilter>),
    Not(Box<SldFilter>),
}

/// SLD symbolizer types.
#[derive(Debug, Clone)]
pub enum Symbolizer {
    Point {
        size: f64,
        color: String,
        shape: String,
    },
    Line {
        color: String,
        width: f64,
        dash_array: Option<Vec<f64>>,
    },
    Polygon {
        fill_color: String,
        fill_opacity: f64,
        stroke_color: String,
        stroke_width: f64,
    },
    Text {
        label_property: String,
        font_family: String,
        font_size: f64,
        color: String,
        halo_color: Option<String>,
        halo_radius: Option<f64>,
    },
}

/// Parse an SLD XML string into an SldDocument.
pub fn parse_sld(xml: &str) -> Result<SldDocument, SldError> {
    // Minimal XML parser using string scanning
    // (avoiding heavy XML crate dependency)
    let version = extract_attribute(xml, "StyledLayerDescriptor", "version")
        .unwrap_or_else(|| "1.1.0".to_string());

    let named_layers = parse_named_layers(xml)?;

    Ok(SldDocument {
        version,
        named_layers,
    })
}

/// Convert an SLD document to Jung style rules.
pub fn sld_to_rules(doc: &SldDocument) -> Vec<StyleRule> {
    let mut rules = Vec::new();
    let mut priority = 0;

    for layer in &doc.named_layers {
        for style in &layer.styles {
            for sld_rule in &style.rules {
                let filter = sld_rule.filter.as_ref().and_then(sld_filter_to_expr);

                let mut properties = HashMap::new();
                for sym in &sld_rule.symbolizers {
                    match sym {
                        Symbolizer::Line { color, width, .. } => {
                            properties.insert("line-color".into(), RuleValue::Color(color.clone()));
                            properties.insert("line-width".into(), RuleValue::Number(*width));
                        }
                        Symbolizer::Polygon {
                            fill_color,
                            fill_opacity,
                            stroke_color,
                            stroke_width,
                        } => {
                            properties
                                .insert("fill-color".into(), RuleValue::Color(fill_color.clone()));
                            properties
                                .insert("fill-opacity".into(), RuleValue::Number(*fill_opacity));
                            properties.insert(
                                "stroke-color".into(),
                                RuleValue::Color(stroke_color.clone()),
                            );
                            properties
                                .insert("stroke-width".into(), RuleValue::Number(*stroke_width));
                        }
                        Symbolizer::Point { size, color, .. } => {
                            properties
                                .insert("circle-color".into(), RuleValue::Color(color.clone()));
                            properties.insert("circle-radius".into(), RuleValue::Number(*size));
                        }
                        Symbolizer::Text {
                            label_property,
                            font_size,
                            color,
                            ..
                        } => {
                            properties.insert(
                                "text-field".into(),
                                RuleValue::Text(label_property.clone()),
                            );
                            properties.insert("text-size".into(), RuleValue::Number(*font_size));
                            properties.insert("text-color".into(), RuleValue::Color(color.clone()));
                        }
                    }
                }

                let min_zoom = sld_rule.max_scale.map(scale_to_zoom);
                let max_zoom = sld_rule.min_scale.map(scale_to_zoom);

                rules.push(StyleRule {
                    name: sld_rule
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("rule-{priority}")),
                    priority,
                    filter,
                    properties,
                    min_zoom,
                    max_zoom,
                });
                priority += 1;
            }
        }
    }

    rules
}

/// Export Jung style rules to SLD XML.
pub fn rules_to_sld(rules: &[StyleRule], layer_name: &str) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(r#"<StyledLayerDescriptor version="1.1.0" "#);
    xml.push_str(r#"xmlns="http://www.opengis.net/sld" "#);
    xml.push_str(r#"xmlns:ogc="http://www.opengis.net/ogc" "#);
    xml.push_str(r#"xmlns:se="http://www.opengis.net/se">"#);
    xml.push('\n');
    xml.push_str("  <NamedLayer>\n");
    xml.push_str(&format!("    <se:Name>{layer_name}</se:Name>\n"));
    xml.push_str("    <UserStyle>\n");
    xml.push_str("      <se:Name>default</se:Name>\n");

    for rule in rules {
        xml.push_str("      <se:Rule>\n");
        xml.push_str(&format!("        <se:Name>{}</se:Name>\n", rule.name));

        if let Some(min_zoom) = rule.min_zoom {
            let scale = zoom_to_scale(min_zoom);
            xml.push_str(&format!(
                "        <se:MaxScaleDenominator>{scale}</se:MaxScaleDenominator>\n"
            ));
        }
        if let Some(max_zoom) = rule.max_zoom {
            let scale = zoom_to_scale(max_zoom);
            xml.push_str(&format!(
                "        <se:MinScaleDenominator>{scale}</se:MinScaleDenominator>\n"
            ));
        }

        // Emit symbolizers based on properties
        if rule.properties.contains_key("line-color") {
            xml.push_str("        <se:LineSymbolizer>\n");
            xml.push_str("          <se:Stroke>\n");
            if let Some(RuleValue::Color(c)) = rule.properties.get("line-color") {
                xml.push_str(&format!(
                    "            <se:SvgParameter name=\"stroke\">{c}</se:SvgParameter>\n"
                ));
            }
            if let Some(RuleValue::Number(w)) = rule.properties.get("line-width") {
                xml.push_str(&format!(
                    "            <se:SvgParameter name=\"stroke-width\">{w}</se:SvgParameter>\n"
                ));
            }
            xml.push_str("          </se:Stroke>\n");
            xml.push_str("        </se:LineSymbolizer>\n");
        }

        if rule.properties.contains_key("fill-color") {
            xml.push_str("        <se:PolygonSymbolizer>\n");
            xml.push_str("          <se:Fill>\n");
            if let Some(RuleValue::Color(c)) = rule.properties.get("fill-color") {
                xml.push_str(&format!(
                    "            <se:SvgParameter name=\"fill\">{c}</se:SvgParameter>\n"
                ));
            }
            xml.push_str("          </se:Fill>\n");
            xml.push_str("        </se:PolygonSymbolizer>\n");
        }

        xml.push_str("      </se:Rule>\n");
    }

    xml.push_str("    </UserStyle>\n");
    xml.push_str("  </NamedLayer>\n");
    xml.push_str("</StyledLayerDescriptor>\n");

    xml
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Convert OGC scale denominator to approximate zoom level.
fn scale_to_zoom(scale: f64) -> f64 {
    if scale <= 0.0 {
        return 0.0;
    }
    (559_082_264.0 / scale).log2()
}

/// Convert zoom level to OGC scale denominator.
fn zoom_to_scale(zoom: f64) -> f64 {
    559_082_264.0 / 2.0_f64.powf(zoom)
}

/// Convert an SLD filter to a Jung expression.
fn sld_filter_to_expr(filter: &SldFilter) -> Option<Expression> {
    match filter {
        SldFilter::PropertyIsEqualTo { property, literal } => Some(Expression::Eq(
            Box::new(Expression::Get(property.clone())),
            Box::new(Expression::Literal(ExprValue::String(literal.clone()))),
        )),
        SldFilter::PropertyIsGreaterThan { property, literal } => {
            if let Ok(n) = literal.parse::<f64>() {
                Some(Expression::Gt(
                    Box::new(Expression::Get(property.clone())),
                    Box::new(Expression::Literal(ExprValue::Number(n))),
                ))
            } else {
                None
            }
        }
        SldFilter::PropertyIsLessThan { property, literal } => {
            if let Ok(n) = literal.parse::<f64>() {
                Some(Expression::Lt(
                    Box::new(Expression::Get(property.clone())),
                    Box::new(Expression::Literal(ExprValue::Number(n))),
                ))
            } else {
                None
            }
        }
        SldFilter::And(filters) => {
            let exprs: Vec<Expression> = filters.iter().filter_map(sld_filter_to_expr).collect();
            if exprs.len() >= 2 {
                Some(Expression::All(exprs))
            } else {
                exprs.into_iter().next()
            }
        }
        SldFilter::Or(filters) => {
            let exprs: Vec<Expression> = filters.iter().filter_map(sld_filter_to_expr).collect();
            if exprs.len() >= 2 {
                Some(Expression::Any(exprs))
            } else {
                exprs.into_iter().next()
            }
        }
        SldFilter::Not(inner) => sld_filter_to_expr(inner).map(|e| Expression::Not(Box::new(e))),
    }
}

fn extract_attribute(xml: &str, element: &str, attr: &str) -> Option<String> {
    let tag_start = xml.find(element)?;
    let after_tag = &xml[tag_start..];
    let attr_pattern = format!("{attr}=\"");
    let attr_start = after_tag.find(&attr_pattern)?;
    let value_start = attr_start + attr_pattern.len();
    let value_end = after_tag[value_start..].find('"')?;
    Some(after_tag[value_start..value_start + value_end].to_string())
}

fn parse_named_layers(xml: &str) -> Result<Vec<NamedLayer>, SldError> {
    let mut layers = Vec::new();
    let mut search_from = 0;

    while let Some(start) = xml[search_from..].find("<NamedLayer") {
        let abs_start = search_from + start;
        let end = xml[abs_start..]
            .find("</NamedLayer>")
            .ok_or_else(|| SldError::XmlParse("unclosed NamedLayer".into()))?;
        let layer_xml = &xml[abs_start..abs_start + end + "</NamedLayer>".len()];

        let name = extract_element_text(layer_xml, "se:Name")
            .or_else(|| extract_element_text(layer_xml, "Name"))
            .unwrap_or_else(|| "unnamed".to_string());

        let styles = parse_user_styles(layer_xml)?;
        layers.push(NamedLayer { name, styles });

        search_from = abs_start + end + "</NamedLayer>".len();
    }

    Ok(layers)
}

fn parse_user_styles(xml: &str) -> Result<Vec<SldStyle>, SldError> {
    let mut styles = Vec::new();
    let mut search_from = 0;

    while let Some(start) = xml[search_from..].find("<UserStyle") {
        let abs_start = search_from + start;
        let end = xml[abs_start..]
            .find("</UserStyle>")
            .ok_or_else(|| SldError::XmlParse("unclosed UserStyle".into()))?;
        let style_xml = &xml[abs_start..abs_start + end + "</UserStyle>".len()];

        let name =
            extract_element_text(style_xml, "se:Name").unwrap_or_else(|| "default".to_string());
        let rules = parse_sld_rules(style_xml);

        styles.push(SldStyle { name, rules });
        search_from = abs_start + end + "</UserStyle>".len();
    }

    Ok(styles)
}

fn parse_sld_rules(xml: &str) -> Vec<SldRule> {
    let mut rules = Vec::new();
    let mut search_from = 0;

    while let Some(start) = xml[search_from..].find("<se:Rule") {
        let abs_start = search_from + start;
        let end = match xml[abs_start..].find("</se:Rule>") {
            Some(e) => e,
            None => break,
        };
        let rule_xml = &xml[abs_start..abs_start + end + "</se:Rule>".len()];

        let name = extract_element_text(rule_xml, "se:Name");
        let min_scale =
            extract_element_text(rule_xml, "se:MinScaleDenominator").and_then(|s| s.parse().ok());
        let max_scale =
            extract_element_text(rule_xml, "se:MaxScaleDenominator").and_then(|s| s.parse().ok());

        let symbolizers = parse_symbolizers(rule_xml);

        rules.push(SldRule {
            name,
            min_scale,
            max_scale,
            filter: None, // Simplified: filter parsing can be added later
            symbolizers,
        });

        search_from = abs_start + end + "</se:Rule>".len();
    }

    rules
}

fn parse_symbolizers(xml: &str) -> Vec<Symbolizer> {
    let mut symbolizers = Vec::new();

    if xml.contains("<se:LineSymbolizer") {
        let color = extract_svg_param(xml, "stroke").unwrap_or_else(|| "#000000".into());
        let width = extract_svg_param(xml, "stroke-width")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);
        symbolizers.push(Symbolizer::Line {
            color,
            width,
            dash_array: None,
        });
    }

    if xml.contains("<se:PolygonSymbolizer") {
        let fill = extract_svg_param(xml, "fill").unwrap_or_else(|| "#cccccc".into());
        let opacity = extract_svg_param(xml, "fill-opacity")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);
        symbolizers.push(Symbolizer::Polygon {
            fill_color: fill,
            fill_opacity: opacity,
            stroke_color: "#000000".into(),
            stroke_width: 1.0,
        });
    }

    symbolizers
}

fn extract_element_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)?;
    Some(xml[start..start + end].trim().to_string())
}

fn extract_svg_param(xml: &str, name: &str) -> Option<String> {
    let pattern = format!(r#"name="{name}""#);
    let pos = xml.find(&pattern)?;
    let after = &xml[pos + pattern.len()..];
    let start = after.find('>')? + 1;
    let end = after[start..].find('<')?;
    Some(after[start..start + end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SLD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<StyledLayerDescriptor version="1.1.0" xmlns="http://www.opengis.net/sld"
  xmlns:se="http://www.opengis.net/se" xmlns:ogc="http://www.opengis.net/ogc">
  <NamedLayer>
    <se:Name>roads</se:Name>
    <UserStyle>
      <se:Name>default</se:Name>
      <se:Rule>
        <se:Name>highway</se:Name>
        <se:MaxScaleDenominator>100000</se:MaxScaleDenominator>
        <se:LineSymbolizer>
          <se:Stroke>
            <se:SvgParameter name="stroke">#ff0000</se:SvgParameter>
            <se:SvgParameter name="stroke-width">3</se:SvgParameter>
          </se:Stroke>
        </se:LineSymbolizer>
      </se:Rule>
      <se:Rule>
        <se:Name>local</se:Name>
        <se:PolygonSymbolizer>
          <se:Fill>
            <se:SvgParameter name="fill">#00ff00</se:SvgParameter>
            <se:SvgParameter name="fill-opacity">0.5</se:SvgParameter>
          </se:Fill>
        </se:PolygonSymbolizer>
      </se:Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

    #[test]
    fn test_parse_sld() {
        let doc = parse_sld(SAMPLE_SLD).unwrap();
        assert_eq!(doc.version, "1.1.0");
        assert_eq!(doc.named_layers.len(), 1);
        assert_eq!(doc.named_layers[0].name, "roads");
        assert_eq!(doc.named_layers[0].styles[0].rules.len(), 2);
    }

    #[test]
    fn test_sld_to_rules() {
        let doc = parse_sld(SAMPLE_SLD).unwrap();
        let rules = sld_to_rules(&doc);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name, "highway");
        assert!(rules[0].properties.contains_key("line-color"));
    }

    #[test]
    fn test_rules_to_sld() {
        let rules = vec![StyleRule {
            name: "test-rule".into(),
            priority: 0,
            filter: None,
            properties: HashMap::from([
                ("line-color".into(), RuleValue::Color("#0000ff".into())),
                ("line-width".into(), RuleValue::Number(2.0)),
            ]),
            min_zoom: Some(5.0),
            max_zoom: Some(15.0),
        }];
        let xml = rules_to_sld(&rules, "test-layer");
        assert!(xml.contains("StyledLayerDescriptor"));
        assert!(xml.contains("test-layer"));
        assert!(xml.contains("#0000ff"));
    }

    #[test]
    fn test_scale_zoom_conversion() {
        let zoom = scale_to_zoom(559_082_264.0);
        assert!((zoom - 0.0).abs() < 0.01);

        let scale = zoom_to_scale(10.0);
        let roundtrip = scale_to_zoom(scale);
        assert!((roundtrip - 10.0).abs() < 0.01);
    }
}
