use crate::{
    CLOSE_PATH, FEATURE_GEOMETRY, FEATURE_ID, FEATURE_TAGS, FEATURE_TYPE, GEOM_LINESTRING,
    GEOM_POINT, GEOM_POLYGON, LAYER_EXTENT, LAYER_FEATURES, LAYER_KEYS, LAYER_NAME, LAYER_VALUES,
    LAYER_VERSION, LINE_TO, MOVE_TO, TILE_LAYERS, VALUE_BOOL, VALUE_DOUBLE, VALUE_FLOAT,
    VALUE_SINT, VALUE_STRING, VALUE_UINT, WIRE_BYTES, WIRE_FIXED32, WIRE_FIXED64, WIRE_VARINT,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

pub const MVT_MEDIA_TYPE: &str = "application/vnd.mapbox-vector-tile";

#[derive(Debug, Clone)]
pub struct MvtFeature {
    pub id: Option<u64>,
    pub geometry: MvtGeometry,
    pub properties: BTreeMap<String, MvtValue>,
}

#[derive(Debug, Clone)]
pub enum MvtGeometry {
    Point(Vec<[f64; 2]>),
    LineString(Vec<Vec<[f64; 2]>>),
    Polygon(Vec<MvtPolygon>),
}

#[derive(Debug, Clone)]
pub struct MvtPolygon {
    pub exterior: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MvtValue {
    String(String),
    Float(f32),
    Double(f64),
    Int(i64),
    UInt(u64),
    Bool(bool),
}

#[derive(Debug, Clone)]
pub struct MvtLayer {
    pub name: String,
    pub extent: u32,
    pub features: Vec<MvtFeature>,
}

pub fn encode_tile(layers: &[MvtLayer], bbox: [f64; 4]) -> Vec<u8> {
    let mut buf = Vec::new();
    for layer in layers {
        let layer_bytes = encode_layer(layer, bbox);
        write_tag(&mut buf, TILE_LAYERS, WIRE_BYTES);
        write_varint(&mut buf, layer_bytes.len() as u64);
        buf.extend_from_slice(&layer_bytes);
    }
    buf
}

fn encode_layer(layer: &MvtLayer, bbox: [f64; 4]) -> Vec<u8> {
    let mut buf = Vec::new();

    write_tag(&mut buf, LAYER_VERSION, WIRE_VARINT);
    write_varint(&mut buf, 2);

    write_tag(&mut buf, LAYER_NAME, WIRE_BYTES);
    write_varint(&mut buf, layer.name.len() as u64);
    buf.extend_from_slice(layer.name.as_bytes());

    let mut keys: Vec<String> = Vec::new();
    let mut key_index: HashMap<String, u32> = HashMap::new();
    let mut values: Vec<MvtValue> = Vec::new();
    let mut value_index: HashMap<u64, u32> = HashMap::new();

    for feature in &layer.features {
        for (k, v) in &feature.properties {
            if let std::collections::hash_map::Entry::Vacant(e) = key_index.entry(k.clone()) {
                e.insert(keys.len() as u32);
                keys.push(k.clone());
            }
            let vh = value_hash(v);
            if let std::collections::hash_map::Entry::Vacant(e) = value_index.entry(vh) {
                e.insert(values.len() as u32);
                values.push(v.clone());
            }
        }
    }

    for feature in &layer.features {
        let feature_bytes = encode_feature(feature, layer.extent, bbox, &key_index, &value_index);
        write_tag(&mut buf, LAYER_FEATURES, WIRE_BYTES);
        write_varint(&mut buf, feature_bytes.len() as u64);
        buf.extend_from_slice(&feature_bytes);
    }

    for key in &keys {
        write_tag(&mut buf, LAYER_KEYS, WIRE_BYTES);
        write_varint(&mut buf, key.len() as u64);
        buf.extend_from_slice(key.as_bytes());
    }

    for value in &values {
        let val_bytes = encode_value(value);
        write_tag(&mut buf, LAYER_VALUES, WIRE_BYTES);
        write_varint(&mut buf, val_bytes.len() as u64);
        buf.extend_from_slice(&val_bytes);
    }

    write_tag(&mut buf, LAYER_EXTENT, WIRE_VARINT);
    write_varint(&mut buf, u64::from(layer.extent));

    buf
}

fn encode_feature(
    feature: &MvtFeature,
    extent: u32,
    bbox: [f64; 4],
    key_index: &HashMap<String, u32>,
    value_index: &HashMap<u64, u32>,
) -> Vec<u8> {
    let mut buf = Vec::new();

    if let Some(id) = feature.id {
        write_tag(&mut buf, FEATURE_ID, WIRE_VARINT);
        write_varint(&mut buf, id);
    }

    // tags are packed key index, value index pairs
    let mut tags = Vec::new();
    for (k, v) in &feature.properties {
        if let Some(&ki) = key_index.get(k) {
            let vh = value_hash(v);
            if let Some(&vi) = value_index.get(&vh) {
                write_varint(&mut tags, u64::from(ki));
                write_varint(&mut tags, u64::from(vi));
            }
        }
    }
    if !tags.is_empty() {
        write_tag(&mut buf, FEATURE_TAGS, WIRE_BYTES);
        write_varint(&mut buf, tags.len() as u64);
        buf.extend_from_slice(&tags);
    }

    let geom_type = match &feature.geometry {
        MvtGeometry::Point(_) => GEOM_POINT,
        MvtGeometry::LineString(_) => GEOM_LINESTRING,
        MvtGeometry::Polygon(_) => GEOM_POLYGON,
    };
    write_tag(&mut buf, FEATURE_TYPE, WIRE_VARINT);
    write_varint(&mut buf, geom_type);

    let commands = encode_geometry(&feature.geometry, extent, bbox);
    let mut geom_bytes = Vec::new();
    for cmd in &commands {
        write_varint(&mut geom_bytes, u64::from(*cmd));
    }
    write_tag(&mut buf, FEATURE_GEOMETRY, WIRE_BYTES);
    write_varint(&mut buf, geom_bytes.len() as u64);
    buf.extend_from_slice(&geom_bytes);

    buf
}

fn encode_geometry(geom: &MvtGeometry, extent: u32, bbox: [f64; 4]) -> Vec<u32> {
    let ext = f64::from(extent);
    let bw = bbox[2] - bbox[0];
    let bh = bbox[3] - bbox[1];
    let tile_points = |points: &[[f64; 2]]| -> Vec<[i32; 2]> {
        points
            .iter()
            .map(|point| {
                [
                    ((point[0] - bbox[0]) / bw * ext) as i32,
                    ((bbox[3] - point[1]) / bh * ext) as i32,
                ]
            })
            .collect()
    };

    let mut commands = Vec::new();
    let mut cursor = [0i32; 2];
    match geom {
        MvtGeometry::Point(points) => {
            if points.is_empty() {
                return commands;
            }
            commands.push(command_integer(MOVE_TO, points.len() as u32));
            push_deltas(&mut commands, &mut cursor, &tile_points(points));
        }
        MvtGeometry::LineString(lines) => {
            for line in lines.iter().filter(|line| line.len() >= 2) {
                push_path(&mut commands, &mut cursor, &tile_points(line));
            }
        }
        MvtGeometry::Polygon(polygons) => {
            for polygon in polygons {
                let Some(exterior) = tile_ring(tile_points(&polygon.exterior), Ordering::Greater)
                else {
                    continue;
                };
                let holes = polygon
                    .holes
                    .iter()
                    .filter_map(|hole| tile_ring(tile_points(hole), Ordering::Less));
                for ring in std::iter::once(exterior).chain(holes) {
                    push_path(&mut commands, &mut cursor, &ring);
                    commands.push(command_integer(CLOSE_PATH, 1));
                }
            }
        }
    }
    commands
}

fn tile_ring(mut ring: Vec<[i32; 2]>, area_sign: Ordering) -> Option<Vec<[i32; 2]>> {
    ring.dedup();
    if ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    if ring.len() < 3 {
        return None;
    }
    let area = doubled_area(&ring);
    if area == 0.0 {
        return None;
    }
    if area.partial_cmp(&0.0) != Some(area_sign) {
        ring[1..].reverse();
    }
    Some(ring)
}

fn doubled_area(ring: &[[i32; 2]]) -> f64 {
    let mut sum = 0.0;
    for (i, a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        sum += f64::from(a[0]) * f64::from(b[1]) - f64::from(b[0]) * f64::from(a[1]);
    }
    sum
}

fn push_path(commands: &mut Vec<u32>, cursor: &mut [i32; 2], path: &[[i32; 2]]) {
    commands.push(command_integer(MOVE_TO, 1));
    push_deltas(commands, cursor, &path[..1]);
    commands.push(command_integer(LINE_TO, (path.len() - 1) as u32));
    push_deltas(commands, cursor, &path[1..]);
}

fn push_deltas(commands: &mut Vec<u32>, cursor: &mut [i32; 2], points: &[[i32; 2]]) {
    for point in points {
        commands.push(zigzag_encode(point[0] - cursor[0]));
        commands.push(zigzag_encode(point[1] - cursor[1]));
        *cursor = *point;
    }
}

fn encode_value(value: &MvtValue) -> Vec<u8> {
    let mut buf = Vec::new();
    match value {
        MvtValue::String(s) => {
            write_tag(&mut buf, VALUE_STRING, WIRE_BYTES);
            write_varint(&mut buf, s.len() as u64);
            buf.extend_from_slice(s.as_bytes());
        }
        MvtValue::Float(f) => {
            write_tag(&mut buf, VALUE_FLOAT, WIRE_FIXED32);
            buf.extend_from_slice(&f.to_le_bytes());
        }
        MvtValue::Double(d) => {
            write_tag(&mut buf, VALUE_DOUBLE, WIRE_FIXED64);
            buf.extend_from_slice(&d.to_le_bytes());
        }
        MvtValue::Int(i) => {
            write_tag(&mut buf, VALUE_SINT, WIRE_VARINT);
            write_varint(&mut buf, zigzag_encode_64(*i));
        }
        MvtValue::UInt(u) => {
            write_tag(&mut buf, VALUE_UINT, WIRE_VARINT);
            write_varint(&mut buf, *u);
        }
        MvtValue::Bool(b) => {
            write_tag(&mut buf, VALUE_BOOL, WIRE_VARINT);
            write_varint(&mut buf, u64::from(*b));
        }
    }
    buf
}

fn value_hash(v: &MvtValue) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::mem::discriminant(v).hash(&mut h);
    match v {
        MvtValue::String(s) => s.hash(&mut h),
        MvtValue::Float(f) => f.to_bits().hash(&mut h),
        MvtValue::Double(d) => d.to_bits().hash(&mut h),
        MvtValue::Int(i) => i.hash(&mut h),
        MvtValue::UInt(u) => u.hash(&mut h),
        MvtValue::Bool(b) => b.hash(&mut h),
    }
    h.finish()
}

fn write_varint(buf: &mut Vec<u8>, mut value: u64) {
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

fn write_tag(buf: &mut Vec<u8>, field: u32, wire_type: u8) {
    write_varint(buf, u64::from(field) << 3 | u64::from(wire_type));
}

fn command_integer(id: u32, count: u32) -> u32 {
    (id & 0x7) | (count << 3)
}

fn zigzag_encode(n: i32) -> u32 {
    ((n << 1) ^ (n >> 31)) as u32
}

fn zigzag_encode_64(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty_tile() {
        let tile = encode_tile(&[], [0.0, 0.0, 1.0, 1.0]);
        assert!(tile.is_empty());
    }

    #[test]
    fn zigzag_encode_values() {
        assert_eq!(zigzag_encode(0), 0);
        assert_eq!(zigzag_encode(-1), 1);
        assert_eq!(zigzag_encode(1), 2);
        assert_eq!(zigzag_encode(-2), 3);
        assert_eq!(zigzag_encode(2), 4);
    }

    #[test]
    fn command_integer_encoding() {
        // MoveTo(1) with count 1
        assert_eq!(command_integer(1, 1), 9);
        // LineTo(2) with count 3
        assert_eq!(command_integer(2, 3), 26);
        // ClosePath(7) with count 1
        assert_eq!(command_integer(7, 1), 15);
    }

    #[test]
    fn varint_encoding() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 1);
        assert_eq!(buf, vec![1]);

        buf.clear();
        write_varint(&mut buf, 300);
        assert_eq!(buf, vec![0xAC, 0x02]);
    }
}
