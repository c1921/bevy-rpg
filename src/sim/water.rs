// water.rs — Drop particle and descend() logic

use super::cell::*;
use bevy::math::Vec2;

pub struct Drop {
    pub age: i32,
    pub pos: Vec2,
    pub speed: Vec2,
    pub volume: f32,
    pub sediment: f32,
}

// Static parameters (matching original Drop)
impl Drop {
    pub const MAX_AGE: i32 = 500;
    pub const MIN_VOL: f32 = 0.01;
    pub const EVAP_RATE: f32 = 0.001;
    pub const DEPOSITION_RATE: f32 = 0.1;
    pub const ENTRAINMENT: f32 = 10.0;
    pub const GRAVITY: f32 = 1.0;
    pub const MOMENTUM_TRANSFER: f32 = 1.0;

    pub fn new(pos: Vec2) -> Self {
        Self {
            age: 0,
            pos,
            speed: Vec2::ZERO,
            volume: 1.0,
            sediment: 0.0,
        }
    }

    /// One step of the particle descent (serial). Returns true while alive.
    #[allow(dead_code)]
    pub fn descend(&mut self, map: &mut WorldMap) -> bool {
        let ipos = self.pos.as_ivec2();

        // Termination: age limit
        if self.age > Self::MAX_AGE {
            if !map.oob_i(ipos.x, ipos.y) {
                let idx = map.index(ipos.x, ipos.y);
                map.cells[idx].height += self.sediment;
            }
            return false;
        }

        // Termination: volume too low
        if self.volume < Self::MIN_VOL {
            if !map.oob_i(ipos.x, ipos.y) {
                let idx = map.index(ipos.x, ipos.y);
                map.cells[idx].height += self.sediment;
            }
            return false;
        }

        // Get current cell data
        let idx = map.index(ipos.x, ipos.y);
        if map.oob_i(ipos.x, ipos.y) { return false; }
        let cell = &map.cells[idx];
        let cell_height = cell.height;
        let cell_root_density = cell.root_density;
        let cell_momentum_x = cell.momentum_x;
        let cell_momentum_y = cell.momentum_y;
        let cell_discharge = cell.discharge;

        let n = map.normal(ipos.x, ipos.y);

        // Effective deposition rate (reduced by root density)
        let eff_d = Self::DEPOSITION_RATE * (1.0 - cell_root_density).max(0.0);

        // === Apply forces ===

        // Gravity
        self.speed += LOD_SIZE_F * Self::GRAVITY * Vec2::new(n.x, n.z) / self.volume;

        // Momentum transfer from flow field
        let fspeed = Vec2::new(cell_momentum_x, cell_momentum_y);
        if fspeed.length() > 0.0 && self.speed.length() > 0.0 {
            self.speed += LOD_SIZE_F
                * Self::MOMENTUM_TRANSFER
                * fspeed.normalize().dot(self.speed.normalize())
                / (self.volume + cell_discharge)
                * fspeed;
        }

        // Dynamic time-step normalization
        if self.speed.length() > 0.0 {
            self.speed = LOD_SIZE_F * (2.0_f32).sqrt() * self.speed.normalize();
        }

        self.pos += self.speed;

        // Track discharge and momentum
        map.cells[idx].discharge_track += self.volume;
        map.cells[idx].momentum_x_track += self.volume * self.speed.x;
        map.cells[idx].momentum_y_track += self.volume * self.speed.y;

        // Height at destination
        let h2 = if map.oob_f(self.pos) {
            cell_height - 0.002
        } else {
            map.height_f(self.pos)
        };

        // Mass transfer (erosion/deposition)
        let cell_discharge_val = map.discharge(ipos.x, ipos.y);
        let c_eq = (1.0 + Self::ENTRAINMENT * cell_discharge_val) * (cell_height - h2);
        let c_eq = c_eq.max(0.0);
        let cdiff = c_eq - self.sediment;

        self.sediment += eff_d * cdiff;
        map.cells[idx].height -= eff_d * cdiff;

        // Evaporation (mass-conservative)
        self.sediment /= 1.0 - Self::EVAP_RATE;
        self.volume *= 1.0 - Self::EVAP_RATE;

        // Out of bounds check
        if map.oob_f(self.pos) {
            return false;
        }

        // Cascade settling
        map.cascade(self.pos);

        self.age += 1;
        true
    }

    /// Parallel-safe descent: reads base map + delta, writes to delta arrays.
    /// Does NOT call cascade — caller runs cascade_delta after each step.
    pub fn descend_delta(
        &mut self,
        map: &WorldMap,
        height_delta: &mut [f32],
        discharge_track: &mut [f32],
        momentum_x_track: &mut [f32],
        momentum_y_track: &mut [f32],
    ) -> bool {
        let ipos = self.pos.as_ivec2();

        // Termination: age limit
        if self.age > Self::MAX_AGE {
            if !map.oob_i(ipos.x, ipos.y) {
                height_delta[map.index(ipos.x, ipos.y)] += self.sediment;
            }
            return false;
        }

        // Termination: volume too low
        if self.volume < Self::MIN_VOL {
            if !map.oob_i(ipos.x, ipos.y) {
                height_delta[map.index(ipos.x, ipos.y)] += self.sediment;
            }
            return false;
        }

        if map.oob_i(ipos.x, ipos.y) { return false; }
        let idx = map.index(ipos.x, ipos.y);
        let cell = &map.cells[idx];
        let cell_height = cell.height + height_delta[idx];
        let cell_root_density = cell.root_density;
        let cell_momentum_x = cell.momentum_x;
        let cell_momentum_y = cell.momentum_y;
        let cell_discharge = cell.discharge;

        let n = map.normal_delta(height_delta, ipos.x, ipos.y);

        // Effective deposition rate (reduced by root density)
        let eff_d = Self::DEPOSITION_RATE * (1.0 - cell_root_density).max(0.0);

        // === Apply forces ===

        // Gravity
        self.speed += LOD_SIZE_F * Self::GRAVITY * Vec2::new(n.x, n.z) / self.volume;

        // Momentum transfer from flow field
        let fspeed = Vec2::new(cell_momentum_x, cell_momentum_y);
        if fspeed.length() > 0.0 && self.speed.length() > 0.0 {
            self.speed += LOD_SIZE_F
                * Self::MOMENTUM_TRANSFER
                * fspeed.normalize().dot(self.speed.normalize())
                / (self.volume + cell_discharge)
                * fspeed;
        }

        // Dynamic time-step normalization
        if self.speed.length() > 0.0 {
            self.speed = LOD_SIZE_F * (2.0_f32).sqrt() * self.speed.normalize();
        }

        self.pos += self.speed;

        // Track discharge and momentum (write to thread-local arrays)
        discharge_track[idx] += self.volume;
        momentum_x_track[idx] += self.volume * self.speed.x;
        momentum_y_track[idx] += self.volume * self.speed.y;

        // Height at destination (base + delta)
        let h2 = if map.oob_f(self.pos) {
            cell_height - 0.002
        } else {
            map.height_f_delta(height_delta, self.pos)
        };

        // Mass transfer (erosion/deposition)
        let cell_discharge_val = map.discharge(ipos.x, ipos.y);
        let c_eq = (1.0 + Self::ENTRAINMENT * cell_discharge_val) * (cell_height - h2);
        let c_eq = c_eq.max(0.0);
        let cdiff = c_eq - self.sediment;

        self.sediment += eff_d * cdiff;
        height_delta[idx] -= eff_d * cdiff;

        // Evaporation (mass-conservative)
        self.sediment /= 1.0 - Self::EVAP_RATE;
        self.volume *= 1.0 - Self::EVAP_RATE;

        // Out of bounds check
        if map.oob_f(self.pos) {
            return false;
        }

        // NOTE: cascade_delta must be called by the caller after each step

        self.age += 1;
        true
    }
}
