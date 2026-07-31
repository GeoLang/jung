//! # jung-esri
//!
//! Translates the `drawingInfo` block an ArcGIS FeatureServer layer publishes into
//! Mapbox GL style layers, plus a list of everything that could not be translated.
//!
//! Layers come out as raw JSON, so a server can hand them to MapLibre without linking
//! the rest of jung. Input is treated as untrusted: any missing, null or wrongly typed
//! field becomes a [`Loss`] or a skipped layer, never a panic.
//!
//! ## Units and colors
//!
//! Esri sizes and widths are points. They are converted to css pixels at 96 dpi, so one
//! point becomes 4/3 pixels. Marker `size` is a diameter, `circle-radius` is half of it.
//! Esri colors are `[r, g, b, a]` with alpha in 0-255 and become `rgba(r, g, b, a/255)`.
//! Layer wide `transparency` (0-100) becomes the matching per layer opacity property.
//!
//! ## Scope
//!
//! Renderers: `simple`, `uniqueValue` (match on the joined field values) and
//! `classBreaks` (step over the class field). Symbols: `esriSMS`, `esriSLS` and
//! `esriSFS`. Labels: the first `labelingInfo` class, with `labelExpression` in the
//! plain `[field]` form. Everything else is refused by its esri name into
//! [`Translation::losses`], nothing is guessed.
//!
//! Known approximations, each recorded as a loss: non circle marker shapes become
//! circles, hatch fills become solid fills, and per class dash patterns collapse to the
//! first pattern because `line-dasharray` is not data driven.
//!
//! ## Translation choices
//!
//! A `uniqueValue` renderer with `field2` or `field3` matches on the fields joined with
//! `fieldDelimiter`, which is how esri stores the joined `value` strings. Match inputs
//! are wrapped in `to-string` so numeric fields still compare against those strings.
//!
//! A `classBreaks` renderer becomes one `step` per paint property. A step has no upper
//! bound, so features above the last class keep the last class symbol instead of falling
//! back to `defaultSymbol`.
//!
//! An `esriSFSNull` fill emits only the outline, which is what esri draws, so it is not
//! reported as a loss. An `esriSLSNull` line emits no layer at all and is reported.
//!
//! One layer is emitted per drawn kind, and its id is the source-layer (or the source
//! name when there is none) plus that kind: `wells-circle`, `wells-line`, `wells-fill`,
//! `wells-outline`, `wells-label`.
//!
//! ## Example
//!
//! ```
//! use jung_esri::{Geometry, Source, translate};
//!
//! let drawing_info = serde_json::json!({
//!     "renderer": {
//!         "type": "simple",
//!         "symbol": {
//!             "type": "esriSMS",
//!             "style": "esriSMSCircle",
//!             "color": [255, 0, 0, 255],
//!             "size": 12
//!         }
//!     }
//! });
//! let source = Source {
//!     source: "ptolemy".to_string(),
//!     source_layer: "wells".to_string(),
//! };
//! let out = translate(&drawing_info, &source, Geometry::Point);
//! assert_eq!(out.layers.len(), 1);
//! assert_eq!(out.layers[0]["paint"]["circle-radius"], 8.0);
//! assert!(out.losses.is_empty());
//! ```

mod convert;
mod label;
mod symbol;

use convert::{TRANSPARENT, loss, number, round2, text};
use serde_json::{Map, Value, json};
use symbol::Paint;

/// The vector source the emitted layers reference.
#[derive(Clone, Debug)]
pub struct Source {
    /// source name in the style's `sources` block
    pub source: String,
    /// layer inside that source, empty for sources without layers
    pub source_layer: String,
}

impl Source {
    /// prefix for generated layer ids
    fn prefix(&self) -> &str {
        if self.source_layer.is_empty() {
            &self.source
        } else {
            &self.source_layer
        }
    }
}

/// Geometry type of the dataset the drawingInfo belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geometry {
    Point,
    Line,
    Polygon,
}

/// Something in the drawingInfo that did not survive translation.
#[derive(Clone, Debug, PartialEq)]
pub struct Loss {
    /// where it sits in the drawingInfo, with the offending value after a colon,
    /// for example `renderer.symbol.style: esriSFSCross`
    pub path: String,
    /// what happened, in plain words
    pub reason: String,
}

/// Mapbox GL layers plus what could not be translated.
#[derive(Clone, Debug, Default)]
pub struct Translation {
    /// raw mapbox gl layer objects, in draw order
    pub layers: Vec<Value>,
    pub losses: Vec<Loss>,
}

/// Translate an ArcGIS `drawingInfo` object into Mapbox GL layers.
///
/// `geometry` is the dataset's geometry type, which decides whether the symbols become
/// circle, line or fill layers.
pub fn translate(drawing_info: &Value, source: &Source, geometry: Geometry) -> Translation {
    let mut out = Translation::default();
    if !drawing_info.is_object() {
        loss(
            &mut out.losses,
            "drawingInfo",
            "expected a JSON object, nothing translated",
        );
        return out;
    }

    let opacity = opacity(drawing_info, &mut out.losses);
    if let Some(branched) = renderer(drawing_info, geometry, &mut out.losses) {
        emit(&branched, source, geometry, opacity, &mut out);
    }
    if let Some(layer) = label::translate(drawing_info, source, geometry, opacity, &mut out.losses)
    {
        out.layers.push(layer);
    }
    out
}

/// layer wide transparency (0-100) as an opacity, None when fully opaque
fn opacity(drawing_info: &Value, losses: &mut Vec<Loss>) -> Option<f64> {
    let raw = drawing_info.get("transparency").filter(|v| !v.is_null())?;
    let Some(t) = raw.as_f64().filter(|t| t.is_finite()) else {
        loss(losses, "transparency", "expected a number in 0-100");
        return None;
    };
    let t = t.clamp(0.0, 100.0);
    if t == 0.0 {
        return None;
    }
    Some(round2(1.0 - t / 100.0))
}

/// how the renderer picks a symbol per feature
enum Keys {
    /// one branch per uniqueValue, matched on the joined field values
    Match { input: Value, values: Vec<String> },
    /// one branch per class break, stepped over the class field
    Step { input: Value, thresholds: Vec<f64> },
}

/// a renderer flattened into branches plus a fallback
struct Branched {
    keys: Keys,
    paints: Vec<Paint>,
    /// used for the expression fallback, and it is the whole renderer for `simple`
    default: Paint,
}

impl Branched {
    fn simple(paint: Paint) -> Self {
        Branched {
            keys: Keys::Match {
                input: Value::Null,
                values: Vec::new(),
            },
            paints: Vec::new(),
            default: paint,
        }
    }

    fn all(&self) -> impl Iterator<Item = &Paint> {
        self.paints.iter().chain(std::iter::once(&self.default))
    }

    /// one paint property, as a literal when every branch agrees and an expression otherwise
    fn prop(&self, f: impl Fn(&Paint) -> Value) -> Value {
        let fallback = f(&self.default);
        let outs: Vec<Value> = self.paints.iter().map(&f).collect();
        if outs.iter().all(|o| *o == fallback) {
            return fallback;
        }
        match &self.keys {
            Keys::Match { input, values } => {
                let mut arr = vec![json!("match"), input.clone()];
                for (value, out) in values.iter().zip(outs) {
                    arr.push(json!(value));
                    arr.push(out);
                }
                arr.push(fallback);
                Value::Array(arr)
            }
            Keys::Step { input, thresholds } => {
                let mut arr = vec![json!("step"), input.clone(), fallback];
                for (threshold, out) in thresholds.iter().zip(outs) {
                    arr.push(json!(threshold));
                    arr.push(out);
                }
                Value::Array(arr)
            }
        }
    }
}

fn renderer(drawing_info: &Value, geometry: Geometry, losses: &mut Vec<Loss>) -> Option<Branched> {
    let Some(renderer) = drawing_info.get("renderer").filter(|r| !r.is_null()) else {
        loss(losses, "renderer", "missing, no layers emitted");
        return None;
    };
    if !renderer.is_object() {
        loss(losses, "renderer", "expected a JSON object");
        return None;
    }
    if renderer
        .get("visualVariables")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty())
    {
        loss(
            losses,
            "renderer.visualVariables",
            "visual variables (size, color, opacity, rotation ramps) are not translated",
        );
    }
    if text(renderer, "rotationExpression").is_some() {
        loss(
            losses,
            "renderer.rotationExpression",
            "rotation expressions are not translated",
        );
    }
    match text(renderer, "type") {
        Some("simple") => Some(Branched::simple(symbol::parse(
            renderer.get("symbol"),
            "renderer.symbol",
            geometry,
            losses,
        ))),
        Some("uniqueValue") => unique_value(renderer, geometry, losses),
        Some("classBreaks") => class_breaks(renderer, geometry, losses),
        Some(other) => {
            loss(
                losses,
                format!("renderer.type: {other}"),
                "renderer type is not translated",
            );
            None
        }
        None => {
            loss(losses, "renderer.type", "renderer type missing");
            None
        }
    }
}

fn unique_value(renderer: &Value, geometry: Geometry, losses: &mut Vec<Loss>) -> Option<Branched> {
    let Some(field1) = text(renderer, "field1") else {
        loss(
            losses,
            "renderer.field1",
            "missing, unique value renderer not translated",
        );
        return None;
    };
    let mut fields = vec![field1];
    if let Some(field2) = text(renderer, "field2") {
        fields.push(field2);
        if let Some(field3) = text(renderer, "field3") {
            fields.push(field3);
        }
    } else if let Some(field3) = text(renderer, "field3") {
        loss(
            losses,
            format!("renderer.field3: {field3}"),
            "field3 without field2, only field1 is matched",
        );
    }
    // esri stores multi field values already joined with fieldDelimiter
    let delimiter = renderer
        .get("fieldDelimiter")
        .and_then(Value::as_str)
        .unwrap_or(",");
    let input = join_fields(&fields, delimiter);

    if renderer.get("uniqueValueGroups").is_some() {
        loss(
            losses,
            "renderer.uniqueValueGroups",
            "unique value groups are not translated, only uniqueValueInfos",
        );
    }
    let infos: &[Value] = match renderer.get("uniqueValueInfos").filter(|v| !v.is_null()) {
        Some(Value::Array(infos)) => infos,
        Some(_) => {
            loss(losses, "renderer.uniqueValueInfos", "expected an array");
            &[]
        }
        None => &[],
    };

    let mut values = Vec::new();
    let mut paints = Vec::new();
    for (i, info) in infos.iter().enumerate() {
        let path = format!("renderer.uniqueValueInfos[{i}]");
        let Some(key) = value_key(info.get("value")) else {
            loss(
                losses,
                format!("{path}.value"),
                "value is missing or not a string or number, branch skipped",
            );
            continue;
        };
        values.push(key);
        paints.push(symbol::parse(
            info.get("symbol"),
            &format!("{path}.symbol"),
            geometry,
            losses,
        ));
    }
    if values.is_empty() {
        loss(
            losses,
            "renderer.uniqueValueInfos",
            "no usable unique values, only the default symbol is drawn",
        );
    }

    Some(Branched {
        keys: Keys::Match { input, values },
        paints,
        default: symbol::parse_optional(
            renderer.get("defaultSymbol"),
            "renderer.defaultSymbol",
            geometry,
            losses,
        ),
    })
}

/// match input, always stringified so numeric fields compare against esri's string values
fn join_fields(fields: &[&str], delimiter: &str) -> Value {
    let mut parts: Vec<Value> = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            parts.push(json!(delimiter));
        }
        parts.push(json!(["to-string", ["get", field]]));
    }
    if parts.len() == 1 {
        return parts.remove(0);
    }
    let mut arr = vec![json!("concat")];
    arr.append(&mut parts);
    Value::Array(arr)
}

fn value_key(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn class_breaks(renderer: &Value, geometry: Geometry, losses: &mut Vec<Loss>) -> Option<Branched> {
    let Some(field) = text(renderer, "field") else {
        loss(
            losses,
            "renderer.field",
            "missing, class breaks renderer not translated",
        );
        return None;
    };
    if text(renderer, "normalizationType").is_some() {
        loss(
            losses,
            "renderer.normalizationType",
            "normalized class breaks are not translated, raw field values are stepped",
        );
    }
    let Some(Value::Array(infos)) = renderer.get("classBreakInfos") else {
        loss(
            losses,
            "renderer.classBreakInfos",
            "missing or not an array, class breaks renderer not translated",
        );
        return None;
    };
    if infos.is_empty() {
        loss(
            losses,
            "renderer.classBreakInfos",
            "no class breaks, class breaks renderer not translated",
        );
        return None;
    }

    // lower bound of each class: its own classMinValue, else the previous class max,
    // else the renderer minValue for the first class
    let mut bounds: Vec<Option<f64>> = Vec::new();
    let mut previous_max = number(renderer, "minValue");
    for info in infos {
        bounds.push(number(info, "classMinValue").or(previous_max));
        previous_max = number(info, "classMaxValue");
    }

    let default = symbol::parse_optional(
        renderer.get("defaultSymbol"),
        "renderer.defaultSymbol",
        geometry,
        losses,
    );
    let mut paints = Vec::new();
    let mut thresholds = Vec::new();
    // a first class with no lower bound becomes the fallback instead of a step stop
    let mut fallback = default;
    for (i, info) in infos.iter().enumerate() {
        let paint = symbol::parse(
            info.get("symbol"),
            &format!("renderer.classBreakInfos[{i}].symbol"),
            geometry,
            losses,
        );
        match bounds[i] {
            Some(bound) => {
                thresholds.push(bound);
                paints.push(paint);
            }
            None if i == 0 => {
                loss(
                    losses,
                    "renderer.minValue",
                    "missing, the first class is used below the second class break",
                );
                fallback = paint;
            }
            None => {
                loss(
                    losses,
                    format!("renderer.classBreakInfos[{i}].classMinValue"),
                    "no lower bound for this class, class skipped",
                );
            }
        }
    }
    if thresholds.windows(2).any(|w| w[1] <= w[0]) {
        loss(
            losses,
            "renderer.classBreakInfos",
            "class breaks are not ascending, class breaks renderer not translated",
        );
        return None;
    }
    if let Some(max) = number(infos.last()?, "classMaxValue") {
        loss(
            losses,
            format!(
                "renderer.classBreakInfos[{}].classMaxValue: {max}",
                infos.len() - 1
            ),
            "values above the last class keep the last class symbol, a step has no upper bound",
        );
    }

    Some(Branched {
        keys: Keys::Step {
            input: json!(["get", field]),
            thresholds,
        },
        paints,
        default: fallback,
    })
}

/// build the layers for one flattened renderer
fn emit(
    branched: &Branched,
    src: &Source,
    geometry: Geometry,
    opacity: Option<f64>,
    out: &mut Translation,
) {
    match geometry {
        Geometry::Point => {
            if !branched.all().any(|p| p.marker().is_some()) {
                return;
            }
            let mut layer = LayerBuilder::new(format!("{}-circle", src.prefix()), "circle", src);
            layer.paint("circle-color", branched.prop(marker_color));
            layer.paint("circle-radius", branched.prop(marker_radius));
            if branched
                .all()
                .any(|p| p.marker().is_some_and(|m| m.stroke.is_some()))
            {
                layer.paint("circle-stroke-color", branched.prop(marker_stroke_color));
                layer.paint("circle-stroke-width", branched.prop(marker_stroke_width));
                if branched
                    .all()
                    .any(|p| p.marker().is_some_and(|m| stroke_dash(&m.stroke).is_some()))
                {
                    loss(
                        &mut out.losses,
                        "renderer.symbol.outline.style",
                        "dashed marker outlines are not supported on circle layers",
                    );
                }
            }
            if let Some(o) = opacity {
                layer.paint("circle-opacity", json!(o));
                layer.paint("circle-stroke-opacity", json!(o));
            }
            out.layers.push(layer.build());
        }
        Geometry::Line => {
            if !branched.all().any(|p| p.line().is_some()) {
                return;
            }
            let dashes: Vec<Option<Vec<f64>>> = branched
                .all()
                .filter_map(|p| p.line())
                .map(|s| s.dash.clone())
                .collect();
            let mut layer = LayerBuilder::new(format!("{}-line", src.prefix()), "line", src);
            layer.paint("line-color", branched.prop(line_color));
            layer.paint("line-width", branched.prop(line_width));
            if let Some(dash) = single_dash(&dashes, "renderer.symbol.style", &mut out.losses) {
                layer.paint("line-dasharray", json!(dash));
            }
            if let Some(o) = opacity {
                layer.paint("line-opacity", json!(o));
            }
            out.layers.push(layer.build());
        }
        Geometry::Polygon => {
            if branched
                .all()
                .any(|p| p.fill().is_some_and(|f| f.color.is_some()))
            {
                let mut layer = LayerBuilder::new(format!("{}-fill", src.prefix()), "fill", src);
                layer.paint("fill-color", branched.prop(fill_color));
                if let Some(o) = opacity {
                    layer.paint("fill-opacity", json!(o));
                }
                out.layers.push(layer.build());
            }
            if branched
                .all()
                .any(|p| p.fill().is_some_and(|f| f.stroke.is_some()))
            {
                let dashes: Vec<Option<Vec<f64>>> = branched
                    .all()
                    .filter_map(|p| p.fill())
                    .filter_map(|f| f.stroke.as_ref())
                    .map(|s| s.dash.clone())
                    .collect();
                let mut layer = LayerBuilder::new(format!("{}-outline", src.prefix()), "line", src);
                layer.paint("line-color", branched.prop(outline_color));
                layer.paint("line-width", branched.prop(outline_width));
                if let Some(dash) =
                    single_dash(&dashes, "renderer.symbol.outline.style", &mut out.losses)
                {
                    layer.paint("line-dasharray", json!(dash));
                }
                if let Some(o) = opacity {
                    layer.paint("line-opacity", json!(o));
                }
                out.layers.push(layer.build());
            }
        }
    }
}

/// line-dasharray is not data driven, so every branch has to share one pattern
fn single_dash(
    dashes: &[Option<Vec<f64>>],
    path: &str,
    losses: &mut Vec<Loss>,
) -> Option<Vec<f64>> {
    let first = dashes.first()?;
    if dashes.iter().any(|d| d != first) {
        loss(
            losses,
            path,
            "per class dash patterns are not data driven, the first pattern is used",
        );
    }
    first.clone()
}

fn stroke_dash(stroke: &Option<symbol::Stroke>) -> Option<&Vec<f64>> {
    stroke.as_ref()?.dash.as_ref()
}

fn marker_color(paint: &Paint) -> Value {
    json!(paint.marker().map_or(TRANSPARENT, |m| &m.color))
}

fn marker_radius(paint: &Paint) -> Value {
    json!(paint.marker().map_or(0.0, |m| m.radius))
}

fn marker_stroke_color(paint: &Paint) -> Value {
    json!(
        paint
            .marker()
            .and_then(|m| m.stroke.as_ref())
            .map_or(TRANSPARENT, |s| &s.color)
    )
}

fn marker_stroke_width(paint: &Paint) -> Value {
    json!(
        paint
            .marker()
            .and_then(|m| m.stroke.as_ref())
            .map_or(0.0, |s| s.width)
    )
}

fn line_color(paint: &Paint) -> Value {
    json!(paint.line().map_or(TRANSPARENT, |s| &s.color))
}

fn line_width(paint: &Paint) -> Value {
    json!(paint.line().map_or(0.0, |s| s.width))
}

fn fill_color(paint: &Paint) -> Value {
    json!(
        paint
            .fill()
            .and_then(|f| f.color.as_deref())
            .unwrap_or(TRANSPARENT)
    )
}

fn outline_color(paint: &Paint) -> Value {
    json!(
        paint
            .fill()
            .and_then(|f| f.stroke.as_ref())
            .map_or(TRANSPARENT, |s| &s.color)
    )
}

fn outline_width(paint: &Paint) -> Value {
    json!(
        paint
            .fill()
            .and_then(|f| f.stroke.as_ref())
            .map_or(0.0, |s| s.width)
    )
}

/// collects a mapbox gl layer object, skipping empty paint and layout blocks
pub(crate) struct LayerBuilder {
    layer: Map<String, Value>,
    paint: Map<String, Value>,
    layout: Map<String, Value>,
}

impl LayerBuilder {
    pub(crate) fn new(id: String, kind: &str, src: &Source) -> Self {
        let mut layer = Map::new();
        layer.insert("id".to_string(), json!(id));
        layer.insert("type".to_string(), json!(kind));
        layer.insert("source".to_string(), json!(src.source));
        if !src.source_layer.is_empty() {
            layer.insert("source-layer".to_string(), json!(src.source_layer));
        }
        LayerBuilder {
            layer,
            paint: Map::new(),
            layout: Map::new(),
        }
    }

    pub(crate) fn paint(&mut self, key: &str, value: Value) {
        self.paint.insert(key.to_string(), value);
    }

    pub(crate) fn layout(&mut self, key: &str, value: Value) {
        self.layout.insert(key.to_string(), value);
    }

    pub(crate) fn build(mut self) -> Value {
        if !self.layout.is_empty() {
            self.layer
                .insert("layout".to_string(), Value::Object(self.layout));
        }
        if !self.paint.is_empty() {
            self.layer
                .insert("paint".to_string(), Value::Object(self.paint));
        }
        Value::Object(self.layer)
    }
}
