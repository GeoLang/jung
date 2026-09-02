# Jung

[![CI](https://github.com/GeoLang/jung/actions/workflows/ci.yml/badge.svg)](https://github.com/GeoLang/jung/actions)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)

A high-performance geospatial symbology and cartographic rendering engine written in Rust.

Jung transforms geospatial features + style definitions into rendered raster pixels. Named after Carl Jung and his work on archetypal symbols.

## Features

### Core Rendering
- **Line Rendering** — variable width, dash patterns, line caps (butt/round/square), line joins (miter/round/bevel), offset
- **Polygon Rendering** — fill, stroke, opacity, scanline rasterization
- **Data-Driven Styling** — property-based expressions for dynamic colors, widths, sizes
- **Zoom-Dependent Styling** — interpolated stops for smooth transitions across zoom levels
- **Icon/Marker Rendering** — sprite atlases, built-in shapes (circle, square, diamond, star, triangle), alpha-composited blitting
- **Symbol Library** — 16 built-in vector symbols (pin, flag, airport, hospital, fuel, parking, tree, mountain, shields, hazards) rendered at any resolution
- **Labels** — `Renderer::render` draws a style's `text-field` on point and line features: TTF glyphs, priority deconfliction, text along lines. You supply the font, jung embeds none
- **TrueType Font Rendering** — TTF/OTF parsing via ttf-parser, glyph rasterization at arbitrary sizes, kerning, grayscale coverage anti-aliasing, rotated glyphs
- **Curved Labels** — text placed along line geometries, per-character rotation, max angle rejection, repeat spacing

### Advanced Symbology
- **Graduated/Classified** — equal interval, quantile, natural breaks (Fisher-Jenks), standard deviation, manual classification with color ramps
- **Proportional Symbols** — Flannery scaling (perceptual), data-driven size mapping
- **Heatmap** — Gaussian kernel density estimation, configurable radius/intensity, weighted points
- **Temporal Animation** — time-range filtering, keyframe generation, trajectory interpolation, easing functions (linear, ease-in, ease-out, ease-in-out)
- **3D Extrusion** — pseudo-3D building rendering, directional lighting, painter's algorithm
- **Clustering** — grid-based spatial hashing, hierarchical multi-zoom, DBSCAN density-based

### Specialized Symbology
- **MIL-STD-2525** — 15-character SIDC parsing, affiliation-based frame shapes (rectangle/diamond/square/circle), color coding, status indicators (planned/destroyed). There is no glyph set: the entity code is parsed and never drawn, so every unit of a given affiliation renders as the same empty frame. Echelon, task force and feint/dummy are parsed and never drawn. Which 2525 revision the SIDC layout follows is inconsistent in the code, treat the parser as unversioned
- **Maritime S-52/S-57** — IHO color palettes (day/dusk/night modes), depth zone classification, chart symbols (buoys, soundings), safety depth highlighting
- **Topographic** — contour lines (index/intermediate/supplementary), analytical hillshading (Horn's method), hypsometric tinting (elevation-to-color), DEM processing
- **Rule-Based Cascading** — multiple rules per feature with priority cascade, zoom-bounded rules, expression-based filters, source tracking for debugging

### GPU Rendering
- **Vello Backend** — `jung-vello` builds a `vello::Scene` from styled features, geometry plus `text-field` labels once you give it a font with `SceneBuilder::with_font`. Submitting that scene to a GPU is the caller's job, jung does not own a wgpu device
- **Layer Composition** — per-layer scene building with configurable paint properties
- **Coordinate Projection** — geographic-to-screen transform with bbox mapping

### OGC Standards
- **Well-Known Text (WKT)** — parse and serialize all geometry types
- **Well-Known Binary (WKB)** — binary geometry serialization (little-endian)
- **Filter predicates** — property comparisons, LIKE patterns, logical operators (AND/OR/NOT), BBox spatial filter, as a Rust enum and evaluator. This is not OGC Filter Encoding: the XML grammar is neither read nor written
- **Simple Features** — envelope, area, length, centroid operations
- **SLD/SE 1.1** — parse Styled Layer Descriptor XML into jung rules and export jung styles back to SLD. Two limits on import: rules are matched as the literal `<se:Rule`, so a document using any other namespace prefix yields zero rules with no error, and filters are not parsed at all, so every imported rule matches every feature

### Output Formats
- **Raster (RGBA pixels)** — direct pixel buffer output for tile generation
- **GPU (Vello)** — scene graph handed to a caller-supplied wgpu renderer. Point and polygon labels draw as glyph runs, line labels do not

`Renderer` takes no DPI or scale factor. You can allocate a larger pixel buffer, but nothing scales stroke widths or symbol sizes with it, so a 1px line is still 1px at 600 DPI.

### Input Formats
- **Mapbox Vector Tiles (MVT/PBF)** — protobuf decoder with `thiserror` its only dependency, geometry command parsing, zigzag coordinate decoding, attribute extraction
- **Esri drawingInfo** (`jung-esri`), translates the symbology an ArcGIS FeatureServer layer publishes into Mapbox GL style layers: simple, uniqueValue and classBreaks renderers, esriSMS/esriSLS/esriSFS symbols, esriPMS/esriPFS picture symbols that carry their image inline as base64, and the first labelingInfo class. Sizes convert from points to pixels at 96 dpi and esri color arrays become `rgba()` strings. Layers come out as raw JSON so a server can hand them straight to MapLibre, picture symbols also come back as named data uri images the consumer registers at the declared pixel size, and whatever cannot be reproduced (picture symbols that only name a url, arcade label expressions, visual variables, hatch fill patterns, non circle marker shapes) comes back in a structured loss list naming the esri value it gave up on
- **GeoJSON** (`jung_core::geojson::parse_geojson_geometry`), read by both front doors, so `jung-cli` and `jung-wasm` accept every geometry type: Point, LineString, Polygon, MultiPoint, MultiLineString, MultiPolygon and GeometryCollection. A GeometryCollection renders as one feature per member, each carrying the properties of the feature it came from. A polygon's first ring is the exterior and the rest are holes, and ordinates past the second are dropped. A geometry that does not parse fails the whole input with the feature's index in the message

Features carry their properties, so `{property}` label tokens and data-driven expressions work, except for array and object members, which are dropped as nothing can read them.

### Expression Engine
- **Mapbox GL Compatible** — full expression language: `get`, `has`, `zoom`, comparison, logical, math, string, case/match, coalesce, interpolate, step
- **Custom Functions** — a standalone user-defined function registry with built-ins: `clamp`, `lerp`, `pow`, `sqrt`, `log`, `log10`, `len`, `contains`, `if_null`. It is not wired into style parsing: no style expression can call a registered function, and `evaluate_with_functions` discards the registry it is given. Call the registry directly from your own code
- **StyleValue&lt;T&gt;** — expressions or literals for any style property, enabling fully data-driven maps

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  jung-esri  │────▶│  jung-style │────▶│  jung-core  │────▶│  jung-cli   │
│ (drawingInfo│     │  (parsing)  │     │ (rendering) │     │   (CLI)     │
│  to style)  │     └─────────────┘     └─────────────┘     └─────────────┘
└─────────────┘                          ▲      │
                    ┌─────────────┐      │ ┌────┴────┐
                    │  jung-mvt   │──────┘ ▼         ▼
                    │  (tiles)    │ ┌───────────┐ ┌───────────┐
                    └─────────────┘ │ jung-wasm │ │jung-vello │
                                    │ (browser) │ │  (GPU)    │
                                    └───────────┘ └───────────┘
```

### Crates

| Crate | Description |
|-------|-------------|
| `jung-core` | Core rendering engine: geometry, symbology, labels, classification, OGC standards |
| `jung-style` | Style specification parser (Mapbox GL JSON), expression engine, custom functions |
| `jung-mvt` | Mapbox Vector Tile decoder, geometry in tile units |
| `jung-esri` | Esri `drawingInfo` to Mapbox GL style translator, with a loss report |
| `jung-vello` | GPU-accelerated rendering backend via Vello/wgpu |
| `jung-wasm` | WebAssembly bindings for browser-side rendering |
| `jung-cli` | Command-line tool for batch rendering |

### Module Map

```
jung-core/
├── renderer.rs       — Main render orchestration, pixel buffers, bbox
├── geometry.rs       — Point, Geometry, Feature types
├── geojson.rs        — GeoJSON geometry parsing, one geometry per collection member
├── line.rs           — Line rendering with caps, joins, dash patterns
├── polygon.rs        — Polygon fill and stroke
├── antialias.rs      — Anti-aliased lines, circles, polygons (Wu/distance)
├── marker.rs         — Icon/sprite rendering and blitting
├── symbols.rs        — Built-in vector symbol library (16 icons)
├── text.rs           — TrueType/OTF font rasterization
├── curved_label.rs   — Text along line geometries
├── mvt.rs            — jung-mvt tiles normalised into engine coordinates
├── classification.rs — Data classification and color ramps
├── clustering.rs     — Point clustering (grid, hierarchical, DBSCAN)
├── heatmap.rs        — Kernel density heatmap
├── temporal.rs       — Time-based animation and trajectories
├── extrusion.rs      — Pseudo-3D building rendering
├── milstd2525.rs     — MIL-STD-2525 military symbology
├── maritime.rs       — S-52/S-57 nautical chart symbology
├── topographic.rs    — Contours, hillshade, hypsometric tinting
├── ogc.rs            — OGC WKT/WKB, filter predicates, Simple Features ops
├── sld.rs            — SLD/SE 1.1 XML import and export
├── rules.rs          — Rule-based cascading style engine
├── tiling.rs         — XYZ slippy-map tile addressing and per-tile filtering
│                       (no clipping: a feature is kept only if one of its
│                        vertices lies inside the tile, so a line crossing a
│                        tile without a vertex in it is dropped)
├── label_priority.rs — Priority-ordered label placement with a deconfliction grid
└── layout.rs         — Serde model of a page layout (elements, paper sizes).
                        Nothing renders it

jung-vello/
└── lib.rs            — Vello GPU scene builder, geometry and glyph runs

jung-mvt/
└── lib.rs            — Mapbox Vector Tile protobuf decoder

jung-style/
├── expr.rs           — Expression AST, evaluation, StyleValue<T>
├── functions.rs      — Custom function registry
└── parse.rs          — JSON style parser (Mapbox GL compatible)

jung-esri/
├── convert.rs        — Esri colors, point to pixel sizes
├── symbol.rs         — esriSMS/esriSLS/esriSFS and esriPMS/esriPFS symbols to paint properties
├── label.rs          — labelingInfo to a symbol layer
└── lib.rs            — Renderer translation, layer assembly, image names, loss list
```

## Quick Start

### CLI Usage

```bash
# Render a GeoJSON file with a style
jung --style style.json --input data.geojson --output tile.rgba --width 256 --height 256

# Specify a custom bounding box
jung --style style.json --input data.geojson --output tile.rgba --bbox "-180,-90,180,90"

# Draw the style's text layers with a font, named so text-font can pick it
jung --style style.json --input data.geojson --output tile.rgba \
  --font /usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf --font-family "DejaVu Sans"
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

### Labels

```rust
use jung_core::renderer::Renderer;
use jung_core::text::{FontFace, FontSet};

let face = FontFace::from_bytes(std::fs::read("DejaVuSans.ttf").unwrap()).unwrap();
let mut fonts = FontSet::new();
fonts.insert("DejaVu Sans", face);

// text-field, text-size, text-font and text-color now draw
let renderer = Renderer::new(512, 512).unwrap().with_fonts(fonts);
let pixels = renderer.render(&style, &features, &bbox).unwrap();
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
import init, { Renderer } from 'jung-wasm';

await init();

const renderer = new Renderer(256, 256);

// jung embeds no font, so text layers draw nothing until you add one. The
// family name has to match what the layer's text-font asks for, and the first
// family added is the fallback for any other name. Bad bytes throw.
const font = await fetch('/fonts/DejaVuSans.ttf').then((r) => r.arrayBuffer());
renderer.add_font('DejaVu Sans', new Uint8Array(font));

const styleJson = JSON.stringify({
    layers: [{
        id: 'labels',
        paint: { 'text-color': '#003366' },
        layout: { 'text-field': '{name}', 'text-size': 20, 'text-font': ['DejaVu Sans'] },
    }],
});

const pixels = renderer.render_to_pixels(
    styleJson,
    geojsonString,
    -180, -90, 180, 90
);

const imageData = new ImageData(new Uint8ClampedArray(pixels), 256, 256);
ctx.putImageData(imageData, 0, 0);
```

One `Renderer` serves any number of renders, so load the font once and reuse it
per tile. `{name}` reads the `name` member of each feature's `properties`, and a
feature without it draws no label.

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

### Style Properties

The Block column is the style-layer object the property deserializes from. A property put in the wrong block is ignored silently.

| Property | Block | Type | Data-Driven | Description |
|----------|-------|------|:-----------:|-------------|
| `fill-color` | paint | color | ✓ | Polygon fill color |
| `line-color` | paint | color | ✓ | Line/stroke color |
| `line-width` | paint | number | ✓ | Line width in pixels |
| `line-cap` | layout | enum | | `butt`, `round`, `square` |
| `line-join` | layout | enum | | `miter`, `round`, `bevel` |
| `line-dasharray` | paint | number[] | | Dash/gap pattern |
| `line-offset` | paint | number | ✓ | Perpendicular offset |
| `line-opacity` | paint | number | ✓ | Line opacity (0-1) |
| `circle-color` | paint | color | ✓ | Point circle color |
| `circle-radius` | paint | number | ✓ | Point circle radius |
| `icon-image` | layout | string | ✓ | Sprite name for icon |
| `icon-size` | layout | number | ✓ | Icon scale factor |
| `icon-rotate` | layout | number | ✓ | Icon rotation, degrees clockwise |
| `icon-anchor` | layout | enum | | `center`, `left`, `right`, `top`, `bottom`, `top-left`, `top-right`, `bottom-left`, `bottom-right` |
| `icon-offset` | layout | number[2] | | Extra [x, y] pixel shift after the anchor |
| `text-color` | paint | color | ✓ | Label color, black by default |
| `text-field` | layout | string | ✓ | Label text, with `{property}` tokens |
| `text-size` | layout | number | ✓ | Label size in pixels, 16 by default |
| `text-font` | layout | string[] | | First entry picks a family from the renderer's `FontSet` |

Labels need a font: `Renderer::with_fonts`, since jung embeds none. Without one, text layers draw nothing and `text_layers_without_font` names them. Point features label straight, line features label along the line, and polygons do not label at all. `text-size` also sets collision priority, so bigger text survives where labels overlap.

`jung-vello` labels from the same properties but places them differently: `SceneBuilder::with_font` takes one face that draws every text layer whatever `text-font` names, points and every part of a multipoint label at the feature, polygons label at the centroid of each exterior ring, and lines do not label. There is no collision deconfliction, no halo and no kerning, so two labels over the same pixels both draw.

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
| Literal | `literal` |
| Conversion | `to-number`, `to-string`, `to-boolean`, `to-color` |

### Color Formats

- Hex: `#rgb`, `#rrggbb`, `#rrggbbaa`
- Named: `red`, `green`, `blue`, `white`, `black`, `yellow`, `cyan`, `magenta`, `transparent`
- Function: `rgb(r, g, b)`, `rgba(r, g, b, a)`

## Building

```bash
# Build all crates
cargo build --all

# Run tests (338 tests)
cargo test --all

# Clippy lint check
cargo clippy --all-targets --all-features -- -D warnings

# Build WASM (requires wasm-pack)
cd crates/jung-wasm
wasm-pack build --target web
```

## Integration with GeoLang Ecosystem

Jung is a library, not a compose service.

- **[Ptolemy](https://github.com/GeoLang/ptolemy)** uses `jung-esri` to translate an ArcGIS `drawingInfo` into Mapbox GL style layers on `GET /style`.
- **[TerraVista](https://github.com/GeoLang/terravista)** uses `jung-mvt` to decode vector tiles.
- **[Fenestra](https://github.com/GeoLang/fenestra)** can build a Vello scene behind an optional feature; the platform deploy does not enable it.
- **[ViewTopia](https://github.com/GeoLang/viewtopia)** does not import `jung-wasm`. Client styling is MapLibre / Cesium.

`Renderer` draws points, lines and polygons from a Mapbox GL style, with hard-edged integer rasterization, plus labels from the `text-*` properties when the caller supplies a font. The label path uses `text.rs`, `label_priority.rs` and `curved_label.rs`. The other `jung-core` modules (anti-aliasing, symbols, MIL-STD, maritime, topographic, heatmap, clustering, classification, temporal, extrusion, tiling, rules, layout, SLD) are library code with their own tests that nothing on the default render path calls. Two `ogc` functions are called from outside: `jung-cli` computes its default bbox with `envelope`, and `jung-vello` places polygon labels with `polygon_centroid`. SVG output and print furniture are gone.

## License

AGPL-3.0-or-later, see [LICENSE](LICENSE).

Copyright (C) 2026 Grok Image Compression Inc.
