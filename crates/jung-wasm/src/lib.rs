//! # jung-wasm
//!
//! WebAssembly bindings for the Jung symbology engine.
//! Allows browser-side rendering of styled geospatial features.

use jung_core::geometry::{Feature, Geometry, Point};
use jung_core::renderer::{BBox, Renderer};
use jung_core::text::{FontFace, FontSet};
use jung_style::{parse_style, properties_from_json};
use wasm_bindgen::prelude::*;

/// A reusable browser-side renderer at a fixed pixel size.
///
/// jung embeds no font, so a style's text layers draw nothing until `add_font`
/// supplies a face for the family their `text-font` names.
#[wasm_bindgen(js_name = Renderer)]
pub struct WasmRenderer {
    width: u32,
    height: u32,
    fonts: FontSet,
}

#[wasm_bindgen(js_class = Renderer)]
impl WasmRenderer {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            fonts: FontSet::new(),
        }
    }

    /// Add a face from TTF or OTF bytes under the family name a style's
    /// `text-font` asks for. The first family added is the fallback for styles
    /// naming a family nothing was added under. Throws when the bytes are not a
    /// font this build can read.
    pub fn add_font(&mut self, family: &str, bytes: &[u8]) -> Result<(), JsValue> {
        self.insert_font(family, bytes)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Render features from a GeoJSON string using a Mapbox GL style JSON.
    /// Returns raw RGBA pixel data.
    pub fn render_to_pixels(
        &self,
        style_json: &str,
        geojson: &str,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    ) -> Result<Vec<u8>, JsValue> {
        let bbox = BBox {
            min_x,
            min_y,
            max_x,
            max_y,
        };
        self.render_pixels(style_json, geojson, bbox)
            .map_err(|e| JsValue::from_str(&e))
    }
}

impl WasmRenderer {
    fn insert_font(&mut self, family: &str, bytes: &[u8]) -> Result<(), String> {
        let face = FontFace::from_bytes(bytes.to_vec())
            .ok_or_else(|| format!("Font error: '{family}' is not readable TTF or OTF"))?;
        self.fonts.insert(family, face);
        Ok(())
    }

    fn render_pixels(
        &self,
        style_json: &str,
        geojson: &str,
        bbox: BBox,
    ) -> Result<Vec<u8>, String> {
        let style = parse_style(style_json).map_err(|e| format!("Style error: {e}"))?;

        let features = parse_geojson_points(geojson).map_err(|e| format!("GeoJSON error: {e}"))?;

        let renderer = Renderer::new(self.width, self.height)
            .map_err(|e| format!("Renderer error: {e}"))?
            .with_fonts(self.fonts.clone());

        let buffer = renderer
            .render(&style, &features, &bbox)
            .map_err(|e| format!("Render error: {e}"))?;

        Ok(buffer.data)
    }
}

/// Minimal GeoJSON point parser (FeatureCollection with Point geometries),
/// carrying each feature's properties so `{token}` labels and data-driven
/// expressions have something to read.
fn parse_geojson_points(geojson: &str) -> Result<Vec<Feature>, String> {
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

        if geom_type != "Point" {
            continue;
        }

        let coords = geom
            .get("coordinates")
            .and_then(|c| c.as_array())
            .ok_or("Point missing coordinates")?;

        if coords.len() < 2 {
            continue;
        }

        let x = coords[0].as_f64().ok_or("invalid x coordinate")?;
        let y = coords[1].as_f64().ok_or("invalid y coordinate")?;

        features.push(Feature {
            geometry: Geometry::Point(Point { x, y }),
            properties: feat_val
                .get("properties")
                .map(properties_from_json)
                .unwrap_or_default(),
        });
    }

    Ok(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A style whose one layer draws nothing but green label text.
    const GREEN_LABEL_STYLE: &str = r##"{"layers": [{
        "id": "labels",
        "paint": { "text-color": "#00ff00" },
        "layout": { "text-field": "Springfield", "text-size": 24.0, "text-font": ["Test Sans"] }
    }]}"##;

    /// The same layer, labelling each feature with its own `name`.
    const GREEN_TOKEN_STYLE: &str = r##"{"layers": [{
        "id": "labels",
        "paint": { "text-color": "#00ff00" },
        "layout": { "text-field": "{name}", "text-size": 24.0, "text-font": ["Test Sans"] }
    }]}"##;

    const CENTRE_POINT: &str = r#"{
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [0.5, 0.5] },
                "properties": {}
            }
        ]
    }"#;

    const NAMED_CENTRE_POINT: &str = r#"{
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [0.5, 0.5] },
                "properties": { "name": "Springfield", "population": 30720 }
            }
        ]
    }"#;

    const UNIT_BBOX: BBox = BBox {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 1.0,
        max_y: 1.0,
    };

    /// jung embeds no font, so a test needing one reads the machine's and skips
    /// when it finds none.
    fn system_font_bytes() -> Option<Vec<u8>> {
        let paths = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
            "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
            "C:/Windows/Fonts/arial.ttf",
        ];
        paths.iter().find_map(|path| std::fs::read(path).ok())
    }

    fn green_pixel_count(pixels: &[u8]) -> usize {
        pixels
            .chunks(4)
            .filter(|px| px[1] == 255 && px[0] == 0 && px[3] > 0)
            .count()
    }

    #[test]
    fn a_supplied_font_turns_a_text_layer_into_pixels() {
        let Some(font_bytes) = system_font_bytes() else {
            eprintln!("skipping: no system font");
            return;
        };

        let mut renderer = WasmRenderer::new(256, 256);
        let unlabelled = renderer
            .render_pixels(GREEN_LABEL_STYLE, CENTRE_POINT, UNIT_BBOX)
            .unwrap();
        assert_eq!(
            green_pixel_count(&unlabelled),
            0,
            "no font means no label pixels"
        );

        renderer.insert_font("Test Sans", &font_bytes).unwrap();
        let labelled = renderer
            .render_pixels(GREEN_LABEL_STYLE, CENTRE_POINT, UNIT_BBOX)
            .unwrap();
        assert!(
            green_pixel_count(&labelled) > 20,
            "expected label pixels, got {}",
            green_pixel_count(&labelled)
        );
    }

    #[test]
    fn a_family_nothing_was_added_under_falls_back_to_the_first() {
        let Some(font_bytes) = system_font_bytes() else {
            eprintln!("skipping: no system font");
            return;
        };
        let mut renderer = WasmRenderer::new(256, 256);
        renderer
            .insert_font("Not The Named Family", &font_bytes)
            .unwrap();
        let pixels = renderer
            .render_pixels(GREEN_LABEL_STYLE, CENTRE_POINT, UNIT_BBOX)
            .unwrap();
        assert!(green_pixel_count(&pixels) > 20);
    }

    #[test]
    fn a_token_label_draws_from_the_feature_property() {
        let Some(font_bytes) = system_font_bytes() else {
            eprintln!("skipping: no system font");
            return;
        };
        let mut renderer = WasmRenderer::new(256, 256);
        renderer.insert_font("Test Sans", &font_bytes).unwrap();

        let named = renderer
            .render_pixels(GREEN_TOKEN_STYLE, NAMED_CENTRE_POINT, UNIT_BBOX)
            .unwrap();
        assert!(
            green_pixel_count(&named) > 20,
            "expected label pixels from the name property, got {}",
            green_pixel_count(&named)
        );

        let unnamed = renderer
            .render_pixels(GREEN_TOKEN_STYLE, CENTRE_POINT, UNIT_BBOX)
            .unwrap();
        assert_eq!(
            green_pixel_count(&unnamed),
            0,
            "a feature with no name property has nothing to label"
        );
    }

    #[test]
    fn properties_reach_the_features() {
        let features = parse_geojson_points(NAMED_CENTRE_POINT).unwrap();
        assert_eq!(
            features[0].properties.get("name"),
            Some(&jung_style::PropertyValue::String("Springfield".into()))
        );
        assert_eq!(
            features[0].properties.get("population"),
            Some(&jung_style::PropertyValue::Integer(30720))
        );
    }

    #[test]
    fn bytes_that_are_not_a_font_are_an_error() {
        let mut renderer = WasmRenderer::new(64, 64);
        let error = renderer
            .insert_font("Test Sans", b"this is not a font")
            .unwrap_err();
        assert!(error.contains("Test Sans"), "unhelpful message: {error}");
    }

    #[test]
    fn parse_simple_geojson() {
        let geojson = r#"{
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [1.0, 2.0] },
                    "properties": {}
                }
            ]
        }"#;
        let features = parse_geojson_points(geojson).unwrap();
        assert_eq!(features.len(), 1);
        assert_eq!(
            features[0].geometry,
            Geometry::Point(Point { x: 1.0, y: 2.0 })
        );
    }
}
