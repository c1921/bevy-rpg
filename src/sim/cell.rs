// cell.rs — Core data model: Cell, WorldMap, and constants

use bevy::math::{Vec2, Vec3};

// ============================================================
//  Constants (matching original quad:: namespace)
// ============================================================

pub const MAP_SCALE: f32 = 80.0;
pub const TILE_SIZE: usize = 512;
pub const TILE_AREA: usize = TILE_SIZE * TILE_SIZE;  // unused, kept for reference

pub const MAP_SIZE: usize = 1;
pub const MAP_AREA: usize = MAP_SIZE * MAP_SIZE;

pub const WORLD_SIZE: usize = MAP_SIZE * TILE_SIZE;
pub const WORLD_AREA: usize = MAP_AREA * TILE_AREA;  // unused, kept for reference

pub const LOD_SIZE: i32 = 1;
pub const LOD_SIZE_F: f32 = 1.0;

// ============================================================
//  Cell — interleaved cell data (matching quad::cell)
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
        }
    }
}

// ============================================================
//  WorldMap — the full 512×512 grid
// ============================================================

pub struct WorldMap {
    pub cells: Vec<Cell>,
}

impl WorldMap {
    pub fn new() -> Self {
        Self {
            cells: vec![Cell::default(); WORLD_AREA],
        }
    }

    /// Flatten a 2D index into 1D (row-major, matching original math::flatten)
    #[inline]
    pub fn index(&self, x: i32, y: i32) -> usize {
        (x as usize) * WORLD_SIZE + (y as usize)
    }

    /// Out-of-bounds check
    #[inline]
    pub fn oob_i(&self, x: i32, y: i32) -> bool {
        x < 0 || y < 0 || x >= WORLD_SIZE as i32 || y >= WORLD_SIZE as i32
    }

    #[inline]
    pub fn oob_f(&self, pos: Vec2) -> bool {
        pos.x < 0.0 || pos.y < 0.0 || pos.x >= WORLD_SIZE as f32 || pos.y >= WORLD_SIZE as f32
    }

    /// Get immutable cell reference
    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Option<&Cell> {
        if self.oob_i(x, y) {
            None
        } else {
            Some(&self.cells[self.index(x, y)])
        }
    }

    /// Get mutable cell reference
    #[inline]
    pub fn get_mut(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        if self.oob_i(x, y) {
            None
        } else {
            let idx = self.index(x, y);
            Some(&mut self.cells[idx])
        }
    }

    /// Height at integer position
    #[inline]
    pub fn height_i(&self, x: i32, y: i32) -> f32 {
        self.get(x, y).map(|c| c.height).unwrap_or(0.0)
    }

    /// Bilinear-interpolated height at floating position (for particle queries)
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

    /// Discharge at integer position (erf-mapped like original)
    #[inline]
    pub fn discharge(&self, x: i32, y: i32) -> f32 {
        self.get(x, y)
            .map(|c| erf_approx(0.4 * c.discharge))
            .unwrap_or(0.0)
    }

    /// Discharge at float position (bilinear)
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

    /// Normal at integer position, matching original quad::_normal
    pub fn normal(&self, x: i32, y: i32) -> Vec3 {
        let s = Vec3::new(1.0, MAP_SCALE, 1.0);
        let mut n = Vec3::ZERO;

        let h = self.height_i(x, y);

        // (+X, +Z) plane
        if !self.oob_i(x + LOD_SIZE, y + LOD_SIZE) {
            let v1 = s * Vec3::new(0.0, self.height_i(x, y + LOD_SIZE) - h, 1.0);
            let v2 = s * Vec3::new(1.0, self.height_i(x + LOD_SIZE, y) - h, 0.0);
            n += v1.cross(v2);
        }

        // (-X, -Z) plane
        if !self.oob_i(x - LOD_SIZE, y - LOD_SIZE) {
            let v1 = s * Vec3::new(0.0, self.height_i(x, y - LOD_SIZE) - h, -1.0);
            let v2 = s * Vec3::new(-1.0, self.height_i(x - LOD_SIZE, y) - h, 0.0);
            n += v1.cross(v2);
        }

        // (+X, -Z) plane
        if !self.oob_i(x + LOD_SIZE, y - LOD_SIZE) {
            let v1 = s * Vec3::new(1.0, self.height_i(x + LOD_SIZE, y) - h, 0.0);
            let v2 = s * Vec3::new(0.0, self.height_i(x, y - LOD_SIZE) - h, -1.0);
            n += v1.cross(v2);
        }

        // (-X, +Z) plane
        if !self.oob_i(x - LOD_SIZE, y + LOD_SIZE) {
            let v1 = s * Vec3::new(-1.0, self.height_i(x - LOD_SIZE, y) - h, 0.0);
            let v2 = s * Vec3::new(0.0, self.height_i(x, y + LOD_SIZE) - h, 1.0);
            n += v1.cross(v2);
        }

        if n.length() > 0.0 {
            n = n.normalize();
        }
        n
    }

    pub fn get_cell(&self, pos: Vec2) -> Option<&Cell> {
        self.get(pos.x.floor() as i32, pos.y.floor() as i32)
    }

    pub fn get_cell_mut(&mut self, pos: Vec2) -> Option<&mut Cell> {
        self.get_mut(pos.x.floor() as i32, pos.y.floor() as i32)
    }
}

/// Cheap erf approximation (Abramowitz & Stegun 7.1.26)
#[inline]
fn erf_approx(x: f32) -> f32 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * (-x * x).exp();
    sign * y
}
