//! MIL-STD-2525 military symbology support.
//!
//! Implements the Symbol Identification Code (SIDC) decoder, frame shapes
//! (affiliation-based), and standard military icon rendering.
//!
//! SIDC format (15 characters): Version + Identity + Symbol Set + Status +
//! HQ/TF/Dummy + Amplifier + Descriptor + Entity + Modifier1 + Modifier2

use crate::geometry::Point;
use crate::marker::Icon;
use crate::renderer::PixelBuffer;
use jung_style::Color;

/// Affiliation of a military symbol.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Affiliation {
    Pending,
    Unknown,
    AssumedFriend,
    Friend,
    Neutral,
    Suspect,
    Hostile,
    Joker,
    Faker,
}

/// Battle dimension / symbol set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    Air,
    Ground,
    Sea,
    Subsurface,
    Space,
    Unknown,
}

/// Operational status.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Present,
    Planned,
    FullyCapable,
    Damaged,
    Destroyed,
    FullToCapacity,
}

/// Echelon/size indicator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Echelon {
    Team,
    Squad,
    Section,
    Platoon,
    Company,
    Battalion,
    Regiment,
    Brigade,
    Division,
    Corps,
    Army,
    ArmyGroup,
    Region,
    Command,
    None,
}

/// HQ / Task Force / Dummy indicator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modifiers {
    pub headquarters: bool,
    pub task_force: bool,
    pub feint_dummy: bool,
}

/// A parsed MIL-STD-2525 Symbol Identification Code.
#[derive(Debug, Clone)]
pub struct Sidc {
    pub affiliation: Affiliation,
    pub dimension: Dimension,
    pub status: Status,
    pub echelon: Echelon,
    pub modifiers: Modifiers,
    pub entity_code: String,
    pub raw: String,
}

impl Sidc {
    /// Parse a 15-character SIDC string (2525D format).
    pub fn parse(sidc: &str) -> Option<Self> {
        if sidc.len() < 15 {
            return None;
        }
        let chars: Vec<char> = sidc.chars().collect();

        let affiliation = match chars[1] {
            'P' | '0' => Affiliation::Pending,
            'U' | '1' => Affiliation::Unknown,
            'A' | '2' => Affiliation::AssumedFriend,
            'F' | '3' => Affiliation::Friend,
            'N' | '4' => Affiliation::Neutral,
            'S' | '5' => Affiliation::Suspect,
            'H' | '6' => Affiliation::Hostile,
            'J' => Affiliation::Joker,
            'K' => Affiliation::Faker,
            _ => Affiliation::Unknown,
        };

        let dimension = match chars[2] {
            'A' | '0' => Dimension::Air,
            'G' | '1' => Dimension::Ground,
            'S' | '2' => Dimension::Sea,
            'U' | '3' => Dimension::Subsurface,
            'P' | '4' => Dimension::Space,
            _ => Dimension::Unknown,
        };

        let status = match chars[3] {
            'P' | '0' => Status::Present,
            'A' | '1' => Status::Planned,
            'C' | '2' => Status::FullyCapable,
            'D' | '3' => Status::Damaged,
            'X' | '4' => Status::Destroyed,
            'F' | '5' => Status::FullToCapacity,
            _ => Status::Present,
        };

        let modifiers = Modifiers {
            headquarters: chars[4] == 'A' || chars[4] == '1',
            task_force: chars[4] == 'B' || chars[4] == '2' || chars[4] == 'D' || chars[4] == '4',
            feint_dummy: chars[4] == 'C' || chars[4] == '3' || chars[4] == 'D' || chars[4] == '4',
        };

        let echelon = match chars[5] {
            'A' => Echelon::Team,
            'B' => Echelon::Squad,
            'C' => Echelon::Section,
            'D' => Echelon::Platoon,
            'E' => Echelon::Company,
            'F' => Echelon::Battalion,
            'G' => Echelon::Regiment,
            'H' => Echelon::Brigade,
            'I' => Echelon::Division,
            'J' => Echelon::Corps,
            'K' => Echelon::Army,
            'L' => Echelon::ArmyGroup,
            'M' => Echelon::Region,
            'N' => Echelon::Command,
            _ => Echelon::None,
        };

        let entity_code: String = chars[6..15].iter().collect();

        Some(Self {
            affiliation,
            dimension,
            status,
            echelon,
            modifiers,
            entity_code,
            raw: sidc.to_string(),
        })
    }

    /// Get the frame color for this symbol.
    pub fn frame_color(&self) -> Color {
        match self.affiliation {
            Affiliation::Friend | Affiliation::AssumedFriend => Color::rgb(128, 224, 255), // cyan
            Affiliation::Hostile
            | Affiliation::Suspect
            | Affiliation::Joker
            | Affiliation::Faker => {
                Color::rgb(255, 128, 128) // red
            }
            Affiliation::Neutral => Color::rgb(170, 255, 170), // green
            Affiliation::Unknown | Affiliation::Pending => Color::rgb(255, 255, 128), // yellow
        }
    }

    /// Get the frame shape for this symbol.
    pub fn frame_shape(&self) -> FrameShape {
        match (&self.affiliation, &self.dimension) {
            (Affiliation::Friend | Affiliation::AssumedFriend, _) => FrameShape::Rectangle,
            (
                Affiliation::Hostile
                | Affiliation::Suspect
                | Affiliation::Joker
                | Affiliation::Faker,
                _,
            ) => FrameShape::Diamond,
            (Affiliation::Neutral, _) => FrameShape::Square,
            (_, Dimension::Air | Dimension::Space) => FrameShape::Arc,
            _ => FrameShape::Circle,
        }
    }
}

/// Frame shape for military symbols.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameShape {
    Rectangle, // Friendly
    Diamond,   // Hostile
    Square,    // Neutral
    Circle,    // Unknown (ground)
    Arc,       // Unknown (air/space)
}

/// Render a MIL-STD-2525 symbol as an icon.
pub fn render_milsym(sidc: &Sidc, size: u32) -> Icon {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let center = size as f64 / 2.0;
    let radius = center * 0.8;

    let frame_color = sidc.frame_color();
    let fill_color = Color::rgba(frame_color.r, frame_color.g, frame_color.b, 180);

    // Draw frame shape
    match sidc.frame_shape() {
        FrameShape::Rectangle => {
            draw_rect(&mut data, size, center, radius, fill_color, frame_color);
        }
        FrameShape::Diamond => {
            draw_diamond(&mut data, size, center, radius, fill_color, frame_color);
        }
        FrameShape::Square => {
            draw_square(&mut data, size, center, radius, fill_color, frame_color);
        }
        FrameShape::Circle | FrameShape::Arc => {
            draw_circle(&mut data, size, center, radius, fill_color, frame_color);
        }
    }

    // Draw status indicator (dashed border for planned)
    if sidc.status == Status::Planned {
        draw_planned_indicator(&mut data, size, center, radius);
    }

    // Draw destroyed indicator (X)
    if sidc.status == Status::Destroyed {
        draw_destroyed_indicator(&mut data, size, center, radius);
    }

    // Draw HQ indicator (vertical line below)
    if sidc.modifiers.headquarters {
        draw_hq_indicator(&mut data, size, center, radius);
    }

    Icon {
        width: size,
        height: size,
        data,
    }
}

/// Render a mil-sym onto a pixel buffer at a specific location.
pub fn render_milsym_at(
    buffer: &mut PixelBuffer,
    sidc: &Sidc,
    position: &Point,
    bbox: &crate::renderer::BBox,
    size: u32,
) {
    let icon = render_milsym(sidc, size);
    let px = (position.x - bbox.min_x) / (bbox.max_x - bbox.min_x) * buffer.width as f64;
    let py = (bbox.max_y - position.y) / (bbox.max_y - bbox.min_y) * buffer.height as f64;
    crate::marker::blit_icon(buffer, &icon, px, py, 1.0);
}

fn draw_rect(data: &mut [u8], size: u32, center: f64, radius: f64, fill: Color, border: Color) {
    let half_w = radius;
    let half_h = radius * 0.7;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f64 - center).abs();
            let dy = (y as f64 - center).abs();
            if dx <= half_w && dy <= half_h {
                let idx = ((y * size + x) * 4) as usize;
                if dx >= half_w - 2.0 || dy >= half_h - 2.0 {
                    set_pixel(data, idx, border);
                } else {
                    set_pixel(data, idx, fill);
                }
            }
        }
    }
}

fn draw_diamond(data: &mut [u8], size: u32, center: f64, radius: f64, fill: Color, border: Color) {
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f64 - center).abs();
            let dy = (y as f64 - center).abs();
            let dist = dx + dy;
            if dist <= radius {
                let idx = ((y * size + x) * 4) as usize;
                if dist >= radius - 2.5 {
                    set_pixel(data, idx, border);
                } else {
                    set_pixel(data, idx, fill);
                }
            }
        }
    }
}

fn draw_square(data: &mut [u8], size: u32, center: f64, radius: f64, fill: Color, border: Color) {
    let half = radius * 0.85;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f64 - center).abs();
            let dy = (y as f64 - center).abs();
            if dx <= half && dy <= half {
                let idx = ((y * size + x) * 4) as usize;
                if dx >= half - 2.0 || dy >= half - 2.0 {
                    set_pixel(data, idx, border);
                } else {
                    set_pixel(data, idx, fill);
                }
            }
        }
    }
}

fn draw_circle(data: &mut [u8], size: u32, center: f64, radius: f64, fill: Color, border: Color) {
    let r2 = radius * radius;
    let inner_r2 = (radius - 2.5) * (radius - 2.5);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f64 - center;
            let dy = y as f64 - center;
            let dist2 = dx * dx + dy * dy;
            if dist2 <= r2 {
                let idx = ((y * size + x) * 4) as usize;
                if dist2 >= inner_r2 {
                    set_pixel(data, idx, border);
                } else {
                    set_pixel(data, idx, fill);
                }
            }
        }
    }
}

fn draw_planned_indicator(data: &mut [u8], size: u32, center: f64, radius: f64) {
    // Dashes at corners to indicate planned status
    let r = radius + 3.0;
    for angle_idx in 0..8 {
        let angle = angle_idx as f64 * std::f64::consts::FRAC_PI_4;
        let x = (center + angle.cos() * r) as u32;
        let y = (center + angle.sin() * r) as u32;
        if x < size && y < size {
            let idx = ((y * size + x) * 4) as usize;
            set_pixel(data, idx, Color::rgb(0, 0, 0));
        }
    }
}

fn draw_destroyed_indicator(data: &mut [u8], size: u32, center: f64, radius: f64) {
    // Draw an X through the symbol
    let r = radius * 0.7;
    for i in 0..=(r as i32 * 2) {
        let t = i as f64 / (r * 2.0);
        let x1 = (center - r + t * r * 2.0) as u32;
        let y1 = (center - r + t * r * 2.0) as u32;
        let x2 = (center + r - t * r * 2.0) as u32;
        let y2 = (center - r + t * r * 2.0) as u32;
        if x1 < size && y1 < size {
            let idx = ((y1 * size + x1) * 4) as usize;
            set_pixel(data, idx, Color::rgb(0, 0, 0));
        }
        if x2 < size && y2 < size {
            let idx = ((y2 * size + x2) * 4) as usize;
            set_pixel(data, idx, Color::rgb(0, 0, 0));
        }
    }
}

fn draw_hq_indicator(data: &mut [u8], size: u32, center: f64, radius: f64) {
    // Vertical line extending below the symbol
    let x = center as u32;
    let y_start = (center + radius) as u32;
    let y_end = (y_start + 5).min(size - 1);
    for y in y_start..=y_end {
        if x < size {
            let idx = ((y * size + x) * 4) as usize;
            set_pixel(data, idx, Color::rgb(0, 0, 0));
        }
    }
}

fn set_pixel(data: &mut [u8], idx: usize, color: Color) {
    data[idx] = color.r;
    data[idx + 1] = color.g;
    data[idx + 2] = color.b;
    data[idx + 3] = color.a;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_friendly_ground_sidc() {
        // chars[1]='3'=Friend, chars[2]='1'=Ground, chars[3]='0'=Present
        let sidc = Sidc::parse("13100000000000-").unwrap();
        assert_eq!(sidc.affiliation, Affiliation::Friend);
        assert_eq!(sidc.dimension, Dimension::Ground);
        assert_eq!(sidc.status, Status::Present);
    }

    #[test]
    fn parse_hostile_air_sidc() {
        // chars[1]='5'=Suspect, chars[2]='0'=Air
        let sidc = Sidc::parse("15000000000000-").unwrap();
        assert_eq!(sidc.affiliation, Affiliation::Suspect);
        assert_eq!(sidc.dimension, Dimension::Air);
    }

    #[test]
    fn frame_color_friend() {
        let sidc = Sidc::parse("13100000000000-").unwrap();
        let color = sidc.frame_color();
        assert_eq!(color.r, 128);
        assert_eq!(color.g, 224);
        assert_eq!(color.b, 255);
    }

    #[test]
    fn frame_color_hostile() {
        let sidc = Sidc::parse("16100000000000-").unwrap();
        let color = sidc.frame_color();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 128);
        assert_eq!(color.b, 128);
    }

    #[test]
    fn frame_shape_mapping() {
        let friend = Sidc::parse("13100000000000-").unwrap();
        assert_eq!(friend.frame_shape(), FrameShape::Rectangle);

        let hostile = Sidc::parse("16100000000000-").unwrap();
        assert_eq!(hostile.frame_shape(), FrameShape::Diamond);

        let neutral = Sidc::parse("14100000000000-").unwrap();
        assert_eq!(neutral.frame_shape(), FrameShape::Square);
    }

    #[test]
    fn render_milsym_produces_pixels() {
        let sidc = Sidc::parse("13100000000000-").unwrap();
        let icon = render_milsym(&sidc, 32);
        assert_eq!(icon.width, 32);
        assert_eq!(icon.height, 32);
        let filled = icon.data.chunks(4).filter(|px| px[3] > 0).count();
        assert!(filled > 50);
    }

    #[test]
    fn destroyed_symbol_has_x() {
        // chars[3]='4'=Destroyed
        let sidc = Sidc::parse("13140000000000-").unwrap();
        assert_eq!(sidc.status, Status::Destroyed);
        let icon = render_milsym(&sidc, 32);
        let black = icon
            .data
            .chunks(4)
            .filter(|px| px[0] == 0 && px[1] == 0 && px[2] == 0 && px[3] == 255)
            .count();
        assert!(black > 0);
    }

    #[test]
    fn hq_modifier() {
        // chars[4]='1'=headquarters
        let sidc = Sidc::parse("13101000000000-").unwrap();
        assert!(sidc.modifiers.headquarters);
    }

    #[test]
    fn invalid_sidc_too_short() {
        assert!(Sidc::parse("1003").is_none());
    }

    #[test]
    fn echelon_parsing() {
        // chars[5]='F'=Battalion
        let sidc = Sidc::parse("13100F00000000-").unwrap();
        assert_eq!(sidc.echelon, Echelon::Battalion);
    }
}
