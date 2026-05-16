//! # jung-core
//!
//! Core rendering engine for geospatial symbology.
//!
//! Takes styled geospatial features and produces rendered output
//! (raw pixels, SVG, or vector draw commands).

pub mod classification;
pub mod clustering;
pub mod extrusion;
pub mod geometry;
pub mod heatmap;
pub mod label;
pub mod line;
pub mod maritime;
pub mod marker;
pub mod milstd2525;
pub mod output;
pub mod polygon;
pub mod renderer;
pub mod rules;
pub mod temporal;
pub mod topographic;

pub use renderer::Renderer;
