//! Temporal animation: time-based feature filtering and interpolation.
//!
//! Supports filtering features by a time window, interpolating positions
//! along trajectories, and generating animation keyframes.

use crate::geometry::{Feature, Point};
use jung_style::PropertyValue;

/// A time range for temporal queries.
#[derive(Debug, Clone, Copy)]
pub struct TimeRange {
    pub start: f64,
    pub end: f64,
}

impl TimeRange {
    pub fn new(start: f64, end: f64) -> Self {
        Self { start, end }
    }

    pub fn contains(&self, t: f64) -> bool {
        t >= self.start && t <= self.end
    }

    pub fn duration(&self) -> f64 {
        self.end - self.start
    }
}

/// Configuration for temporal animation.
#[derive(Debug, Clone)]
pub struct TemporalConfig {
    /// The property name containing the timestamp.
    pub time_field: String,
    /// Total animation duration range.
    pub time_range: TimeRange,
    /// Duration of the visible window (trail length).
    pub window_duration: f64,
    /// If true, features fade out as they age within the window.
    pub fade: bool,
    /// If true, interpolate position between timestamps.
    pub interpolate: bool,
}

/// A keyframe in an animation sequence.
#[derive(Debug, Clone)]
pub struct Keyframe {
    /// Normalized time (0.0..1.0) within the animation.
    pub t: f64,
    /// Features visible at this time.
    pub features: Vec<TemporalFeature>,
}

/// A feature with temporal metadata.
#[derive(Debug, Clone)]
pub struct TemporalFeature {
    /// Reference to the original feature index.
    pub feature_idx: usize,
    /// Opacity based on age within the time window (1.0 = newest, 0.0 = oldest).
    pub opacity: f64,
    /// Interpolated position (if trajectory interpolation is enabled).
    pub position: Option<Point>,
}

/// Filter features that are visible within a given time window.
pub fn filter_by_time(
    features: &[Feature],
    config: &TemporalConfig,
    current_time: f64,
) -> Vec<TemporalFeature> {
    let window_start = current_time - config.window_duration;
    let window_end = current_time;

    features
        .iter()
        .enumerate()
        .filter_map(|(idx, feature)| {
            let t = get_time_value(feature, &config.time_field)?;
            if t >= window_start && t <= window_end {
                let age = (window_end - t) / config.window_duration;
                let opacity = if config.fade { 1.0 - age } else { 1.0 };
                Some(TemporalFeature {
                    feature_idx: idx,
                    opacity,
                    position: None,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Generate keyframes for an animation.
pub fn generate_keyframes(
    features: &[Feature],
    config: &TemporalConfig,
    num_frames: usize,
) -> Vec<Keyframe> {
    let duration = config.time_range.duration();
    (0..num_frames)
        .map(|i| {
            let t = i as f64 / (num_frames - 1).max(1) as f64;
            let current_time = config.time_range.start + t * duration;
            let visible = filter_by_time(features, config, current_time);
            Keyframe {
                t,
                features: visible,
            }
        })
        .collect()
}

/// Interpolate position along a trajectory (sequence of timestamped points).
pub fn interpolate_trajectory(
    points: &[(f64, Point)], // (timestamp, position) pairs
    t: f64,
) -> Option<Point> {
    if points.is_empty() {
        return None;
    }
    if points.len() == 1 {
        return Some(points[0].1);
    }
    if t <= points[0].0 {
        return Some(points[0].1);
    }
    if t >= points[points.len() - 1].0 {
        return Some(points[points.len() - 1].1);
    }

    // Find bracketing pair
    for window in points.windows(2) {
        let (t0, p0) = window[0];
        let (t1, p1) = window[1];
        if t >= t0 && t <= t1 {
            let frac = if (t1 - t0).abs() < f64::EPSILON {
                0.0
            } else {
                (t - t0) / (t1 - t0)
            };
            return Some(Point {
                x: p0.x + (p1.x - p0.x) * frac,
                y: p0.y + (p1.y - p0.y) * frac,
            });
        }
    }
    None
}

/// Extract a numeric time value from a feature's properties.
fn get_time_value(feature: &Feature, field: &str) -> Option<f64> {
    match feature.properties.get(field)? {
        PropertyValue::Number(n) => Some(*n),
        PropertyValue::Integer(n) => Some(*n as f64),
        PropertyValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// Easing functions for animation timing.
#[derive(Debug, Clone, Copy)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Easing {
    pub fn apply(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_timed_feature(time: f64) -> Feature {
        let mut props = HashMap::new();
        props.insert("timestamp".to_string(), PropertyValue::Number(time));
        Feature {
            geometry: crate::geometry::Geometry::Point(Point { x: 0.5, y: 0.5 }),
            properties: props,
        }
    }

    #[test]
    fn filter_features_in_window() {
        let features: Vec<Feature> = (0..10).map(|i| make_timed_feature(i as f64)).collect();
        let config = TemporalConfig {
            time_field: "timestamp".to_string(),
            time_range: TimeRange::new(0.0, 9.0),
            window_duration: 3.0,
            fade: false,
            interpolate: false,
        };
        let visible = filter_by_time(&features, &config, 5.0);
        // Window: [2.0, 5.0] → features at t=2,3,4,5
        assert_eq!(visible.len(), 4);
    }

    #[test]
    fn fade_opacity() {
        let features: Vec<Feature> = (0..10).map(|i| make_timed_feature(i as f64)).collect();
        let config = TemporalConfig {
            time_field: "timestamp".to_string(),
            time_range: TimeRange::new(0.0, 9.0),
            window_duration: 4.0,
            fade: true,
            interpolate: false,
        };
        let visible = filter_by_time(&features, &config, 6.0);
        // Window: [2.0, 6.0] → features at t=2,3,4,5,6
        // t=6 (newest) should have opacity ≈ 1.0
        // t=2 (oldest) should have opacity ≈ 0.0
        let newest = visible.iter().find(|f| f.feature_idx == 6).unwrap();
        assert!((newest.opacity - 1.0).abs() < 0.01);
        let oldest = visible.iter().find(|f| f.feature_idx == 2).unwrap();
        assert!(oldest.opacity < 0.01);
    }

    #[test]
    fn generate_animation_keyframes() {
        let features: Vec<Feature> = (0..5).map(|i| make_timed_feature(i as f64)).collect();
        let config = TemporalConfig {
            time_field: "timestamp".to_string(),
            time_range: TimeRange::new(0.0, 4.0),
            window_duration: 2.0,
            fade: false,
            interpolate: false,
        };
        let keyframes = generate_keyframes(&features, &config, 5);
        assert_eq!(keyframes.len(), 5);
        assert!((keyframes[0].t - 0.0).abs() < 0.01);
        assert!((keyframes[4].t - 1.0).abs() < 0.01);
    }

    #[test]
    fn interpolate_trajectory_midpoint() {
        let trajectory = vec![
            (0.0, Point { x: 0.0, y: 0.0 }),
            (1.0, Point { x: 1.0, y: 1.0 }),
            (2.0, Point { x: 2.0, y: 0.0 }),
        ];
        let p = interpolate_trajectory(&trajectory, 0.5).unwrap();
        assert!((p.x - 0.5).abs() < 0.01);
        assert!((p.y - 0.5).abs() < 0.01);

        let p2 = interpolate_trajectory(&trajectory, 1.5).unwrap();
        assert!((p2.x - 1.5).abs() < 0.01);
        assert!((p2.y - 0.5).abs() < 0.01);
    }

    #[test]
    fn easing_functions() {
        assert!((Easing::Linear.apply(0.5) - 0.5).abs() < 0.01);
        assert!(Easing::EaseIn.apply(0.5) < 0.5); // slower start
        assert!(Easing::EaseOut.apply(0.5) > 0.5); // faster start
        assert!((Easing::EaseInOut.apply(0.0) - 0.0).abs() < 0.01);
        assert!((Easing::EaseInOut.apply(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn time_range_contains() {
        let range = TimeRange::new(1.0, 5.0);
        assert!(range.contains(3.0));
        assert!(!range.contains(0.5));
        assert!(!range.contains(5.5));
        assert!((range.duration() - 4.0).abs() < 0.01);
    }
}
