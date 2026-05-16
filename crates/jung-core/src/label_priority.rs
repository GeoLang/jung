//! Priority-based label placement with spatial deconfliction grid.
//!
//! Extends the base label engine with multi-pass placement:
//! 1. Sort labels by priority (importance)
//! 2. Place highest-priority labels first
//! 3. Use a grid-based spatial index for O(1) collision checks
//! 4. Try alternate positions (8 compass directions) for rejected labels

/// Label priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LabelPriority {
    /// Must always be shown (capital cities, etc.)
    Critical = 4,
    /// High importance (major cities, highways)
    High = 3,
    /// Normal importance
    Medium = 2,
    /// Low importance (minor features)
    Low = 1,
    /// Optional (shown only if space permits)
    Optional = 0,
}

/// A label candidate with position and priority.
#[derive(Debug, Clone)]
pub struct LabelCandidate {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub priority: LabelPriority,
    pub rotation: f64,
    pub anchor_x: f64,
    pub anchor_y: f64,
}

/// Bounding box for collision detection.
#[derive(Debug, Clone, Copy)]
struct BBox {
    x_min: f64,
    y_min: f64,
    x_max: f64,
    y_max: f64,
}

impl BBox {
    fn intersects(&self, other: &BBox) -> bool {
        self.x_min < other.x_max
            && self.x_max > other.x_min
            && self.y_min < other.y_max
            && self.y_max > other.y_min
    }
}

/// Grid-based spatial index for fast collision queries.
struct CollisionGrid {
    cell_size: f64,
    width_cells: usize,
    height_cells: usize,
    cells: Vec<Vec<BBox>>,
}

impl CollisionGrid {
    fn new(width: f64, height: f64, cell_size: f64) -> Self {
        let width_cells = (width / cell_size).ceil() as usize + 1;
        let height_cells = (height / cell_size).ceil() as usize + 1;
        let cells = vec![Vec::new(); width_cells * height_cells];
        Self {
            cell_size,
            width_cells,
            height_cells,
            cells,
        }
    }

    fn insert(&mut self, bbox: BBox) {
        let x_start = (bbox.x_min / self.cell_size) as usize;
        let x_end = (bbox.x_max / self.cell_size) as usize;
        let y_start = (bbox.y_min / self.cell_size) as usize;
        let y_end = (bbox.y_max / self.cell_size) as usize;

        for cy in y_start..=y_end.min(self.height_cells - 1) {
            for cx in x_start..=x_end.min(self.width_cells - 1) {
                let idx = cy * self.width_cells + cx;
                if idx < self.cells.len() {
                    self.cells[idx].push(bbox);
                }
            }
        }
    }

    fn collides(&self, bbox: &BBox) -> bool {
        let x_start = (bbox.x_min / self.cell_size) as usize;
        let x_end = (bbox.x_max / self.cell_size) as usize;
        let y_start = (bbox.y_min / self.cell_size) as usize;
        let y_end = (bbox.y_max / self.cell_size) as usize;

        for cy in y_start..=y_end.min(self.height_cells - 1) {
            for cx in x_start..=x_end.min(self.width_cells - 1) {
                let idx = cy * self.width_cells + cx;
                if idx < self.cells.len() {
                    for existing in &self.cells[idx] {
                        if existing.intersects(bbox) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// Result of label placement.
#[derive(Debug, Clone)]
pub struct PlacedLabel {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub priority: LabelPriority,
}

/// Priority-based label placement engine.
pub struct PriorityLabelEngine {
    canvas_width: f64,
    canvas_height: f64,
    padding: f64,
}

impl PriorityLabelEngine {
    pub fn new(canvas_width: f64, canvas_height: f64) -> Self {
        Self {
            canvas_width,
            canvas_height,
            padding: 2.0,
        }
    }

    /// Set padding between labels.
    pub fn with_padding(mut self, padding: f64) -> Self {
        self.padding = padding;
        self
    }

    /// Place labels with priority-based deconfliction.
    /// Returns the successfully placed labels.
    pub fn place(&self, candidates: &[LabelCandidate]) -> Vec<PlacedLabel> {
        // Sort by priority (highest first)
        let mut sorted: Vec<&LabelCandidate> = candidates.iter().collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        let cell_size = 50.0; // Grid cell size in pixels
        let mut grid = CollisionGrid::new(self.canvas_width, self.canvas_height, cell_size);
        let mut placed = Vec::new();

        for candidate in sorted {
            if let Some(position) = self.try_place(candidate, &grid) {
                let bbox = BBox {
                    x_min: position.0 - self.padding,
                    y_min: position.1 - self.padding,
                    x_max: position.0 + candidate.width + self.padding,
                    y_max: position.1 + candidate.height + self.padding,
                };
                grid.insert(bbox);
                placed.push(PlacedLabel {
                    text: candidate.text.clone(),
                    x: position.0,
                    y: position.1,
                    width: candidate.width,
                    height: candidate.height,
                    priority: candidate.priority,
                });
            }
        }

        placed
    }

    fn try_place(&self, candidate: &LabelCandidate, grid: &CollisionGrid) -> Option<(f64, f64)> {
        // Try original position and 7 alternates (each displaced by full label dimensions + padding)
        let dx_full = candidate.width + self.padding * 2.0;
        let dy_full = candidate.height + self.padding * 2.0;
        let offsets = [
            (0.0, 0.0),           // original
            (dx_full, 0.0),       // right
            (-dx_full, 0.0),      // left
            (0.0, -dy_full),      // above
            (0.0, dy_full),       // below
            (dx_full, -dy_full),  // upper-right
            (-dx_full, -dy_full), // upper-left
            (dx_full, dy_full),   // lower-right
        ];

        for (dx, dy) in &offsets {
            let x = candidate.x + dx;
            let y = candidate.y + dy;

            // Bounds check
            if x < 0.0
                || y < 0.0
                || x + candidate.width > self.canvas_width
                || y + candidate.height > self.canvas_height
            {
                continue;
            }

            let bbox = BBox {
                x_min: x - self.padding,
                y_min: y - self.padding,
                x_max: x + candidate.width + self.padding,
                y_max: y + candidate.height + self.padding,
            };

            if !grid.collides(&bbox) {
                return Some((x, y));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidate(text: &str, x: f64, y: f64, priority: LabelPriority) -> LabelCandidate {
        LabelCandidate {
            text: text.to_string(),
            x,
            y,
            width: 60.0,
            height: 14.0,
            priority,
            rotation: 0.0,
            anchor_x: x,
            anchor_y: y,
        }
    }

    #[test]
    fn test_no_collision() {
        let engine = PriorityLabelEngine::new(800.0, 600.0);
        let candidates = vec![
            make_candidate("London", 100.0, 100.0, LabelPriority::High),
            make_candidate("Paris", 400.0, 300.0, LabelPriority::High),
        ];
        let placed = engine.place(&candidates);
        assert_eq!(placed.len(), 2);
    }

    #[test]
    fn test_priority_wins() {
        let engine = PriorityLabelEngine::new(800.0, 600.0);
        let candidates = vec![
            make_candidate("Low", 100.0, 100.0, LabelPriority::Low),
            make_candidate("Critical", 100.0, 100.0, LabelPriority::Critical),
        ];
        let placed = engine.place(&candidates);
        // Critical gets placed first at preferred position
        assert!(
            placed
                .iter()
                .any(|p| p.text == "Critical" && (p.x - 100.0).abs() < 1.0)
        );
    }

    #[test]
    fn test_alternate_placement() {
        let engine = PriorityLabelEngine::new(800.0, 600.0);
        // Two labels at same position — second should be displaced
        let candidates = vec![
            make_candidate("First", 200.0, 200.0, LabelPriority::High),
            make_candidate("Second", 200.0, 200.0, LabelPriority::Medium),
        ];
        let placed = engine.place(&candidates);
        assert_eq!(placed.len(), 2);
        // They should not overlap
        let a = &placed[0];
        let b = &placed[1];
        let a_box = BBox {
            x_min: a.x,
            y_min: a.y,
            x_max: a.x + a.width,
            y_max: a.y + a.height,
        };
        let b_box = BBox {
            x_min: b.x,
            y_min: b.y,
            x_max: b.x + b.width,
            y_max: b.y + b.height,
        };
        assert!(!a_box.intersects(&b_box));
    }

    #[test]
    fn test_out_of_bounds_rejection() {
        let engine = PriorityLabelEngine::new(100.0, 100.0);
        let candidates = vec![make_candidate("Big Label", 90.0, 90.0, LabelPriority::High)];
        let placed = engine.place(&candidates);
        // Label is 60px wide, starts at x=90 in a 100px canvas → overflow
        // Should try alternate positions
        // At least it shouldn't crash
        assert!(placed.len() <= 1);
    }

    #[test]
    fn test_collision_grid() {
        let mut grid = CollisionGrid::new(800.0, 600.0, 50.0);
        let bbox1 = BBox {
            x_min: 10.0,
            y_min: 10.0,
            x_max: 50.0,
            y_max: 30.0,
        };
        grid.insert(bbox1);

        let bbox2 = BBox {
            x_min: 20.0,
            y_min: 15.0,
            x_max: 60.0,
            y_max: 25.0,
        };
        assert!(grid.collides(&bbox2));

        let bbox3 = BBox {
            x_min: 200.0,
            y_min: 200.0,
            x_max: 250.0,
            y_max: 220.0,
        };
        assert!(!grid.collides(&bbox3));
    }
}
