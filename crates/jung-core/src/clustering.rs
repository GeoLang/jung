//! Point clustering: groups nearby points into clusters for cleaner visualization.
//!
//! Implements grid-based clustering and supercluster (hierarchical) for
//! efficient display at multiple zoom levels.

use crate::geometry::Point;

/// A cluster of points.
#[derive(Debug, Clone)]
pub struct Cluster {
    /// Centroid of the cluster.
    pub center: Point,
    /// Number of points in this cluster.
    pub count: usize,
    /// Indices of original points in this cluster.
    pub members: Vec<usize>,
    /// Optional aggregate value (sum/mean of a property).
    pub aggregate: Option<f64>,
}

/// Clustering parameters.
#[derive(Debug, Clone)]
pub struct ClusterParams {
    /// Clustering radius in map units.
    pub radius: f64,
    /// Minimum points to form a cluster (below this, show individual points).
    pub min_points: usize,
    /// Maximum zoom level at which to cluster (above this, show all points).
    pub max_zoom: f64,
}

impl Default for ClusterParams {
    fn default() -> Self {
        Self {
            radius: 0.05,
            min_points: 2,
            max_zoom: 18.0,
        }
    }
}

/// Grid-based spatial clustering (fast, O(n) with spatial hashing).
pub fn cluster_grid(points: &[Point], params: &ClusterParams) -> Vec<Cluster> {
    if points.is_empty() {
        return vec![];
    }

    let cell_size = params.radius * 2.0;
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();

    // Assign points to grid cells
    for (idx, pt) in points.iter().enumerate() {
        let cx = (pt.x / cell_size).floor() as i64;
        let cy = (pt.y / cell_size).floor() as i64;
        grid.entry((cx, cy)).or_default().push(idx);
    }

    let mut clusters: Vec<Cluster> = Vec::new();
    let mut clustered: Vec<bool> = vec![false; points.len()];

    for cell_points in grid.values() {
        if cell_points.len() >= params.min_points {
            // Form a cluster
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            let mut members = Vec::new();

            for &idx in cell_points {
                if !clustered[idx] {
                    sum_x += points[idx].x;
                    sum_y += points[idx].y;
                    members.push(idx);
                    clustered[idx] = true;
                }
            }

            if !members.is_empty() {
                let count = members.len();
                clusters.push(Cluster {
                    center: Point {
                        x: sum_x / count as f64,
                        y: sum_y / count as f64,
                    },
                    count,
                    members,
                    aggregate: None,
                });
            }
        }
    }

    // Remaining unclustered points become singleton clusters
    for (idx, pt) in points.iter().enumerate() {
        if !clustered[idx] {
            clusters.push(Cluster {
                center: *pt,
                count: 1,
                members: vec![idx],
                aggregate: None,
            });
        }
    }

    clusters
}

/// Hierarchical clustering (supercluster): clusters at multiple zoom levels.
pub fn cluster_hierarchical(
    points: &[Point],
    min_zoom: u8,
    max_zoom: u8,
    radius: f64,
) -> Vec<Vec<Cluster>> {
    let mut levels: Vec<Vec<Cluster>> = Vec::new();

    for zoom in min_zoom..=max_zoom {
        // Radius decreases as zoom increases (points spread out)
        let zoom_radius = radius / 2.0f64.powi(zoom as i32);
        let params = ClusterParams {
            radius: zoom_radius,
            min_points: 2,
            max_zoom: max_zoom as f64,
        };
        levels.push(cluster_grid(points, &params));
    }

    levels
}

/// Distance-based clustering (DBSCAN-like).
pub fn cluster_dbscan(points: &[Point], eps: f64, min_points: usize) -> Vec<Cluster> {
    let n = points.len();
    let mut labels: Vec<Option<usize>> = vec![None; n];
    let mut cluster_id = 0;

    for i in 0..n {
        if labels[i].is_some() {
            continue;
        }

        let neighbors = range_query(points, i, eps);
        if neighbors.len() < min_points {
            continue; // noise point
        }

        // Start new cluster
        labels[i] = Some(cluster_id);
        let mut queue: Vec<usize> = neighbors.clone();
        let mut qi = 0;

        while qi < queue.len() {
            let j = queue[qi];
            qi += 1;

            if labels[j].is_some() {
                continue;
            }
            labels[j] = Some(cluster_id);

            let j_neighbors = range_query(points, j, eps);
            if j_neighbors.len() >= min_points {
                for &nb in &j_neighbors {
                    if labels[nb].is_none() && !queue.contains(&nb) {
                        queue.push(nb);
                    }
                }
            }
        }

        cluster_id += 1;
    }

    // Build cluster objects
    let mut clusters: Vec<Cluster> = (0..cluster_id)
        .map(|_| Cluster {
            center: Point { x: 0.0, y: 0.0 },
            count: 0,
            members: vec![],
            aggregate: None,
        })
        .collect();

    for (idx, label) in labels.iter().enumerate() {
        if let Some(cid) = label {
            clusters[*cid].members.push(idx);
            clusters[*cid].count += 1;
            clusters[*cid].center.x += points[idx].x;
            clusters[*cid].center.y += points[idx].y;
        }
    }

    // Compute centroids
    for cluster in &mut clusters {
        if cluster.count > 0 {
            cluster.center.x /= cluster.count as f64;
            cluster.center.y /= cluster.count as f64;
        }
    }

    // Add noise points as singletons
    for (idx, label) in labels.iter().enumerate() {
        if label.is_none() {
            clusters.push(Cluster {
                center: points[idx],
                count: 1,
                members: vec![idx],
                aggregate: None,
            });
        }
    }

    clusters
}

fn range_query(points: &[Point], idx: usize, eps: f64) -> Vec<usize> {
    let p = &points[idx];
    let eps2 = eps * eps;
    points
        .iter()
        .enumerate()
        .filter(|(i, q)| {
            *i != idx && {
                let dx = q.x - p.x;
                let dy = q.y - p.y;
                dx * dx + dy * dy <= eps2
            }
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_cluster_tight_group() {
        let points = vec![
            Point { x: 0.1, y: 0.1 },
            Point { x: 0.11, y: 0.11 },
            Point { x: 0.12, y: 0.1 },
            Point { x: 0.9, y: 0.9 }, // far away
        ];
        let params = ClusterParams {
            radius: 0.05,
            min_points: 2,
            max_zoom: 18.0,
        };
        let clusters = cluster_grid(&points, &params);
        // Should have 2 clusters: one with 3 points, one singleton
        let big = clusters.iter().find(|c| c.count >= 3);
        assert!(big.is_some());
        assert_eq!(clusters.iter().map(|c| c.count).sum::<usize>(), 4);
    }

    #[test]
    fn empty_points() {
        let clusters = cluster_grid(&[], &ClusterParams::default());
        assert!(clusters.is_empty());
    }

    #[test]
    fn all_far_apart() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 0.5, y: 0.5 },
            Point { x: 1.0, y: 1.0 },
        ];
        let params = ClusterParams {
            radius: 0.01,
            min_points: 2,
            max_zoom: 18.0,
        };
        let clusters = cluster_grid(&points, &params);
        // All singletons
        assert_eq!(clusters.len(), 3);
        assert!(clusters.iter().all(|c| c.count == 1));
    }

    #[test]
    fn hierarchical_zoom_levels() {
        let points: Vec<Point> = (0..20)
            .map(|i| Point {
                x: (i as f64 * 0.01) % 0.5,
                y: (i as f64 * 0.02) % 0.5,
            })
            .collect();
        let levels = cluster_hierarchical(&points, 0, 5, 0.1);
        assert_eq!(levels.len(), 6); // zoom 0..5 inclusive
        // Lower zoom should have fewer clusters
        assert!(levels[0].len() <= levels[5].len());
    }

    #[test]
    fn dbscan_two_clusters() {
        let mut points = Vec::new();
        // Cluster 1 around (0.1, 0.1)
        for i in 0..5 {
            points.push(Point {
                x: 0.1 + i as f64 * 0.01,
                y: 0.1 + i as f64 * 0.01,
            });
        }
        // Cluster 2 around (0.9, 0.9)
        for i in 0..5 {
            points.push(Point {
                x: 0.9 + i as f64 * 0.01,
                y: 0.9 + i as f64 * 0.01,
            });
        }
        let clusters = cluster_dbscan(&points, 0.05, 3);
        let real_clusters: Vec<_> = clusters.iter().filter(|c| c.count >= 3).collect();
        assert_eq!(real_clusters.len(), 2);
    }

    #[test]
    fn cluster_centroid() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 1.0, y: 0.0 },
            Point { x: 0.5, y: 1.0 },
        ];
        let clusters = cluster_dbscan(&points, 2.0, 2);
        let big = clusters.iter().find(|c| c.count == 3).unwrap();
        assert!((big.center.x - 0.5).abs() < 0.01);
        assert!((big.center.y - 0.333).abs() < 0.01);
    }
}
