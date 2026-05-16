//! Mapbox Vector Tile (MVT) decoding.
//!
//! Decodes Protocol Buffer-encoded vector tiles (.mvt/.pbf) into
//! geospatial features that can be rendered by the core engine.
//!
//! Implements the [Mapbox Vector Tile spec v2](https://github.com/mapbox/vector-tile-spec).

use crate::geometry::{Feature, Geometry, Point};
use jung_style::PropertyValue;
use std::collections::HashMap;

/// A decoded vector tile.
#[derive(Debug, Clone)]
pub struct VectorTile {
    pub layers: Vec<TileLayer>,
}

/// A layer within a vector tile.
#[derive(Debug, Clone)]
pub struct TileLayer {
    pub name: String,
    pub extent: u32,
    pub features: Vec<Feature>,
}

/// MVT geometry command types.
#[derive(Debug, Clone, Copy)]
enum Command {
    MoveTo,
    LineTo,
    ClosePath,
}

/// Decode a vector tile from raw protobuf bytes.
pub fn decode_tile(data: &[u8]) -> Result<VectorTile, MvtError> {
    let mut reader = PbfReader::new(data);
    let mut layers = Vec::new();

    while let Some((field, wire_type)) = reader.next_tag()? {
        if field == 3 && wire_type == 2 {
            // Tile.layers (repeated)
            let layer_data = reader.read_bytes()?;
            layers.push(decode_layer(layer_data)?);
        } else {
            reader.skip(wire_type)?;
        }
    }

    Ok(VectorTile { layers })
}

fn decode_layer(data: &[u8]) -> Result<TileLayer, MvtError> {
    let mut reader = PbfReader::new(data);
    let mut name = String::new();
    let mut extent = 4096u32;
    let mut keys: Vec<String> = Vec::new();
    let mut values: Vec<PropertyValue> = Vec::new();
    let mut raw_features: Vec<RawFeature> = Vec::new();

    while let Some((field, wire_type)) = reader.next_tag()? {
        match field {
            1 if wire_type == 2 => {
                // name
                name = reader.read_string()?;
            }
            2 if wire_type == 2 => {
                // features
                let feat_data = reader.read_bytes()?;
                raw_features.push(decode_raw_feature(feat_data)?);
            }
            3 if wire_type == 2 => {
                // keys
                keys.push(reader.read_string()?);
            }
            4 if wire_type == 2 => {
                // values
                let val_data = reader.read_bytes()?;
                values.push(decode_value(val_data)?);
            }
            5 if wire_type == 0 => {
                // extent
                extent = reader.read_varint()? as u32;
            }
            _ => {
                reader.skip(wire_type)?;
            }
        }
    }

    // Convert raw features to Features
    let features = raw_features
        .into_iter()
        .filter_map(|rf| {
            let geometry = decode_geometry(rf.geom_type, &rf.geometry, extent)?;
            let mut properties = HashMap::new();

            for pair in rf.tags.chunks(2) {
                if pair.len() == 2 {
                    let key_idx = pair[0] as usize;
                    let val_idx = pair[1] as usize;
                    if key_idx < keys.len() && val_idx < values.len() {
                        properties.insert(keys[key_idx].clone(), values[val_idx].clone());
                    }
                }
            }

            Some(Feature {
                geometry,
                properties,
            })
        })
        .collect();

    Ok(TileLayer {
        name,
        extent,
        features,
    })
}

#[derive(Debug)]
struct RawFeature {
    geom_type: u32,
    geometry: Vec<u32>,
    tags: Vec<u32>,
}

fn decode_raw_feature(data: &[u8]) -> Result<RawFeature, MvtError> {
    let mut reader = PbfReader::new(data);
    let mut geom_type = 0u32;
    let mut geometry = Vec::new();
    let mut tags = Vec::new();

    while let Some((field, wire_type)) = reader.next_tag()? {
        match field {
            2 if wire_type == 2 => {
                // tags (packed)
                let tag_data = reader.read_bytes()?;
                let mut tr = PbfReader::new(tag_data);
                while tr.has_data() {
                    tags.push(tr.read_varint()? as u32);
                }
            }
            3 if wire_type == 0 => {
                // type
                geom_type = reader.read_varint()? as u32;
            }
            4 if wire_type == 2 => {
                // geometry (packed)
                let geom_data = reader.read_bytes()?;
                let mut gr = PbfReader::new(geom_data);
                while gr.has_data() {
                    geometry.push(gr.read_varint()? as u32);
                }
            }
            _ => {
                reader.skip(wire_type)?;
            }
        }
    }

    Ok(RawFeature {
        geom_type,
        geometry,
        tags,
    })
}

fn decode_value(data: &[u8]) -> Result<PropertyValue, MvtError> {
    let mut reader = PbfReader::new(data);
    while let Some((field, wire_type)) = reader.next_tag()? {
        match field {
            1 if wire_type == 2 => return Ok(PropertyValue::String(reader.read_string()?)),
            2 if wire_type == 5 => {
                let v = reader.read_f32()?;
                return Ok(PropertyValue::Number(v as f64));
            }
            3 if wire_type == 1 => {
                let v = reader.read_f64()?;
                return Ok(PropertyValue::Number(v));
            }
            4 if wire_type == 0 => {
                let v = reader.read_varint()?;
                return Ok(PropertyValue::Integer(v));
            }
            5 if wire_type == 0 => {
                return Ok(PropertyValue::Integer(reader.read_varint()?));
            }
            6 if wire_type == 0 => {
                let v = reader.read_varint()?;
                return Ok(PropertyValue::Integer(zigzag_decode(v as u32) as i64));
            }
            7 if wire_type == 0 => {
                let v = reader.read_varint()?;
                return Ok(PropertyValue::Boolean(v != 0));
            }
            _ => {
                reader.skip(wire_type)?;
            }
        }
    }
    Ok(PropertyValue::Null)
}

fn decode_geometry(geom_type: u32, commands: &[u32], extent: u32) -> Option<Geometry> {
    let mut cursor_x: i32 = 0;
    let mut cursor_y: i32 = 0;
    let mut rings: Vec<Vec<Point>> = Vec::new();
    let mut current_ring: Vec<Point> = Vec::new();

    let mut i = 0;
    while i < commands.len() {
        let cmd_int = commands[i];
        let cmd_id = cmd_int & 0x7;
        let count = cmd_int >> 3;
        i += 1;

        let cmd = match cmd_id {
            1 => Command::MoveTo,
            2 => Command::LineTo,
            7 => Command::ClosePath,
            _ => return None,
        };

        match cmd {
            Command::MoveTo | Command::LineTo => {
                for _ in 0..count {
                    if i + 1 >= commands.len() {
                        break;
                    }
                    let dx = zigzag_decode(commands[i]);
                    let dy = zigzag_decode(commands[i + 1]);
                    i += 2;
                    cursor_x += dx;
                    cursor_y += dy;

                    let x = cursor_x as f64 / extent as f64;
                    let y = 1.0 - cursor_y as f64 / extent as f64; // flip Y

                    if matches!(cmd, Command::MoveTo) && !current_ring.is_empty() {
                        rings.push(std::mem::take(&mut current_ring));
                    }
                    current_ring.push(Point { x, y });
                }
            }
            Command::ClosePath => {
                if let Some(first) = current_ring.first().cloned() {
                    current_ring.push(first);
                }
                rings.push(std::mem::take(&mut current_ring));
            }
        }
    }

    if !current_ring.is_empty() {
        rings.push(current_ring);
    }

    match geom_type {
        1 => {
            // Point
            let all_points: Vec<Point> = rings.into_iter().flatten().collect();
            if all_points.len() == 1 {
                Some(Geometry::Point(all_points[0]))
            } else if all_points.is_empty() {
                None
            } else {
                Some(Geometry::MultiPoint(all_points))
            }
        }
        2 => {
            // LineString
            if rings.len() == 1 {
                Some(Geometry::LineString(rings.into_iter().next().unwrap()))
            } else {
                Some(Geometry::MultiLineString(rings))
            }
        }
        3 => {
            // Polygon
            if rings.is_empty() {
                None
            } else {
                let exterior = rings.remove(0);
                Some(Geometry::Polygon {
                    exterior,
                    holes: rings,
                })
            }
        }
        _ => None,
    }
}

fn zigzag_decode(n: u32) -> i32 {
    ((n >> 1) as i32) ^ -((n & 1) as i32)
}

/// MVT decode errors.
#[derive(Debug, thiserror::Error)]
pub enum MvtError {
    #[error("unexpected end of data")]
    UnexpectedEof,
    #[error("invalid wire type: {0}")]
    InvalidWireType(u8),
    #[error("invalid UTF-8 string")]
    InvalidUtf8,
}

/// Minimal protobuf reader.
struct PbfReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> PbfReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn has_data(&self) -> bool {
        self.pos < self.data.len()
    }

    fn next_tag(&mut self) -> Result<Option<(u32, u8)>, MvtError> {
        if self.pos >= self.data.len() {
            return Ok(None);
        }
        let v = self.read_varint()? as u32;
        let field = v >> 3;
        let wire_type = (v & 0x7) as u8;
        Ok(Some((field, wire_type)))
    }

    fn read_varint(&mut self) -> Result<i64, MvtError> {
        let mut result: u64 = 0;
        let mut shift = 0;
        loop {
            if self.pos >= self.data.len() {
                return Err(MvtError::UnexpectedEof);
            }
            let byte = self.data[self.pos];
            self.pos += 1;
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift >= 64 {
                return Err(MvtError::UnexpectedEof);
            }
        }
        Ok(result as i64)
    }

    fn read_bytes(&mut self) -> Result<&'a [u8], MvtError> {
        let len = self.read_varint()? as usize;
        if self.pos + len > self.data.len() {
            return Err(MvtError::UnexpectedEof);
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    fn read_string(&mut self) -> Result<String, MvtError> {
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes.to_vec()).map_err(|_| MvtError::InvalidUtf8)
    }

    fn read_f32(&mut self) -> Result<f32, MvtError> {
        if self.pos + 4 > self.data.len() {
            return Err(MvtError::UnexpectedEof);
        }
        let bytes: [u8; 4] = self.data[self.pos..self.pos + 4].try_into().unwrap();
        self.pos += 4;
        Ok(f32::from_le_bytes(bytes))
    }

    fn read_f64(&mut self) -> Result<f64, MvtError> {
        if self.pos + 8 > self.data.len() {
            return Err(MvtError::UnexpectedEof);
        }
        let bytes: [u8; 8] = self.data[self.pos..self.pos + 8].try_into().unwrap();
        self.pos += 8;
        Ok(f64::from_le_bytes(bytes))
    }

    fn skip(&mut self, wire_type: u8) -> Result<(), MvtError> {
        match wire_type {
            0 => {
                self.read_varint()?;
            }
            1 => {
                if self.pos + 8 > self.data.len() {
                    return Err(MvtError::UnexpectedEof);
                }
                self.pos += 8;
            }
            2 => {
                self.read_bytes()?;
            }
            5 => {
                if self.pos + 4 > self.data.len() {
                    return Err(MvtError::UnexpectedEof);
                }
                self.pos += 4;
            }
            _ => return Err(MvtError::InvalidWireType(wire_type)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_decode_values() {
        assert_eq!(zigzag_decode(0), 0);
        assert_eq!(zigzag_decode(1), -1);
        assert_eq!(zigzag_decode(2), 1);
        assert_eq!(zigzag_decode(3), -2);
        assert_eq!(zigzag_decode(4), 2);
    }

    #[test]
    fn decode_empty_tile() {
        let tile = decode_tile(&[]).unwrap();
        assert!(tile.layers.is_empty());
    }

    #[test]
    fn decode_geometry_point() {
        // MoveTo(1) + coords (10, 10) in MVT command encoding
        // command_integer = (1 << 3) | 1 = 9
        // zigzag(10) = 20, zigzag(10) = 20
        let commands = vec![9, 20, 20];
        let geom = decode_geometry(1, &commands, 4096).unwrap();
        match geom {
            Geometry::Point(p) => {
                assert!((p.x - 10.0 / 4096.0).abs() < 0.001);
            }
            _ => panic!("expected Point"),
        }
    }

    #[test]
    fn decode_geometry_linestring() {
        // MoveTo(1) x=0,y=0, LineTo(2) x=10,y=0 x=10,y=10
        let commands = vec![
            9, 0, 0, // MoveTo(1): (0,0)
            18, 20, 0, // LineTo(2): dx=10,dy=0
            0, 20, //            dx=0,dy=10
        ];
        let geom = decode_geometry(2, &commands, 4096).unwrap();
        match geom {
            Geometry::LineString(pts) => {
                assert_eq!(pts.len(), 3);
            }
            _ => panic!("expected LineString"),
        }
    }

    #[test]
    fn decode_geometry_polygon() {
        // Simple triangle: MoveTo, LineTo(3), ClosePath
        let commands = vec![
            9, 0, 0, // MoveTo: (0,0)
            26, 20, 0, 0, 20, 21, 21, // LineTo(3): (+10,0),(0,+10),(-10,-10) zigzag
            15, // ClosePath
        ];
        let geom = decode_geometry(3, &commands, 4096).unwrap();
        match geom {
            Geometry::Polygon { exterior, holes } => {
                assert!(exterior.len() >= 4); // closed ring
                assert!(holes.is_empty());
            }
            _ => panic!("expected Polygon"),
        }
    }

    #[test]
    fn pbf_reader_varint() {
        let data = [0x96, 0x01]; // varint for 150
        let mut reader = PbfReader::new(&data);
        assert_eq!(reader.read_varint().unwrap(), 150);
    }

    #[test]
    fn pbf_reader_string() {
        // length-prefixed string "abc"
        let data = [3, b'a', b'b', b'c'];
        let mut reader = PbfReader::new(&data);
        assert_eq!(reader.read_string().unwrap(), "abc");
    }

    /// Build a minimal valid MVT tile with one layer, one point feature.
    fn build_test_tile() -> Vec<u8> {
        let mut tile = Vec::new();

        // Build layer
        let mut layer = vec![
            0x0A, // tag: field 1, wire type 2 (name)
            4,    // length
            b't', b'e', b's', b't', 0x1A, // tag: field 3, wire type 2 (keys)
            4,    // length
            b'n', b'a', b'm', b'e',
        ];

        // field 4: values = string "hello"
        let value = vec![
            0x0A, // field 1, wire type 2 (string_value)
            5, b'h', b'e', b'l', b'l', b'o',
        ];
        layer.push(0x22); // tag: field 4, wire type 2
        layer.push(value.len() as u8);
        layer.extend_from_slice(&value);

        // field 2: feature
        let mut feature = vec![
            0x12, // field 2, wire type 2 (tags packed)
            2,    // length
            0,    // key index 0
            0,    // value index 0
            0x18, // field 3, wire type 0 (type)
            1,    // POINT
        ];

        // geometry: MoveTo(1) x=10 y=10
        // zigzag(10) = 20, which fits in one varint byte
        let mut geom = Vec::new();
        encode_varint(&mut geom, 9); // MoveTo(1): (1 << 3) | 1 = 9
        encode_varint(&mut geom, 20); // zigzag(10) = 20
        encode_varint(&mut geom, 20); // zigzag(10) = 20
        feature.push(0x22); // field 4, wire type 2
        encode_varint(&mut feature, geom.len() as u64);
        feature.extend_from_slice(&geom);

        layer.push(0x12); // tag: field 2, wire type 2
        encode_varint(&mut layer, feature.len() as u64);
        layer.extend_from_slice(&feature);

        // field 5: extent = 4096
        layer.push(0x28); // field 5, wire type 0
        encode_varint(&mut layer, 4096);

        // Tile: field 3 = layer
        tile.push(0x1A); // tag: field 3, wire type 2
        encode_varint(&mut tile, layer.len() as u64);
        tile.extend_from_slice(&layer);

        tile
    }

    fn encode_varint(buf: &mut Vec<u8>, mut value: u64) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                buf.push(byte);
                break;
            }
            buf.push(byte | 0x80);
        }
    }

    #[test]
    fn decode_minimal_tile() {
        let data = build_test_tile();
        let tile = decode_tile(&data).unwrap();
        assert_eq!(tile.layers.len(), 1);
        assert_eq!(tile.layers[0].name, "test");
        assert_eq!(tile.layers[0].extent, 4096);
        assert_eq!(tile.layers[0].features.len(), 1);

        let feature = &tile.layers[0].features[0];
        match &feature.geometry {
            Geometry::Point(p) => {
                assert!((p.x - 10.0 / 4096.0).abs() < 0.001);
            }
            _ => panic!("expected Point"),
        }
        assert_eq!(
            feature.properties.get("name"),
            Some(&PropertyValue::String("hello".to_string()))
        );
    }
}
