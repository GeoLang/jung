use crate::Loss;
use serde_json::Value;

/// esri sizes are points, one point is 4/3 css pixels at 96 dpi
const PT_TO_PX: f64 = 4.0 / 3.0;

/// used wherever esri means "no paint here"
pub(crate) const TRANSPARENT: &str = "rgba(0,0,0,0)";

pub(crate) fn loss(losses: &mut Vec<Loss>, path: impl Into<String>, reason: impl Into<String>) {
    losses.push(Loss {
        path: path.into(),
        reason: reason.into(),
    });
}

pub(crate) fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// points to css pixels
pub(crate) fn px(points: f64) -> f64 {
    round2(points * PT_TO_PX)
}

/// finite number at `key`, None when absent or unusable
pub(crate) fn number(obj: &Value, key: &str) -> Option<f64> {
    obj.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
}

/// non-empty string at `key`
pub(crate) fn text<'a>(obj: &'a Value, key: &str) -> Option<&'a str> {
    obj.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// esri colors are [r, g, b, a] with alpha in 0-255, absent or null means no paint
pub(crate) fn color(value: Option<&Value>, path: &str, losses: &mut Vec<Loss>) -> String {
    let Some(v) = value.filter(|v| !v.is_null()) else {
        return TRANSPARENT.to_string();
    };
    let Some(parts) = v.as_array() else {
        loss(losses, path, "expected an [r, g, b, a] color array");
        return TRANSPARENT.to_string();
    };
    if parts.len() < 3 || parts.len() > 4 {
        loss(
            losses,
            path,
            format!("color array has {} entries, expected 3 or 4", parts.len()),
        );
        return TRANSPARENT.to_string();
    }
    let mut comps = [0.0, 0.0, 0.0, 255.0];
    for (i, p) in parts.iter().enumerate() {
        match p.as_f64() {
            Some(n) if n.is_finite() => comps[i] = n.clamp(0.0, 255.0),
            _ => {
                loss(losses, path, "color components must be numbers in 0-255");
                return TRANSPARENT.to_string();
            }
        }
    }
    format!(
        "rgba({},{},{},{})",
        comps[0].round() as u8,
        comps[1].round() as u8,
        comps[2].round() as u8,
        alpha(comps[3] / 255.0)
    )
}

/// alpha as the shortest decimal jung and css both read back
fn alpha(a: f64) -> String {
    let s = format!("{a:.3}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn opaque_color() {
        let mut losses = Vec::new();
        let v = json!([255, 128, 0, 255]);
        assert_eq!(color(Some(&v), "p", &mut losses), "rgba(255,128,0,1)");
        assert!(losses.is_empty());
    }

    #[test]
    fn semi_transparent_color() {
        let mut losses = Vec::new();
        let v = json!([0, 0, 0, 128]);
        assert_eq!(color(Some(&v), "p", &mut losses), "rgba(0,0,0,0.502)");
        assert!(losses.is_empty());
    }

    #[test]
    fn three_component_color_is_opaque() {
        let mut losses = Vec::new();
        let v = json!([1, 2, 3]);
        assert_eq!(color(Some(&v), "p", &mut losses), "rgba(1,2,3,1)");
    }

    #[test]
    fn null_color_is_transparent_without_loss() {
        let mut losses = Vec::new();
        assert_eq!(color(Some(&Value::Null), "p", &mut losses), TRANSPARENT);
        assert_eq!(color(None, "p", &mut losses), TRANSPARENT);
        assert!(losses.is_empty());
    }

    #[test]
    fn odd_color_is_a_loss() {
        let mut losses = Vec::new();
        let v = json!("#ff0000");
        assert_eq!(
            color(Some(&v), "renderer.symbol.color", &mut losses),
            TRANSPARENT
        );
        let v = json!([1, 2]);
        assert_eq!(
            color(Some(&v), "renderer.symbol.color", &mut losses),
            TRANSPARENT
        );
        let v = json!([1, 2, "x", 4]);
        assert_eq!(
            color(Some(&v), "renderer.symbol.color", &mut losses),
            TRANSPARENT
        );
        assert_eq!(losses.len(), 3);
    }

    #[test]
    fn points_to_pixels() {
        assert_eq!(px(9.0), 12.0);
        assert_eq!(px(8.0), 10.67);
    }

    #[test]
    fn number_rejects_non_finite_and_wrong_types() {
        let obj = json!({ "a": 3.5, "b": "3.5", "c": null });
        assert_eq!(number(&obj, "a"), Some(3.5));
        assert_eq!(number(&obj, "b"), None);
        assert_eq!(number(&obj, "c"), None);
        assert_eq!(number(&json!(7), "a"), None);
    }
}
