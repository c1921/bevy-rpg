pub mod chaining;
pub mod marching;

use chaining::chain_segments;
use marching::{extract_segments, extract_segments_flat};

/// One contour level: all polylines at the same elevation.
pub struct ContourLevel {
    pub elevation: f64,
    pub polylines: Vec<Vec<[f64; 2]>>,
}

/// Extract contour polylines from the terrain height field.
///
/// Extract contour polylines from a pre‑sampled 2-D height grid.
///
/// `heights[r][c]` is the elevation at row `r`, column `c`.
/// `dx`, `dy` are the cell sizes in world units.
pub fn marching_squares_from_heights(
    heights: &[Vec<f64>],
    world_x_min: f64,
    world_y_min: f64,
    dx: f64,
    dy: f64,
    interval: f64,
) -> Vec<ContourLevel> {
    // ── elevation range ─────────────────────────────────────────
    let mut min_h = f64::MAX;
    let mut max_h = f64::MIN;
    for row in heights {
        for &h in row {
            min_h = min_h.min(h);
            max_h = max_h.max(h);
        }
    }

    let start_level = (min_h / interval).ceil() * interval;
    let end_level = (max_h / interval).floor() * interval;
    if start_level > end_level {
        return Vec::new();
    }

    let mut levels: Vec<ContourLevel> = Vec::new();
    let mut level = start_level;
    while level <= end_level + 1e-9 {
        let segments = extract_segments(heights, world_x_min, world_y_min, dx, dy, level);
        if !segments.is_empty() {
            let polylines = chain_segments(&segments);
            levels.push(ContourLevel {
                elevation: level,
                polylines,
            });
        }
        level += interval;
    }

    levels
}

/// Extract contour polylines from a flat height slice (row-major).
///
/// `heights.len() == cols * rows`.  Avoids the `Vec<Vec<f64>>` conversion.
/// Levels are processed in parallel via rayon.
pub fn marching_squares_from_flat(
    heights: &[f64],
    cols: usize,
    rows: usize,
    world_x_min: f64,
    world_y_min: f64,
    dx: f64,
    dy: f64,
    interval: f64,
) -> Vec<ContourLevel> {
    let mut min_h = f64::MAX;
    let mut max_h = f64::MIN;
    for &h in heights {
        min_h = min_h.min(h);
        max_h = max_h.max(h);
    }

    let start_level = (min_h / interval).ceil() * interval;
    let end_level = (max_h / interval).floor() * interval;
    if start_level > end_level {
        return Vec::new();
    }

    // Collect level elevations.
    let mut elevations: Vec<f64> = Vec::new();
    let mut level = start_level;
    while level <= end_level + 1e-9 {
        elevations.push(level);
        level += interval;
    }

    // Process each level in parallel.
    use rayon::prelude::*;
    elevations
        .par_iter()
        .filter_map(|&elev| {
            let segments =
                extract_segments_flat(heights, cols, rows, world_x_min, world_y_min, dx, dy, elev);
            if segments.is_empty() {
                None
            } else {
                let polylines = chain_segments(&segments);
                Some(ContourLevel {
                    elevation: elev,
                    polylines,
                })
            }
        })
        .collect()
}
