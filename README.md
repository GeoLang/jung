# Jung

A high-performance geospatial symbology and cartographic rendering engine written in Rust.

Jung transforms geospatial features + style definitions into rendered output (raster pixels, SVG vector graphics, or high-DPI print output). Named after Carl Jung and his work on archetypal symbols.

## Features

### Core Rendering
- **Line Rendering** — variable width, dash patterns, line caps (butt/round/square), line joins (miter/round/bevel), offset
- **Polygon Rendering** — fill, stroke, opacity, scanline rasterization
- **Anti-Aliased Rendering** — Xiaolin Wu line AA, distance-based thick line AA, subpixel polygon coverage, smooth circle edges
- **Data-Driven Styling** — property-based expressions for dynamic colors, widths, sizes
- **Zoom-Dependent Styling** — interpolated stops for smooth transitions across zoom levels
- **Icon/Marker Rendering** — sprite atlases, built-in shapes (circle, square, diamond, star, triangle), alpha-composited blitting
- **Symbol Library** — 16+ built-in vector symbols (pin, flag, airport, hospital, fuel, parking, tree, mountain, shields, hazards) rendered at any resolution
- **Label Engine** — bitmap text, word wrap, collision detection/decluttering, anchor positioning, halo/buffer rendering
- **TrueType Font Rendering** — TTF/OTF parsing via ttf-parser, glyph rasterization at arbitrary sizes, kerning, subpixel anti-aliasing
- **Curved Labels** — text placed along line geometries, per-character rotation, max angle rejection, halo outlines, repeat spacing

### Advanced Symbology
- **Graduated/Classified** — equal interval, quantile, natural breaks (Fisher-Jenks), standard deviation, manual classification with color ramps
- **Proportional Symbols** — Flannery scaling (perceptual), data-driven size mapping
- **Heatmap** — Gaussian kernel density estimation, configurable radius/intensity, weighted points
- **Temporal Animation** — time-range filtering, keyframe generation, trajectory interpolation, easing functions (linear, ease-in, ease-out, ease-in-out)
- **3D Extrusion** — pseudo-3D building rendering, directional lighting, painter's algorithm
- **Clustering** — grid-based spatial hashing, hierarchical multi-zoom, DBSCAN density-based

### Specialized Symbology
- **MIL-STD-2525** — 15-character SIDC parsing, affiliation-based frame shapes (rectangle/diamond/square/circle), color coding, status indicators (planned/destroyed), HQ/task force modifiers, echelon display
- **Maritime S-52/S-57** — IHO color palettes (day/dusk/night modes), depth zone classification, chart symbols (buoys, soundings), safety depth highlighting
- **Topographic** — contour lines (index/intermediate/supplementary), analytical hillshading (Horn's method), hypsometric tinting (elevation-to-color), DEM processing
- **Rule-Based Cascading** — multiple rules per feature with priority cascade, zoom-bounded rules, expression-based filters, source tracking for debugging

### Output Formats
- **Raster (RGBA pixels)** — direct pixel buffer output for tile generation
- **SVG Vector Export** — circles, paths, polygons, text, groups with transforms, proper XML escaping
- **High-DPI Print** — configurable DPI (72/300/600+), paper sizes (A4 etc.), margins, scale bars, north arrows
- **WebAssembly** — full engine in the browser via wasm-bindgen

### Input Formats
- **GeoJSON** — standard feature collections
- **Mapbox Vector Tiles (MVT/PBF)** — zero-dependency protobuf decoder, geometry command parsing, zigzag coordinate decoding, attribute extraction

### Expression Engine
- **Mapbox GL Compatible** — full expression language: `get`, `has`, `zoom`, comparison, logical, math, string, case/match, coalesce, interpolate, step
- **Custom Functions** — user-defined function registry with built-ins: `clamp`, `lerp`, `pow`, `sqrt`, `log`, `log10`, `len`, `contains`, `if_null`
- **StyleValue&lt;T&gt;** — expressions or literals for any style property, enabling fully data-driven maps

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  jung-style │────▶│  jung-core  │────▶│  jung-cli   │
│  (parsing)  │     │ (rendering) │     │   (CLI)     │
└─────────────┘     └─────────────┘     └─────────────┘
                           │
                           ▼
                    ┌─────────────┐
                    │  jung-wasm  │
                    │  (browser)  │
                    └─────────────┘
```

### Crates

| Crate | Description |
|-------|-------------|
| `jung-core` | Core rendering engine: geometry, symbology, classification, output |
| `jung-style` | Style specification parser (Mapbox GL JSON), expression engine, custom functions |
| `jung-wasm` | WebAssembly bindings for browser-side rendering |
| `jung-cli` | Command-line tool for batch rendering |

### Module Map

```
jung-core/
├── renderer.rs       — Main render orchestration, pixel buffers, bbox
├── geometry.rs       — Point, Geometry, Feature types
├── line.rs           — Line rendering with caps, joins, dash patterns
├── polygon.rs        — Polygon fill and stroke
├── antialias.rs      — Anti-aliased lines, circles, polygons (Wu/distance)
├── marker.rs         — Icon/sprite rendering and blitting
├── symbols.rs        — Built-in vector symbol library (16+ icons)
├── label.rs          — Text placement with collision detection
├── text.rs           — TrueType/OTF font rasterization
├── curved_label.rs   — Text along line geometries
├── mvt.rs            — Mapbox Vector Tile protobuf decoder
├── classification.rs — Data classification and color ramps
├── clustering.rs     — Point clustering (grid, hierarchical, DBSCAN)
├── heatmap.rs        — Kernel density heatmap
├── temporal.rs       — Time-based animation and trajectories
├── extrusion.rs      — Pseudo-3D building rendering
├── milstd2525.rs     — MIL-STD-2525 military symbology
├── maritime.rs       — S-52/S-57 nautical chart symbology
├── topographic.rs    — Contours, hillshade, hypsometric tinting
├── rules.rs          — Rule-based cascading style engine
└── output.rs         — SVG export, print output, map furniture

jung-style/
├── expr.rs           — Expression AST, evaluation, StyleValue<T>
├── functions.rs      — Custom function registry
└── parse.rs          — JSON style parser (Mapbox GL compatible)
```

## Quick Start

### CLI Usage

```bash
# Render a GeoJSON file with a style
jung --style style.json --input data.geojson --output tile.rgba --width 256 --height 256

# Specify a custom bounding box
jung --style style.json --input data.geojson --output tile.rgba --bbox "-180,-90,180,90"
```

### Library Usage

```rust
use jung_core::geometry::{Feature, Geometry, Point};
use jung_core::renderer::{BBox, Renderer};
use jung_style::parse_style;

let style_json = r#"{
    "name": "my-style",
    "layers": [{
        "id": "cities",
        "paint": { "circle-color": "#ff0000", "circle-radius": 5.0 }
    }]
}"#;

let style = parse_style(style_json).unwrap();
let renderer = Renderer::new(512, 512).unwrap();

let features = vec![Feature {
    geometry: Geometry::Point(Point { x: -73.9857, y: 40.7484 }),
    properties: Default::default(),
}];

let bbox = BBox { min_x: -74.1, min_y: 40.6, max_x: -73.8, max_y: 40.9 };
let pixels = renderer.render(&style, &features, &bbox).unwrap();
```

### Military Symbology

```rust
use jung_core::milstd2525::{Sidc, FrameShape, render_milsym};

// Parse a 15-character SIDC (Friendly Ground Unit)
let sidc = Sidc::parse("13100000000000-").unwrap();
assert_eq!(sidc.frame_shape(), FrameShape::Rectangle); // friendly = rectangle

// Render a 64x64 pixel icon
let icon = render_milsym(&sidc, 64);
```

### Maritime Charts

```rust
use jung_core::maritime::{ChartParams, DepthZones, PaletteMode, render_depth_area};

let params = ChartParams {
    palette: PaletteMode::Night,
    depth_zones: DepthZones { safety_contour: 10.0, ..Default::default() },
    ..Default::default()
};
render_depth_area(&mut buffer, &polygon, &bbox, 5.0, &params);
```

### Hillshade

```rust
use jung_core::topographic::{HillshadeParams, compute_hillshade, apply_hillshade};

let shade = compute_hillshade(&dem_data, width, height, cell_size, &HillshadeParams {
    azimuth: 315.0,
    altitude: 45.0,
    z_factor: 2.0,
});
apply_hillshade(&mut buffer, &shade, 0.5);
```

### SVG Export

```rust
use jung_core::output::SvgDocument;

let mut doc = SvgDocument::new(800.0, 600.0);
doc.add_polyline(&line_points, &bbox, "#3388ff", 2.0);
doc.add_polygon(&poly_points, &bbox, "rgba(51,136,255,0.3)", "#3388ff", 1.0);
doc.add_text(10.0, 20.0, "Map Title", 16.0, "black");
let svg_string = doc.to_svg();
```

### Custom Functions

```rust
use jung_style::functions::FunctionRegistry;
use jung_style::ExprValue;

let mut reg = FunctionRegistry::with_builtins();
reg.register("population_class", |args| {
    match args.first() {
        Some(ExprValue::Number(pop)) if *pop > 1_000_000.0 => {
            ExprValue::String("major".into())
        }
        Some(ExprValue::Number(pop)) if *pop > 100_000.0 => {
            ExprValue::String("city".into())
        }
        _ => ExprValue::String("town".into()),
    }
});
```

### Rule-Based Styling

```rust
use jung_core::rules::{Ruleset, RuleBuilder};
use jung_style::{Expression, ExprValue};

let mut rules = Ruleset::new();
rules.add_rule(RuleBuilder::new("base-roads")
    .color("stroke", "#cccccc")
    .number("width", 1.0)
    .build());
rules.add_rule(RuleBuilder::new("highways")
    .priority(10)
    .filter(Expression::Eq(
        Box::new(Expression::Get("class".into())),
        Box::new(Expression::Literal(ExprValue::String("highway".into()))),
    ))
    .color("stroke", "#ff6600")
    .number("width", 4.0)
    .build());

let style = rules.evaluate(&context); // cascades matching rules
```

### WebAssembly

```javascript
import init, { render_to_pixels } from 'jung-wasm';

await init();

const pixels = render_to_pixels(
    256, 256,
    styleJson,
    geojsonString,
    -180, -90, 180, 90
);

const imageData = new ImageData(new Uint8ClampedArray(pixels), 256, 256);
ctx.putImageData(imageData, 0, 0);
```

## Style Specification

Jung uses a Mapbox GL-compatible style format:

```json
{
    "name": "urban-map",
    "layers": [
        {
            "id": "buildings",
            "source": "buildings-source",
            "paint": {
                "fill-color": ["interpolate", ["linear"], ["get", "height"],
                    0, "#d4d4d4",
                    50, "#888888"
                ],
                "line-color": "#666666",
                "line-width": 1.0
            }
        },
        {
            "id": "roads",
            "source": "roads-source",
            "paint": {
                "line-color": ["match", ["get", "class"],
                    "highway", "#ff6600",
                    "primary", "#ffaa00",
                    "#ffffff"
                ],
                "line-width": ["interpolate", ["exponential", 1.5], ["zoom"],
                    5, 0.5,
                    18, 12
                ]
            }
        }
    ]
}
```

### Paint Properties

| Property | Type | Data-Driven | Description |
|----------|------|:-----------:|-------------|
| `fill-color` | color | ✓ | Polygon fill color |
| `line-color` | color | ✓ | Line/stroke color |
| `line-width` | number | ✓ | Line width in pixels |
| `line-cap` | enum | | `butt`, `round`, `square` |
| `line-join` | enum | | `miter`, `round`, `bevel` |
| `line-dasharray` | number[] | | Dash/gap pattern |
| `line-offset` | number | ✓ | Perpendicular offset |
| `line-opacity` | number | ✓ | Line opacity (0-1) |
| `circle-color` | color | ✓ | Point circle color |
| `circle-radius` | number | ✓ | Point circle radius |
| `icon-image` | string | ✓ | Sprite name for icon |
| `icon-size` | number | ✓ | Icon scale factor |
| `text-color` | color | ✓ | Label text color |
| `text-field` | string | ✓ | Property for label text |
| `text-size` | number | ✓ | Font size |

### Expression Operators

| Category | Operators |
|----------|-----------|
| Data | `get`, `has`, `geometry-type`, `id` |
| Zoom | `zoom` |
| Comparison | `==`, `!=`, `>`, `>=`, `<`, `<=` |
| Logical | `all`, `any`, `!` |
| Math | `+`, `*`, `-`, `/`, `%`, `min`, `max`, `abs`, `floor`, `ceil`, `round` |
| String | `concat`, `upcase`, `downcase` |
| Control | `case`, `match`, `coalesce` |
| Interpolation | `interpolate` (linear, exponential, cubic-bezier) |
| Steps | `step` |
| Conversion | `to-number`, `to-string`, `to-boolean`, `to-color` |

### Color Formats

- Hex: `#rgb`, `#rrggbb`, `#rrggbbaa`
- Named: `red`, `green`, `blue`, `white`, `black`, `yellow`, `cyan`, `magenta`, `transparent`
- Function: `rgb(r, g, b)`, `rgba(r, g, b, a)`

## Building

```bash
# Build all crates
cargo build --all

# Run tests (132 tests)
cargo test --all

# Clippy lint check
cargo clippy --all-targets --all-features -- -D warnings

# Build WASM (requires wasm-pack)
cd crates/jung-wasm
wasm-pack build --target web
```

## Integration with TileTopia Ecosystem

Jung is part of the TileTopia geospatial platform:

- **[TileTopia](https://github.com/TileTopia-HQ/tiletopia)** — 3D tile generation pipeline
- **[ViewTopia](https://github.com/TileTopia-HQ/viewtopia)** — Geospatial viewer with agentic AI
- **[Ptolemy](https://github.com/TileTopia-HQ/ptolemy)** — Versioned geospatial database

Jung provides the symbology engine that TileTopia uses for raster tile rendering and ViewTopia uses (via WASM) for client-side styling.

## License

GNU Affero General Public License v3.0 or later. See [LICENSE](LICENSE) for details.
