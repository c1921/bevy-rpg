pub mod chaining;
pub mod marching;

use chaining::chain_segments;
use marching::{extract_segments, extract_segments_flat};
use crate::terrain::Terrain;

/// One contour level: all polylines at the same elevation.
pub struct ContourLevel {
    pub elevation: f64,
    pub polylines: Vec<Vec<[f64; 2]>>,
}

/// Extract contour polylines from the terrain height field.
///
/// * `grid_cols`, `grid_rows` – number of *cells* (samples = cols+1 × rows+1).
/// * `world_*` – axis-aligned rectangle in world space (metres).
/// * `interval` – vertical spacing between contour levels (metres).
#[allow(dead_code)]
pub fn marching_squares(
    terrain: &Terrain,
    world_x_min: f64,
    world_y_min: f64,
    world_x_max: f64,
    world_y_max: f64,
    grid_cols: usize,
    grid_rows: usize,
    interval: f64,
) -> Vec<ContourLevel> {
    let dx = (world_x_max - world_x_min) / grid_cols as f64;
    let dy = (world_y_max - world_y_min) / grid_rows as f64;

    // ── sample heights ──────────────────────────────────────────
    let rows = grid_rows + 1;
    let cols = grid_cols + 1;
    let mut heights: Vec<Vec<f64>> = Vec::with_capacity(rows);
    for r in 0..rows {
        let wy = world_y_min + r as f64 * dy;
        let mut row = Vec::with_capacity(cols);
        for c in 0..cols {
            let wx = world_x_min + c as f64 * dx;
            row.push(terrain.height(wx, wy));
        }
        heights.push(row);
    }

    marching_squares_from_heights(&heights, world_x_min, world_y_min, dx, dy, interval)
}

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

    let mut levels: Vec<ContourLevel> = Vec::new();
    let mut level = start_level;
    while level <= end_level + 1e-9 {
        let segments = extract_segments_flat(heights, cols, rows, world_x_min, world_y_min, dx, dy, level);
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
