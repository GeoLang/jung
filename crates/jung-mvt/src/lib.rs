//! # jung-mvt
//!
//! Mapbox Vector Tile decoding.
//!
//! Decodes the protobuf of the [MVT spec v2](https://github.com/mapbox/vector-tile-spec)
//! into layers, features, geometry and attributes. Coordinates stay in the
//! tile's own units, 0 to `extent` with y increasing southward, so placing a
//! tile on screen is a scale and a translate.

use std::collections::HashMap;

/// A vector tile that cannot be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("tile data ends mid-field")]
    Truncated,
    #[error("varint longer than 64 bits")]
    VarintTooLong,
    #[error("string is not valid UTF-8")]
    InvalidUtf8,
    #[error("wire type {0}")]
    UnknownWireType(u8),
}

/// A decoded vector tile.
#[derive(Debug, Clone)]
pub struct VectorTile {
    pub layers: Vec<VectorLayer>,
}

impl VectorTile {
    pub fn layer(&self, name: &str) -> Option<&VectorLayer> {
        self.layers.iter().find(|l| l.name == name)
    }
}

/// One layer of a vector tile.
#[derive(Debug, Clone)]
pub struct VectorLayer {
    pub name: String,
    /// Tile-local coordinate span, 4096 for most sources.
    pub extent: u32,
    pub features: Vec<VectorFeature>,
}

/// A feature, with its geometry in tile units.
#[derive(Debug, Clone)]
pub struct VectorFeature {
    pub geometry: TileGeometry,
    pub attributes: HashMap<String, AttributeValue>,
}

/// Feature geometry in tile units. Each variant holds every part of a multi-part
/// geometry, so a single point is a `Points` of length one.
#[derive(Debug, Clone, PartialEq)]
pub enum TileGeometry {
    Points(Vec<[f32; 2]>),
    Lines(Vec<Vec<[f32; 2]>>),
    Polygons(Vec<TilePolygon>),
}

/// One polygon: an exterior ring and the rings punched out of it.
#[derive(Debug, Clone, PartialEq)]
pub struct TilePolygon {
    pub exterior: Vec<[f32; 2]>,
    pub holes: Vec<Vec<[f32; 2]>>,
}

/// A feature attribute value.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    String(String),
    Number(f64),
    Integer(i64),
    Boolean(bool),
}

/// Decode a vector tile from raw protobuf bytes.
pub fn decode_tile(data: &[u8]) -> Result<VectorTile, Error> {
    let mut reader = PbfReader::new(data);
    let mut layers = Vec::new();

    while let Some((field, wire_type)) = reader.next_tag()? {
        if field == TILE_LAYERS && wire_type == WIRE_BYTES {
            let layer = reader.read_bytes()?;
            layers.push(decode_layer(layer)?);
        } else {
            reader.skip(wire_type)?;
        }
    }

    Ok(VectorTile { layers })
}

// Tile
const TILE_LAYERS: u32 = 3;

// Layer
const LAYER_NAME: u32 = 1;
const LAYER_FEATURES: u32 = 2;
const LAYER_KEYS: u32 = 3;
const LAYER_VALUES: u32 = 4;
const LAYER_EXTENT: u32 = 5;

// Feature
const FEATURE_TAGS: u32 = 2;
const FEATURE_TYPE: u32 = 3;
const FEATURE_GEOMETRY: u32 = 4;

// Value
const VALUE_STRING: u32 = 1;
const VALUE_FLOAT: u32 = 2;
const VALUE_DOUBLE: u32 = 3;
const VALUE_INT: u32 = 4;
const VALUE_UINT: u32 = 5;
const VALUE_SINT: u32 = 6;
const VALUE_BOOL: u32 = 7;

// GeomType
const GEOM_POINT: u64 = 1;
const GEOM_LINESTRING: u64 = 2;
const GEOM_POLYGON: u64 = 3;

// geometry commands
const MOVE_TO: u32 = 1;
const LINE_TO: u32 = 2;
const CLOSE_PATH: u32 = 7;

const DEFAULT_EXTENT: u32 = 4096;

fn decode_layer(data: &[u8]) -> Result<VectorLayer, Error> {
    let mut reader = PbfReader::new(data);
    let mut name = String::new();
    let mut extent = DEFAULT_EXTENT;
    let mut keys: Vec<String> = Vec::new();
    let mut values: Vec<Option<AttributeValue>> = Vec::new();
    let mut raw_features: Vec<RawFeature> = Vec::new();

    while let Some((field, wire_type)) = reader.next_tag()? {
        match (field, wire_type) {
            (LAYER_NAME, WIRE_BYTES) => name = reader.read_string()?,
            (LAYER_FEATURES, WIRE_BYTES) => {
                let feature = reader.read_bytes()?;
                raw_features.push(decode_raw_feature(feature)?);
            }
            (LAYER_KEYS, WIRE_BYTES) => keys.push(reader.read_string()?),
            (LAYER_VALUES, WIRE_BYTES) => {
                let value = reader.read_bytes()?;
                values.push(decode_value(value)?);
            }
            (LAYER_EXTENT, WIRE_VARINT) => extent = reader.read_varint()? as u32,
            _ => reader.skip(wire_type)?,
        }
    }

    let features = raw_features
        .into_iter()
        .filter_map(|raw| {
            let geometry = decode_geometry(raw.geom_type, &raw.geometry)?;
            let mut attributes = HashMap::new();
            for pair in raw.tags.as_chunks::<2>().0 {
                let (Some(key), Some(Some(value))) =
                    (keys.get(pair[0] as usize), values.get(pair[1] as usize))
                else {
                    continue;
                };
                attributes.insert(key.clone(), value.clone());
            }
            Some(VectorFeature {
                geometry,
                attributes,
            })
        })
        .collect();

    Ok(VectorLayer {
        name,
        extent,
        features,
    })
}

struct RawFeature {
    geom_type: u64,
    geometry: Vec<u32>,
    tags: Vec<u32>,
}

fn decode_raw_feature(data: &[u8]) -> Result<RawFeature, Error> {
    let mut reader = PbfReader::new(data);
    let mut geom_type = 0;
    let mut geometry = Vec::new();
    let mut tags = Vec::new();

    while let Some((field, wire_type)) = reader.next_tag()? {
        match (field, wire_type) {
            (FEATURE_TAGS, WIRE_BYTES) => tags = reader.read_packed_varints()?,
            (FEATURE_TYPE, WIRE_VARINT) => geom_type = reader.read_varint()?,
            (FEATURE_GEOMETRY, WIRE_BYTES) => geometry = reader.read_packed_varints()?,
            _ => reader.skip(wire_type)?,
        }
    }

    Ok(RawFeature {
        geom_type,
        geometry,
        tags,
    })
}

/// A `Value` message carries exactly one field. An empty one decodes to `None`,
/// which keeps the index alignment the tags rely on.
fn decode_value(data: &[u8]) -> Result<Option<AttributeValue>, Error> {
    let mut reader = PbfReader::new(data);
    while let Some((field, wire_type)) = reader.next_tag()? {
        let value = match (field, wire_type) {
            (VALUE_STRING, WIRE_BYTES) => AttributeValue::String(reader.read_string()?),
            (VALUE_FLOAT, WIRE_FIXED32) => AttributeValue::Number(reader.read_f32()? as f64),
            (VALUE_DOUBLE, WIRE_FIXED64) => AttributeValue::Number(reader.read_f64()?),
            (VALUE_INT | VALUE_UINT, WIRE_VARINT) => {
                AttributeValue::Integer(reader.read_varint()? as i64)
            }
            (VALUE_SINT, WIRE_VARINT) => {
                AttributeValue::Integer(zigzag_decode_64(reader.read_varint()?))
            }
            (VALUE_BOOL, WIRE_VARINT) => AttributeValue::Boolean(reader.read_varint()? != 0),
            _ => {
                reader.skip(wire_type)?;
                continue;
            }
        };
        return Ok(Some(value));
    }
    Ok(None)
}

fn decode_geometry(geom_type: u64, commands: &[u32]) -> Option<TileGeometry> {
    let parts = decode_parts(commands)?;
    if parts.is_empty() {
        return None;
    }

    match geom_type {
        GEOM_POINT => Some(TileGeometry::Points(parts.concat())),
        GEOM_LINESTRING => {
            let lines: Vec<_> = parts.into_iter().filter(|part| part.len() > 1).collect();
            (!lines.is_empty()).then_some(TileGeometry::Lines(lines))
        }
        GEOM_POLYGON => {
            let polygons = assemble_polygons(parts);
            (!polygons.is_empty()).then_some(TileGeometry::Polygons(polygons))
        }
        _ => None,
    }
}

/// Walk the command stream into rings and line parts. A `MoveTo` starts a new
/// part, a `ClosePath` ends one. Returns `None` for an unknown command, which
/// drops the feature rather than the whole tile.
fn decode_parts(commands: &[u32]) -> Option<Vec<Vec<[f32; 2]>>> {
    let mut parts: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut current: Vec<[f32; 2]> = Vec::new();
    let mut cursor_x = 0i32;
    let mut cursor_y = 0i32;

    let mut i = 0;
    while i < commands.len() {
        let command = commands[i] & 0x7;
        let count = commands[i] >> 3;
        i += 1;

        match command {
            MOVE_TO | LINE_TO => {
                for _ in 0..count {
                    if i + 1 >= commands.len() {
                        break;
                    }
                    cursor_x = cursor_x.wrapping_add(zigzag_decode(commands[i]));
                    cursor_y = cursor_y.wrapping_add(zigzag_decode(commands[i + 1]));
                    i += 2;
                    if command == MOVE_TO && !current.is_empty() {
                        parts.push(std::mem::take(&mut current));
                    }
                    current.push([cursor_x as f32, cursor_y as f32]);
                }
            }
            CLOSE_PATH => {
                if let Some(&first) = current.first() {
                    current.push(first);
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => return None,
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }
    Some(parts)
}

/// Split polygon rings by winding: a positive area starts a new polygon, a
/// negative one is a hole in the polygon before it.
fn assemble_polygons(rings: Vec<Vec<[f32; 2]>>) -> Vec<TilePolygon> {
    let mut polygons: Vec<TilePolygon> = Vec::new();
    for ring in rings {
        if ring.len() < 4 {
            continue;
        }
        match polygons.last_mut() {
            Some(polygon) if signed_area(&ring) < 0.0 => polygon.holes.push(ring),
            _ => polygons.push(TilePolygon {
                exterior: ring,
                holes: Vec::new(),
            }),
        }
    }
    polygons
}

/// Twice the shoelace area. Positive means clockwise in tile space, which the
/// spec uses for exterior rings.
fn signed_area(ring: &[[f32; 2]]) -> f32 {
    let mut sum = 0.0;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum
}

fn zigzag_decode(n: u32) -> i32 {
    ((n >> 1) as i32) ^ -((n & 1) as i32)
}

fn zigzag_decode_64(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

const WIRE_VARINT: u8 = 0;
const WIRE_FIXED64: u8 = 1;
const WIRE_BYTES: u8 = 2;
const WIRE_FIXED32: u8 = 5;

/// Minimal protobuf reader.
struct PbfReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> PbfReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn next_tag(&mut self) -> Result<Option<(u32, u8)>, Error> {
        if self.pos >= self.data.len() {
            return Ok(None);
        }
        let tag = self.read_varint()?;
        Ok(Some(((tag >> 3) as u32, (tag & 0x7) as u8)))
    }

    fn read_varint(&mut self) -> Result<u64, Error> {
        let mut result: u64 = 0;
        let mut shift = 0;
        loop {
            let byte = *self.data.get(self.pos).ok_or(Error::Truncated)?;
            self.pos += 1;
            result |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift >= 64 {
                return Err(Error::VarintTooLong);
            }
        }
    }

    fn read_bytes(&mut self) -> Result<&'a [u8], Error> {
        let len = self.read_varint()? as usize;
        let end = self.pos.checked_add(len).ok_or(Error::Truncated)?;
        let slice = self.data.get(self.pos..end).ok_or(Error::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn read_string(&mut self) -> Result<String, Error> {
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidUtf8)
    }

    fn read_packed_varints(&mut self) -> Result<Vec<u32>, Error> {
        let mut reader = PbfReader::new(self.read_bytes()?);
        let mut out = Vec::new();
        while reader.pos < reader.data.len() {
            out.push(reader.read_varint()? as u32);
        }
        Ok(out)
    }

    fn read_f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_le_bytes(self.read_fixed::<4>()?))
    }

    fn read_f64(&mut self) -> Result<f64, Error> {
        Ok(f64::from_le_bytes(self.read_fixed::<8>()?))
    }

    fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let end = self.pos + N;
        let bytes: [u8; N] = self
            .data
            .get(self.pos..end)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?;
        self.pos = end;
        Ok(bytes)
    }

    fn skip(&mut self, wire_type: u8) -> Result<(), Error> {
        match wire_type {
            WIRE_VARINT => {
                self.read_varint()?;
            }
            WIRE_FIXED64 => {
                self.read_fixed::<8>()?;
            }
            WIRE_BYTES => {
                self.read_bytes()?;
            }
            WIRE_FIXED32 => {
                self.read_fixed::<4>()?;
            }
            _ => return Err(Error::UnknownWireType(wire_type)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zigzag() {
        assert_eq!(zigzag_decode(0), 0);
        assert_eq!(zigzag_decode(1), -1);
        assert_eq!(zigzag_decode(2), 1);
        assert_eq!(zigzag_decode(4294967294), 2147483647);
        assert_eq!(zigzag_decode_64(3), -2);
    }

    #[test]
    fn test_empty_tile_has_no_layers() {
        assert!(decode_tile(&[]).unwrap().layers.is_empty());
    }

    #[test]
    fn test_reads_a_multi_byte_varint() {
        let data = [0x96, 0x01];
        assert_eq!(PbfReader::new(&data).read_varint().unwrap(), 150);
    }

    #[test]
    fn test_reads_a_length_prefixed_string() {
        let data = [3, b'a', b'b', b'c'];
        assert_eq!(PbfReader::new(&data).read_string().unwrap(), "abc");
    }

    #[test]
    fn test_point_keeps_tile_units() {
        // MoveTo(1) at (10, 20), zigzagged
        let geometry = decode_geometry(GEOM_POINT, &[9, 20, 40]).unwrap();
        assert_eq!(geometry, TileGeometry::Points(vec![[10.0, 20.0]]));
    }

    #[test]
    fn test_multipoint() {
        // MoveTo(2): (10, 10) then a further (5, 5)
        let geometry = decode_geometry(GEOM_POINT, &[17, 20, 20, 10, 10]).unwrap();
        assert_eq!(
            geometry,
            TileGeometry::Points(vec![[10.0, 10.0], [15.0, 15.0]])
        );
    }

    #[test]
    fn test_linestring_is_relative_to_the_cursor() {
        // MoveTo(1) (0,0), LineTo(2) (+10,0) then (0,+10)
        let geometry = decode_geometry(GEOM_LINESTRING, &[9, 0, 0, 18, 20, 0, 0, 20]).unwrap();
        assert_eq!(
            geometry,
            TileGeometry::Lines(vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]])
        );
    }

    #[test]
    fn test_multilinestring_splits_on_move_to() {
        let mut commands = vec![9, 0, 0, 18, 20, 0, 0, 20];
        commands.extend_from_slice(&[9, 20, 20, 10, 40, 0]);
        let TileGeometry::Lines(lines) = decode_geometry(GEOM_LINESTRING, &commands).unwrap()
        else {
            panic!("expected lines");
        };
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1][0], [20.0, 20.0]);
    }

    /// A single point is not a line, and dropping it must not drop the feature's
    /// other parts.
    #[test]
    fn test_degenerate_line_part_is_dropped() {
        let mut commands = vec![9, 0, 0, 18, 20, 0, 0, 20];
        commands.extend_from_slice(&[9, 20, 20]);
        let TileGeometry::Lines(lines) = decode_geometry(GEOM_LINESTRING, &commands).unwrap()
        else {
            panic!("expected lines");
        };
        assert_eq!(lines.len(), 1);
        assert!(decode_geometry(GEOM_LINESTRING, &[9, 20, 20]).is_none());
    }

    /// A square, wound clockwise in tile space, is one polygon with no holes.
    #[test]
    fn test_polygon_closes_its_ring() {
        let commands = clockwise_square(0, 0, 10);
        let TileGeometry::Polygons(polygons) = decode_geometry(GEOM_POLYGON, &commands).unwrap()
        else {
            panic!("expected polygons");
        };
        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].exterior.len(), 5);
        assert_eq!(polygons[0].exterior[0], polygons[0].exterior[4]);
        assert!(polygons[0].holes.is_empty());
    }

    /// Winding decides: a counter-clockwise ring after an exterior is a hole,
    /// a second clockwise ring is a second polygon.
    #[test]
    fn test_polygon_winding_separates_holes_from_parts() {
        let mut with_hole = clockwise_square(0, 0, 100);
        with_hole.extend_from_slice(&counter_clockwise_square(10, -90, 10));
        let TileGeometry::Polygons(polygons) = decode_geometry(GEOM_POLYGON, &with_hole).unwrap()
        else {
            panic!("expected polygons");
        };
        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].holes.len(), 1);

        let mut multi = clockwise_square(0, 0, 10);
        multi.extend_from_slice(&clockwise_square(50, 50, 10));
        let TileGeometry::Polygons(polygons) = decode_geometry(GEOM_POLYGON, &multi).unwrap()
        else {
            panic!("expected polygons");
        };
        assert_eq!(polygons.len(), 2);
        assert!(polygons.iter().all(|p| p.holes.is_empty()));
    }

    /// A tile whose first ring is wound the wrong way still draws as a polygon.
    #[test]
    fn test_leading_hole_becomes_an_exterior() {
        let commands = counter_clockwise_square(0, 0, 10);
        let TileGeometry::Polygons(polygons) = decode_geometry(GEOM_POLYGON, &commands).unwrap()
        else {
            panic!("expected polygons");
        };
        assert_eq!(polygons.len(), 1);
        assert!(polygons[0].holes.is_empty());
    }

    #[test]
    fn test_unknown_command_drops_the_feature() {
        assert!(decode_geometry(GEOM_POINT, &[9, 20, 20, 4]).is_none());
        assert!(decode_geometry(9, &[9, 20, 20]).is_none());
    }

    #[test]
    fn test_truncated_tile_is_an_error() {
        // a layer field claiming more bytes than the tile holds
        assert!(decode_tile(&[0x1A, 40, 1, 2, 3]).is_err());
        assert!(decode_tile(&[0x08]).is_err());
    }

    #[test]
    fn test_decodes_layer_features_and_attributes() {
        let tile = decode_tile(&fixture_tile()).unwrap();
        assert_eq!(tile.layers.len(), 1);

        let layer = tile.layer("places").unwrap();
        assert_eq!(layer.extent, 4096);
        assert_eq!(layer.features.len(), 1);

        let feature = &layer.features[0];
        assert_eq!(feature.geometry, TileGeometry::Points(vec![[25.0, 17.0]]));
        assert_eq!(
            feature.attributes.get("name"),
            Some(&AttributeValue::String("London".into()))
        );
        assert_eq!(
            feature.attributes.get("population"),
            Some(&AttributeValue::Integer(8_982_000))
        );
        assert_eq!(
            feature.attributes.get("capital"),
            Some(&AttributeValue::Boolean(true))
        );
        assert!(!feature.attributes.contains_key("missing"));
    }

    /// Rings are wound in tile space, where y grows downward, so a clockwise
    /// ring reads counter-clockwise on paper.
    fn clockwise_square(x: i32, y: i32, size: i32) -> Vec<u32> {
        ring(x, y, [(size, 0), (0, size), (-size, 0)])
    }

    fn counter_clockwise_square(x: i32, y: i32, size: i32) -> Vec<u32> {
        ring(x, y, [(0, size), (size, 0), (0, -size)])
    }

    fn ring(x: i32, y: i32, steps: [(i32, i32); 3]) -> Vec<u32> {
        let mut commands = vec![command(MOVE_TO, 1), zigzag(x), zigzag(y)];
        commands.push(command(LINE_TO, 3));
        for (dx, dy) in steps {
            commands.push(zigzag(dx));
            commands.push(zigzag(dy));
        }
        commands.push(command(CLOSE_PATH, 1));
        commands
    }

    fn command(id: u32, count: u32) -> u32 {
        (count << 3) | id
    }

    fn zigzag(value: i32) -> u32 {
        ((value << 1) ^ (value >> 31)) as u32
    }

    /// One layer, one point feature with three attributes.
    fn fixture_tile() -> Vec<u8> {
        let mut layer = Vec::new();
        write_bytes(&mut layer, LAYER_NAME, b"places");

        let mut feature = Vec::new();
        write_bytes(&mut feature, FEATURE_TAGS, &packed(&[0, 0, 1, 1, 2, 2]));
        write_varint_field(&mut feature, FEATURE_TYPE, GEOM_POINT);
        write_bytes(
            &mut feature,
            FEATURE_GEOMETRY,
            &packed(&[
                command(MOVE_TO, 1) as u64,
                zigzag(25) as u64,
                zigzag(17) as u64,
            ]),
        );
        write_bytes(&mut layer, LAYER_FEATURES, &feature);

        for key in ["name", "population", "capital"] {
            write_bytes(&mut layer, LAYER_KEYS, key.as_bytes());
        }

        let mut text = Vec::new();
        write_bytes(&mut text, VALUE_STRING, b"London");
        write_bytes(&mut layer, LAYER_VALUES, &text);

        let mut number = Vec::new();
        write_varint_field(&mut number, VALUE_INT, 8_982_000);
        write_bytes(&mut layer, LAYER_VALUES, &number);

        let mut flag = Vec::new();
        write_varint_field(&mut flag, VALUE_BOOL, 1);
        write_bytes(&mut layer, LAYER_VALUES, &flag);

        write_varint_field(&mut layer, LAYER_EXTENT, u64::from(DEFAULT_EXTENT));

        let mut tile = Vec::new();
        write_bytes(&mut tile, TILE_LAYERS, &layer);
        tile
    }

    fn write_bytes(buffer: &mut Vec<u8>, field: u32, value: &[u8]) {
        write_varint(buffer, u64::from(field) << 3 | u64::from(WIRE_BYTES));
        write_varint(buffer, value.len() as u64);
        buffer.extend_from_slice(value);
    }

    fn write_varint_field(buffer: &mut Vec<u8>, field: u32, value: u64) {
        write_varint(buffer, u64::from(field) << 3 | u64::from(WIRE_VARINT));
        write_varint(buffer, value);
    }

    fn packed(values: &[u64]) -> Vec<u8> {
        let mut out = Vec::new();
        for value in values {
            write_varint(&mut out, *value);
        }
        out
    }

    fn write_varint(buffer: &mut Vec<u8>, mut value: u64) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                buffer.push(byte);
                return;
            }
            buffer.push(byte | 0x80);
        }
    }
}
