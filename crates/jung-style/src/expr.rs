use crate::Color;
use serde_json::Value;
use std::collections::HashMap;

/// Runtime value produced by expression evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprValue {
    Number(f64),
    String(String),
    Boolean(bool),
    Color(Color),
    Array(Vec<ExprValue>),
    Null,
}

impl ExprValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            ExprValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            ExprValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ExprValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        match self {
            ExprValue::Color(c) => Some(*c),
            _ => None,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            ExprValue::Null => false,
            ExprValue::Boolean(b) => *b,
            ExprValue::Number(n) => *n != 0.0,
            ExprValue::String(s) => !s.is_empty(),
            ExprValue::Array(a) => !a.is_empty(),
            ExprValue::Color(_) => true,
        }
    }
}

/// Context passed to expression evaluation.
pub struct EvalContext<'a> {
    pub properties: &'a HashMap<String, PropertyValue>,
    pub zoom: f64,
    pub geometry_type: &'a str,
}

/// Property values from features (mirrors jung-core's PropertyValue).
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    String(String),
    Number(f64),
    Integer(i64),
    Boolean(bool),
    Null,
}

impl PropertyValue {
    fn to_expr_value(&self) -> ExprValue {
        match self {
            PropertyValue::String(s) => ExprValue::String(s.clone()),
            PropertyValue::Number(n) => ExprValue::Number(*n),
            PropertyValue::Integer(i) => ExprValue::Number(*i as f64),
            PropertyValue::Boolean(b) => ExprValue::Boolean(*b),
            PropertyValue::Null => ExprValue::Null,
        }
    }
}

/// A style value that is either a literal or a data-driven expression.
#[derive(Debug, Clone, PartialEq)]
pub enum StyleValue<T> {
    Literal(T),
    Expression(Expression),
}

impl StyleValue<Color> {
    pub fn resolve(&self, ctx: &EvalContext) -> Option<Color> {
        match self {
            StyleValue::Literal(c) => Some(*c),
            StyleValue::Expression(expr) => evaluate(expr, ctx).as_color(),
        }
    }
}

impl StyleValue<f32> {
    pub fn resolve(&self, ctx: &EvalContext) -> Option<f32> {
        match self {
            StyleValue::Literal(v) => Some(*v),
            StyleValue::Expression(expr) => evaluate(expr, ctx).as_number().map(|n| n as f32),
        }
    }
}

impl StyleValue<String> {
    pub fn resolve(&self, ctx: &EvalContext) -> Option<String> {
        match self {
            StyleValue::Literal(s) => Some(s.clone()),
            StyleValue::Expression(expr) => evaluate(expr, ctx).as_string().map(|s| s.to_string()),
        }
    }
}

/// Expression AST node — Mapbox GL expression specification.
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    // Literals
    Literal(ExprValue),

    // Data access
    Get(String),
    Has(String),
    GeometryType,
    Id,

    // Zoom
    Zoom,

    // Comparison
    Eq(Box<Expression>, Box<Expression>),
    Neq(Box<Expression>, Box<Expression>),
    Gt(Box<Expression>, Box<Expression>),
    Gte(Box<Expression>, Box<Expression>),
    Lt(Box<Expression>, Box<Expression>),
    Lte(Box<Expression>, Box<Expression>),

    // Logical
    All(Vec<Expression>),
    Any(Vec<Expression>),
    Not(Box<Expression>),

    // Math
    Add(Vec<Expression>),
    Mul(Vec<Expression>),
    Sub(Box<Expression>, Box<Expression>),
    Div(Box<Expression>, Box<Expression>),
    Mod(Box<Expression>, Box<Expression>),
    Min(Vec<Expression>),
    Max(Vec<Expression>),
    Abs(Box<Expression>),
    Floor(Box<Expression>),
    Ceil(Box<Expression>),
    Round(Box<Expression>),

    // String
    Concat(Vec<Expression>),
    Upcase(Box<Expression>),
    Downcase(Box<Expression>),

    // Control flow
    Case(Vec<(Expression, Expression)>, Box<Expression>),
    Match {
        input: Box<Expression>,
        cases: Vec<(ExprValue, Expression)>,
        fallback: Box<Expression>,
    },
    Coalesce(Vec<Expression>),

    // Interpolation
    Interpolate {
        interpolation: Interpolation,
        input: Box<Expression>,
        stops: Vec<(f64, Expression)>,
    },

    // Step function
    Step {
        input: Box<Expression>,
        default: Box<Expression>,
        stops: Vec<(f64, Expression)>,
    },

    // Type conversion
    ToNumber(Box<Expression>),
    ToString(Box<Expression>),
    ToBoolean(Box<Expression>),
    ToColor(Box<Expression>),
}

/// Interpolation method for interpolate expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Interpolation {
    Linear,
    Exponential(f64),
    CubicBezier(f64, f64, f64, f64),
}

/// Evaluate an expression in the given context.
pub fn evaluate(expr: &Expression, ctx: &EvalContext) -> ExprValue {
    match expr {
        Expression::Literal(v) => v.clone(),

        Expression::Get(key) => ctx
            .properties
            .get(key)
            .map(|v| v.to_expr_value())
            .unwrap_or(ExprValue::Null),

        Expression::Has(key) => ExprValue::Boolean(ctx.properties.contains_key(key)),

        Expression::GeometryType => ExprValue::String(ctx.geometry_type.to_string()),

        Expression::Id => ExprValue::Null,

        Expression::Zoom => ExprValue::Number(ctx.zoom),

        // Comparison operators
        Expression::Eq(a, b) => ExprValue::Boolean(evaluate(a, ctx) == evaluate(b, ctx)),
        Expression::Neq(a, b) => ExprValue::Boolean(evaluate(a, ctx) != evaluate(b, ctx)),
        Expression::Gt(a, b) => {
            let (va, vb) = (evaluate(a, ctx), evaluate(b, ctx));
            ExprValue::Boolean(compare_values(&va, &vb) == Some(std::cmp::Ordering::Greater))
        }
        Expression::Gte(a, b) => {
            let (va, vb) = (evaluate(a, ctx), evaluate(b, ctx));
            ExprValue::Boolean(matches!(
                compare_values(&va, &vb),
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            ))
        }
        Expression::Lt(a, b) => {
            let (va, vb) = (evaluate(a, ctx), evaluate(b, ctx));
            ExprValue::Boolean(compare_values(&va, &vb) == Some(std::cmp::Ordering::Less))
        }
        Expression::Lte(a, b) => {
            let (va, vb) = (evaluate(a, ctx), evaluate(b, ctx));
            ExprValue::Boolean(matches!(
                compare_values(&va, &vb),
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            ))
        }

        // Logical
        Expression::All(exprs) => {
            ExprValue::Boolean(exprs.iter().all(|e| evaluate(e, ctx).is_truthy()))
        }
        Expression::Any(exprs) => {
            ExprValue::Boolean(exprs.iter().any(|e| evaluate(e, ctx).is_truthy()))
        }
        Expression::Not(e) => ExprValue::Boolean(!evaluate(e, ctx).is_truthy()),

        // Math
        Expression::Add(exprs) => {
            let sum: f64 = exprs
                .iter()
                .filter_map(|e| evaluate(e, ctx).as_number())
                .sum();
            ExprValue::Number(sum)
        }
        Expression::Mul(exprs) => {
            let product: f64 = exprs
                .iter()
                .filter_map(|e| evaluate(e, ctx).as_number())
                .product();
            ExprValue::Number(product)
        }
        Expression::Sub(a, b) => {
            let va = evaluate(a, ctx).as_number().unwrap_or(0.0);
            let vb = evaluate(b, ctx).as_number().unwrap_or(0.0);
            ExprValue::Number(va - vb)
        }
        Expression::Div(a, b) => {
            let va = evaluate(a, ctx).as_number().unwrap_or(0.0);
            let vb = evaluate(b, ctx).as_number().unwrap_or(1.0);
            if vb == 0.0 {
                ExprValue::Number(0.0)
            } else {
                ExprValue::Number(va / vb)
            }
        }
        Expression::Mod(a, b) => {
            let va = evaluate(a, ctx).as_number().unwrap_or(0.0);
            let vb = evaluate(b, ctx).as_number().unwrap_or(1.0);
            if vb == 0.0 {
                ExprValue::Number(0.0)
            } else {
                ExprValue::Number(va % vb)
            }
        }
        Expression::Min(exprs) => {
            let min = exprs
                .iter()
                .filter_map(|e| evaluate(e, ctx).as_number())
                .fold(f64::INFINITY, f64::min);
            ExprValue::Number(min)
        }
        Expression::Max(exprs) => {
            let max = exprs
                .iter()
                .filter_map(|e| evaluate(e, ctx).as_number())
                .fold(f64::NEG_INFINITY, f64::max);
            ExprValue::Number(max)
        }
        Expression::Abs(e) => ExprValue::Number(evaluate(e, ctx).as_number().unwrap_or(0.0).abs()),
        Expression::Floor(e) => {
            ExprValue::Number(evaluate(e, ctx).as_number().unwrap_or(0.0).floor())
        }
        Expression::Ceil(e) => {
            ExprValue::Number(evaluate(e, ctx).as_number().unwrap_or(0.0).ceil())
        }
        Expression::Round(e) => {
            ExprValue::Number(evaluate(e, ctx).as_number().unwrap_or(0.0).round())
        }

        // String
        Expression::Concat(exprs) => {
            let s: String = exprs
                .iter()
                .map(|e| value_to_string(&evaluate(e, ctx)))
                .collect();
            ExprValue::String(s)
        }
        Expression::Upcase(e) => {
            ExprValue::String(value_to_string(&evaluate(e, ctx)).to_uppercase())
        }
        Expression::Downcase(e) => {
            ExprValue::String(value_to_string(&evaluate(e, ctx)).to_lowercase())
        }

        // Control flow
        Expression::Case(branches, fallback) => {
            for (condition, result) in branches {
                if evaluate(condition, ctx).is_truthy() {
                    return evaluate(result, ctx);
                }
            }
            evaluate(fallback, ctx)
        }
        Expression::Match {
            input,
            cases,
            fallback,
        } => {
            let input_val = evaluate(input, ctx);
            for (match_val, result) in cases {
                if input_val == *match_val {
                    return evaluate(result, ctx);
                }
            }
            evaluate(fallback, ctx)
        }
        Expression::Coalesce(exprs) => {
            for e in exprs {
                let val = evaluate(e, ctx);
                if val != ExprValue::Null {
                    return val;
                }
            }
            ExprValue::Null
        }

        // Interpolation
        Expression::Interpolate {
            interpolation,
            input,
            stops,
        } => {
            let input_val = evaluate(input, ctx).as_number().unwrap_or(0.0);
            interpolate_stops(interpolation, input_val, stops, ctx)
        }

        // Step
        Expression::Step {
            input,
            default,
            stops,
        } => {
            let input_val = evaluate(input, ctx).as_number().unwrap_or(0.0);
            step_stops(input_val, default, stops, ctx)
        }

        // Type conversion
        Expression::ToNumber(e) => {
            let val = evaluate(e, ctx);
            match val {
                ExprValue::Number(_) => val,
                ExprValue::String(ref s) => ExprValue::Number(s.parse::<f64>().unwrap_or(0.0)),
                ExprValue::Boolean(b) => ExprValue::Number(if b { 1.0 } else { 0.0 }),
                _ => ExprValue::Number(0.0),
            }
        }
        Expression::ToString(e) => ExprValue::String(value_to_string(&evaluate(e, ctx))),
        Expression::ToBoolean(e) => ExprValue::Boolean(evaluate(e, ctx).is_truthy()),
        Expression::ToColor(e) => {
            let val = evaluate(e, ctx);
            match val {
                ExprValue::Color(_) => val,
                ExprValue::String(ref s) => crate::parse_css_color_pub(s)
                    .map(ExprValue::Color)
                    .unwrap_or(ExprValue::Null),
                _ => ExprValue::Null,
            }
        }
    }
}

fn compare_values(a: &ExprValue, b: &ExprValue) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (ExprValue::Number(a), ExprValue::Number(b)) => a.partial_cmp(b),
        (ExprValue::String(a), ExprValue::String(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

fn value_to_string(val: &ExprValue) -> String {
    match val {
        ExprValue::String(s) => s.clone(),
        ExprValue::Number(n) => n.to_string(),
        ExprValue::Boolean(b) => b.to_string(),
        ExprValue::Null => String::new(),
        ExprValue::Color(c) => format!("rgba({},{},{},{})", c.r, c.g, c.b, c.a),
        ExprValue::Array(_) => "[array]".to_string(),
    }
}

fn interpolate_stops(
    interpolation: &Interpolation,
    input: f64,
    stops: &[(f64, Expression)],
    ctx: &EvalContext,
) -> ExprValue {
    if stops.is_empty() {
        return ExprValue::Null;
    }

    // Below first stop
    if input <= stops[0].0 {
        return evaluate(&stops[0].1, ctx);
    }

    // Above last stop
    if input >= stops[stops.len() - 1].0 {
        return evaluate(&stops[stops.len() - 1].1, ctx);
    }

    // Find the two stops to interpolate between
    for i in 0..stops.len() - 1 {
        if input >= stops[i].0 && input < stops[i + 1].0 {
            let t = (input - stops[i].0) / (stops[i + 1].0 - stops[i].0);
            let t = apply_interpolation(interpolation, t);

            let v0 = evaluate(&stops[i].1, ctx);
            let v1 = evaluate(&stops[i + 1].1, ctx);

            return interpolate_values(&v0, &v1, t);
        }
    }

    ExprValue::Null
}

fn apply_interpolation(interpolation: &Interpolation, t: f64) -> f64 {
    match interpolation {
        Interpolation::Linear => t,
        Interpolation::Exponential(base) => {
            if (*base - 1.0).abs() < 1e-6 {
                t
            } else {
                (base.powf(t) - 1.0) / (base - 1.0)
            }
        }
        Interpolation::CubicBezier(x1, y1, x2, y2) => cubic_bezier_at(t, *x1, *y1, *x2, *y2),
    }
}

fn interpolate_values(a: &ExprValue, b: &ExprValue, t: f64) -> ExprValue {
    match (a, b) {
        (ExprValue::Number(a), ExprValue::Number(b)) => ExprValue::Number(a + (b - a) * t),
        (ExprValue::Color(a), ExprValue::Color(b)) => ExprValue::Color(Color::rgba(
            lerp_u8(a.r, b.r, t),
            lerp_u8(a.g, b.g, t),
            lerp_u8(a.b, b.b, t),
            lerp_u8(a.a, b.a, t),
        )),
        _ => {
            // Can't interpolate — return first value
            a.clone()
        }
    }
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t).round() as u8
}

fn step_stops(
    input: f64,
    default: &Expression,
    stops: &[(f64, Expression)],
    ctx: &EvalContext,
) -> ExprValue {
    if stops.is_empty() {
        return evaluate(default, ctx);
    }

    // Below first stop → use default
    if input < stops[0].0 {
        return evaluate(default, ctx);
    }

    // Find the last stop <= input
    let mut result = default;
    for (threshold, expr) in stops {
        if input >= *threshold {
            result = expr;
        } else {
            break;
        }
    }
    evaluate(result, ctx)
}

/// Approximate cubic bezier evaluation for interpolation easing.
fn cubic_bezier_at(t: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    // Newton-Raphson to find parameter for x, then evaluate y
    let mut guess = t;
    for _ in 0..8 {
        let x = bezier_component(guess, x1, x2) - t;
        let dx = bezier_derivative(guess, x1, x2);
        if dx.abs() < 1e-10 {
            break;
        }
        guess -= x / dx;
        guess = guess.clamp(0.0, 1.0);
    }
    bezier_component(guess, y1, y2)
}

fn bezier_component(t: f64, p1: f64, p2: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    3.0 * (1.0 - t) * (1.0 - t) * t * p1 + 3.0 * (1.0 - t) * t2 * p2 + t3
}

fn bezier_derivative(t: f64, p1: f64, p2: f64) -> f64 {
    let t2 = t * t;
    3.0 * (1.0 - t) * (1.0 - t) * p1 + 6.0 * (1.0 - t) * t * (p2 - p1) + 3.0 * t2 * (1.0 - p2)
}

/// Parse a JSON value into an Expression.
pub fn parse_expression(value: &Value) -> Option<Expression> {
    match value {
        Value::Null => Some(Expression::Literal(ExprValue::Null)),
        Value::Bool(b) => Some(Expression::Literal(ExprValue::Boolean(*b))),
        Value::Number(n) => Some(Expression::Literal(ExprValue::Number(n.as_f64()?))),
        Value::String(s) => {
            // Try as color first, else string literal
            if let Some(color) = crate::parse_css_color_pub(s) {
                Some(Expression::Literal(ExprValue::Color(color)))
            } else {
                Some(Expression::Literal(ExprValue::String(s.clone())))
            }
        }
        Value::Array(arr) => parse_array_expression(arr),
        Value::Object(_) => None,
    }
}

fn parse_array_expression(arr: &[Value]) -> Option<Expression> {
    let op = arr.first()?.as_str()?;

    match op {
        "get" => {
            let key = arr.get(1)?.as_str()?;
            Some(Expression::Get(key.to_string()))
        }
        "has" => {
            let key = arr.get(1)?.as_str()?;
            Some(Expression::Has(key.to_string()))
        }
        "geometry-type" => Some(Expression::GeometryType),
        "id" => Some(Expression::Id),
        "zoom" => Some(Expression::Zoom),

        "==" => {
            let a = parse_expression(arr.get(1)?)?;
            let b = parse_expression(arr.get(2)?)?;
            Some(Expression::Eq(Box::new(a), Box::new(b)))
        }
        "!=" => {
            let a = parse_expression(arr.get(1)?)?;
            let b = parse_expression(arr.get(2)?)?;
            Some(Expression::Neq(Box::new(a), Box::new(b)))
        }
        ">" => {
            let a = parse_expression(arr.get(1)?)?;
            let b = parse_expression(arr.get(2)?)?;
            Some(Expression::Gt(Box::new(a), Box::new(b)))
        }
        ">=" => {
            let a = parse_expression(arr.get(1)?)?;
            let b = parse_expression(arr.get(2)?)?;
            Some(Expression::Gte(Box::new(a), Box::new(b)))
        }
        "<" => {
            let a = parse_expression(arr.get(1)?)?;
            let b = parse_expression(arr.get(2)?)?;
            Some(Expression::Lt(Box::new(a), Box::new(b)))
        }
        "<=" => {
            let a = parse_expression(arr.get(1)?)?;
            let b = parse_expression(arr.get(2)?)?;
            Some(Expression::Lte(Box::new(a), Box::new(b)))
        }

        "all" => {
            let exprs: Option<Vec<_>> = arr[1..].iter().map(parse_expression).collect();
            Some(Expression::All(exprs?))
        }
        "any" => {
            let exprs: Option<Vec<_>> = arr[1..].iter().map(parse_expression).collect();
            Some(Expression::Any(exprs?))
        }
        "!" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::Not(Box::new(e)))
        }

        "+" => {
            let exprs: Option<Vec<_>> = arr[1..].iter().map(parse_expression).collect();
            Some(Expression::Add(exprs?))
        }
        "*" => {
            let exprs: Option<Vec<_>> = arr[1..].iter().map(parse_expression).collect();
            Some(Expression::Mul(exprs?))
        }
        "-" => {
            let a = parse_expression(arr.get(1)?)?;
            let b = parse_expression(arr.get(2)?)?;
            Some(Expression::Sub(Box::new(a), Box::new(b)))
        }
        "/" => {
            let a = parse_expression(arr.get(1)?)?;
            let b = parse_expression(arr.get(2)?)?;
            Some(Expression::Div(Box::new(a), Box::new(b)))
        }
        "%" => {
            let a = parse_expression(arr.get(1)?)?;
            let b = parse_expression(arr.get(2)?)?;
            Some(Expression::Mod(Box::new(a), Box::new(b)))
        }
        "min" => {
            let exprs: Option<Vec<_>> = arr[1..].iter().map(parse_expression).collect();
            Some(Expression::Min(exprs?))
        }
        "max" => {
            let exprs: Option<Vec<_>> = arr[1..].iter().map(parse_expression).collect();
            Some(Expression::Max(exprs?))
        }
        "abs" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::Abs(Box::new(e)))
        }
        "floor" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::Floor(Box::new(e)))
        }
        "ceil" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::Ceil(Box::new(e)))
        }
        "round" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::Round(Box::new(e)))
        }

        "concat" => {
            let exprs: Option<Vec<_>> = arr[1..].iter().map(parse_expression).collect();
            Some(Expression::Concat(exprs?))
        }
        "upcase" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::Upcase(Box::new(e)))
        }
        "downcase" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::Downcase(Box::new(e)))
        }

        "case" => parse_case_expression(arr),
        "match" => parse_match_expression(arr),
        "coalesce" => {
            let exprs: Option<Vec<_>> = arr[1..].iter().map(parse_expression).collect();
            Some(Expression::Coalesce(exprs?))
        }

        "interpolate" => parse_interpolate_expression(arr),
        "step" => parse_step_expression(arr),

        "to-number" | "number" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::ToNumber(Box::new(e)))
        }
        "to-string" | "string" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::ToString(Box::new(e)))
        }
        "to-boolean" | "boolean" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::ToBoolean(Box::new(e)))
        }
        "to-color" | "to-rgba" => {
            let e = parse_expression(arr.get(1)?)?;
            Some(Expression::ToColor(Box::new(e)))
        }

        "literal" => {
            // ["literal", value] — pass through as-is
            let val = arr.get(1)?;
            match val {
                Value::Array(items) => {
                    let exprs: Option<Vec<_>> = items
                        .iter()
                        .map(|v| match v {
                            Value::Number(n) => Some(ExprValue::Number(n.as_f64()?)),
                            Value::String(s) => Some(ExprValue::String(s.clone())),
                            Value::Bool(b) => Some(ExprValue::Boolean(*b)),
                            _ => Some(ExprValue::Null),
                        })
                        .collect();
                    Some(Expression::Literal(ExprValue::Array(exprs?)))
                }
                _ => parse_expression(val),
            }
        }

        _ => None,
    }
}

fn parse_case_expression(arr: &[Value]) -> Option<Expression> {
    // ["case", cond1, result1, cond2, result2, ..., fallback]
    if arr.len() < 4 {
        return None;
    }
    let mut branches = Vec::new();
    let mut i = 1;
    while i + 2 < arr.len() {
        let cond = parse_expression(&arr[i])?;
        let result = parse_expression(&arr[i + 1])?;
        branches.push((cond, result));
        i += 2;
    }
    let fallback = parse_expression(&arr[arr.len() - 1])?;
    Some(Expression::Case(branches, Box::new(fallback)))
}

fn parse_match_expression(arr: &[Value]) -> Option<Expression> {
    // ["match", input, val1, out1, val2, out2, ..., fallback]
    if arr.len() < 5 {
        return None;
    }
    let input = parse_expression(&arr[1])?;
    let mut cases = Vec::new();
    let mut i = 2;
    while i + 2 < arr.len() {
        let match_val = json_to_expr_value(&arr[i])?;
        let result = parse_expression(&arr[i + 1])?;
        cases.push((match_val, result));
        i += 2;
    }
    let fallback = parse_expression(&arr[arr.len() - 1])?;
    Some(Expression::Match {
        input: Box::new(input),
        cases,
        fallback: Box::new(fallback),
    })
}

fn parse_interpolate_expression(arr: &[Value]) -> Option<Expression> {
    // ["interpolate", interpolation, input, stop1, val1, stop2, val2, ...]
    if arr.len() < 6 {
        return None;
    }
    let interpolation = parse_interpolation_type(&arr[1])?;
    let input = parse_expression(&arr[2])?;

    let mut stops = Vec::new();
    let mut i = 3;
    while i + 1 < arr.len() {
        let stop_val = arr[i].as_f64()?;
        let expr = parse_expression(&arr[i + 1])?;
        stops.push((stop_val, expr));
        i += 2;
    }

    Some(Expression::Interpolate {
        interpolation,
        input: Box::new(input),
        stops,
    })
}

fn parse_step_expression(arr: &[Value]) -> Option<Expression> {
    // ["step", input, default, stop1, val1, stop2, val2, ...]
    if arr.len() < 4 {
        return None;
    }
    let input = parse_expression(&arr[1])?;
    let default = parse_expression(&arr[2])?;

    let mut stops = Vec::new();
    let mut i = 3;
    while i + 1 < arr.len() {
        let stop_val = arr[i].as_f64()?;
        let expr = parse_expression(&arr[i + 1])?;
        stops.push((stop_val, expr));
        i += 2;
    }

    Some(Expression::Step {
        input: Box::new(input),
        default: Box::new(default),
        stops,
    })
}

fn parse_interpolation_type(value: &Value) -> Option<Interpolation> {
    let arr = value.as_array()?;
    let kind = arr.first()?.as_str()?;
    match kind {
        "linear" => Some(Interpolation::Linear),
        "exponential" => {
            let base = arr.get(1)?.as_f64()?;
            Some(Interpolation::Exponential(base))
        }
        "cubic-bezier" => {
            let x1 = arr.get(1)?.as_f64()?;
            let y1 = arr.get(2)?.as_f64()?;
            let x2 = arr.get(3)?.as_f64()?;
            let y2 = arr.get(4)?.as_f64()?;
            Some(Interpolation::CubicBezier(x1, y1, x2, y2))
        }
        _ => None,
    }
}

fn json_to_expr_value(value: &Value) -> Option<ExprValue> {
    match value {
        Value::Null => Some(ExprValue::Null),
        Value::Bool(b) => Some(ExprValue::Boolean(*b)),
        Value::Number(n) => Some(ExprValue::Number(n.as_f64()?)),
        Value::String(s) => Some(ExprValue::String(s.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(props: &[(&str, PropertyValue)], zoom: f64) -> EvalContext<'static> {
        let properties: HashMap<String, PropertyValue> = props
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        // Leak to get 'static lifetime for test convenience
        let props_box = Box::new(properties);
        let props_ref: &'static HashMap<String, PropertyValue> = Box::leak(props_box);
        EvalContext {
            properties: props_ref,
            zoom,
            geometry_type: "Point",
        }
    }

    #[test]
    fn eval_get() {
        let ctx = make_ctx(&[("name", PropertyValue::String("NYC".into()))], 10.0);
        let expr = Expression::Get("name".into());
        assert_eq!(evaluate(&expr, &ctx), ExprValue::String("NYC".into()));
    }

    #[test]
    fn eval_get_missing() {
        let ctx = make_ctx(&[], 10.0);
        let expr = Expression::Get("missing".into());
        assert_eq!(evaluate(&expr, &ctx), ExprValue::Null);
    }

    #[test]
    fn eval_math() {
        let ctx = make_ctx(&[("pop", PropertyValue::Number(1000.0))], 5.0);
        let expr = Expression::Mul(vec![
            Expression::Get("pop".into()),
            Expression::Literal(ExprValue::Number(0.001)),
        ]);
        assert_eq!(evaluate(&expr, &ctx), ExprValue::Number(1.0));
    }

    #[test]
    fn eval_comparison() {
        let ctx = make_ctx(&[("pop", PropertyValue::Number(5000.0))], 5.0);
        let expr = Expression::Gt(
            Box::new(Expression::Get("pop".into())),
            Box::new(Expression::Literal(ExprValue::Number(1000.0))),
        );
        assert_eq!(evaluate(&expr, &ctx), ExprValue::Boolean(true));
    }

    #[test]
    fn eval_match() {
        let ctx = make_ctx(
            &[("type", PropertyValue::String("residential".into()))],
            5.0,
        );
        let expr = Expression::Match {
            input: Box::new(Expression::Get("type".into())),
            cases: vec![
                (
                    ExprValue::String("residential".into()),
                    Expression::Literal(ExprValue::String("red".into())),
                ),
                (
                    ExprValue::String("commercial".into()),
                    Expression::Literal(ExprValue::String("blue".into())),
                ),
            ],
            fallback: Box::new(Expression::Literal(ExprValue::String("gray".into()))),
        };
        assert_eq!(evaluate(&expr, &ctx), ExprValue::String("red".into()));
    }

    #[test]
    fn eval_match_fallback() {
        let ctx = make_ctx(&[("type", PropertyValue::String("industrial".into()))], 5.0);
        let expr = Expression::Match {
            input: Box::new(Expression::Get("type".into())),
            cases: vec![(
                ExprValue::String("residential".into()),
                Expression::Literal(ExprValue::String("red".into())),
            )],
            fallback: Box::new(Expression::Literal(ExprValue::String("gray".into()))),
        };
        assert_eq!(evaluate(&expr, &ctx), ExprValue::String("gray".into()));
    }

    #[test]
    fn eval_interpolate_linear() {
        let ctx = make_ctx(&[], 7.5);
        let expr = Expression::Interpolate {
            interpolation: Interpolation::Linear,
            input: Box::new(Expression::Zoom),
            stops: vec![
                (5.0, Expression::Literal(ExprValue::Number(1.0))),
                (10.0, Expression::Literal(ExprValue::Number(5.0))),
            ],
        };
        let result = evaluate(&expr, &ctx).as_number().unwrap();
        assert!((result - 3.0).abs() < 0.01);
    }

    #[test]
    fn eval_step() {
        let ctx = make_ctx(&[], 7.0);
        let expr = Expression::Step {
            input: Box::new(Expression::Zoom),
            default: Box::new(Expression::Literal(ExprValue::Number(1.0))),
            stops: vec![
                (5.0, Expression::Literal(ExprValue::Number(2.0))),
                (10.0, Expression::Literal(ExprValue::Number(4.0))),
            ],
        };
        assert_eq!(evaluate(&expr, &ctx), ExprValue::Number(2.0));
    }

    #[test]
    fn eval_case() {
        let ctx = make_ctx(&[("pop", PropertyValue::Number(5000.0))], 5.0);
        let expr = Expression::Case(
            vec![
                (
                    Expression::Gt(
                        Box::new(Expression::Get("pop".into())),
                        Box::new(Expression::Literal(ExprValue::Number(10000.0))),
                    ),
                    Expression::Literal(ExprValue::String("big".into())),
                ),
                (
                    Expression::Gt(
                        Box::new(Expression::Get("pop".into())),
                        Box::new(Expression::Literal(ExprValue::Number(1000.0))),
                    ),
                    Expression::Literal(ExprValue::String("medium".into())),
                ),
            ],
            Box::new(Expression::Literal(ExprValue::String("small".into()))),
        );
        assert_eq!(evaluate(&expr, &ctx), ExprValue::String("medium".into()));
    }

    #[test]
    fn parse_get_expression() {
        let json: Value = serde_json::from_str(r#"["get", "name"]"#).unwrap();
        let expr = parse_expression(&json).unwrap();
        let ctx = make_ctx(&[("name", PropertyValue::String("test".into()))], 0.0);
        assert_eq!(evaluate(&expr, &ctx), ExprValue::String("test".into()));
    }

    #[test]
    fn parse_interpolate_expression_json() {
        let json: Value =
            serde_json::from_str(r#"["interpolate", ["linear"], ["zoom"], 5, 1, 10, 5]"#).unwrap();
        let expr = parse_expression(&json).unwrap();
        let ctx = make_ctx(&[], 7.5);
        let result = evaluate(&expr, &ctx).as_number().unwrap();
        assert!((result - 3.0).abs() < 0.01);
    }

    #[test]
    fn parse_match_expression_json() {
        let json: Value = serde_json::from_str(
            r##"["match", ["get", "type"], "residential", "#ff0000", "commercial", "#0000ff", "#808080"]"##,
        )
        .unwrap();
        let expr = parse_expression(&json).unwrap();
        let ctx = make_ctx(&[("type", PropertyValue::String("commercial".into()))], 0.0);
        assert_eq!(
            evaluate(&expr, &ctx),
            ExprValue::Color(Color::rgb(0, 0, 255))
        );
    }

    #[test]
    fn parse_step_expression_json() {
        let json: Value =
            serde_json::from_str(r#"["step", ["get", "pop"], 1, 1000, 2, 5000, 4]"#).unwrap();
        let expr = parse_expression(&json).unwrap();
        let ctx = make_ctx(&[("pop", PropertyValue::Number(3000.0))], 0.0);
        assert_eq!(evaluate(&expr, &ctx), ExprValue::Number(2.0));
    }

    #[test]
    fn eval_color_interpolation() {
        let ctx = make_ctx(&[], 5.0);
        let expr = Expression::Interpolate {
            interpolation: Interpolation::Linear,
            input: Box::new(Expression::Zoom),
            stops: vec![
                (
                    0.0,
                    Expression::Literal(ExprValue::Color(Color::rgb(0, 0, 0))),
                ),
                (
                    10.0,
                    Expression::Literal(ExprValue::Color(Color::rgb(255, 255, 255))),
                ),
            ],
        };
        let result = evaluate(&expr, &ctx).as_color().unwrap();
        assert_eq!(result.r, 128);
        assert_eq!(result.g, 128);
        assert_eq!(result.b, 128);
    }
}
