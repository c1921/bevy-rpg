// cell.rs — Core data model: Cell, WorldMap, and constants
// Adapted from reference project (sim/cell.rs) with dynamic grid sizing.

use bevy::math::{IVec2, Vec2, Vec3};

// ============================================================
//  Constants
// ============================================================

pub const MAP_SCALE: f32 = 80.0;
pub const LOD_SIZE: i32 = 1;
pub const LOD_SIZE_F: f32 = 1.0;

// Erosion constants (shared between World and WorldMap)
pub const SETTLING: f32 = 0.55;
pub const MAXDIFF: f32 = 0.01;

// ============================================================
//  Cell — interleaved cell data
// ============================================================

#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub height: f32,
    pub discharge: f32,
    pub momentum_x: f32,
    pub momentum_y: f32,

    pub discharge_track: f32,
    pub momentum_x_track: f32,
    pub momentum_y_track: f32,

    pub root_density: f32,

    /// Precomputed surface normal (updated after generation and erosion).
    /// Used by the render path to avoid per-pixel normal recomputation.
    pub cached_normal: Vec3,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            height: 0.0,
            discharge: 0.0,
            momentum_x: 0.0,
            momentum_y: 0.0,
            discharge_track: 0.0,
            momentum_x_track: 0.0,
            momentum_y_track: 0.0,
            root_density: 0.0,
            cached_normal: Vec3::Y,
        }
    }
}

// ============================================================
//  WorldMap — dynamic-sized grid
// ============================================================

pub struct WorldMap {
    pub cells: Vec<Cell>,
    /// Number of columns (stride for row-major indexing).
    pub width: usize,
    /// Number of rows.
    pub height: usize,
}

impl WorldMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            cells: vec![Cell::default(); width * height],
            width,
            height,
        }
    }

    /// Total number of cells.
    #[inline]
    pub fn area(&self) -> usize {
        self.width * self.height
    }

    /// Flatten a 2D index into 1D (row-major, matching original: x = row, y = col).
    #[inline]
    pub fn index(&self, x: i32, y: i32) -> usize {
        (x as usize) * self.width + (y as usize)
    }

    /// Out-of-bounds check (integer).
    #[inline]
    pub fn oob_i(&self, x: i32, y: i32) -> bool {
        x < 0 || y < 0 || x >= self.height as i32 || y >= self.width as i32
    }

    /// Out-of-bounds check (float).
    #[inline]
    pub fn oob_f(&self, pos: Vec2) -> bool {
        pos.x < 0.0
            || pos.y < 0.0
            || pos.x >= self.height as f32
            || pos.y >= self.width as f32
    }

    /// Get immutable cell reference.
    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Option<&Cell> {
        if self.oob_i(x, y) {
            None
        } else {
            Some(&self.cells[self.index(x, y)])
        }
    }

    /// Get mutable cell reference.
    #[inline]
    pub fn get_mut(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        if self.oob_i(x, y) {
            None
        } else {
            let idx = self.index(x, y);
            Some(&mut self.cells[idx])
        }
    }

    /// Height at integer position.
    #[inline]
    #[allow(dead_code)]
    pub fn height_i(&self, x: i32, y: i32) -> f32 {
        self.get(x, y).map(|c| c.height).unwrap_or(0.0)
    }

    /// Bilinear-interpolated height at floating position (for particle queries).
    #[allow(dead_code)]
    pub fn height_f(&self, pos: Vec2) -> f32 {
        let x0 = pos.x.floor() as i32;
        let y0 = pos.y.floor() as i32;
        let fx = pos.x - x0 as f32;
        let fy = pos.y - y0 as f32;

        let h00 = self.height_i(x0, y0);
        let h10 = self.height_i(x0 + 1, y0);
        let h01 = self.height_i(x0, y0 + 1);
        let h11 = self.height_i(x0 + 1, y0 + 1);

        let h0 = h00 + (h10 - h00) * fx;
        let h1 = h01 + (h11 - h01) * fx;
        h0 + (h1 - h0) * fy
    }

    /// Discharge at integer position (erf-mapped like original).
    #[inline]
    pub fn discharge(&self, x: i32, y: i32) -> f32 {
        self.get(x, y)
            .map(|c| erf_approx(0.4 * c.discharge))
            .unwrap_or(0.0)
    }

    /// Discharge at float position (bilinear).
    #[allow(dead_code)]
    pub fn discharge_f(&self, pos: Vec2) -> f32 {
        let x0 = pos.x.floor() as i32;
        let y0 = pos.y.floor() as i32;
        let fx = pos.x - x0 as f32;
        let fy = pos.y - y0 as f32;

        let d00 = self.discharge(x0, y0);
        let d10 = self.discharge(x0 + 1, y0);
        let d01 = self.discharge(x0, y0 + 1);
        let d11 = self.discharge(x0 + 1, y0 + 1);

        let d0 = d00 + (d10 - d00) * fx;
        let d1 = d01 + (d11 - d01) * fx;
        d0 + (d1 - d0) * fy
    }

    /// Normal at integer position.
    #[allow(dead_code)]
    pub fn normal(&self, x: i32, y: i32) -> Vec3 {
        compute_normal(&self.cells, self.width, self.height, x, y)
    }

    /// Recompute cached normals for every cell (parallel).
    /// Call after any bulk height change (generation, erosion merge).
    pub fn recompute_normals(&mut self) {
        use rayon::prelude::*;
        let w = self.width;
        let h = self.height;
        let cells = &self.cells;
        let normals: Vec<Vec3> = (0..cells.len())
            .into_par_iter()
            .map(|idx| {
                let x = (idx / w) as i32;
                let y = (idx % w) as i32;
                compute_normal(cells, w, h, x, y)
            })
            .collect();
        self.cells
            .iter_mut()
            .zip(normals)
            .for_each(|(cell, n)| cell.cached_normal = n);
    }

    #[allow(dead_code)]
    pub fn get_cell(&self, pos: Vec2) -> Option<&Cell> {
        self.get(pos.x.floor() as i32, pos.y.floor() as i32)
    }

    #[allow(dead_code)]
    pub fn get_cell_mut(&mut self, pos: Vec2) -> Option<&mut Cell> {
        self.get_mut(pos.x.floor() as i32, pos.y.floor() as i32)
    }

    /// Cascade settling: propagate excess height to lower neighbors.
    /// Reads/writes self.cells directly (serial use only).
    #[allow(dead_code)]
    pub fn cascade(&mut self, pos: Vec2) {
        let ipos = IVec2::new(pos.x.floor() as i32, pos.y.floor() as i32);

        let neighbors: [(i32, i32); 8] = [
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, -1),
            (0, 1),
            (1, -1),
            (1, 0),
            (1, 1),
        ];

        struct Pt {
            idx: usize,
            h: f32,
            d: f32,
        }
        let mut pts: [Option<Pt>; 8] = [None, None, None, None, None, None, None, None];
        let mut num = 0;

        for (dx, dy) in &neighbors {
            let nx = ipos.x + LOD_SIZE * dx;
            let ny = ipos.y + LOD_SIZE * dy;
            if self.oob_i(nx, ny) {
                continue;
            }
            let idx = self.index(nx, ny);
            let d = Vec2::new(*dx as f32, *dy as f32).length();
            pts[num] = Some(Pt {
                idx,
                h: self.cells[idx].height,
                d,
            });
            num += 1;
        }

        let mut refs: [Option<&Pt>; 8] = [None, None, None, None, None, None, None, None];
        for i in 0..num {
            refs[i] = pts[i].as_ref();
        }
        refs[..num].sort_unstable_by(|a, b| a.unwrap().h.partial_cmp(&b.unwrap().h).unwrap());

        let cur_idx = self.index(ipos.x, ipos.y);
        let cur_h = self.cells[cur_idx].height;

        for opt in &refs[..num] {
            let pt = opt.unwrap();
            let diff = cur_h - pt.h;
            if diff == 0.0 {
                continue;
            }
            let excess = if pt.h > 0.1 {
                diff.abs() - pt.d * MAXDIFF * LOD_SIZE_F
            } else {
                diff.abs()
            };
            if excess <= 0.0 {
                continue;
            }
            let transfer = SETTLING * excess / 2.0;
            if diff > 0.0 {
                self.cells[cur_idx].height -= transfer;
                self.cells[pt.idx].height += transfer;
            } else {
                self.cells[cur_idx].height += transfer;
                self.cells[pt.idx].height -= transfer;
            }
        }
    }

    /// Parallel-safe cascade: reads base height from self + delta, writes to delta.
    /// Used inside per-thread erosion loops.
    pub fn cascade_delta(&self, height_delta: &mut [f32], pos: Vec2) {
        let ipos = IVec2::new(pos.x.floor() as i32, pos.y.floor() as i32);

        let neighbors: [(i32, i32); 8] = [
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, -1),
            (0, 1),
            (1, -1),
            (1, 0),
            (1, 1),
        ];

        struct Pt {
            idx: usize,
            h: f32,
            d: f32,
        }
        let mut pts: [Option<Pt>; 8] = [None, None, None, None, None, None, None, None];
        let mut num = 0;

        for (dx, dy) in &neighbors {
            let nx = ipos.x + LOD_SIZE * dx;
            let ny = ipos.y + LOD_SIZE * dy;
            if self.oob_i(nx, ny) {
                continue;
            }
            let idx = self.index(nx, ny);
            let d = Vec2::new(*dx as f32, *dy as f32).length();
            pts[num] = Some(Pt {
                idx,
                h: self.cells[idx].height + height_delta[idx],
                d,
            });
            num += 1;
        }

        let mut refs: [Option<&Pt>; 8] = [None, None, None, None, None, None, None, None];
        for i in 0..num {
            refs[i] = pts[i].as_ref();
        }
        refs[..num].sort_unstable_by(|a, b| a.unwrap().h.partial_cmp(&b.unwrap().h).unwrap());

        let cur_idx = self.index(ipos.x, ipos.y);
        let cur_h = self.cells[cur_idx].height + height_delta[cur_idx];

        for opt in &refs[..num] {
            let pt = opt.unwrap();
            let diff = cur_h - pt.h;
            if diff == 0.0 {
                continue;
            }
            let excess = if pt.h > 0.1 {
                diff.abs() - pt.d * MAXDIFF * LOD_SIZE_F
            } else {
                diff.abs()
            };
            if excess <= 0.0 {
                continue;
            }
            let transfer = SETTLING * excess / 2.0;
            if diff > 0.0 {
                height_delta[cur_idx] -= transfer;
                height_delta[pt.idx] += transfer;
            } else {
                height_delta[cur_idx] += transfer;
                height_delta[pt.idx] -= transfer;
            }
        }
    }

    /// Bilinear-interpolated height from base + delta (for parallel droplet use).
    #[inline]
    pub fn height_f_delta(&self, height_delta: &[f32], pos: Vec2) -> f32 {
        let x0 = pos.x.floor() as i32;
        let y0 = pos.y.floor() as i32;
        let fx = pos.x - x0 as f32;
        let fy = pos.y - y0 as f32;

        let h = |x: i32, y: i32| -> f32 {
            if self.oob_i(x, y) {
                return 0.0;
            }
            let idx = self.index(x, y);
            self.cells[idx].height + height_delta[idx]
        };

        let h00 = h(x0, y0);
        let h10 = h(x0 + 1, y0);
        let h01 = h(x0, y0 + 1);
        let h11 = h(x0 + 1, y0 + 1);

        let h0 = h00 + (h10 - h00) * fx;
        let h1 = h01 + (h11 - h01) * fx;
        h0 + (h1 - h0) * fy
    }

    /// Normal from base + delta (for parallel droplet use).
    pub fn normal_delta(&self, height_delta: &[f32], x: i32, y: i32) -> Vec3 {
        let s = Vec3::new(1.0, MAP_SCALE, 1.0);
        let mut n = Vec3::ZERO;

        let w = self.width as i32;
        let h = self.height as i32;

        // Interior fast path: >99% of cells — all neighbors guaranteed in bounds.
        let interior = x > 0 && y > 0 && x < h - 1 && y < w - 1;
        if interior {
            let idx = |x: i32, y: i32| -> usize { (x as usize) * self.width + (y as usize) };
            let hf = |x: i32, y: i32| -> f32 {
                let i = idx(x, y);
                self.cells[i].height + height_delta[i]
            };
            let center = hf(x, y);

            // (+X, +Z)
            {
                let v1 = s * Vec3::new(0.0, hf(x, y + LOD_SIZE) - center, 1.0);
                let v2 = s * Vec3::new(1.0, hf(x + LOD_SIZE, y) - center, 0.0);
                n += v1.cross(v2);
            }
            // (-X, -Z)
            {
                let v1 = s * Vec3::new(0.0, hf(x, y - LOD_SIZE) - center, -1.0);
                let v2 = s * Vec3::new(-1.0, hf(x - LOD_SIZE, y) - center, 0.0);
                n += v1.cross(v2);
            }
            // (+X, -Z)
            {
                let v1 = s * Vec3::new(1.0, hf(x + LOD_SIZE, y) - center, 0.0);
                let v2 = s * Vec3::new(0.0, hf(x, y - LOD_SIZE) - center, -1.0);
                n += v1.cross(v2);
            }
            // (-X, +Z)
            {
                let v1 = s * Vec3::new(-1.0, hf(x - LOD_SIZE, y) - center, 0.0);
                let v2 = s * Vec3::new(0.0, hf(x, y + LOD_SIZE) - center, 1.0);
                n += v1.cross(v2);
            }
        } else {
            // Boundary slow path: check each corner.
            let hf = |x: i32, y: i32| -> f32 {
                if self.oob_i(x, y) {
                    return 0.0;
                }
                let idx = self.index(x, y);
                self.cells[idx].height + height_delta[idx]
            };
            let center = hf(x, y);

            if !self.oob_i(x + LOD_SIZE, y + LOD_SIZE) {
                let v1 = s * Vec3::new(0.0, hf(x, y + LOD_SIZE) - center, 1.0);
                let v2 = s * Vec3::new(1.0, hf(x + LOD_SIZE, y) - center, 0.0);
                n += v1.cross(v2);
            }
            if !self.oob_i(x - LOD_SIZE, y - LOD_SIZE) {
                let v1 = s * Vec3::new(0.0, hf(x, y - LOD_SIZE) - center, -1.0);
                let v2 = s * Vec3::new(-1.0, hf(x - LOD_SIZE, y) - center, 0.0);
                n += v1.cross(v2);
            }
            if !self.oob_i(x + LOD_SIZE, y - LOD_SIZE) {
                let v1 = s * Vec3::new(1.0, hf(x + LOD_SIZE, y) - center, 0.0);
                let v2 = s * Vec3::new(0.0, hf(x, y - LOD_SIZE) - center, -1.0);
                n += v1.cross(v2);
            }
            if !self.oob_i(x - LOD_SIZE, y + LOD_SIZE) {
                let v1 = s * Vec3::new(-1.0, hf(x - LOD_SIZE, y) - center, 0.0);
                let v2 = s * Vec3::new(0.0, hf(x, y + LOD_SIZE) - center, 1.0);
                n += v1.cross(v2);
            }
        }

        if n.length() > 0.0 {
            n = n.normalize();
        }
        n
    }
}

// ============================================================
//  Utility functions
// ============================================================

/// Cheap erf approximation (Abramowitz & Stegun 7.1.26).
#[inline]
pub fn erf_approx(x: f32) -> f32 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    sign * y
}

/// Compute surface normal from a raw cell slice at integer position (x,y).
/// Standalone function so it can be called from parallel iterators.
#[inline]
pub fn compute_normal(cells: &[Cell], width: usize, height: usize, x: i32, y: i32) -> Vec3 {
    let s = Vec3::new(1.0, MAP_SCALE, 1.0);
    let mut n = Vec3::ZERO;

    let w = width as i32;
    let h = height as i32;

    let oob = |x: i32, y: i32| -> bool { x < 0 || y < 0 || x >= h || y >= w };

    let hf = |x: i32, y: i32| -> f32 {
        if oob(x, y) {
            return 0.0;
        }
        cells[(x as usize) * width + (y as usize)].height
    };

    let center = hf(x, y);

    // (+X, +Z) plane
    if !oob(x + LOD_SIZE, y + LOD_SIZE) {
        let v1 = s * Vec3::new(0.0, hf(x, y + LOD_SIZE) - center, 1.0);
        let v2 = s * Vec3::new(1.0, hf(x + LOD_SIZE, y) - center, 0.0);
        n += v1.cross(v2);
    }

    // (-X, -Z) plane
    if !oob(x - LOD_SIZE, y - LOD_SIZE) {
        let v1 = s * Vec3::new(0.0, hf(x, y - LOD_SIZE) - center, -1.0);
        let v2 = s * Vec3::new(-1.0, hf(x - LOD_SIZE, y) - center, 0.0);
        n += v1.cross(v2);
    }

    // (+X, -Z) plane
    if !oob(x + LOD_SIZE, y - LOD_SIZE) {
        let v1 = s * Vec3::new(1.0, hf(x + LOD_SIZE, y) - center, 0.0);
        let v2 = s * Vec3::new(0.0, hf(x, y - LOD_SIZE) - center, -1.0);
        n += v1.cross(v2);
    }

    // (-X, +Z) plane
    if !oob(x - LOD_SIZE, y + LOD_SIZE) {
        let v1 = s * Vec3::new(-1.0, hf(x - LOD_SIZE, y) - center, 0.0);
        let v2 = s * Vec3::new(0.0, hf(x, y + LOD_SIZE) - center, 1.0);
        n += v1.cross(v2);
    }

    if n.length() > 0.0 {
        n = n.normalize();
    }
    n
}
