use jung_esri::Source;
use serde_json::{Value, json};

pub fn source() -> Source {
    Source {
        source: "ptolemy".to_string(),
        source_layer: "wells".to_string(),
    }
}

/// simple renderer with an esriSMS circle, the shape a hosted point layer publishes
pub fn simple_point() -> Value {
    json!({
        "renderer": {
            "type": "simple",
            "symbol": {
                "type": "esriSMS",
                "style": "esriSMSCircle",
                "color": [227, 139, 79, 255],
                "size": 9,
                "angle": 0,
                "xoffset": 0,
                "yoffset": 0,
                "outline": {
                    "type": "esriSLS",
                    "style": "esriSLSSolid",
                    "color": [255, 255, 255, 255],
                    "width": 0.75
                }
            },
            "label": "",
            "description": ""
        },
        "transparency": 0,
        "labelingInfo": null
    })
}

/// uniqueValue on one field with two values and a default symbol
pub fn unique_value_polygon() -> Value {
    json!({
        "renderer": {
            "type": "uniqueValue",
            "field1": "LANDUSE",
            "field2": null,
            "field3": null,
            "fieldDelimiter": ",",
            "defaultSymbol": {
                "type": "esriSFS",
                "style": "esriSFSSolid",
                "color": [200, 200, 200, 255],
                "outline": {
                    "type": "esriSLS",
                    "style": "esriSLSSolid",
                    "color": [110, 110, 110, 255],
                    "width": 0.4
                }
            },
            "defaultLabel": "Other",
            "uniqueValueInfos": [
                {
                    "value": "residential",
                    "label": "Residential",
                    "description": "",
                    "symbol": {
                        "type": "esriSFS",
                        "style": "esriSFSSolid",
                        "color": [255, 170, 0, 255],
                        "outline": {
                            "type": "esriSLS",
                            "style": "esriSLSSolid",
                            "color": [110, 110, 110, 255],
                            "width": 0.4
                        }
                    }
                },
                {
                    "value": "commercial",
                    "label": "Commercial",
                    "description": "",
                    "symbol": {
                        "type": "esriSFS",
                        "style": "esriSFSSolid",
                        "color": [0, 92, 230, 255],
                        "outline": {
                            "type": "esriSLS",
                            "style": "esriSLSSolid",
                            "color": [110, 110, 110, 255],
                            "width": 0.4
                        }
                    }
                }
            ]
        },
        "transparency": 25
    })
}

/// classBreaks with three breaks over a numeric field
pub fn class_breaks_line() -> Value {
    json!({
        "renderer": {
            "type": "classBreaks",
            "field": "AADT",
            "classificationMethod": "esriClassifyManual",
            "minValue": 0,
            "defaultSymbol": {
                "type": "esriSLS",
                "style": "esriSLSSolid",
                "color": [190, 190, 190, 255],
                "width": 0.5
            },
            "defaultLabel": "No data",
            "classBreakInfos": [
                {
                    "classMinValue": 0,
                    "classMaxValue": 1000,
                    "label": "0 - 1000",
                    "description": "",
                    "symbol": {
                        "type": "esriSLS",
                        "style": "esriSLSSolid",
                        "color": [255, 235, 175, 255],
                        "width": 1
                    }
                },
                {
                    "classMinValue": 1000,
                    "classMaxValue": 5000,
                    "label": "1000 - 5000",
                    "description": "",
                    "symbol": {
                        "type": "esriSLS",
                        "style": "esriSLSSolid",
                        "color": [255, 170, 0, 255],
                        "width": 2
                    }
                },
                {
                    "classMinValue": 5000,
                    "classMaxValue": 25000,
                    "label": "5000 - 25000",
                    "description": "",
                    "symbol": {
                        "type": "esriSLS",
                        "style": "esriSLSSolid",
                        "color": [230, 0, 0, 255],
                        "width": 4
                    }
                }
            ]
        }
    })
}

/// simple renderer plus a labelingInfo class with a halo
pub fn labeled_point() -> Value {
    json!({
        "renderer": {
            "type": "simple",
            "symbol": {
                "type": "esriSMS",
                "style": "esriSMSCircle",
                "color": [0, 0, 0, 255],
                "size": 6
            }
        },
        "transparency": 0,
        "labelingInfo": [
            {
                "labelPlacement": "esriServerPointLabelPlacementAboveRight",
                "labelExpression": "[NAME]",
                "useCodedValues": true,
                "symbol": {
                    "type": "esriTS",
                    "color": [39, 39, 39, 255],
                    "backgroundColor": null,
                    "borderLineColor": null,
                    "verticalAlignment": "baseline",
                    "horizontalAlignment": "left",
                    "rightToLeft": false,
                    "angle": 0,
                    "xoffset": 0,
                    "yoffset": 0,
                    "kerning": true,
                    "haloColor": [255, 255, 255, 255],
                    "haloSize": 1.5,
                    "font": {
                        "size": 9,
                        "style": "normal",
                        "weight": "normal",
                        "decoration": "none"
                    }
                },
                "minScale": 0,
                "maxScale": 0,
                "where": null
            }
        ]
    })
}
