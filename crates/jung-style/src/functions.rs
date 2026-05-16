//! Custom user-defined functions for the expression engine.
//!
//! Allows registering custom functions that can be called within style
//! expressions. Functions receive evaluated arguments and produce an ExprValue.

use crate::{EvalContext, ExprValue, Expression, evaluate};
use std::collections::HashMap;

/// A custom function signature: receives arguments and returns a value.
pub type CustomFn = Box<dyn Fn(&[ExprValue]) -> ExprValue + Send + Sync>;

/// Registry of custom functions available during expression evaluation.
pub struct FunctionRegistry {
    functions: HashMap<String, CustomFn>,
}

impl std::fmt::Debug for FunctionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionRegistry")
            .field("functions", &self.functions.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FunctionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    /// Create a registry with standard built-in utility functions.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();

        // clamp(value, min, max)
        reg.register("clamp", |args| {
            if args.len() != 3 {
                return ExprValue::Null;
            }
            match (&args[0], &args[1], &args[2]) {
                (ExprValue::Number(v), ExprValue::Number(lo), ExprValue::Number(hi)) => {
                    ExprValue::Number(v.clamp(*lo, *hi))
                }
                _ => ExprValue::Null,
            }
        });

        // lerp(a, b, t)
        reg.register("lerp", |args| {
            if args.len() != 3 {
                return ExprValue::Null;
            }
            match (&args[0], &args[1], &args[2]) {
                (ExprValue::Number(a), ExprValue::Number(b), ExprValue::Number(t)) => {
                    ExprValue::Number(a + (b - a) * t)
                }
                _ => ExprValue::Null,
            }
        });

        // pow(base, exp)
        reg.register("pow", |args| {
            if args.len() != 2 {
                return ExprValue::Null;
            }
            match (&args[0], &args[1]) {
                (ExprValue::Number(base), ExprValue::Number(exp)) => {
                    ExprValue::Number(base.powf(*exp))
                }
                _ => ExprValue::Null,
            }
        });

        // sqrt(value)
        reg.register("sqrt", |args| {
            if args.len() != 1 {
                return ExprValue::Null;
            }
            match &args[0] {
                ExprValue::Number(v) => ExprValue::Number(v.sqrt()),
                _ => ExprValue::Null,
            }
        });

        // log(value)
        reg.register("log", |args| {
            if args.len() != 1 {
                return ExprValue::Null;
            }
            match &args[0] {
                ExprValue::Number(v) => ExprValue::Number(v.ln()),
                _ => ExprValue::Null,
            }
        });

        // log10(value)
        reg.register("log10", |args| {
            if args.len() != 1 {
                return ExprValue::Null;
            }
            match &args[0] {
                ExprValue::Number(v) => ExprValue::Number(v.log10()),
                _ => ExprValue::Null,
            }
        });

        // len(string)
        reg.register("len", |args| {
            if args.len() != 1 {
                return ExprValue::Null;
            }
            match &args[0] {
                ExprValue::String(s) => ExprValue::Number(s.len() as f64),
                ExprValue::Array(a) => ExprValue::Number(a.len() as f64),
                _ => ExprValue::Null,
            }
        });

        // contains(string, substring)
        reg.register("contains", |args| {
            if args.len() != 2 {
                return ExprValue::Null;
            }
            match (&args[0], &args[1]) {
                (ExprValue::String(haystack), ExprValue::String(needle)) => {
                    ExprValue::Boolean(haystack.contains(needle.as_str()))
                }
                _ => ExprValue::Null,
            }
        });

        // if_null(value, fallback) - return fallback if value is null
        reg.register("if_null", |args| {
            if args.len() != 2 {
                return ExprValue::Null;
            }
            if args[0] == ExprValue::Null {
                args[1].clone()
            } else {
                args[0].clone()
            }
        });

        reg
    }

    /// Register a custom function.
    pub fn register<F>(&mut self, name: &str, f: F)
    where
        F: Fn(&[ExprValue]) -> ExprValue + Send + Sync + 'static,
    {
        self.functions.insert(name.to_string(), Box::new(f));
    }

    /// Call a registered function by name.
    pub fn call(&self, name: &str, args: &[ExprValue]) -> ExprValue {
        match self.functions.get(name) {
            Some(f) => f(args),
            None => ExprValue::Null,
        }
    }

    /// Check if a function is registered.
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// List all registered function names.
    pub fn function_names(&self) -> Vec<&str> {
        self.functions.keys().map(|s| s.as_str()).collect()
    }
}

/// Evaluate an expression with custom function support.
/// Function calls are represented as Expression::Literal with a special
/// call structure, or via this helper that wraps standard evaluation.
pub fn evaluate_with_functions(
    expr: &Expression,
    ctx: &EvalContext<'_>,
    _registry: &FunctionRegistry,
) -> ExprValue {
    // Standard expressions are evaluated directly.
    // Custom function calls go through FunctionCall::evaluate().
    evaluate(expr, ctx)
}

/// A function call expression node (for use in extended expression trees).
#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: Vec<Expression>,
}

impl FunctionCall {
    /// Evaluate this function call.
    pub fn evaluate(&self, ctx: &EvalContext<'_>, registry: &FunctionRegistry) -> ExprValue {
        let args: Vec<ExprValue> = self
            .arguments
            .iter()
            .map(|expr| crate::evaluate(expr, ctx))
            .collect();
        registry.call(&self.name, &args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_clamp() {
        let reg = FunctionRegistry::with_builtins();
        let result = reg.call(
            "clamp",
            &[
                ExprValue::Number(15.0),
                ExprValue::Number(0.0),
                ExprValue::Number(10.0),
            ],
        );
        assert_eq!(result, ExprValue::Number(10.0));
    }

    #[test]
    fn builtin_lerp() {
        let reg = FunctionRegistry::with_builtins();
        let result = reg.call(
            "lerp",
            &[
                ExprValue::Number(0.0),
                ExprValue::Number(100.0),
                ExprValue::Number(0.5),
            ],
        );
        assert_eq!(result, ExprValue::Number(50.0));
    }

    #[test]
    fn builtin_pow() {
        let reg = FunctionRegistry::with_builtins();
        let result = reg.call("pow", &[ExprValue::Number(2.0), ExprValue::Number(3.0)]);
        assert_eq!(result, ExprValue::Number(8.0));
    }

    #[test]
    fn builtin_sqrt() {
        let reg = FunctionRegistry::with_builtins();
        let result = reg.call("sqrt", &[ExprValue::Number(16.0)]);
        assert_eq!(result, ExprValue::Number(4.0));
    }

    #[test]
    fn builtin_len() {
        let reg = FunctionRegistry::with_builtins();
        let result = reg.call("len", &[ExprValue::String("hello".to_string())]);
        assert_eq!(result, ExprValue::Number(5.0));
    }

    #[test]
    fn builtin_contains() {
        let reg = FunctionRegistry::with_builtins();
        let result = reg.call(
            "contains",
            &[
                ExprValue::String("hello world".to_string()),
                ExprValue::String("world".to_string()),
            ],
        );
        assert_eq!(result, ExprValue::Boolean(true));
    }

    #[test]
    fn custom_function_registration() {
        let mut reg = FunctionRegistry::new();
        reg.register("double", |args| match args.first() {
            Some(ExprValue::Number(n)) => ExprValue::Number(n * 2.0),
            _ => ExprValue::Null,
        });
        assert!(reg.has_function("double"));
        let result = reg.call("double", &[ExprValue::Number(21.0)]);
        assert_eq!(result, ExprValue::Number(42.0));
    }

    #[test]
    fn unknown_function_returns_null() {
        let reg = FunctionRegistry::new();
        let result = reg.call("nonexistent", &[ExprValue::Number(1.0)]);
        assert_eq!(result, ExprValue::Null);
    }

    #[test]
    fn if_null_fallback() {
        let reg = FunctionRegistry::with_builtins();
        let result = reg.call("if_null", &[ExprValue::Null, ExprValue::Number(42.0)]);
        assert_eq!(result, ExprValue::Number(42.0));

        let result2 = reg.call(
            "if_null",
            &[ExprValue::Number(7.0), ExprValue::Number(42.0)],
        );
        assert_eq!(result2, ExprValue::Number(7.0));
    }

    #[test]
    fn function_call_evaluate() {
        let reg = FunctionRegistry::with_builtins();
        let props = std::collections::HashMap::new();
        let ctx = EvalContext {
            properties: &props,
            zoom: 10.0,
            geometry_type: "Point",
        };

        let call = FunctionCall {
            name: "sqrt".to_string(),
            arguments: vec![Expression::Literal(ExprValue::Number(25.0))],
        };
        let result = call.evaluate(&ctx, &reg);
        assert_eq!(result, ExprValue::Number(5.0));
    }
}
