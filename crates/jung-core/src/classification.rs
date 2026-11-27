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

#[derive(Debug, thiserror::Error)]
pub enum ColorFileError {
    #[error("colour file line {line}: {detail}")]
    Line { line: usize, detail: String },
    #[error("the colour file holds no colour entry that applies")]
    NoEntries,
    #[error("percent stops need the raster's min and max")]
    PercentWithoutRange,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopValue {
    Value(f64),
    Percent(f64),
    NoData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorFileEntry {
    pub value: StopValue,
    pub color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorFile {
    pub entries: Vec<ColorFileEntry>,
}

// gdal GDALFindNamedColor, each channel 255 times a float fraction, truncated
const GDALDEM_NAMED_COLORS: [(&str, Color); 16] = [
    ("white", Color::rgb(255, 255, 255)),
    ("black", Color::rgb(0, 0, 0)),
    ("red", Color::rgb(255, 0, 0)),
    ("green", Color::rgb(0, 255, 0)),
    ("blue", Color::rgb(0, 0, 255)),
    ("yellow", Color::rgb(255, 255, 0)),
    ("magenta", Color::rgb(255, 0, 255)),
    ("cyan", Color::rgb(0, 255, 255)),
    ("aqua", Color::rgb(0, 191, 191)),
    ("grey", Color::rgb(191, 191, 191)),
    ("gray", Color::rgb(191, 191, 191)),
    ("orange", Color::rgb(255, 127, 0)),
    ("brown", Color::rgb(191, 127, 63)),
    ("purple", Color::rgb(127, 0, 255)),
    ("violet", Color::rgb(127, 0, 255)),
    ("indigo", Color::rgb(0, 127, 255)),
];

fn color_channel(token: &str) -> Result<u8, String> {
    token
        .parse::<u8>()
        .map_err(|_| format!("colour channel {token:?} is not a whole number from 0 to 255"))
}

fn stop_value(token: &str) -> Result<StopValue, String> {
    if token.eq_ignore_ascii_case("nv") {
        return Ok(StopValue::NoData);
    }
    let number = |text: &str| {
        text.parse::<f64>()
            .ok()
            .filter(|value| !value.is_nan())
            .ok_or_else(|| format!("value {token:?} is not a number"))
    };
    let Some(percent) = token.strip_suffix('%') else {
        return number(token).map(StopValue::Value);
    };
    let percent = number(percent)?;
    if !(0.0..=100.0).contains(&percent) {
        return Err(format!("percentage {token:?} is outside 0% to 100%"));
    }
    Ok(StopValue::Percent(percent))
}

fn color_file_entry(tokens: &[&str]) -> Result<ColorFileEntry, String> {
    let value = stop_value(tokens[0])?;
    let color = match &tokens[1..] {
        [r, g, b] => Color::rgb(color_channel(r)?, color_channel(g)?, color_channel(b)?),
        [r, g, b, a, ..] => Color::rgba(
            color_channel(r)?,
            color_channel(g)?,
            color_channel(b)?,
            color_channel(a)?,
        ),
        [name, ..] => {
            let (_, named) = GDALDEM_NAMED_COLORS
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("unknown colour name {name:?}"))?;
            let alpha = match tokens.get(2) {
                Some(alpha) => color_channel(alpha)?,
                None => 255,
            };
            Color::rgba(named.r, named.g, named.b, alpha)
        }
        [] => return Err(format!("value {:?} has no colour", tokens[0])),
    };
    Ok(ColorFileEntry { value, color })
}

impl ColorFile {
    pub fn parse(text: &str) -> Result<ColorFile, ColorFileError> {
        let mut entries = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line_error = |detail: String| ColorFileError::Line {
                line: index + 1,
                detail,
            };
            if line.starts_with('#') && line.contains("COLOR_MODEL") {
                return Err(line_error("GMT palettes are not read".into()));
            }
            if line.starts_with(['#', '/']) {
                continue;
            }
            // gdal GDALLoadTextColorMap splits on spaces, commas, tabs and colons
            let tokens: Vec<&str> = line
                .split([' ', ',', '\t', ':'])
                .filter(|token| !token.is_empty())
                .collect();
            if tokens.is_empty() {
                continue;
            }
            entries.push(color_file_entry(&tokens).map_err(line_error)?);
        }
        if entries.is_empty() {
            return Err(ColorFileError::NoEntries);
        }
        Ok(ColorFile { entries })
    }

    pub fn has_percent_stops(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry.value, StopValue::Percent(_)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSelection {
    #[default]
    Interpolate,
    Exact,
    Nearest,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ValueStop {
    value: f64,
    color: Color,
}

const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);

fn sort_stops(stops: &mut [ValueStop]) {
    stops.sort_by(|a, b| {
        a.value
            .partial_cmp(&b.value)
            .expect("NaN stops are left out")
    });
}

fn nodata_neighbours(stops: &[ValueStop], nodata: f64) -> Vec<ValueStop> {
    // gdal GDALColorReliefProcessColors puts a stop with the neighbour's colour one ulp either side of the nodata value
    let mut neighbours = Vec::new();
    for pair in stops.windows(2) {
        let (previous, current) = (pair[0], pair[1]);
        let below = current.value.next_down();
        let above = previous.value.next_up();
        if current.value == nodata && below > previous.value {
            neighbours.push(ValueStop {
                value: below,
                color: previous.color,
            });
        } else if previous.value == nodata && above < current.value {
            neighbours.push(ValueStop {
                value: above,
                color: current.color,
            });
        }
    }
    neighbours
}

fn stop_color(stops: &[ValueStop], selection: ColorSelection, value: f64) -> Color {
    // ported from gdal GDALColorReliefGetRGBA
    let above_index = stops.partition_point(|stop| stop.value < value);
    let outside_stops = above_index == 0 || above_index == stops.len();
    if outside_stops {
        let end = stops[above_index.min(stops.len() - 1)];
        if selection == ColorSelection::Exact && end.value != value {
            return TRANSPARENT;
        }
        return end.color;
    }
    let (below, above) = (stops[above_index - 1], stops[above_index]);
    if above.value == value {
        return above.color;
    }
    match selection {
        ColorSelection::Exact => TRANSPARENT,
        ColorSelection::Nearest if value - below.value < above.value - value => below.color,
        ColorSelection::Nearest => above.color,
        ColorSelection::Interpolate => {
            let t = (value - below.value) / (above.value - below.value);
            Color {
                r: lerp_u8(below.color.r, above.color.r, t),
                g: lerp_u8(below.color.g, above.color.g, t),
                b: lerp_u8(below.color.b, above.color.b, t),
                a: lerp_u8(below.color.a, above.color.a, t),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValueRamp {
    stops: Vec<ValueStop>,
    selection: ColorSelection,
    nodata_color: Color,
}

impl ValueRamp {
    pub fn new(
        file: &ColorFile,
        selection: ColorSelection,
        nodata: Option<f64>,
        range: Option<ValueRange>,
    ) -> Result<ValueRamp, ColorFileError> {
        let mut stops = Vec::with_capacity(file.entries.len());
        let mut nodata_entry_color = None;
        for entry in &file.entries {
            let value = match (entry.value, nodata, range) {
                // gdal drops an nv entry when the raster has no nodata value
                (StopValue::NoData, None, _) => continue,
                (StopValue::NoData, Some(nodata), _) => {
                    nodata_entry_color = nodata_entry_color.or(Some(entry.color));
                    nodata
                }
                (StopValue::Percent(_), _, None) => {
                    return Err(ColorFileError::PercentWithoutRange);
                }
                (StopValue::Percent(percent), _, Some(range)) => {
                    range.min + percent / 100.0 * (range.max - range.min)
                }
                (StopValue::Value(value), _, _) => value,
            };
            if value.is_nan() {
                continue;
            }
            stops.push(ValueStop {
                value,
                color: entry.color,
            });
        }
        if stops.is_empty() {
            return Err(ColorFileError::NoEntries);
        }
        sort_stops(&mut stops);
        let separates_nodata = selection != ColorSelection::Exact;
        if let Some(nodata) = nodata.filter(|_| separates_nodata) {
            let neighbours = nodata_neighbours(&stops, nodata);
            stops.extend(neighbours);
            sort_stops(&mut stops);
        }
        let nodata_color = match (nodata_entry_color, nodata) {
            (Some(color), Some(nodata)) if nodata.is_nan() => color,
            (Some(_), Some(nodata)) => stop_color(&stops, selection, nodata),
            _ => TRANSPARENT,
        };
        Ok(ValueRamp {
            stops,
            selection,
            nodata_color,
        })
    }

    pub fn nodata_color(&self) -> Color {
        self.nodata_color
    }

    pub fn color(&self, value: f64) -> Color {
        if value.is_nan() {
            return self.nodata_color;
        }
        stop_color(&self.stops, self.selection, value)
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

    // gdaldem's documented example, its 0 line comma separated, plus a tab line and both comment forms
    const GDALDEM_EXAMPLE: &str = "\
# elevation colours
3500   white
2500   235:220:175
50%   190 185 135
700    240 250 150
/ lowlands
0,50,180,50
-100\t0\t0\t255\t128
nv     0   0   0   0
";

    fn ramp(text: &str, selection: ColorSelection, nodata: Option<f64>) -> ValueRamp {
        ValueRamp::new(&ColorFile::parse(text).unwrap(), selection, nodata, None).unwrap()
    }

    #[test]
    fn color_file_reads_the_gdaldem_example() {
        let file = ColorFile::parse(GDALDEM_EXAMPLE).unwrap();
        let entry = |value, color| ColorFileEntry { value, color };
        assert_eq!(
            file.entries,
            vec![
                entry(StopValue::Value(3500.0), Color::rgb(255, 255, 255)),
                entry(StopValue::Value(2500.0), Color::rgb(235, 220, 175)),
                entry(StopValue::Percent(50.0), Color::rgb(190, 185, 135)),
                entry(StopValue::Value(700.0), Color::rgb(240, 250, 150)),
                entry(StopValue::Value(0.0), Color::rgb(50, 180, 50)),
                entry(StopValue::Value(-100.0), Color::rgba(0, 0, 255, 128)),
                entry(StopValue::NoData, Color::rgba(0, 0, 0, 0)),
            ]
        );
        assert!(file.has_percent_stops());
    }

    #[test]
    fn color_file_takes_gdaldem_colour_names_with_an_alpha() {
        let file = ColorFile::parse("10 Brown 100\n20 indigo").unwrap();
        assert_eq!(file.entries[0].color, Color::rgba(191, 127, 63, 100));
        assert_eq!(file.entries[1].color, Color::rgb(0, 127, 255));
    }

    #[test]
    fn color_file_names_the_line_it_cannot_read() {
        let error = |text| ColorFile::parse(text).unwrap_err().to_string();
        assert!(error("0 0 0 0\n10 mauve").contains("line 2"));
        assert!(error("0 0 0 256").contains("256"));
        assert!(error("150% 0 0 0").contains("150%"));
        assert!(error("high 0 0 0").contains("high"));
        assert!(error("# COLOR_MODEL = RGB\n0 0 0 0 1 1 1 1").contains("GMT"));
        assert!(matches!(
            ColorFile::parse("# nothing\n"),
            Err(ColorFileError::NoEntries)
        ));
    }

    #[test]
    fn value_ramp_interpolates_between_stops_and_clamps_past_the_ends() {
        let ramp = ramp(
            "0 0 0 0\n100 200 100 50\n",
            ColorSelection::Interpolate,
            None,
        );
        assert_eq!(ramp.color(50.0), Color::rgb(100, 50, 25));
        // gdal rounds 12.5 up
        assert_eq!(ramp.color(25.0), Color::rgb(50, 25, 13));
        assert_eq!(ramp.color(100.0), Color::rgb(200, 100, 50));
        assert_eq!(ramp.color(-5.0), Color::rgb(0, 0, 0));
        assert_eq!(ramp.color(1e9), Color::rgb(200, 100, 50));
    }

    #[test]
    fn value_ramp_sorts_stops_from_any_file_order() {
        let ramp = ramp("100 white\n0 black\n", ColorSelection::Interpolate, None);
        assert_eq!(ramp.color(50.0), Color::rgb(128, 128, 128));
    }

    #[test]
    fn percent_stops_resolve_against_the_range_and_need_one() {
        let file = ColorFile::parse("0% black\n100% white\n").unwrap();
        let range = ValueRange {
            min: 100.0,
            max: 300.0,
        };
        let ramp = ValueRamp::new(&file, ColorSelection::Interpolate, None, Some(range)).unwrap();
        assert_eq!(ramp.color(100.0), Color::rgb(0, 0, 0));
        assert_eq!(ramp.color(200.0), Color::rgb(128, 128, 128));
        assert_eq!(ramp.color(300.0), Color::rgb(255, 255, 255));
        assert!(matches!(
            ValueRamp::new(&file, ColorSelection::Interpolate, None, None),
            Err(ColorFileError::PercentWithoutRange)
        ));
    }

    #[test]
    fn nodata_takes_the_nv_colour_and_is_transparent_without_one() {
        let with_nv = ramp(
            "0 black\n100 white\nnv 255 0 0\n",
            ColorSelection::Interpolate,
            Some(-9999.0),
        );
        assert_eq!(with_nv.nodata_color(), Color::rgb(255, 0, 0));
        assert_eq!(with_nv.color(f64::NAN), Color::rgb(255, 0, 0));
        let float_nodata = ramp(
            "0 black\nnv 0 255 0\n",
            ColorSelection::Interpolate,
            Some(f64::NAN),
        );
        assert_eq!(float_nodata.nodata_color(), Color::rgb(0, 255, 0));
        let without_nv = ramp(
            "0 black\n100 white\n",
            ColorSelection::Interpolate,
            Some(-9999.0),
        );
        assert_eq!(without_nv.nodata_color(), Color::rgba(0, 0, 0, 0));
        // gdal drops the nv entry of a raster with no nodata value
        let no_nodata = ramp(
            "0 black\n100 white\nnv 255 0 0\n",
            ColorSelection::Interpolate,
            None,
        );
        assert_eq!(no_nodata.nodata_color(), Color::rgba(0, 0, 0, 0));
        assert_eq!(no_nodata.color(50.0), Color::rgb(128, 128, 128));
    }

    #[test]
    fn values_beside_a_nodata_stop_keep_their_neighbours_colour() {
        let ramp = ramp(
            "0 black\nnv 255 0 0\n100 white\n",
            ColorSelection::Interpolate,
            Some(50.0),
        );
        assert_eq!(ramp.color(25.0), Color::rgb(0, 0, 0));
        assert_eq!(ramp.color(75.0), Color::rgb(255, 255, 255));
        assert_eq!(ramp.color(50.0), Color::rgb(255, 0, 0));
        assert_eq!(ramp.nodata_color(), Color::rgb(255, 0, 0));
    }

    #[test]
    fn exact_selection_colours_only_stop_values() {
        let ramp = ramp("1 red\n2 blue\n", ColorSelection::Exact, None);
        assert_eq!(ramp.color(1.0), Color::rgb(255, 0, 0));
        assert_eq!(ramp.color(2.0), Color::rgb(0, 0, 255));
        assert_eq!(ramp.color(1.5), Color::rgba(0, 0, 0, 0));
        assert_eq!(ramp.color(0.0), Color::rgba(0, 0, 0, 0));
        assert_eq!(ramp.color(3.0), Color::rgba(0, 0, 0, 0));
    }

    #[test]
    fn nearest_selection_takes_the_closer_stop_and_the_upper_one_on_a_tie() {
        let ramp = ramp("0 red\n10 blue\n", ColorSelection::Nearest, None);
        assert_eq!(ramp.color(4.0), Color::rgb(255, 0, 0));
        assert_eq!(ramp.color(6.0), Color::rgb(0, 0, 255));
        assert_eq!(ramp.color(5.0), Color::rgb(0, 0, 255));
        assert_eq!(ramp.color(-3.0), Color::rgb(255, 0, 0));
    }

    #[test]
    fn diverging_ramp() {
        let ramp = ColorRamp::diverging(5);
        // Middle should be white-ish (high R, G, B)
        let mid = ramp.colors[2];
        assert!(mid.r > 200);
    }
}
