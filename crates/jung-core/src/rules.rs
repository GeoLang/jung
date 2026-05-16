//! Rule-based cascading style engine.
//!
//! Evaluates multiple style rules per feature with priority-based cascading.
//! Rules are evaluated in order; matching rules accumulate their style
//! properties. Higher-priority rules override lower-priority ones.

use jung_style::{EvalContext, Expression, evaluate};
use std::collections::HashMap;

/// A style rule with a filter condition and style properties.
#[derive(Debug, Clone)]
pub struct StyleRule {
    /// Rule name (for debugging / identification).
    pub name: String,
    /// Priority (higher = overrides lower).
    pub priority: i32,
    /// Filter expression. If None, rule always matches.
    pub filter: Option<Expression>,
    /// Style properties applied when this rule matches.
    pub properties: HashMap<String, RuleValue>,
    /// Minimum zoom level for this rule.
    pub min_zoom: Option<f64>,
    /// Maximum zoom level for this rule.
    pub max_zoom: Option<f64>,
}

/// A resolved style value from a rule.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleValue {
    Color(String),
    Number(f64),
    Text(String),
    Bool(bool),
}

/// A cascaded style: the result of evaluating all matching rules.
#[derive(Debug, Clone, Default)]
pub struct CascadedStyle {
    pub properties: HashMap<String, RuleValue>,
    /// Track which rule set each property (for debugging).
    pub sources: HashMap<String, String>,
}

impl CascadedStyle {
    /// Get a color property value.
    pub fn get_color(&self, key: &str) -> Option<&str> {
        match self.properties.get(key) {
            Some(RuleValue::Color(c)) => Some(c.as_str()),
            _ => None,
        }
    }

    /// Get a numeric property value.
    pub fn get_number(&self, key: &str) -> Option<f64> {
        match self.properties.get(key) {
            Some(RuleValue::Number(n)) => Some(*n),
            _ => None,
        }
    }

    /// Get a text property value.
    pub fn get_text(&self, key: &str) -> Option<&str> {
        match self.properties.get(key) {
            Some(RuleValue::Text(t)) => Some(t.as_str()),
            _ => None,
        }
    }

    /// Get a boolean property value.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.properties.get(key) {
            Some(RuleValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }
}

/// A style ruleset containing multiple ordered rules.
#[derive(Debug, Clone)]
pub struct Ruleset {
    pub rules: Vec<StyleRule>,
}

impl Ruleset {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a rule to the set.
    pub fn add_rule(&mut self, rule: StyleRule) {
        self.rules.push(rule);
        self.rules.sort_by_key(|r| r.priority);
    }

    /// Evaluate all rules against a feature and return the cascaded style.
    pub fn evaluate(&self, context: &EvalContext<'_>) -> CascadedStyle {
        let mut result = CascadedStyle::default();

        for rule in &self.rules {
            // Check zoom bounds
            if rule.min_zoom.is_some_and(|min_z| context.zoom < min_z) {
                continue;
            }
            if rule.max_zoom.is_some_and(|max_z| context.zoom > max_z) {
                continue;
            }

            // Evaluate filter
            if let Some(filter) = &rule.filter {
                match evaluate(filter, context) {
                    jung_style::ExprValue::Boolean(true) => {}
                    _ => continue,
                }
            }

            // Apply properties (cascade: later/higher-priority overrides)
            for (key, value) in &rule.properties {
                result.properties.insert(key.clone(), value.clone());
                result.sources.insert(key.clone(), rule.name.clone());
            }
        }

        result
    }
}

impl Default for Ruleset {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing rules fluently.
pub struct RuleBuilder {
    rule: StyleRule,
}

impl RuleBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            rule: StyleRule {
                name: name.to_string(),
                priority: 0,
                filter: None,
                properties: HashMap::new(),
                min_zoom: None,
                max_zoom: None,
            },
        }
    }

    pub fn priority(mut self, p: i32) -> Self {
        self.rule.priority = p;
        self
    }

    pub fn filter(mut self, expr: Expression) -> Self {
        self.rule.filter = Some(expr);
        self
    }

    pub fn min_zoom(mut self, z: f64) -> Self {
        self.rule.min_zoom = Some(z);
        self
    }

    pub fn max_zoom(mut self, z: f64) -> Self {
        self.rule.max_zoom = Some(z);
        self
    }

    pub fn color(mut self, key: &str, value: &str) -> Self {
        self.rule
            .properties
            .insert(key.to_string(), RuleValue::Color(value.to_string()));
        self
    }

    pub fn number(mut self, key: &str, value: f64) -> Self {
        self.rule
            .properties
            .insert(key.to_string(), RuleValue::Number(value));
        self
    }

    pub fn text(mut self, key: &str, value: &str) -> Self {
        self.rule
            .properties
            .insert(key.to_string(), RuleValue::Text(value.to_string()));
        self
    }

    pub fn boolean(mut self, key: &str, value: bool) -> Self {
        self.rule
            .properties
            .insert(key.to_string(), RuleValue::Bool(value));
        self
    }

    pub fn build(self) -> StyleRule {
        self.rule
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jung_style::{Expression, PropertyValue};

    fn make_context(props: &[(&str, &str)], zoom: f64) -> (HashMap<String, PropertyValue>, f64) {
        let map: HashMap<String, PropertyValue> = props
            .iter()
            .map(|(k, v)| (k.to_string(), PropertyValue::String(v.to_string())))
            .collect();
        (map, zoom)
    }

    #[test]
    fn basic_cascade() {
        let mut ruleset = Ruleset::new();
        ruleset.add_rule(
            RuleBuilder::new("base")
                .priority(0)
                .color("fill", "#ff0000")
                .number("width", 1.0)
                .build(),
        );
        ruleset.add_rule(
            RuleBuilder::new("override")
                .priority(1)
                .color("fill", "#00ff00")
                .build(),
        );

        let (props, zoom) = make_context(&[], 10.0);
        let ctx = EvalContext {
            properties: &props,
            zoom,
            geometry_type: "Polygon",
        };
        let style = ruleset.evaluate(&ctx);
        // Higher priority overrides fill
        assert_eq!(style.get_color("fill"), Some("#00ff00"));
        // Width comes from base
        assert_eq!(style.get_number("width"), Some(1.0));
    }

    #[test]
    fn zoom_filtered_rule() {
        let mut ruleset = Ruleset::new();
        ruleset.add_rule(
            RuleBuilder::new("low-zoom")
                .max_zoom(10.0)
                .number("width", 1.0)
                .build(),
        );
        ruleset.add_rule(
            RuleBuilder::new("high-zoom")
                .min_zoom(10.0)
                .number("width", 3.0)
                .build(),
        );

        let (props, _) = make_context(&[], 5.0);
        let ctx = EvalContext {
            properties: &props,
            zoom: 5.0,
            geometry_type: "LineString",
        };
        let style = ruleset.evaluate(&ctx);
        assert_eq!(style.get_number("width"), Some(1.0));

        let ctx2 = EvalContext {
            properties: &props,
            zoom: 15.0,
            geometry_type: "LineString",
        };
        let style2 = ruleset.evaluate(&ctx2);
        assert_eq!(style2.get_number("width"), Some(3.0));
    }

    #[test]
    fn filter_expression() {
        let mut ruleset = Ruleset::new();
        ruleset.add_rule(
            RuleBuilder::new("highways")
                .filter(Expression::Eq(
                    Box::new(Expression::Get("type".to_string())),
                    Box::new(Expression::Literal(jung_style::ExprValue::String(
                        "highway".to_string(),
                    ))),
                ))
                .color("stroke", "#ff0000")
                .build(),
        );

        let (props, _) = make_context(&[("type", "highway")], 10.0);
        let ctx = EvalContext {
            properties: &props,
            zoom: 10.0,
            geometry_type: "LineString",
        };
        let style = ruleset.evaluate(&ctx);
        assert_eq!(style.get_color("stroke"), Some("#ff0000"));

        // Non-matching feature
        let (props2, _) = make_context(&[("type", "residential")], 10.0);
        let ctx2 = EvalContext {
            properties: &props2,
            zoom: 10.0,
            geometry_type: "LineString",
        };
        let style2 = ruleset.evaluate(&ctx2);
        assert_eq!(style2.get_color("stroke"), None);
    }

    #[test]
    fn sources_tracking() {
        let mut ruleset = Ruleset::new();
        ruleset.add_rule(
            RuleBuilder::new("rule_a")
                .priority(0)
                .color("fill", "#aaa")
                .build(),
        );
        ruleset.add_rule(
            RuleBuilder::new("rule_b")
                .priority(1)
                .color("fill", "#bbb")
                .build(),
        );

        let (props, _) = make_context(&[], 10.0);
        let ctx = EvalContext {
            properties: &props,
            zoom: 10.0,
            geometry_type: "Polygon",
        };
        let style = ruleset.evaluate(&ctx);
        assert_eq!(
            style.sources.get("fill").map(|s| s.as_str()),
            Some("rule_b")
        );
    }

    #[test]
    fn empty_ruleset() {
        let ruleset = Ruleset::new();
        let (props, _) = make_context(&[], 10.0);
        let ctx = EvalContext {
            properties: &props,
            zoom: 10.0,
            geometry_type: "Point",
        };
        let style = ruleset.evaluate(&ctx);
        assert!(style.properties.is_empty());
    }

    #[test]
    fn builder_fluent_api() {
        let rule = RuleBuilder::new("test")
            .priority(5)
            .min_zoom(3.0)
            .max_zoom(18.0)
            .color("fill", "#fff")
            .number("opacity", 0.8)
            .boolean("visible", true)
            .text("label", "hello")
            .build();

        assert_eq!(rule.name, "test");
        assert_eq!(rule.priority, 5);
        assert_eq!(rule.min_zoom, Some(3.0));
        assert_eq!(rule.max_zoom, Some(18.0));
        assert_eq!(rule.properties.len(), 4);
    }
}
