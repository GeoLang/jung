use crate::renderer::PixelBuffer;

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

/// Blit (alpha-composite) an icon onto a pixel buffer at the given center position.
pub fn blit_icon(buffer: &mut PixelBuffer, icon: &Icon, center_x: f64, center_y: f64, scale: f64) {
    let scaled_w = (icon.width as f64 * scale) as i32;
    let scaled_h = (icon.height as f64 * scale) as i32;

    if scaled_w <= 0 || scaled_h <= 0 {
        return;
    }

    let start_x = center_x as i32 - scaled_w / 2;
    let start_y = center_y as i32 - scaled_h / 2;

    for dy in 0..scaled_h {
        for dx in 0..scaled_w {
            let dest_x = start_x + dx;
            let dest_y = start_y + dy;

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
            let src_idx = ((src_y * icon.width + src_x) * 4) as usize;

            let sa = icon.data[src_idx + 3] as u32;
            if sa == 0 {
                continue;
            }

            let sr = icon.data[src_idx] as u32;
            let sg = icon.data[src_idx + 1] as u32;
            let sb = icon.data[src_idx + 2] as u32;

            let dest_idx = ((dest_y as u32 * buffer.width + dest_x as u32) * 4) as usize;
            let da = buffer.data[dest_idx + 3] as u32;
            let dr = buffer.data[dest_idx] as u32;
            let dg = buffer.data[dest_idx + 1] as u32;
            let db = buffer.data[dest_idx + 2] as u32;

            // Alpha compositing (src over dst)
            let out_a = sa + da * (255 - sa) / 255;
            if out_a == 0 {
                continue;
            }
            let out_r = (sr * sa + dr * da * (255 - sa) / 255) / out_a;
            let out_g = (sg * sa + dg * da * (255 - sa) / 255) / out_a;
            let out_b = (sb * sa + db * da * (255 - sa) / 255) / out_a;

            buffer.data[dest_idx] = out_r.min(255) as u8;
            buffer.data[dest_idx + 1] = out_g.min(255) as u8;
            buffer.data[dest_idx + 2] = out_b.min(255) as u8;
            buffer.data[dest_idx + 3] = out_a.min(255) as u8;
        }
    }
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

    #[test]
    fn triangle_icon() {
        let icon = Icon::triangle(8, 0, 255, 0, 255);
        assert_eq!(icon.width, 17);
        // Center-ish area should be filled
        let center = ((9 * 17 + 8) * 4) as usize;
        assert_eq!(icon.data[center + 3], 255);
    }
}
