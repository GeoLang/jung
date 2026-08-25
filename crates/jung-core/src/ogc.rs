//! OGC standards support.
//!
//! Implements relevant Open Geospatial Consortium (OGC) standards:
//!
//! - **Simple Features** — geometry model (Point, LineString, Polygon, Multi*)
//! - **Symbology Encoding (SE)** — feature symbolization rules
//! - **Well-Known Text (WKT)** — geometry text serialization
//! - **Well-Known Binary (WKB)** — geometry binary serialization
//! - **Filter Encoding** — feature filtering expressions

use crate::geometry::{Feature, Geometry, Point, PolygonGeom};
use jung_style::PropertyValue;
use std::collections::HashMap;

// =============================================================================
// Well-Known Text (WKT) - OGC 06-103r4
// =============================================================================

/// Parse a WKT geometry string into a Geometry.
pub fn parse_wkt(wkt: &str) -> Result<Geometry, WktError> {
    let wkt = wkt.trim();
    if let Some(rest) = wkt.strip_prefix("POINT") {
        let coords = parse_coord_single(rest.trim())?;
        Ok(Geometry::Point(coords))
    } else if let Some(rest) = wkt.strip_prefix("MULTIPOINT") {
        let points = parse_multi_coords(rest.trim())?;
        let points: Vec<Point> = points.into_iter().map(|v| v[0]).collect();
        Ok(Geometry::MultiPoint(points))
    } else if let Some(rest) = wkt.strip_prefix("LINESTRING") {
        let coords = parse_coord_list(rest.trim())?;
        Ok(Geometry::LineString(coords))
    } else if let Some(rest) = wkt.strip_prefix("MULTILINESTRING") {
        let lines = parse_multi_coords(rest.trim())?;
        Ok(Geometry::MultiLineString(lines))
    } else if let Some(rest) = wkt.strip_prefix("POLYGON") {
        let rings = parse_multi_coords(rest.trim())?;
        if rings.is_empty() {
            return Err(WktError::InvalidGeometry("empty polygon".to_string()));
        }
        let exterior = rings[0].clone();
        let holes = rings[1..].to_vec();
        Ok(Geometry::Polygon { exterior, holes })
    } else if let Some(rest) = wkt.strip_prefix("MULTIPOLYGON") {
        let polys = parse_multipolygon(rest.trim())?;
        Ok(Geometry::MultiPolygon(polys))
    } else {
        Err(WktError::UnsupportedType(
            wkt.split_whitespace().next().unwrap_or("").to_string(),
        ))
    }
}

/// Serialize a Geometry to WKT format.
pub fn to_wkt(geom: &Geometry) -> String {
    match geom {
        Geometry::Point(p) => format!("POINT ({} {})", p.x, p.y),
        Geometry::MultiPoint(pts) => {
            let coords: Vec<String> = pts.iter().map(|p| format!("({} {})", p.x, p.y)).collect();
            format!("MULTIPOINT ({})", coords.join(", "))
        }
        Geometry::LineString(pts) => {
            format!("LINESTRING ({})", coords_to_string(pts))
        }
        Geometry::MultiLineString(lines) => {
            let parts: Vec<String> = lines
                .iter()
                .map(|l| format!("({})", coords_to_string(l)))
                .collect();
            format!("MULTILINESTRING ({})", parts.join(", "))
        }
        Geometry::Polygon { exterior, holes } => {
            let mut rings = vec![format!("({})", coords_to_string(exterior))];
            for hole in holes {
                rings.push(format!("({})", coords_to_string(hole)));
            }
            format!("POLYGON ({})", rings.join(", "))
        }
        Geometry::MultiPolygon(polys) => {
            let parts: Vec<String> = polys
                .iter()
                .map(|p| {
                    let mut rings = vec![format!("({})", coords_to_string(&p.exterior))];
                    for hole in &p.holes {
                        rings.push(format!("({})", coords_to_string(hole)));
                    }
                    format!("({})", rings.join(", "))
                })
                .collect();
            format!("MULTIPOLYGON ({})", parts.join(", "))
        }
    }
}

// =============================================================================
// Well-Known Binary (WKB) - OGC 06-103r4
// =============================================================================

/// Parse WKB bytes (little-endian) into a Geometry.
pub fn parse_wkb(data: &[u8]) -> Result<Geometry, WkbError> {
    let mut reader = WkbReader { data, pos: 0 };
    reader.read_geometry()
}

/// Serialize a Geometry to WKB format (little-endian).
pub fn to_wkb(geom: &Geometry) -> Vec<u8> {
    let mut buf = Vec::new();
    write_wkb(geom, &mut buf);
    buf
}

// =============================================================================
// OGC Filter Encoding (subset) - OGC 09-026r2
// =============================================================================

/// A spatial/attribute filter expression.
#[derive(Debug, Clone)]
pub enum OgcFilter {
    /// Property equals value.
    PropertyIsEqualTo {
        property: String,
        value: PropertyValue,
    },
    /// Property not equal to value.
    PropertyIsNotEqualTo {
        property: String,
        value: PropertyValue,
    },
    /// Property less than value.
    PropertyIsLessThan { property: String, value: f64 },
    /// Property greater than value.
    PropertyIsGreaterThan { property: String, value: f64 },
    /// Property between two values.
    PropertyIsBetween {
        property: String,
        lower: f64,
        upper: f64,
    },
    /// Property matches pattern (SQL LIKE).
    PropertyIsLike { property: String, pattern: String },
    /// Logical AND.
    And(Vec<OgcFilter>),
    /// Logical OR.
    Or(Vec<OgcFilter>),
    /// Logical NOT.
    Not(Box<OgcFilter>),
    /// Spatial: feature intersects bbox.
    BBox {
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    },
}

/// Evaluate an OGC filter against a feature.
pub fn evaluate_filter(filter: &OgcFilter, feature: &Feature) -> bool {
    match filter {
        OgcFilter::PropertyIsEqualTo { property, value } => {
            feature.properties.get(property) == Some(value)
        }
        OgcFilter::PropertyIsNotEqualTo { property, value } => {
            feature.properties.get(property) != Some(value)
        }
        OgcFilter::PropertyIsLessThan { property, value } => {
            get_numeric(&feature.properties, property).is_some_and(|v| v < *value)
        }
        OgcFilter::PropertyIsGreaterThan { property, value } => {
            get_numeric(&feature.properties, property).is_some_and(|v| v > *value)
        }
        OgcFilter::PropertyIsBetween {
            property,
            lower,
            upper,
        } => get_numeric(&feature.properties, property).is_some_and(|v| v >= *lower && v <= *upper),
        OgcFilter::PropertyIsLike { property, pattern } => {
            if let Some(PropertyValue::String(s)) = feature.properties.get(property) {
                like_match(s, pattern)
            } else {
                false
            }
        }
        OgcFilter::And(filters) => filters.iter().all(|f| evaluate_filter(f, feature)),
        OgcFilter::Or(filters) => filters.iter().any(|f| evaluate_filter(f, feature)),
        OgcFilter::Not(f) => !evaluate_filter(f, feature),
        OgcFilter::BBox {
            min_x,
            min_y,
            max_x,
            max_y,
        } => feature_intersects_bbox(feature, *min_x, *min_y, *max_x, *max_y),
    }
}

// =============================================================================
// OGC Simple Features geometry operations
// =============================================================================

/// Compute the bounding box (envelope) of a geometry.
pub fn envelope(geom: &Geometry) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for_each_point(geom, &mut |p| {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    });

    (min_x, min_y, max_x, max_y)
}

/// Compute the area of a polygon (Shoelace formula).
pub fn area(geom: &Geometry) -> f64 {
    match geom {
        Geometry::Polygon { exterior, holes } => {
            let ext_area = ring_area(exterior);
            let hole_area: f64 = holes.iter().map(|h| ring_area(h).abs()).sum();
            ext_area.abs() - hole_area
        }
        Geometry::MultiPolygon(polys) => polys
            .iter()
            .map(|p| {
                let ext_area = ring_area(&p.exterior);
                let hole_area: f64 = p.holes.iter().map(|h| ring_area(h).abs()).sum();
                ext_area.abs() - hole_area
            })
            .sum(),
        _ => 0.0,
    }
}

/// Compute the length of a geometry.
pub fn length(geom: &Geometry) -> f64 {
    match geom {
        Geometry::LineString(pts) => polyline_length(pts),
        Geometry::MultiLineString(lines) => lines.iter().map(|l| polyline_length(l)).sum(),
        Geometry::Polygon { exterior, holes } => {
            let ext = polyline_length(exterior);
            let h: f64 = holes.iter().map(|h| polyline_length(h)).sum();
            ext + h
        }
        _ => 0.0,
    }
}

/// Compute the centroid of a geometry.
pub fn centroid(geom: &Geometry) -> Point {
    match geom {
        Geometry::Point(p) => *p,
        Geometry::MultiPoint(pts) => {
            let n = pts.len() as f64;
            Point {
                x: pts.iter().map(|p| p.x).sum::<f64>() / n,
                y: pts.iter().map(|p| p.y).sum::<f64>() / n,
            }
        }
        Geometry::LineString(pts) => {
            if pts.is_empty() {
                return Point { x: 0.0, y: 0.0 };
            }
            let n = pts.len() as f64;
            Point {
                x: pts.iter().map(|p| p.x).sum::<f64>() / n,
                y: pts.iter().map(|p| p.y).sum::<f64>() / n,
            }
        }
        Geometry::Polygon { exterior, .. } => polygon_centroid(exterior),
        _ => Point { x: 0.0, y: 0.0 },
    }
}

/// Area-weighted centroid of a polygon ring. Holes do not move it, and a ring
/// with no area falls back to the origin.
pub fn polygon_centroid(exterior: &[Point]) -> Point {
    let n = exterior.len();
    if n == 0 {
        return Point { x: 0.0, y: 0.0 };
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut a = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        let cross = exterior[i].x * exterior[j].y - exterior[j].x * exterior[i].y;
        cx += (exterior[i].x + exterior[j].x) * cross;
        cy += (exterior[i].y + exterior[j].y) * cross;
        a += cross;
    }
    a /= 2.0;
    if a.abs() < 1e-10 {
        return Point { x: 0.0, y: 0.0 };
    }
    Point {
        x: cx / (6.0 * a),
        y: cy / (6.0 * a),
    }
}

// =============================================================================
// Internal helpers
// =============================================================================

fn coords_to_string(pts: &[Point]) -> String {
    pts.iter()
        .map(|p| format!("{} {}", p.x, p.y))
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_coord_single(s: &str) -> Result<Point, WktError> {
    let s = s
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(WktError::InvalidCoordinates);
    }
    let x = parts[0]
        .parse::<f64>()
        .map_err(|_| WktError::InvalidCoordinates)?;
    let y = parts[1]
        .parse::<f64>()
        .map_err(|_| WktError::InvalidCoordinates)?;
    Ok(Point { x, y })
}

fn parse_coord_list(s: &str) -> Result<Vec<Point>, WktError> {
    let s = s
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    let mut points = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        let nums: Vec<&str> = part.split_whitespace().collect();
        if nums.len() < 2 {
            return Err(WktError::InvalidCoordinates);
        }
        let x = nums[0]
            .parse::<f64>()
            .map_err(|_| WktError::InvalidCoordinates)?;
        let y = nums[1]
            .parse::<f64>()
            .map_err(|_| WktError::InvalidCoordinates)?;
        points.push(Point { x, y });
    }
    Ok(points)
}

fn parse_multi_coords(s: &str) -> Result<Vec<Vec<Point>>, WktError> {
    let s = s.trim();
    // Strip exactly one outer paren pair
    let s = if s.starts_with('(') && s.ends_with(')') {
        &s[1..s.len() - 1]
    } else {
        s
    };
    let mut result = Vec::new();
    let mut depth = 0;
    let mut current = String::new();

    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                if depth > 1 {
                    current.push(ch);
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let pts = parse_coord_list(&current)?;
                    result.push(pts);
                    current.clear();
                } else {
                    current.push(ch);
                }
            }
            ',' if depth == 0 => {}
            _ => current.push(ch),
        }
    }

    // Handle case without inner parens (e.g., single ring)
    if result.is_empty() && !current.trim().is_empty() {
        result.push(parse_coord_list(&current)?);
    }

    Ok(result)
}

fn parse_multipolygon(s: &str) -> Result<Vec<PolygonGeom>, WktError> {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let mut polys = Vec::new();
    let mut depth = 0;
    let mut current = String::new();

    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
                if depth == 0 {
                    let rings = parse_multi_coords(&current)?;
                    if !rings.is_empty() {
                        polys.push(PolygonGeom {
                            exterior: rings[0].clone(),
                            holes: rings[1..].to_vec(),
                        });
                    }
                    current.clear();
                }
            }
            ',' if depth == 0 => {}
            _ => current.push(ch),
        }
    }

    Ok(polys)
}

fn ring_area(ring: &[Point]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += ring[i].x * ring[j].y;
        area -= ring[j].x * ring[i].y;
    }
    area / 2.0
}

fn polyline_length(pts: &[Point]) -> f64 {
    pts.windows(2)
        .map(|w| {
            let dx = w[1].x - w[0].x;
            let dy = w[1].y - w[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

fn for_each_point(geom: &Geometry, f: &mut impl FnMut(&Point)) {
    match geom {
        Geometry::Point(p) => f(p),
        Geometry::MultiPoint(pts) => pts.iter().for_each(f),
        Geometry::LineString(pts) => pts.iter().for_each(f),
        Geometry::MultiLineString(lines) => lines.iter().for_each(|l| l.iter().for_each(&mut *f)),
        Geometry::Polygon { exterior, holes } => {
            exterior.iter().for_each(&mut *f);
            holes.iter().for_each(|h| h.iter().for_each(&mut *f));
        }
        Geometry::MultiPolygon(polys) => {
            for p in polys {
                p.exterior.iter().for_each(&mut *f);
                p.holes.iter().for_each(|h| h.iter().for_each(&mut *f));
            }
        }
    }
}

fn feature_intersects_bbox(
    feature: &Feature,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> bool {
    let (fmin_x, fmin_y, fmax_x, fmax_y) = envelope(&feature.geometry);
    fmax_x >= min_x && fmin_x <= max_x && fmax_y >= min_y && fmin_y <= max_y
}

fn get_numeric(props: &HashMap<String, PropertyValue>, key: &str) -> Option<f64> {
    match props.get(key) {
        Some(PropertyValue::Number(n)) => Some(*n),
        Some(PropertyValue::Integer(i)) => Some(*i as f64),
        _ => None,
    }
}

fn like_match(s: &str, pattern: &str) -> bool {
    // Simple LIKE: % matches any sequence, _ matches one char
    let regex = pattern.replace('%', ".*").replace('_', ".");
    // Very basic regex matching
    s.len() == s.len() && simple_regex_match(s, &regex)
}

fn simple_regex_match(s: &str, pattern: &str) -> bool {
    // Convert to a minimal regex-like matcher
    let pattern = format!("^{}$", pattern);
    regex_match_impl(s, &pattern)
}

fn regex_match_impl(s: &str, pattern: &str) -> bool {
    // Simple glob-to-match: just use .* and . for % and _
    let pattern = pattern.trim_start_matches('^').trim_end_matches('$');

    // Simplified: split on .* for % matching
    let parts: Vec<&str> = pattern.split(".*").collect();
    if parts.len() == 1 {
        // No wildcard, exact match (with . for single char)
        return char_match(s, pattern);
    }

    let mut remaining = s;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // Must match at start
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else if let Some(pos) = remaining.find(part) {
            remaining = &remaining[pos + part.len()..];
        } else {
            return false;
        }
    }
    true
}

fn char_match(s: &str, pattern: &str) -> bool {
    let s_chars: Vec<char> = s.chars().collect();
    let p_chars: Vec<char> = pattern.chars().collect();
    if s_chars.len() != p_chars.len() {
        return false;
    }
    s_chars
        .iter()
        .zip(p_chars.iter())
        .all(|(sc, pc)| *pc == '.' || *sc == *pc)
}

// =============================================================================
// WKB reader/writer
// =============================================================================

struct WkbReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> WkbReader<'a> {
    fn read_geometry(&mut self) -> Result<Geometry, WkbError> {
        if self.pos >= self.data.len() {
            return Err(WkbError::UnexpectedEof);
        }
        let byte_order = self.data[self.pos];
        self.pos += 1;
        let le = byte_order == 1;

        let geom_type = self.read_u32(le)?;

        match geom_type {
            1 => {
                let x = self.read_f64(le)?;
                let y = self.read_f64(le)?;
                Ok(Geometry::Point(Point { x, y }))
            }
            2 => {
                let n = self.read_u32(le)? as usize;
                let pts = self.read_points(n, le)?;
                Ok(Geometry::LineString(pts))
            }
            3 => {
                let n_rings = self.read_u32(le)? as usize;
                let mut rings = Vec::with_capacity(n_rings);
                for _ in 0..n_rings {
                    let n_pts = self.read_u32(le)? as usize;
                    rings.push(self.read_points(n_pts, le)?);
                }
                if rings.is_empty() {
                    return Err(WkbError::InvalidGeometry);
                }
                let exterior = rings.remove(0);
                Ok(Geometry::Polygon {
                    exterior,
                    holes: rings,
                })
            }
            4 => {
                let n = self.read_u32(le)? as usize;
                let mut points = Vec::with_capacity(n);
                for _ in 0..n {
                    match self.read_geometry()? {
                        Geometry::Point(p) => points.push(p),
                        _ => return Err(WkbError::InvalidGeometry),
                    }
                }
                Ok(Geometry::MultiPoint(points))
            }
            5 => {
                let n = self.read_u32(le)? as usize;
                let mut lines = Vec::with_capacity(n);
                for _ in 0..n {
                    match self.read_geometry()? {
                        Geometry::LineString(l) => lines.push(l),
                        _ => return Err(WkbError::InvalidGeometry),
                    }
                }
                Ok(Geometry::MultiLineString(lines))
            }
            6 => {
                let n = self.read_u32(le)? as usize;
                let mut polys = Vec::with_capacity(n);
                for _ in 0..n {
                    match self.read_geometry()? {
                        Geometry::Polygon { exterior, holes } => {
                            polys.push(PolygonGeom { exterior, holes });
                        }
                        _ => return Err(WkbError::InvalidGeometry),
                    }
                }
                Ok(Geometry::MultiPolygon(polys))
            }
            _ => Err(WkbError::UnsupportedType(geom_type)),
        }
    }

    fn read_points(&mut self, n: usize, le: bool) -> Result<Vec<Point>, WkbError> {
        let mut pts = Vec::with_capacity(n);
        for _ in 0..n {
            let x = self.read_f64(le)?;
            let y = self.read_f64(le)?;
            pts.push(Point { x, y });
        }
        Ok(pts)
    }

    fn read_u32(&mut self, le: bool) -> Result<u32, WkbError> {
        if self.pos + 4 > self.data.len() {
            return Err(WkbError::UnexpectedEof);
        }
        let bytes: [u8; 4] = self.data[self.pos..self.pos + 4].try_into().unwrap();
        self.pos += 4;
        Ok(if le {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    fn read_f64(&mut self, le: bool) -> Result<f64, WkbError> {
        if self.pos + 8 > self.data.len() {
            return Err(WkbError::UnexpectedEof);
        }
        let bytes: [u8; 8] = self.data[self.pos..self.pos + 8].try_into().unwrap();
        self.pos += 8;
        Ok(if le {
            f64::from_le_bytes(bytes)
        } else {
            f64::from_be_bytes(bytes)
        })
    }
}

fn write_wkb(geom: &Geometry, buf: &mut Vec<u8>) {
    buf.push(1); // little-endian
    match geom {
        Geometry::Point(p) => {
            buf.extend_from_slice(&1u32.to_le_bytes());
            buf.extend_from_slice(&p.x.to_le_bytes());
            buf.extend_from_slice(&p.y.to_le_bytes());
        }
        Geometry::LineString(pts) => {
            buf.extend_from_slice(&2u32.to_le_bytes());
            buf.extend_from_slice(&(pts.len() as u32).to_le_bytes());
            for p in pts {
                buf.extend_from_slice(&p.x.to_le_bytes());
                buf.extend_from_slice(&p.y.to_le_bytes());
            }
        }
        Geometry::Polygon { exterior, holes } => {
            buf.extend_from_slice(&3u32.to_le_bytes());
            let n_rings = 1 + holes.len();
            buf.extend_from_slice(&(n_rings as u32).to_le_bytes());
            write_ring(buf, exterior);
            for hole in holes {
                write_ring(buf, hole);
            }
        }
        Geometry::MultiPoint(pts) => {
            buf.extend_from_slice(&4u32.to_le_bytes());
            buf.extend_from_slice(&(pts.len() as u32).to_le_bytes());
            for p in pts {
                write_wkb(&Geometry::Point(*p), buf);
            }
        }
        Geometry::MultiLineString(lines) => {
            buf.extend_from_slice(&5u32.to_le_bytes());
            buf.extend_from_slice(&(lines.len() as u32).to_le_bytes());
            for l in lines {
                write_wkb(&Geometry::LineString(l.clone()), buf);
            }
        }
        Geometry::MultiPolygon(polys) => {
            buf.extend_from_slice(&6u32.to_le_bytes());
            buf.extend_from_slice(&(polys.len() as u32).to_le_bytes());
            for p in polys {
                write_wkb(
                    &Geometry::Polygon {
                        exterior: p.exterior.clone(),
                        holes: p.holes.clone(),
                    },
                    buf,
                );
            }
        }
    }
}

fn write_ring(buf: &mut Vec<u8>, ring: &[Point]) {
    buf.extend_from_slice(&(ring.len() as u32).to_le_bytes());
    for p in ring {
        buf.extend_from_slice(&p.x.to_le_bytes());
        buf.extend_from_slice(&p.y.to_le_bytes());
    }
}

// =============================================================================
// Error types
// =============================================================================

/// WKT parsing error.
#[derive(Debug, thiserror::Error)]
pub enum WktError {
    #[error("unsupported geometry type: {0}")]
    UnsupportedType(String),
    #[error("invalid coordinates")]
    InvalidCoordinates,
    #[error("invalid geometry: {0}")]
    InvalidGeometry(String),
}

/// WKB parsing error.
#[derive(Debug, thiserror::Error)]
pub enum WkbError {
    #[error("unexpected end of data")]
    UnexpectedEof,
    #[error("unsupported geometry type: {0}")]
    UnsupportedType(u32),
    #[error("invalid geometry structure")]
    InvalidGeometry,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wkt_point_roundtrip() {
        let geom = parse_wkt("POINT (1.5 2.5)").unwrap();
        assert_eq!(geom, Geometry::Point(Point { x: 1.5, y: 2.5 }));
        assert_eq!(to_wkt(&geom), "POINT (1.5 2.5)");
    }

    #[test]
    fn wkt_linestring_roundtrip() {
        let geom = parse_wkt("LINESTRING (0 0, 1 1, 2 0)").unwrap();
        match &geom {
            Geometry::LineString(pts) => assert_eq!(pts.len(), 3),
            _ => panic!("expected LineString"),
        }
        let wkt = to_wkt(&geom);
        assert!(wkt.starts_with("LINESTRING"));
    }

    #[test]
    fn wkt_polygon() {
        let geom = parse_wkt("POLYGON ((0 0, 10 0, 10 10, 0 10, 0 0))").unwrap();
        match &geom {
            Geometry::Polygon { exterior, holes } => {
                assert_eq!(exterior.len(), 5);
                assert!(holes.is_empty());
            }
            _ => panic!("expected Polygon"),
        }
    }

    #[test]
    fn wkb_point_roundtrip() {
        let geom = Geometry::Point(Point { x: 1.0, y: 2.0 });
        let wkb = to_wkb(&geom);
        let parsed = parse_wkb(&wkb).unwrap();
        assert_eq!(parsed, geom);
    }

    #[test]
    fn wkb_linestring_roundtrip() {
        let geom = Geometry::LineString(vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 1.0, y: 1.0 },
            Point { x: 2.0, y: 0.0 },
        ]);
        let wkb = to_wkb(&geom);
        let parsed = parse_wkb(&wkb).unwrap();
        assert_eq!(parsed, geom);
    }

    #[test]
    fn wkb_polygon_roundtrip() {
        let geom = Geometry::Polygon {
            exterior: vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 1.0, y: 0.0 },
                Point { x: 1.0, y: 1.0 },
                Point { x: 0.0, y: 1.0 },
                Point { x: 0.0, y: 0.0 },
            ],
            holes: vec![],
        };
        let wkb = to_wkb(&geom);
        let parsed = parse_wkb(&wkb).unwrap();
        assert_eq!(parsed, geom);
    }

    #[test]
    fn envelope_calculation() {
        let geom = Geometry::LineString(vec![
            Point { x: 1.0, y: 2.0 },
            Point { x: 5.0, y: 8.0 },
            Point { x: 3.0, y: 4.0 },
        ]);
        let (min_x, min_y, max_x, max_y) = envelope(&geom);
        assert_eq!(min_x, 1.0);
        assert_eq!(min_y, 2.0);
        assert_eq!(max_x, 5.0);
        assert_eq!(max_y, 8.0);
    }

    #[test]
    fn area_square() {
        let geom = Geometry::Polygon {
            exterior: vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 10.0, y: 0.0 },
                Point { x: 10.0, y: 10.0 },
                Point { x: 0.0, y: 10.0 },
                Point { x: 0.0, y: 0.0 },
            ],
            holes: vec![],
        };
        assert!((area(&geom) - 100.0).abs() < 0.001);
    }

    #[test]
    fn length_linestring() {
        let geom = Geometry::LineString(vec![Point { x: 0.0, y: 0.0 }, Point { x: 3.0, y: 4.0 }]);
        assert!((length(&geom) - 5.0).abs() < 0.001);
    }

    #[test]
    fn centroid_square() {
        let geom = Geometry::Polygon {
            exterior: vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 10.0, y: 0.0 },
                Point { x: 10.0, y: 10.0 },
                Point { x: 0.0, y: 10.0 },
                Point { x: 0.0, y: 0.0 },
            ],
            holes: vec![],
        };
        let c = centroid(&geom);
        assert!((c.x - 5.0).abs() < 0.001);
        assert!((c.y - 5.0).abs() < 0.001);
    }

    #[test]
    fn filter_property_equals() {
        let mut props = HashMap::new();
        props.insert(
            "type".to_string(),
            PropertyValue::String("road".to_string()),
        );
        let feature = Feature {
            geometry: Geometry::Point(Point { x: 0.0, y: 0.0 }),
            properties: props,
        };
        let filter = OgcFilter::PropertyIsEqualTo {
            property: "type".to_string(),
            value: PropertyValue::String("road".to_string()),
        };
        assert!(evaluate_filter(&filter, &feature));
    }

    #[test]
    fn filter_bbox() {
        let feature = Feature {
            geometry: Geometry::Point(Point { x: 5.0, y: 5.0 }),
            properties: HashMap::new(),
        };
        let inside = OgcFilter::BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 10.0,
            max_y: 10.0,
        };
        let outside = OgcFilter::BBox {
            min_x: 20.0,
            min_y: 20.0,
            max_x: 30.0,
            max_y: 30.0,
        };
        assert!(evaluate_filter(&inside, &feature));
        assert!(!evaluate_filter(&outside, &feature));
    }

    #[test]
    fn filter_and_or_not() {
        let mut props = HashMap::new();
        props.insert("pop".to_string(), PropertyValue::Number(5000.0));
        props.insert(
            "name".to_string(),
            PropertyValue::String("City".to_string()),
        );
        let feature = Feature {
            geometry: Geometry::Point(Point { x: 0.0, y: 0.0 }),
            properties: props,
        };

        let and_filter = OgcFilter::And(vec![
            OgcFilter::PropertyIsGreaterThan {
                property: "pop".to_string(),
                value: 1000.0,
            },
            OgcFilter::PropertyIsEqualTo {
                property: "name".to_string(),
                value: PropertyValue::String("City".to_string()),
            },
        ]);
        assert!(evaluate_filter(&and_filter, &feature));

        let not_filter = OgcFilter::Not(Box::new(OgcFilter::PropertyIsLessThan {
            property: "pop".to_string(),
            value: 1000.0,
        }));
        assert!(evaluate_filter(&not_filter, &feature));
    }

    #[test]
    fn filter_between() {
        let mut props = HashMap::new();
        props.insert("temp".to_string(), PropertyValue::Number(25.0));
        let feature = Feature {
            geometry: Geometry::Point(Point { x: 0.0, y: 0.0 }),
            properties: props,
        };
        let filter = OgcFilter::PropertyIsBetween {
            property: "temp".to_string(),
            lower: 20.0,
            upper: 30.0,
        };
        assert!(evaluate_filter(&filter, &feature));
    }
}
