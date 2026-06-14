//! Types for river network extraction.

/// Tunables for river extraction.
#[derive(Clone, Debug)]
pub struct RiverConfig {
    /// Minimum upstream drainage area (in cells) for a cell to count as river.
    pub accum_threshold: f64,
    /// Narrowest river, in world units.
    pub min_width: f32,
    /// Widest river, in world units.
    pub max_width: f32,
    /// Tiny slope added during depression filling so flat fills still drain.
    pub fill_epsilon: f64,
    /// Minimum height (in metres, same scale as the heightmap) for a cell
    /// to contribute rainfall.  Cells below this height produce zero runoff,
    /// so river sources are forced above this elevation.  Set to 0.0 to
    /// restore uniform rainfall.
    pub min_source_height: f64,
}

impl Default for RiverConfig {
    fn default() -> Self {
        Self {
            accum_threshold: 400.0,
            min_width: 18.0,
            max_width: 140.0,
            fill_epsilon: 1e-6,
            min_source_height: 300.0,
        }
    }
}

/// One drawable river piece in world coordinates.
#[derive(Clone, Copy)]
pub struct RiverSegment {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub width: f32,
}

/// Result of river extraction.
pub struct RiverNetwork {
    pub segments: Vec<RiverSegment>,
    /// Log‑normalised drainage accumulation in [0,1] (row‑major, row 0 = bottom),
    /// for an optional debug heatmap view.
    pub accum_field: Vec<f32>,
    #[allow(dead_code)]
    pub cols: usize,
    #[allow(dead_code)]
    pub rows: usize,
}

/// Per‑cell flow: up to two downstream neighbours with proportions.
/// `usize::MAX` marks "no neighbour" (edge outlet).
#[derive(Clone, Copy)]
pub(crate) struct Flow {
    pub n1: usize,
    pub n2: usize,
    pub p1: f64,
    pub p2: f64,
    /// Flow angle within the winning facet, [0, π/4]; 0 = all to n1.
    pub r: f64,
}

impl Default for Flow {
    fn default() -> Self {
        Flow { n1: usize::MAX, n2: usize::MAX, p1: 0.0, p2: 0.0, r: 0.0 }
    }
}

// Tarboton D‑infinity facet table. Each facet pairs a cardinal neighbour `1`
// with a diagonal neighbour `2`. (dr, dc) offsets; row r grows with world +y.
pub(crate) const DR1: [i64; 8] = [0, -1, -1, 0, 0, 1, 1, 0];
pub(crate) const DC1: [i64; 8] = [1, 0, 0, -1, -1, 0, 0, 1];
pub(crate) const DR2: [i64; 8] = [-1, -1, -1, -1, 1, 1, 1, 1];
pub(crate) const DC2: [i64; 8] = [1, 1, -1, -1, -1, -1, 1, 1];

pub(crate) const NEIGHBORS8: [(i64, i64); 8] = [
    (-1, -1), (-1, 0), (-1, 1),
    (0, -1),           (0, 1),
    (1, -1),  (1, 0),  (1, 1),
];
