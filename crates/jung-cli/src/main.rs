use clap::Parser;
use jung_core::geometry::{Feature, Geometry, Point};
use jung_core::renderer::{BBox, Renderer};
use jung_core::text::{FontFace, FontSet};
use jung_style::parse_style;
use std::fs;
use std::process;

#[derive(Parser)]
#[command(
    name = "jung",
    about = "Render geospatial features with symbology styles"
)]
struct Cli {
    /// Path to the style JSON file
    #[arg(short, long)]
    style: String,

    /// Path to the input GeoJSON file
    #[arg(short, long)]
    input: String,

    /// Output file path (raw RGBA binary)
    #[arg(short, long)]
    output: String,

    /// Output image width in pixels
    #[arg(long, default_value = "512")]
    width: u32,

    /// Output image height in pixels
    #[arg(long, default_value = "512")]
    height: u32,

    /// Bounding box: min_x,min_y,max_x,max_y
    #[arg(long)]
    bbox: Option<String>,

    /// Path to a TTF/OTF font file, required for a style's text layers to draw
    #[arg(long)]
    font: Option<String>,

    /// Family name the font answers to, matching the style's text-font
    #[arg(long, default_value = "default")]
    font_family: String,
}

fn main() {
    let cli = Cli::parse();

    let style_json = fs::read_to_string(&cli.style).unwrap_or_else(|e| {
        eprintln!("Error reading style file '{}': {e}", cli.style);
        process::exit(1);
    });

    let geojson = fs::read_to_string(&cli.input).unwrap_or_else(|e| {
        eprintln!("Error reading input file '{}': {e}", cli.input);
        process::exit(1);
    });

    let style = parse_style(&style_json).unwrap_or_else(|e| {
        eprintln!("Error parsing style: {e}");
        process::exit(1);
    });

    let features = parse_geojson_features(&geojson).unwrap_or_else(|e| {
        eprintln!("Error parsing GeoJSON: {e}");
        process::exit(1);
    });

    let bbox = if let Some(bbox_str) = &cli.bbox {
        parse_bbox(bbox_str).unwrap_or_else(|e| {
            eprintln!("Error parsing bbox: {e}");
            process::exit(1);
        })
    } else {
        compute_bbox(&features).unwrap_or_else(|| {
            eprintln!("Cannot compute bbox from empty feature set; use --bbox");
            process::exit(1);
        })
    };

    let mut renderer = Renderer::new(cli.width, cli.height).unwrap_or_else(|e| {
        eprintln!("Renderer error: {e}");
        process::exit(1);
    });

    if let Some(font_path) = &cli.font {
        let data = fs::read(font_path).unwrap_or_else(|e| {
            eprintln!("Error reading font '{font_path}': {e}");
            process::exit(1);
        });
        let face = FontFace::from_bytes(data).unwrap_or_else(|| {
            eprintln!("Error parsing font '{font_path}': not a TTF or OTF face");
            process::exit(1);
        });
        let mut fonts = FontSet::new();
        fonts.insert(&cli.font_family, face);
        renderer = renderer.with_fonts(fonts);
    }

    let skipped = renderer.text_layers_without_font(&style);
    if !skipped.is_empty() {
        eprintln!(
            "No font for text layers, labels skipped: {}. Pass --font",
            skipped.join(", ")
        );
    }

    let buffer = renderer
        .render(&style, &features, &bbox)
        .unwrap_or_else(|e| {
            eprintln!("Render error: {e}");
            process::exit(1);
        });

    fs::write(&cli.output, &buffer.data).unwrap_or_else(|e| {
        eprintln!("Error writing output '{}': {e}", cli.output);
        process::exit(1);
    });

    eprintln!(
        "Rendered {} features → {} ({}x{} RGBA)",
        features.len(),
        cli.output,
        cli.width,
        cli.height
    );
}

fn parse_bbox(s: &str) -> Result<BBox, String> {
    let parts: Vec<f64> = s
        .split(',')
        .map(|p| {
            p.trim()
                .parse::<f64>()
                .map_err(|e| format!("invalid number: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parts.len() != 4 {
        return Err("bbox must be min_x,min_y,max_x,max_y".to_string());
    }
    Ok(BBox {
        min_x: parts[0],
        min_y: parts[1],
        max_x: parts[2],
        max_y: parts[3],
    })
}

fn compute_bbox(features: &[Feature]) -> Option<BBox> {
    if features.is_empty() {
        return None;
    }

    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for f in features {
        if let Geometry::Point(pt) = &f.geometry {
            min_x = min_x.min(pt.x);
            min_y = min_y.min(pt.y);
            max_x = max_x.max(pt.x);
            max_y = max_y.max(pt.y);
        }
    }

    // Add small padding so single-point doesn't collapse
    if (max_x - min_x).abs() < 1e-10 {
        min_x -= 1.0;
        max_x += 1.0;
    }
    if (max_y - min_y).abs() < 1e-10 {
        min_y -= 1.0;
        max_y += 1.0;
    }

    Some(BBox {
        min_x,
        min_y,
        max_x,
        max_y,
    })
}

fn parse_geojson_features(geojson: &str) -> Result<Vec<Feature>, String> {
    let value: serde_json::Value =
        serde_json::from_str(geojson).map_err(|e| format!("JSON parse: {e}"))?;

    let features_array = value
        .get("features")
        .and_then(|f| f.as_array())
        .ok_or("missing 'features' array")?;

    let mut features = Vec::new();
    for feat_val in features_array {
        let geom = feat_val.get("geometry").ok_or("feature missing geometry")?;
        let geom_type = geom
            .get("type")
            .and_then(|t| t.as_str())
            .ok_or("geometry missing type")?;

        let coords = geom.get("coordinates");

        match geom_type {
            "Point" => {
                if let Some(pt) = coords.and_then(parse_point) {
                    features.push(Feature {
                        geometry: Geometry::Point(pt),
                        properties: std::collections::HashMap::new(),
                    });
                }
            }
            _ => {
                // Other geometry types will be supported in future versions
            }
        }
    }

    Ok(features)
}

fn parse_point(coords: &serde_json::Value) -> Option<Point> {
    let arr = coords.as_array()?;
    if arr.len() < 2 {
        return None;
    }
    Some(Point {
        x: arr[0].as_f64()?,
        y: arr[1].as_f64()?,
    })
}
