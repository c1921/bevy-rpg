//! River network extraction from an eroded heightmap.
//!
//! Pipeline (all on the cropped, [0,1]‑normalised visible grid):
//!   [A] Priority‑Flood depression filling (Barnes 2014, +epsilon for slope)
//!   [B] D‑infinity flow directions (Tarboton 1997)
//!   [C] Flow accumulation (uniform rain, descending‑height topological order)
//!   [D] Network extraction → world‑space polyline segments with width
//!
//! Map edges are the only outlet: flow that points off‑grid simply leaves.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

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
}

impl Default for RiverConfig {
    fn default() -> Self {
        Self {
            accum_threshold: 400.0,
            min_width: 18.0,
            max_width: 140.0,
            fill_epsilon: 1e-6,
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

// Tarboton D‑infinity facet table. Each facet pairs a cardinal neighbour `1`
// with a diagonal neighbour `2`. (dr, dc) offsets; row r grows with world +y.
const DR1: [i64; 8] = [0, -1, -1, 0, 0, 1, 1, 0];
const DC1: [i64; 8] = [1, 0, 0, -1, -1, 0, 0, 1];
const DR2: [i64; 8] = [-1, -1, -1, -1, 1, 1, 1, 1];
const DC2: [i64; 8] = [1, 1, -1, -1, -1, -1, 1, 1];

/// Extract the river network from a flat [0,1] heightmap.
///
/// * `heights` — `cols * rows` row‑major, row 0 = bottom of world.
/// * `world_min` — world coordinate of cell (0,0).
/// * `dx`, `dy` — cell size in world units.
pub fn extract(
    heights: &[f64],
    cols: usize,
    rows: usize,
    world_min_x: f64,
    world_min_y: f64,
    dx: f64,
    dy: f64,
    cfg: &RiverConfig,
) -> RiverNetwork {
    let n = cols * rows;
    debug_assert_eq!(heights.len(), n);

    let filled = priority_flood(heights, cols, rows, cfg.fill_epsilon);
    let flow = flow_directions(&filled, cols, rows, dx);
    let accum = accumulate(&filled, &flow, cols, rows);

    // ── [D] Build segments from each river cell to its primary downstream. ──
    let quarter = std::f64::consts::FRAC_PI_4;
    let max_acc = accum.iter().copied().fold(1.0_f64, f64::max);
    let ln_lo = cfg.accum_threshold.max(1.0).ln();
    let ln_hi = max_acc.max(cfg.accum_threshold + 1.0).ln();
    let ln_range = (ln_hi - ln_lo).max(1e-9);

    let center = |idx: usize| -> [f64; 2] {
        let r = idx / cols;
        let c = idx % cols;
        [world_min_x + c as f64 * dx, world_min_y + r as f64 * dy]
    };

    let mut segments = Vec::new();
    for idx in 0..n {
        if accum[idx] < cfg.accum_threshold {
            continue;
        }
        let f = &flow[idx];
        // Primary downstream = neighbour carrying the larger share.
        let down = if f.n1 != usize::MAX && (f.n2 == usize::MAX || f.r <= quarter * 0.5) {
            f.n1
        } else if f.n2 != usize::MAX {
            f.n2
        } else {
            f.n1
        };
        if down == usize::MAX {
            continue; // edge outlet — flow leaves the map
        }
        let t = ((accum[idx].ln() - ln_lo) / ln_range).clamp(0.0, 1.0);
        let width = cfg.min_width + (cfg.max_width - cfg.min_width) * t as f32;
        segments.push(RiverSegment {
            a: center(idx),
            b: center(down),
            width,
        });
    }

    // Debug field: log‑normalised accumulation.
    let accum_field: Vec<f32> = accum
        .iter()
        .map(|&a| ((a.max(1.0).ln()) / ln_hi.max(1e-9)).clamp(0.0, 1.0) as f32)
        .collect();

    RiverNetwork {
        segments,
        accum_field,
        cols,
        rows,
    }
}

// ── [A] Priority‑Flood (+epsilon) ───────────────────────────────────────

/// Min‑heap entry ordered by height ascending.
#[derive(Copy, Clone)]
struct HeapItem {
    h: f64,
    idx: usize,
}
impl PartialEq for HeapItem {
    fn eq(&self, o: &Self) -> bool {
        self.h == o.h
    }
}
impl Eq for HeapItem {}
impl PartialOrd for HeapItem {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for HeapItem {
    fn cmp(&self, o: &Self) -> Ordering {
        // Reverse for min‑heap on BinaryHeap (which is a max‑heap).
        o.h.partial_cmp(&self.h).unwrap_or(Ordering::Equal)
    }
}

fn priority_flood(heights: &[f64], cols: usize, rows: usize, epsilon: f64) -> Vec<f64> {
    let n = cols * rows;
    let mut filled = heights.to_vec();
    let mut closed = vec![false; n];
    let mut pq: BinaryHeap<HeapItem> = BinaryHeap::new();

    // Seed the queue with every border cell — these drain off the map.
    let push_border = |r: usize, c: usize, pq: &mut BinaryHeap<HeapItem>, closed: &mut Vec<bool>| {
        let idx = r * cols + c;
        if !closed[idx] {
            closed[idx] = true;
            pq.push(HeapItem { h: filled[idx], idx });
        }
    };
    for c in 0..cols {
        push_border(0, c, &mut pq, &mut closed);
        push_border(rows - 1, c, &mut pq, &mut closed);
    }
    for r in 0..rows {
        push_border(r, 0, &mut pq, &mut closed);
        push_border(r, cols - 1, &mut pq, &mut closed);
    }

    while let Some(HeapItem { h, idx }) = pq.pop() {
        let r = (idx / cols) as i64;
        let c = (idx % cols) as i64;
        for (dr, dc) in NEIGHBORS8 {
            let nr = r + dr;
            let nc = c + dc;
            if nr < 0 || nc < 0 || nr >= rows as i64 || nc >= cols as i64 {
                continue;
            }
            let nidx = nr as usize * cols + nc as usize;
            if closed[nidx] {
                continue;
            }
            closed[nidx] = true;
            // Raise to at least current level + epsilon so flats still drain.
            let raised = (h + epsilon).max(filled[nidx]);
            filled[nidx] = raised;
            pq.push(HeapItem { h: raised, idx: nidx });
        }
    }
    filled
}

const NEIGHBORS8: [(i64, i64); 8] = [
    (-1, -1), (-1, 0), (-1, 1),
    (0, -1),           (0, 1),
    (1, -1),  (1, 0),  (1, 1),
];

// ── [B] D‑infinity flow directions ──────────────────────────────────────

/// Per‑cell flow: up to two downstream neighbours with proportions.
/// `usize::MAX` marks "no neighbour" (edge outlet).
#[derive(Clone, Copy)]
struct Flow {
    n1: usize,
    n2: usize,
    p1: f64,
    p2: f64,
    /// Flow angle within the winning facet, [0, π/4]; 0 = all to n1.
    r: f64,
}

impl Default for Flow {
    fn default() -> Self {
        Flow { n1: usize::MAX, n2: usize::MAX, p1: 0.0, p2: 0.0, r: 0.0 }
    }
}

fn flow_directions(filled: &[f64], cols: usize, rows: usize, d: f64) -> Vec<Flow> {
    let n = cols * rows;
    let quarter = std::f64::consts::FRAC_PI_4;
    let diag = d * std::f64::consts::SQRT_2;
    let mut flow = vec![Flow::default(); n];

    for idx in 0..n {
        let r = (idx / cols) as i64;
        let c = (idx % cols) as i64;
        let e0 = filled[idx];

        let mut best_s = 0.0_f64;
        let mut best = Flow::default();

        for f in 0..8 {
            let r1 = r + DR1[f];
            let c1 = c + DC1[f];
            let r2 = r + DR2[f];
            let c2 = c + DC2[f];
            if r1 < 0 || c1 < 0 || r1 >= rows as i64 || c1 >= cols as i64 {
                continue;
            }
            if r2 < 0 || c2 < 0 || r2 >= rows as i64 || c2 >= cols as i64 {
                continue;
            }
            let i1 = r1 as usize * cols + c1 as usize;
            let i2 = r2 as usize * cols + c2 as usize;
            let e1 = filled[i1];
            let e2 = filled[i2];

            let s1 = (e0 - e1) / d;
            let s2 = (e1 - e2) / d;
            let mut ang = s2.atan2(s1);
            let mut s = (s1 * s1 + s2 * s2).sqrt();
            if ang < 0.0 {
                ang = 0.0;
                s = s1;
            } else if ang > quarter {
                ang = quarter;
                s = (e0 - e2) / diag;
            }
            if s > best_s {
                best_s = s;
                let p2 = ang / quarter;
                best = Flow {
                    n1: i1,
                    n2: i2,
                    p1: 1.0 - p2,
                    p2,
                    r: ang,
                };
            }
        }

        if best_s > 0.0 {
            flow[idx] = best;
        } else {
            // Flat / pit after fill: fall to the single lowest neighbour (D8).
            let mut lo = e0;
            let mut lo_idx = usize::MAX;
            for (dr, dc) in NEIGHBORS8 {
                let nr = r + dr;
                let nc = c + dc;
                if nr < 0 || nc < 0 || nr >= rows as i64 || nc >= cols as i64 {
                    continue;
                }
                let nidx = nr as usize * cols + nc as usize;
                if filled[nidx] < lo {
                    lo = filled[nidx];
                    lo_idx = nidx;
                }
            }
            flow[idx] = Flow { n1: lo_idx, n2: usize::MAX, p1: 1.0, p2: 0.0, r: 0.0 };
        }
    }
    flow
}

// ── [C] Flow accumulation ───────────────────────────────────────────────

fn accumulate(filled: &[f64], flow: &[Flow], cols: usize, rows: usize) -> Vec<f64> {
    let n = cols * rows;
    let mut accum = vec![1.0_f64; n]; // uniform rain: each cell contributes 1

    // Topological order = descending filled height (flow only goes downhill).
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_unstable_by(|&a, &b| {
        filled[b].partial_cmp(&filled[a]).unwrap_or(Ordering::Equal)
    });

    for &idx in &order {
        let a = accum[idx];
        let f = &flow[idx];
        if f.n1 != usize::MAX {
            accum[f.n1] += a * f.p1;
        }
        if f.n2 != usize::MAX {
            accum[f.n2] += a * f.p2;
        }
    }
    accum
}
