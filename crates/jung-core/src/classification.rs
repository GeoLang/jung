//! Classification and graduated symbology.
//!
//! Provides utilities for classifying numeric data into breaks (natural breaks,
//! equal interval, quantile, standard deviation) and mapping values to colors
//! via color ramps.

use jung_style::Color;

/// Classification method for numeric data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClassificationMethod {
    /// Equal-width intervals.
    EqualInterval,
    /// Each class has approximately the same number of features.
    Quantile,
    /// Jenks natural breaks optimization (minimizes within-class variance).
    NaturalBreaks,
    /// Classes based on standard deviations from the mean.
    StandardDeviation,
    /// User-defined manual breaks.
    Manual,
}

/// A set of class breaks defining the boundaries of each class.
#[derive(Debug, Clone)]
pub struct ClassBreaks {
    /// Sorted ascending break values. N breaks define N+1 classes.
    /// Class i contains values where `breaks[i-1] <= v < breaks[i]`.
    pub breaks: Vec<f64>,
    pub method: ClassificationMethod,
}

impl ClassBreaks {
    /// Classify a value. Returns the class index (0-based).
    pub fn classify(&self, value: f64) -> usize {
        match self
            .breaks
            .binary_search_by(|b| b.partial_cmp(&value).unwrap())
        {
            Ok(idx) => idx + 1, // value == break → next class
            Err(idx) => idx,
        }
    }

    /// Number of classes.
    pub fn num_classes(&self) -> usize {
        self.breaks.len() + 1
    }

    /// Compute equal-interval breaks.
    pub fn equal_interval(values: &[f64], num_classes: usize) -> Self {
        let (min, max) = min_max(values);
        let step = (max - min) / num_classes as f64;
        let breaks: Vec<f64> = (1..num_classes).map(|i| min + step * i as f64).collect();
        Self {
            breaks,
            method: ClassificationMethod::EqualInterval,
        }
    }

    /// Compute quantile breaks (equal count).
    pub fn quantile(values: &[f64], num_classes: usize) -> Self {
        let mut sorted: Vec<f64> = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = sorted.len();
        let breaks: Vec<f64> = (1..num_classes)
            .map(|i| {
                let idx = (i * n / num_classes).min(n - 1);
                sorted[idx]
            })
            .collect();
        Self {
            breaks,
            method: ClassificationMethod::Quantile,
        }
    }

    /// Compute natural breaks (Jenks) using the Fisher-Jenks algorithm.
    pub fn natural_breaks(values: &[f64], num_classes: usize) -> Self {
        let mut sorted: Vec<f64> = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted.dedup();
        let n = sorted.len();

        if n <= num_classes {
            // Fewer unique values than classes
            let breaks = sorted[1..].to_vec();
            return Self {
                breaks,
                method: ClassificationMethod::NaturalBreaks,
            };
        }

        // Fisher-Jenks: dynamic programming approach
        let k = num_classes;
        let mut mat1 = vec![vec![0usize; k]; n]; // lower class limits
        let mut mat2 = vec![vec![f64::MAX; k]; n]; // variance combinations

        // Initialize first column (1 class)
        for i in 0..n {
            let mut sum = 0.0;
            let mut sum_sq = 0.0;
            for val in sorted.iter().take(i + 1) {
                sum += val;
                sum_sq += val * val;
            }
            let mean = sum / (i + 1) as f64;
            mat2[i][0] = sum_sq - sum * mean;
            mat1[i][0] = 0;
        }

        // Fill remaining columns
        for j in 1..k {
            for i in j..n {
                let mut best_variance = f64::MAX;
                let mut best_lower = j;

                let mut sum = 0.0;
                let mut sum_sq = 0.0;
                for m in (j..=i).rev() {
                    sum += sorted[m];
                    sum_sq += sorted[m] * sorted[m];
                    let count = (i - m + 1) as f64;
                    let mean = sum / count;
                    let variance = sum_sq - sum * mean;

                    if m > 0 {
                        let total = variance + mat2[m - 1][j - 1];
                        if total < best_variance {
                            best_variance = total;
                            best_lower = m;
                        }
                    } else {
                        let total = variance;
                        if total < best_variance {
                            best_variance = total;
                            best_lower = 0;
                        }
                    }
                }
                mat2[i][j] = best_variance;
                mat1[i][j] = best_lower;
            }
        }

        // Extract breaks from lower class limits
        let mut breaks = Vec::with_capacity(k - 1);
        let mut idx = n - 1;
        for j in (1..k).rev() {
            let lower = mat1[idx][j];
            breaks.push(sorted[lower]);
            if lower > 0 {
                idx = lower - 1;
            }
        }
        breaks.reverse();

        Self {
            breaks,
            method: ClassificationMethod::NaturalBreaks,
        }
    }

    /// Compute standard deviation breaks.
    pub fn standard_deviation(values: &[f64], num_std: f64) -> Self {
        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        let mut breaks = Vec::new();
        let mut v = mean - num_std * std_dev;
        let step = std_dev;
        while v < mean + num_std * std_dev + step * 0.5 {
            breaks.push(v);
            v += step;
        }

        Self {
            breaks,
            method: ClassificationMethod::StandardDeviation,
        }
    }

    /// Manual breaks from user-specified values.
    pub fn manual(breaks: Vec<f64>) -> Self {
        let mut sorted = breaks;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Self {
            breaks: sorted,
            method: ClassificationMethod::Manual,
        }
    }
}

/// A color ramp: maps class indices to colors.
#[derive(Debug, Clone)]
pub struct ColorRamp {
    pub colors: Vec<Color>,
}

impl ColorRamp {
    pub fn new(colors: Vec<Color>) -> Self {
        Self { colors }
    }

    /// Get color for a class index.
    pub fn get(&self, class_idx: usize) -> Color {
        if self.colors.is_empty() {
            return Color::rgb(128, 128, 128);
        }
        self.colors[class_idx.min(self.colors.len() - 1)]
    }

    /// Interpolate along the ramp (0.0..1.0).
    pub fn interpolate(&self, t: f64) -> Color {
        if self.colors.is_empty() {
            return Color::rgb(128, 128, 128);
        }
        if self.colors.len() == 1 {
            return self.colors[0];
        }
        let t = t.clamp(0.0, 1.0);
        let max_idx = self.colors.len() - 1;
        let pos = t * max_idx as f64;
        let lower = pos.floor() as usize;
        let upper = (lower + 1).min(max_idx);
        let frac = pos - lower as f64;

        let c1 = &self.colors[lower];
        let c2 = &self.colors[upper];
        Color {
            r: lerp_u8(c1.r, c2.r, frac),
            g: lerp_u8(c1.g, c2.g, frac),
            b: lerp_u8(c1.b, c2.b, frac),
            a: lerp_u8(c1.a, c2.a, frac),
        }
    }

    /// Built-in sequential ramp: blue to red.
    pub fn blue_to_red(n: usize) -> Self {
        let colors: Vec<Color> = (0..n)
            .map(|i| {
                let t = i as f64 / (n - 1).max(1) as f64;
                Color::rgb((t * 255.0) as u8, 0, ((1.0 - t) * 255.0) as u8)
            })
            .collect();
        Self { colors }
    }

    /// Built-in diverging ramp: blue → white → red.
    pub fn diverging(n: usize) -> Self {
        let colors: Vec<Color> = (0..n)
            .map(|i| {
                let t = i as f64 / (n - 1).max(1) as f64;
                if t < 0.5 {
                    let s = t * 2.0;
                    Color::rgb((s * 255.0) as u8, (s * 255.0) as u8, 255)
                } else {
                    let s = (t - 0.5) * 2.0;
                    Color::rgb(255, ((1.0 - s) * 255.0) as u8, ((1.0 - s) * 255.0) as u8)
                }
            })
            .collect();
        Self { colors }
    }

    /// Built-in qualitative/categorical ramp.
    pub fn categorical() -> Self {
        Self {
            colors: vec![
                Color::rgb(228, 26, 28),   // red
                Color::rgb(55, 126, 184),  // blue
                Color::rgb(77, 175, 74),   // green
                Color::rgb(152, 78, 163),  // purple
                Color::rgb(255, 127, 0),   // orange
                Color::rgb(255, 255, 51),  // yellow
                Color::rgb(166, 86, 40),   // brown
                Color::rgb(247, 129, 191), // pink
                Color::rgb(153, 153, 153), // gray
            ],
        }
    }
}

/// Proportional symbol sizing: maps a value to a symbol radius.
#[derive(Debug, Clone)]
pub struct ProportionalSize {
    pub min_value: f64,
    pub max_value: f64,
    pub min_radius: f64,
    pub max_radius: f64,
    /// If true, use Flannery's perceptual scaling.
    pub flannery: bool,
}

impl ProportionalSize {
    pub fn new(min_value: f64, max_value: f64, min_radius: f64, max_radius: f64) -> Self {
        Self {
            min_value,
            max_value,
            min_radius,
            max_radius,
            flannery: false,
        }
    }

    /// Enable Flannery's perceptual scaling (exponent = 0.5716).
    pub fn with_flannery(mut self) -> Self {
        self.flannery = true;
        self
    }

    /// Compute the radius for a given value.
    pub fn radius(&self, value: f64) -> f64 {
        if self.max_value <= self.min_value {
            return self.min_radius;
        }
        let t = ((value - self.min_value) / (self.max_value - self.min_value)).clamp(0.0, 1.0);
        let scaled = if self.flannery {
            t.powf(0.5716) // Flannery's perceptual exponent
        } else {
            t
        };
        self.min_radius + scaled * (self.max_radius - self.min_radius)
    }
}

fn min_max(values: &[f64]) -> (f64, f64) {
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for &v in values {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    (min, max)
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_interval_5_classes() {
        let values: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let cb = ClassBreaks::equal_interval(&values, 5);
        assert_eq!(cb.num_classes(), 5);
        assert_eq!(cb.breaks.len(), 4);
        // Breaks should be at 19.8, 39.6, 59.4, 79.2
        assert!((cb.breaks[0] - 19.8).abs() < 0.01);
    }

    #[test]
    fn classify_value() {
        let cb = ClassBreaks::manual(vec![10.0, 20.0, 30.0]);
        assert_eq!(cb.classify(5.0), 0);
        assert_eq!(cb.classify(15.0), 1);
        assert_eq!(cb.classify(25.0), 2);
        assert_eq!(cb.classify(35.0), 3);
    }

    #[test]
    fn quantile_breaks() {
        let values: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let cb = ClassBreaks::quantile(&values, 4);
        assert_eq!(cb.num_classes(), 4);
        // Quartile breaks should be around 25, 50, 75
        assert!((cb.breaks[0] - 25.0).abs() < 1.0);
        assert!((cb.breaks[1] - 50.0).abs() < 1.0);
        assert!((cb.breaks[2] - 75.0).abs() < 1.0);
    }

    #[test]
    fn natural_breaks_basic() {
        // Three clusters
        let values: Vec<f64> = vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 20.0, 21.0, 22.0, 23.0, 50.0, 51.0, 52.0,
        ];
        let cb = ClassBreaks::natural_breaks(&values, 3);
        assert_eq!(cb.num_classes(), 3);
        // Should find breaks between clusters
        assert!(cb.breaks[0] > 5.0 && cb.breaks[0] <= 20.0);
        assert!(cb.breaks[1] > 23.0 && cb.breaks[1] <= 50.0);
    }

    #[test]
    fn color_ramp_interpolation() {
        let ramp = ColorRamp::new(vec![Color::rgb(0, 0, 0), Color::rgb(255, 255, 255)]);
        let mid = ramp.interpolate(0.5);
        assert!((mid.r as i32 - 128).abs() <= 1);
        assert!((mid.g as i32 - 128).abs() <= 1);
    }

    #[test]
    fn color_ramp_get_class() {
        let ramp = ColorRamp::blue_to_red(5);
        let c0 = ramp.get(0);
        assert_eq!(c0.r, 0);
        assert_eq!(c0.b, 255);
        let c4 = ramp.get(4);
        assert_eq!(c4.r, 255);
        assert_eq!(c4.b, 0);
    }

    #[test]
    fn proportional_linear() {
        let ps = ProportionalSize::new(0.0, 100.0, 2.0, 20.0);
        assert!((ps.radius(0.0) - 2.0).abs() < 0.01);
        assert!((ps.radius(100.0) - 20.0).abs() < 0.01);
        assert!((ps.radius(50.0) - 11.0).abs() < 0.01);
    }

    #[test]
    fn proportional_flannery() {
        let ps = ProportionalSize::new(0.0, 100.0, 2.0, 20.0).with_flannery();
        // Flannery gives larger sizes for smaller values (perceptual correction)
        let mid = ps.radius(50.0);
        // With exponent 0.5716: 0.5^0.5716 ≈ 0.672 → radius = 2 + 0.672 * 18 ≈ 14.1
        assert!(mid > 13.0 && mid < 15.0);
    }

    #[test]
    fn standard_deviation_breaks() {
        let values: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let cb = ClassBreaks::standard_deviation(&values, 2.0);
        // Should produce breaks at mean ± 1σ and ± 2σ
        assert!(cb.breaks.len() >= 3);
    }

    #[test]
    fn categorical_ramp() {
        let ramp = ColorRamp::categorical();
        assert_eq!(ramp.colors.len(), 9);
    }

    #[test]
    fn diverging_ramp() {
        let ramp = ColorRamp::diverging(5);
        // Middle should be white-ish (high R, G, B)
        let mid = ramp.colors[2];
        assert!(mid.r > 200);
    }
}
