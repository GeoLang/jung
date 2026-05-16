//! Web map tiling — slippy map tile coordinate system and tile rendering.
//!
//! Implements XYZ tile addressing, tile bounds calculation, and tile-based
//! feature clipping for web map rendering.

use crate::geometry::{Feature, Geometry};

/// A tile address in XYZ (slippy map) scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub x: u32,
    pub y: u32,
    pub z: u8,
}

/// Geographic bounding box (EPSG:4326).
#[derive(Debug, Clone, Copy)]
pub struct TileBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl TileCoord {
    pub fn new(x: u32, y: u32, z: u8) -> Self {
        Self { x, y, z }
    }

    /// Get the geographic bounds of this tile (EPSG:4326).
    pub fn bounds(&self) -> TileBounds {
        let n = 2.0_f64.powi(self.z as i32);
        let west = self.x as f64 / n * 360.0 - 180.0;
        let east = (self.x + 1) as f64 / n * 360.0 - 180.0;
        let north = tile_lat(self.y, n);
        let south = tile_lat(self.y + 1, n);

        TileBounds {
            west,
            south,
            east,
            north,
        }
    }

    /// Get the center point of this tile.
    pub fn center(&self) -> (f64, f64) {
        let bounds = self.bounds();
        (
            (bounds.west + bounds.east) / 2.0,
            (bounds.south + bounds.north) / 2.0,
        )
    }

    /// Get the parent tile (one zoom level up).
    pub fn parent(&self) -> Option<Self> {
        if self.z == 0 {
            return None;
        }
        Some(Self {
            x: self.x / 2,
            y: self.y / 2,
            z: self.z - 1,
        })
    }

    /// Get the four child tiles (one zoom level down).
    pub fn children(&self) -> [Self; 4] {
        let x = self.x * 2;
        let y = self.y * 2;
        let z = self.z + 1;
        [
            Self { x, y, z },
            Self { x: x + 1, y, z },
            Self { x, y: y + 1, z },
            Self {
                x: x + 1,
                y: y + 1,
                z,
            },
        ]
    }

    /// Get all tiles needed to cover a bounding box at a given zoom level.
    pub fn tiles_for_bounds(bounds: &TileBounds, zoom: u8) -> Vec<TileCoord> {
        let n = 2.0_f64.powi(zoom as i32);

        let x_min = lon_to_tile_x(bounds.west, n);
        let x_max = lon_to_tile_x(bounds.east, n);
        let y_min = lat_to_tile_y(bounds.north, n);
        let y_max = lat_to_tile_y(bounds.south, n);

        let max_tile = (n as u32).saturating_sub(1);
        let x_min = x_min.min(max_tile);
        let x_max = x_max.min(max_tile);
        let y_min = y_min.min(max_tile);
        let y_max = y_max.min(max_tile);

        let mut tiles = Vec::new();
        for y in y_min..=y_max {
            for x in x_min..=x_max {
                tiles.push(TileCoord { x, y, z: zoom });
            }
        }
        tiles
    }
}

impl TileBounds {
    /// Check if a point is within this bounding box.
    pub fn contains_point(&self, lon: f64, lat: f64) -> bool {
        lon >= self.west && lon <= self.east && lat >= self.south && lat <= self.north
    }

    /// Check if another bounding box intersects this one.
    pub fn intersects(&self, other: &TileBounds) -> bool {
        self.west <= other.east
            && self.east >= other.west
            && self.south <= other.north
            && self.north >= other.south
    }

    /// Width in degrees.
    pub fn width(&self) -> f64 {
        self.east - self.west
    }

    /// Height in degrees.
    pub fn height(&self) -> f64 {
        self.north - self.south
    }
}

/// Clip features to a tile's bounding box.
pub fn clip_features_to_tile(features: &[Feature], tile: &TileCoord) -> Vec<Feature> {
    let bounds = tile.bounds();
    features
        .iter()
        .filter(|f| feature_intersects_bounds(f, &bounds))
        .cloned()
        .collect()
}

fn feature_intersects_bounds(feature: &Feature, bounds: &TileBounds) -> bool {
    match &feature.geometry {
        Geometry::Point(p) => bounds.contains_point(p.x, p.y),
        Geometry::MultiPoint(points) => points.iter().any(|p| bounds.contains_point(p.x, p.y)),
        Geometry::LineString(points) => points.iter().any(|p| bounds.contains_point(p.x, p.y)),
        Geometry::MultiLineString(lines) => lines
            .iter()
            .any(|line| line.iter().any(|p| bounds.contains_point(p.x, p.y))),
        Geometry::Polygon { exterior, .. } => {
            exterior.iter().any(|p| bounds.contains_point(p.x, p.y))
        }
        Geometry::MultiPolygon(polys) => polys.iter().any(|poly| {
            poly.exterior
                .iter()
                .any(|p| bounds.contains_point(p.x, p.y))
        }),
    }
}

// ─── Math helpers ────────────────────────────────────────────────────────────

fn tile_lat(y: u32, n: f64) -> f64 {
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * y as f64 / n))
        .sinh()
        .atan();
    lat_rad.to_degrees()
}

fn lon_to_tile_x(lon: f64, n: f64) -> u32 {
    ((lon + 180.0) / 360.0 * n).floor() as u32
}

fn lat_to_tile_y(lat: f64, n: f64) -> u32 {
    let lat_rad = lat.to_radians();
    ((1.0 - lat_rad.tan().asinh() / std::f64::consts::PI) / 2.0 * n).floor() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use std::collections::HashMap;

    #[test]
    fn test_tile_bounds() {
        let tile = TileCoord::new(0, 0, 1);
        let bounds = tile.bounds();
        assert!((bounds.west - (-180.0)).abs() < 1e-6);
        assert!(bounds.north > 0.0);
    }

    #[test]
    fn test_tile_center() {
        let tile = TileCoord::new(0, 0, 0);
        let (cx, cy) = tile.center();
        assert!((cx - 0.0).abs() < 1e-6);
        assert!(cy.abs() < 1e-6);
    }

    #[test]
    fn test_tile_parent_child() {
        let tile = TileCoord::new(2, 3, 3);
        let parent = tile.parent().unwrap();
        assert_eq!(parent.z, 2);
        assert_eq!(parent.x, 1);
        assert_eq!(parent.y, 1);

        let children = parent.children();
        assert!(children.contains(&tile));
    }

    #[test]
    fn test_tiles_for_bounds() {
        let bounds = TileBounds {
            west: -1.0,
            south: 51.0,
            east: 1.0,
            north: 52.0,
        };
        let tiles = TileCoord::tiles_for_bounds(&bounds, 5);
        assert!(!tiles.is_empty());
        for t in &tiles {
            assert_eq!(t.z, 5);
        }
    }

    #[test]
    fn test_clip_features() {
        let tile = TileCoord::new(0, 0, 1); // Western hemisphere, northern
        let bounds = tile.bounds();

        let inside = Feature {
            geometry: Geometry::Point(Point {
                x: bounds.west + 10.0,
                y: bounds.south + 10.0,
            }),
            properties: HashMap::new(),
        };
        let outside = Feature {
            geometry: Geometry::Point(Point { x: 170.0, y: -50.0 }),
            properties: HashMap::new(),
        };

        let clipped = clip_features_to_tile(&[inside.clone(), outside], &tile);
        assert_eq!(clipped.len(), 1);
    }

    #[test]
    fn test_bounds_intersection() {
        let a = TileBounds {
            west: 0.0,
            south: 0.0,
            east: 10.0,
            north: 10.0,
        };
        let b = TileBounds {
            west: 5.0,
            south: 5.0,
            east: 15.0,
            north: 15.0,
        };
        let c = TileBounds {
            west: 20.0,
            south: 20.0,
            east: 30.0,
            north: 30.0,
        };
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }
}
