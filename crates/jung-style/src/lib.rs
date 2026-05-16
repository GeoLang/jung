//! # jung-style
//!
//! Style specification parser for geospatial symbology.
//!
//! Parses a Mapbox GL-compatible JSON style spec into typed Rust structs
//! that the rendering engine (`jung-core`) consumes.

pub mod expr;
pub mod functions;
mod parse;

pub use parse::{Color, Layer, LineCap, LineJoin, Style, StyleError, parse_style};

pub use expr::{
    EvalContext, ExprValue, Expression, Interpolation, PropertyValue, StyleValue, evaluate,
    parse_expression,
};

/// Public color parser for use by the expression evaluator.
pub fn parse_css_color_pub(s: &str) -> Option<Color> {
    parse::parse_css_color_pub(s)
}
