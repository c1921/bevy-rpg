use crate::terrain::Terrain;

/// One contour level: all polylines at the same elevation.
pub struct ContourLevel {
    pub elevation: f64,
    pub polylines: Vec<Vec<[f64; 2]>>,
}

/// Marching-squares lookup table.
///
/// Each entry is up to 2 line segments, each encoded as `(edge_a, edge_b)`.
/// Edges: 0 = top, 1 = right, 2 = bottom, 3 = left.
/// A sentinel value of `4` means "no more lines".
const MS_TABLE: [[u8; 5]; 16] = [
    // case  0: 0000
    [4, 4, 4, 4, 4],
    // case  1: 0001  corner 0 above
    [0, 3, 4, 4, 4],
    // case  2: 0010  corner 1 above
    [0, 1, 4, 4, 4],
    // case  3: 0011  corners 0,1 above
    [1, 3, 4, 4, 4],
    // case  4: 0100  corner 2 above
    [1, 2, 4, 4, 4],
    // case  5: 0101  corners 0,2 above — saddle
    [0, 3, 1, 2, 4],
    // case  6: 0110  corners 1,2 above
    [0, 2, 4, 4, 4],
    // case  7: 0111  corners 0,1,2 above
    [2, 3, 4, 4, 4],
    // case  8: 1000  corner 3 above
    [2, 3, 4, 4, 4],
    // case  9: 1001  corners 0,3 above
    [0, 2, 4, 4, 4],
    // case 10: 1010  corners 1,3 above — saddle
    [0, 1, 2, 3, 4],
    // case 11: 1011  corners 0,1,3 above
    [1, 2, 4, 4, 4],
    // case 12: 1100  corners 2,3 above
    [1, 3, 4, 4, 4],
    // case 13: 1101  corners 0,2,3 above
    [0, 1, 4, 4, 4],
    // case 14: 1110  corners 1,2,3 above
    [0, 3, 4, 4, 4],
    // case 15: 1111
    [4, 4, 4, 4, 4],
];

/// Extract contour polylines from the terrain height field.
///
/// * `grid_cols`, `grid_rows` – number of *cells* (samples = cols+1 × rows+1).
/// * `world_*` – axis-aligned rectangle in world space (metres).
/// * `interval` – vertical spacing between contour levels (metres).
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

    // ── elevation range ─────────────────────────────────────────
    let mut min_h = f64::MAX;
    let mut max_h = f64::MIN;
    for row in &heights {
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
        let segments = extract_segments(&heights, world_x_min, world_y_min, dx, dy, level);
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

// ── helpers ─────────────────────────────────────────────────────

/// 2-D point equality with f64 epsilon.
fn pt_eq(a: &[f64; 2], b: &[f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

/// Linearly interpolate on an edge.
fn edge_pos(
    heights: &[Vec<f64>],
    r0: usize,
    c0: usize,
    r1: usize,
    c1: usize,
    world_x_min: f64,
    world_y_min: f64,
    dx: f64,
    dy: f64,
    threshold: f64,
) -> [f64; 2] {
    let h0 = heights[r0][c0];
    let h1 = heights[r1][c1];
    let t = if (h1 - h0).abs() > 1e-12 {
        ((threshold - h0) / (h1 - h0)).clamp(0.0, 1.0)
    } else {
        0.5
    };

    let x0 = world_x_min + c0 as f64 * dx;
    let y0 = world_y_min + r0 as f64 * dy;
    let x1 = world_x_min + c1 as f64 * dx;
    let y1 = world_y_min + r1 as f64 * dy;

    [x0 + t * (x1 - x0), y0 + t * (y1 - y0)]
}

/// Run marching squares for a single threshold; return raw segments.
fn extract_segments(
    heights: &[Vec<f64>],
    world_x_min: f64,
    world_y_min: f64,
    dx: f64,
    dy: f64,
    threshold: f64,
) -> Vec<([f64; 2], [f64; 2])> {
    let rows = heights.len();
    let cols = heights[0].len();
    let grid_rows = rows - 1;
    let grid_cols = cols - 1;
    let mut segs = Vec::new();

    for r in 0..grid_rows {
        for c in 0..grid_cols {
            // corner heights
            let h = [
                heights[r][c],         // 0  top-left
                heights[r][c + 1],     // 1  top-right
                heights[r + 1][c + 1], // 2  bottom-right
                heights[r + 1][c],     // 3  bottom-left
            ];

            // 4-bit case index
            let mut case = 0u8;
            for i in 0..4 {
                if h[i] >= threshold {
                    case |= 1 << i;
                }
            }

            let entry = MS_TABLE[case as usize];
            // up to two segments per cell
            for pair in entry.chunks(2) {
                if pair[0] == 4 {
                    break;
                }
                let e0 = pair[0] as usize;
                let e1 = pair[1] as usize;

                let p0 = edge_point(heights, r, c, e0, world_x_min, world_y_min, dx, dy, threshold);
                let p1 = edge_point(heights, r, c, e1, world_x_min, world_y_min, dx, dy, threshold);

                // avoid degenerate segments
                if !pt_eq(&p0, &p1) {
                    segs.push((p0, p1));
                }
            }
        }
    }

    segs
}

/// Return the interpolated point on edge `e` of cell `(r, c)`.
fn edge_point(
    heights: &[Vec<f64>],
    r: usize,
    c: usize,
    e: usize,
    world_x_min: f64,
    world_y_min: f64,
    dx: f64,
    dy: f64,
    threshold: f64,
) -> [f64; 2] {
    match e {
        0 => edge_pos(heights, r, c, r, c + 1, world_x_min, world_y_min, dx, dy, threshold),
        1 => edge_pos(heights, r, c + 1, r + 1, c + 1, world_x_min, world_y_min, dx, dy, threshold),
        2 => edge_pos(heights, r + 1, c, r + 1, c + 1, world_x_min, world_y_min, dx, dy, threshold),
        3 => edge_pos(heights, r, c, r + 1, c, world_x_min, world_y_min, dx, dy, threshold),
        _ => unreachable!(),
    }
}

// ── polyline chaining ───────────────────────────────────────────

/// Stitch a set of unordered segments into connected polylines.
fn chain_segments(segments: &[([f64; 2], [f64; 2])]) -> Vec<Vec<[f64; 2]>> {
    if segments.is_empty() {
        return Vec::new();
    }

    // Build adjacency: for each quantised key, list of (neighbour, seg_idx).
    // We quantise to avoid fp mismatch on shared edges.
    let n = segments.len();
    let mut adj: std::collections::HashMap<Key, Vec<(Key, usize)>> =
        std::collections::HashMap::with_capacity(n * 2);

    for (idx, &(a, b)) in segments.iter().enumerate() {
        let ka = key(a);
        let kb = key(b);
        adj.entry(ka).or_default().push((kb, idx));
        adj.entry(kb).or_default().push((ka, idx));
    }

    let mut used = vec![false; n];
    let mut result = Vec::new();

    for start_idx in 0..n {
        if used[start_idx] {
            continue;
        }
        used[start_idx] = true;

        let (mut head, mut tail) = segments[start_idx];

        // start the polyline
        let mut poly = vec![head, tail];

        // extend forward from `tail`
        loop {
            let k = key(tail);
            if let Some(neighbors) = adj.get(&k) {
                let mut extended = false;
                for &(_nk, ni) in neighbors {
                    if !used[ni] {
                        used[ni] = true;
                        let seg = segments[ni];
                        // which end connects?
                        if key(seg.0) == k {
                            tail = seg.1;
                        } else {
                            tail = seg.0;
                        }
                        poly.push(tail);
                        extended = true;
                        break;
                    }
                }
                if !extended {
                    break;
                }
            } else {
                break;
            }
        }

        // extend backward from `head`
        loop {
            let k = key(head);
            if let Some(neighbors) = adj.get(&k) {
                let mut extended = false;
                for &(_nk, ni) in neighbors {
                    if !used[ni] {
                        used[ni] = true;
                        let seg = segments[ni];
                        if key(seg.0) == k {
                            head = seg.1;
                        } else {
                            head = seg.0;
                        }
                        poly.insert(0, head);
                        extended = true;
                        break;
                    }
                }
                if !extended {
                    break;
                }
            } else {
                break;
            }
        }

        result.push(poly);
    }

    result
}

/// Quantise a point so that adjacent-cell identical edge-crossings collide.
fn key(p: [f64; 2]) -> Key {
    // 0.01 m tolerance – well below the 25 m grid spacing.
    let x = (p[0] * 100.0).round() as i64;
    let y = (p[1] * 100.0).round() as i64;
    Key { x, y }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    x: i64,
    y: i64,
}
