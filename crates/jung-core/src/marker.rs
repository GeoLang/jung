use crate::renderer::PixelBuffer;
use jung_style::IconAnchor;

/// A sprite icon image (RGBA pixel data).
#[derive(Debug, Clone)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA row-major
}

impl Icon {
    /// Create a new icon from raw RGBA data.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() != (width * height * 4) as usize {
            return None;
        }
        Some(Self {
            width,
            height,
            data,
        })
    }

    /// Create a simple circle marker icon.
    pub fn circle(radius: u32, r: u8, g: u8, b: u8, a: u8) -> Self {
        let size = radius * 2 + 1;
        let mut data = vec![0u8; (size * size * 4) as usize];
        let center = radius as i32;
        let r2 = (radius * radius) as i32;

        for py in 0..size {
            for px in 0..size {
                let dx = px as i32 - center;
                let dy = py as i32 - center;
                if dx * dx + dy * dy <= r2 {
                    let idx = ((py * size + px) * 4) as usize;
                    data[idx] = r;
                    data[idx + 1] = g;
                    data[idx + 2] = b;
                    data[idx + 3] = a;
                }
            }
        }
        Self {
            width: size,
            height: size,
            data,
        }
    }

    /// Create a square marker icon.
    pub fn square(size: u32, r: u8, g: u8, b: u8, a: u8) -> Self {
        let mut data = vec![0u8; (size * size * 4) as usize];
        for i in 0..(size * size) as usize {
            data[i * 4] = r;
            data[i * 4 + 1] = g;
            data[i * 4 + 2] = b;
            data[i * 4 + 3] = a;
        }
        Self {
            width: size,
            height: size,
            data,
        }
    }

    /// Create a diamond marker icon.
    pub fn diamond(radius: u32, r: u8, g: u8, b: u8, a: u8) -> Self {
        let size = radius * 2 + 1;
        let mut data = vec![0u8; (size * size * 4) as usize];
        let center = radius as i32;

        for py in 0..size {
            for px in 0..size {
                let dx = (px as i32 - center).unsigned_abs();
                let dy = (py as i32 - center).unsigned_abs();
                if dx + dy <= radius {
                    let idx = ((py * size + px) * 4) as usize;
                    data[idx] = r;
                    data[idx + 1] = g;
                    data[idx + 2] = b;
                    data[idx + 3] = a;
                }
            }
        }
        Self {
            width: size,
            height: size,
            data,
        }
    }

    /// Create a star marker icon.
    pub fn star(outer_radius: u32, r: u8, g: u8, b: u8, a: u8) -> Self {
        let size = outer_radius * 2 + 1;
        let mut data = vec![0u8; (size * size * 4) as usize];
        let center = outer_radius as f64;
        let inner_radius = outer_radius as f64 * 0.4;
        let outer_r = outer_radius as f64;

        // 5-pointed star
        let spikes = 5;
        let points: Vec<(f64, f64)> = (0..spikes * 2)
            .map(|i| {
                let angle =
                    std::f64::consts::PI * (i as f64) / spikes as f64 - std::f64::consts::FRAC_PI_2;
                let rad = if i % 2 == 0 { outer_r } else { inner_radius };
                (center + angle.cos() * rad, center + angle.sin() * rad)
            })
            .collect();

        for py in 0..size {
            for px in 0..size {
                if point_in_polygon_f64(px as f64, py as f64, &points) {
                    let idx = ((py * size + px) * 4) as usize;
                    data[idx] = r;
                    data[idx + 1] = g;
                    data[idx + 2] = b;
                    data[idx + 3] = a;
                }
            }
        }
        Self {
            width: size,
            height: size,
            data,
        }
    }

    /// Create a triangle marker icon.
    pub fn triangle(radius: u32, r: u8, g: u8, b: u8, a: u8) -> Self {
        let size = radius * 2 + 1;
        let mut data = vec![0u8; (size * size * 4) as usize];
        let center = radius as f64;

        // Equilateral triangle inscribed in circle
        let points: Vec<(f64, f64)> = (0..3)
            .map(|i| {
                let angle =
                    std::f64::consts::PI * 2.0 * (i as f64) / 3.0 - std::f64::consts::FRAC_PI_2;
                (center + angle.cos() * center, center + angle.sin() * center)
            })
            .collect();

        for py in 0..size {
            for px in 0..size {
                if point_in_polygon_f64(px as f64, py as f64, &points) {
                    let idx = ((py * size + px) * 4) as usize;
                    data[idx] = r;
                    data[idx + 1] = g;
                    data[idx + 2] = b;
                    data[idx + 3] = a;
                }
            }
        }
        Self {
            width: size,
            height: size,
            data,
        }
    }
}

/// How an icon sits on a point: which part of the image is the anchor, an extra pixel
/// offset applied after the anchor, clockwise rotation in degrees, and a scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconPlacement {
    pub anchor: IconAnchor,
    pub offset: [f64; 2],
    pub rotation_deg: f64,
    pub scale: f64,
}

impl Default for IconPlacement {
    fn default() -> Self {
        Self {
            anchor: IconAnchor::Center,
            offset: [0.0, 0.0],
            rotation_deg: 0.0,
            scale: 1.0,
        }
    }
}

/// The unrotated destination rectangle of an icon, in buffer pixels.
struct DestRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

/// Blit (alpha-composite) an icon onto a pixel buffer at the given center position.
pub fn blit_icon(buffer: &mut PixelBuffer, icon: &Icon, center_x: f64, center_y: f64, scale: f64) {
    blit_icon_placed(
        buffer,
        icon,
        center_x,
        center_y,
        &IconPlacement {
            scale,
            ..Default::default()
        },
    );
}

/// Blit (alpha-composite) an icon onto a pixel buffer with an anchor, offset and rotation.
pub fn blit_icon_placed(
    buffer: &mut PixelBuffer,
    icon: &Icon,
    x: f64,
    y: f64,
    placement: &IconPlacement,
) {
    let scale = placement.scale;
    let scaled_w = (icon.width as f64 * scale) as i32;
    let scaled_h = (icon.height as f64 * scale) as i32;

    if scaled_w <= 0 || scaled_h <= 0 {
        return;
    }

    let anchor_x = x + placement.offset[0];
    let anchor_y = y + placement.offset[1];
    let (dx, dy) = anchor_origin(placement.anchor, scaled_w, scaled_h);
    let rect = DestRect {
        x: anchor_x as i32 + dx,
        y: anchor_y as i32 + dy,
        w: scaled_w,
        h: scaled_h,
    };

    if placement.rotation_deg == 0.0 {
        blit_axis_aligned(buffer, icon, &rect, scale);
    } else {
        blit_rotated(
            buffer,
            icon,
            &rect,
            scale,
            (anchor_x, anchor_y),
            placement.rotation_deg,
        );
    }
}

/// Top-left corner of the drawn image relative to the anchor point.
fn anchor_origin(anchor: IconAnchor, w: i32, h: i32) -> (i32, i32) {
    match anchor {
        IconAnchor::Center => (-w / 2, -h / 2),
        IconAnchor::Left => (0, -h / 2),
        IconAnchor::Right => (-w, -h / 2),
        IconAnchor::Top => (-w / 2, 0),
        IconAnchor::Bottom => (-w / 2, -h),
        IconAnchor::TopLeft => (0, 0),
        IconAnchor::TopRight => (-w, 0),
        IconAnchor::BottomLeft => (0, -h),
        IconAnchor::BottomRight => (-w, -h),
    }
}

fn blit_axis_aligned(buffer: &mut PixelBuffer, icon: &Icon, rect: &DestRect, scale: f64) {
    for dy in 0..rect.h {
        for dx in 0..rect.w {
            let dest_x = rect.x + dx;
            let dest_y = rect.y + dy;

            if dest_x < 0
                || dest_y < 0
                || dest_x >= buffer.width as i32
                || dest_y >= buffer.height as i32
            {
                continue;
            }

            // Sample source pixel (nearest-neighbor for scale != 1.0)
            let src_x = ((dx as f64 / scale) as u32).min(icon.width - 1);
            let src_y = ((dy as f64 / scale) as u32).min(icon.height - 1);
            composite_pixel(buffer, dest_x as u32, dest_y as u32, icon, src_x, src_y);
        }
    }
}

fn blit_rotated(
    buffer: &mut PixelBuffer,
    icon: &Icon,
    rect: &DestRect,
    scale: f64,
    pivot: (f64, f64),
    rotation_deg: f64,
) {
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let (x0, y0) = (rect.x as f64, rect.y as f64);
    let (x1, y1) = (x0 + rect.w as f64, y0 + rect.h as f64);

    // scan the rotated bounds so the image is never clipped at its unrotated extent
    let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(cx, cy)| {
        let (rx, ry) = (cx - pivot.0, cy - pivot.1);
        (pivot.0 + rx * cos - ry * sin, pivot.1 + rx * sin + ry * cos)
    });
    let min_x = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|c| c.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|c| c.1)
        .fold(f64::NEG_INFINITY, f64::max);

    let scan_x0 = min_x.floor().max(0.0) as i32;
    let scan_y0 = min_y.floor().max(0.0) as i32;
    let scan_x1 = max_x.ceil().min(buffer.width as f64) as i32;
    let scan_y1 = max_y.ceil().min(buffer.height as f64) as i32;

    for dest_y in scan_y0..scan_y1 {
        for dest_x in scan_x0..scan_x1 {
            // inverse-rotate the destination pixel centre back into unrotated image space
            let rx = dest_x as f64 + 0.5 - pivot.0;
            let ry = dest_y as f64 + 0.5 - pivot.1;
            let lx = pivot.0 + rx * cos + ry * sin - x0;
            let ly = pivot.1 - rx * sin + ry * cos - y0;

            if lx < 0.0 || ly < 0.0 || lx >= rect.w as f64 || ly >= rect.h as f64 {
                continue;
            }

            let src_x = ((lx / scale) as u32).min(icon.width - 1);
            let src_y = ((ly / scale) as u32).min(icon.height - 1);
            composite_pixel(buffer, dest_x as u32, dest_y as u32, icon, src_x, src_y);
        }
    }
}

/// Alpha-composite one icon pixel (src over dst) into the buffer.
fn composite_pixel(
    buffer: &mut PixelBuffer,
    dest_x: u32,
    dest_y: u32,
    icon: &Icon,
    src_x: u32,
    src_y: u32,
) {
    let src_idx = ((src_y * icon.width + src_x) * 4) as usize;

    let sa = icon.data[src_idx + 3] as u32;
    if sa == 0 {
        return;
    }

    let sr = icon.data[src_idx] as u32;
    let sg = icon.data[src_idx + 1] as u32;
    let sb = icon.data[src_idx + 2] as u32;

    let dest_idx = ((dest_y * buffer.width + dest_x) * 4) as usize;
    let da = buffer.data[dest_idx + 3] as u32;
    let dr = buffer.data[dest_idx] as u32;
    let dg = buffer.data[dest_idx + 1] as u32;
    let db = buffer.data[dest_idx + 2] as u32;

    // Alpha compositing (src over dst)
    let out_a = sa + da * (255 - sa) / 255;
    if out_a == 0 {
        return;
    }
    let out_r = (sr * sa + dr * da * (255 - sa) / 255) / out_a;
    let out_g = (sg * sa + dg * da * (255 - sa) / 255) / out_a;
    let out_b = (sb * sa + db * da * (255 - sa) / 255) / out_a;

    buffer.data[dest_idx] = out_r.min(255) as u8;
    buffer.data[dest_idx + 1] = out_g.min(255) as u8;
    buffer.data[dest_idx + 2] = out_b.min(255) as u8;
    buffer.data[dest_idx + 3] = out_a.min(255) as u8;
}

/// Point-in-polygon test for floating-point coordinates (ray-casting).
fn point_in_polygon_f64(x: f64, y: f64, vertices: &[(f64, f64)]) -> bool {
    let n = vertices.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = vertices[i];
        let (xj, yj) = vertices[j];
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// A sprite atlas: a named collection of icons.
#[derive(Debug, Clone, Default)]
pub struct SpriteAtlas {
    icons: std::collections::HashMap<String, Icon>,
}

impl SpriteAtlas {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, icon: Icon) {
        self.icons.insert(name.into(), icon);
    }

    pub fn get(&self, name: &str) -> Option<&Icon> {
        self.icons.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_icon_center_pixel() {
        let icon = Icon::circle(5, 255, 0, 0, 255);
        assert_eq!(icon.width, 11);
        assert_eq!(icon.height, 11);
        // Center pixel should be red
        let center = ((5 * 11 + 5) * 4) as usize;
        assert_eq!(icon.data[center], 255);
        assert_eq!(icon.data[center + 3], 255);
    }

    #[test]
    fn blit_icon_center() {
        let mut buffer = PixelBuffer::new(64, 64);
        let icon = Icon::square(4, 0, 255, 0, 255);
        blit_icon(&mut buffer, &icon, 32.0, 32.0, 1.0);
        // Center pixel should be green
        let idx = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(buffer.data[idx + 1], 255); // G
        assert_eq!(buffer.data[idx + 3], 255); // A
    }

    #[test]
    fn blit_scaled() {
        let mut buffer = PixelBuffer::new(64, 64);
        let icon = Icon::square(4, 255, 0, 0, 255);
        blit_icon(&mut buffer, &icon, 32.0, 32.0, 2.0);
        // Scaled 2x, so 8x8 pixels should be drawn
        let count = buffer.data.chunks(4).filter(|px| px[3] > 0).count();
        assert_eq!(count, 64); // 8x8
    }

    #[test]
    fn sprite_atlas() {
        let mut atlas = SpriteAtlas::new();
        atlas.insert("pin", Icon::circle(3, 255, 0, 0, 255));
        atlas.insert("marker", Icon::diamond(4, 0, 0, 255, 255));
        assert!(atlas.get("pin").is_some());
        assert!(atlas.get("marker").is_some());
        assert!(atlas.get("missing").is_none());
    }

    #[test]
    fn diamond_icon() {
        let icon = Icon::diamond(4, 0, 0, 255, 255);
        assert_eq!(icon.width, 9);
        // Center pixel should be filled
        let center = ((4 * 9 + 4) * 4) as usize;
        assert_eq!(icon.data[center + 2], 255); // B
        assert_eq!(icon.data[center + 3], 255); // A
    }

    #[test]
    fn star_icon() {
        let icon = Icon::star(8, 255, 255, 0, 255);
        assert_eq!(icon.width, 17);
        // Center should be filled
        let center = ((8 * 17 + 8) * 4) as usize;
        assert_eq!(icon.data[center + 3], 255);
    }

    /// 4x4 icon with an opaque L in the top-left corner, so a wrong rotation
    /// direction cannot pass.
    fn asymmetric_icon() -> Icon {
        let mut data = vec![0u8; 4 * 4 * 4];
        for (px, py) in [(0, 0), (1, 0), (0, 1)] {
            let idx = ((py * 4 + px) * 4) as usize;
            data[idx] = 255;
            data[idx + 3] = 255;
        }
        Icon::new(4, 4, data).unwrap()
    }

    /// Opaque pixel coordinates in row-major order.
    fn opaque_pixels(buffer: &PixelBuffer) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for y in 0..buffer.height {
            for x in 0..buffer.width {
                let idx = ((y * buffer.width + x) * 4) as usize;
                if buffer.data[idx + 3] > 0 {
                    out.push((x, y));
                }
            }
        }
        out
    }

    #[test]
    fn placed_center_matches_blit_icon() {
        let icon = Icon::star(6, 255, 0, 0, 255);
        let mut old = PixelBuffer::new(64, 64);
        let mut new = PixelBuffer::new(64, 64);
        blit_icon(&mut old, &icon, 17.3, 40.8, 2.0);
        blit_icon_placed(
            &mut new,
            &icon,
            17.3,
            40.8,
            &IconPlacement {
                scale: 2.0,
                ..Default::default()
            },
        );
        assert_eq!(old.data, new.data);
    }

    #[test]
    fn anchor_top_left_puts_corner_on_point() {
        let mut buffer = PixelBuffer::new(32, 32);
        let icon = Icon::square(4, 0, 255, 0, 255);
        blit_icon_placed(
            &mut buffer,
            &icon,
            10.0,
            10.0,
            &IconPlacement {
                anchor: IconAnchor::TopLeft,
                ..Default::default()
            },
        );
        let pixels = opaque_pixels(&buffer);
        assert_eq!(pixels.first(), Some(&(10, 10)));
        assert_eq!(pixels.last(), Some(&(13, 13)));
        assert_eq!(pixels.len(), 16);
    }

    #[test]
    fn anchor_bottom_puts_image_above_point() {
        let mut buffer = PixelBuffer::new(32, 32);
        let icon = Icon::square(4, 0, 255, 0, 255);
        blit_icon_placed(
            &mut buffer,
            &icon,
            10.0,
            10.0,
            &IconPlacement {
                anchor: IconAnchor::Bottom,
                ..Default::default()
            },
        );
        let pixels = opaque_pixels(&buffer);
        assert_eq!(pixels.first(), Some(&(8, 6)));
        assert_eq!(pixels.last(), Some(&(11, 9)));
    }

    #[test]
    fn offset_shifts_placement() {
        let mut buffer = PixelBuffer::new(32, 32);
        let icon = Icon::square(4, 0, 255, 0, 255);
        blit_icon_placed(
            &mut buffer,
            &icon,
            10.0,
            10.0,
            &IconPlacement {
                anchor: IconAnchor::TopLeft,
                offset: [3.0, -2.0],
                ..Default::default()
            },
        );
        let pixels = opaque_pixels(&buffer);
        assert_eq!(pixels.first(), Some(&(13, 8)));
        assert_eq!(pixels.last(), Some(&(16, 11)));
    }

    #[test]
    fn zero_rotation_matches_unrotated() {
        let icon = asymmetric_icon();
        let mut plain = PixelBuffer::new(32, 32);
        let mut rotated = PixelBuffer::new(32, 32);
        blit_icon(&mut plain, &icon, 10.0, 10.0, 1.0);
        blit_icon_placed(
            &mut rotated,
            &icon,
            10.0,
            10.0,
            &IconPlacement {
                rotation_deg: 0.0,
                ..Default::default()
            },
        );
        assert_eq!(plain.data, rotated.data);
        assert_eq!(opaque_pixels(&plain), vec![(8, 8), (9, 8), (8, 9)]);
    }

    #[test]
    fn rotate_90_clockwise() {
        let mut buffer = PixelBuffer::new(32, 32);
        blit_icon_placed(
            &mut buffer,
            &asymmetric_icon(),
            10.0,
            10.0,
            &IconPlacement {
                rotation_deg: 90.0,
                ..Default::default()
            },
        );
        // the L in the top-left corner lands in the top-right corner
        assert_eq!(opaque_pixels(&buffer), vec![(10, 8), (11, 8), (11, 9)]);
    }

    #[test]
    fn rotate_180() {
        let mut buffer = PixelBuffer::new(32, 32);
        blit_icon_placed(
            &mut buffer,
            &asymmetric_icon(),
            10.0,
            10.0,
            &IconPlacement {
                rotation_deg: 180.0,
                ..Default::default()
            },
        );
        assert_eq!(opaque_pixels(&buffer), vec![(11, 10), (10, 11), (11, 11)]);
    }

    #[test]
    fn rotation_near_edges_does_not_panic() {
        let icon = Icon::square(5, 255, 0, 0, 255);
        for (x, y) in [
            (0.0, 0.0),
            (-3.0, -3.0),
            (63.0, 63.0),
            (70.0, 10.0),
            (10.0, -70.0),
        ] {
            let mut buffer = PixelBuffer::new(16, 16);
            for deg in [37.0, 90.0, 145.0, -60.0, 359.5] {
                blit_icon_placed(
                    &mut buffer,
                    &icon,
                    x,
                    y,
                    &IconPlacement {
                        anchor: IconAnchor::BottomRight,
                        offset: [2.0, -2.0],
                        rotation_deg: deg,
                        scale: 1.5,
                    },
                );
            }
        }
    }

    #[test]
    fn triangle_icon() {
        let icon = Icon::triangle(8, 0, 255, 0, 255);
        assert_eq!(icon.width, 17);
        // Center-ish area should be filled
        let center = ((9 * 17 + 8) * 4) as usize;
        assert_eq!(icon.data[center + 3], 255);
    }
}
