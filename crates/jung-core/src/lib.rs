//! # jung-core
//!
//! Core rendering engine for geospatial symbology.
//!
//! Takes styled geospatial features and produces rendered output
//! (raw pixels, SVG, or vector draw commands).

#![allow(unknown_lints)]
#![allow(clippy::manual_checked_ops)]

pub mod classification;
pub mod clustering;
pub mod extrusion;
pub mod geometry;
pub mod heatmap;
pub mod label;
pub mod layout;
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

pub mod antialias;
pub mod curved_label;
pub mod label_priority;
pub mod mvt;
pub mod ogc;
pub mod print_layout;
pub mod sld;
pub mod symbols;
pub mod text;
pub mod tiling;

pub use renderer::Renderer;
