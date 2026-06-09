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

/// 2-D point equality with f64 epsilon.
pub(crate) fn pt_eq(a: &[f64; 2], b: &[f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

/// Linearly interpolate on an edge.
pub(crate) fn edge_pos(
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

/// Run marching squares for a single threshold; return raw segments.
pub(crate) fn extract_segments(
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

// ── Flat-slice variants (avoid Vec<Vec<f64>> conversion) ────────

#[inline]
fn height_at(heights: &[f64], cols: usize, r: usize, c: usize) -> f64 {
    heights[r * cols + c]
}

/// Linearly interpolate on an edge (flat slice variant).
fn edge_pos_flat(
    heights: &[f64],
    cols: usize,
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
    let h0 = height_at(heights, cols, r0, c0);
    let h1 = height_at(heights, cols, r1, c1);
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

fn edge_point_flat(
    heights: &[f64],
    cols: usize,
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
        0 => edge_pos_flat(heights, cols, r, c, r, c + 1, world_x_min, world_y_min, dx, dy, threshold),
        1 => edge_pos_flat(heights, cols, r, c + 1, r + 1, c + 1, world_x_min, world_y_min, dx, dy, threshold),
        2 => edge_pos_flat(heights, cols, r + 1, c, r + 1, c + 1, world_x_min, world_y_min, dx, dy, threshold),
        3 => edge_pos_flat(heights, cols, r, c, r + 1, c, world_x_min, world_y_min, dx, dy, threshold),
        _ => unreachable!(),
    }
}

/// Run marching squares for a single threshold on a flat height slice.
pub(crate) fn extract_segments_flat(
    heights: &[f64],
    cols: usize,
    rows: usize,
    world_x_min: f64,
    world_y_min: f64,
    dx: f64,
    dy: f64,
    threshold: f64,
) -> Vec<([f64; 2], [f64; 2])> {
    let grid_rows = rows - 1;
    let grid_cols = cols - 1;
    let mut segs = Vec::new();

    for r in 0..grid_rows {
        for c in 0..grid_cols {
            let h = [
                height_at(heights, cols, r, c),
                height_at(heights, cols, r, c + 1),
                height_at(heights, cols, r + 1, c + 1),
                height_at(heights, cols, r + 1, c),
            ];

            let mut case = 0u8;
            for i in 0..4 {
                if h[i] >= threshold {
                    case |= 1 << i;
                }
            }

            let entry = MS_TABLE[case as usize];
            for pair in entry.chunks(2) {
                if pair[0] == 4 {
                    break;
                }
                let e0 = pair[0] as usize;
                let e1 = pair[1] as usize;

                let p0 = edge_point_flat(heights, cols, r, c, e0, world_x_min, world_y_min, dx, dy, threshold);
                let p1 = edge_point_flat(heights, cols, r, c, e1, world_x_min, world_y_min, dx, dy, threshold);

                if !pt_eq(&p0, &p1) {
                    segs.push((p0, p1));
                }
            }
        }
    }

    segs
}
