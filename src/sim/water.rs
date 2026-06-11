// water.rs — Drop particle and descend() logic

use super::cell::*;
use super::world::World;
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

    /// One step of the particle descent. Returns true while alive.
    pub fn descend(&mut self, world: &mut World) -> bool {
        let ipos = self.pos.as_ivec2();

        // Termination: age limit
        if self.age > Self::MAX_AGE {
            if let Some(cell) = world.map.get_cell_mut(self.pos) {
                cell.height += self.sediment;
            }
            return false;
        }

        // Termination: volume too low
        if self.volume < Self::MIN_VOL {
            if let Some(cell) = world.map.get_cell_mut(self.pos) {
                cell.height += self.sediment;
            }
            return false;
        }

        // Get current cell data (clone needed values to avoid borrow conflict)
        let cell_data = match world.map.get_cell(self.pos) {
            Some(c) => (c.height, c.root_density, c.momentum_x, c.momentum_y, c.discharge),
            None => return false,
        };
        let (cell_height, cell_root_density, cell_momentum_x, cell_momentum_y, cell_discharge) = cell_data;

        let n = world.map.normal(ipos.x, ipos.y);

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

        // Track discharge and momentum (deposited in current cell)
        if let Some(cell_mut) = world.map.get_cell_mut(Vec2::new(ipos.x as f32, ipos.y as f32)) {
            cell_mut.discharge_track += self.volume;
            cell_mut.momentum_x_track += self.volume * self.speed.x;
            cell_mut.momentum_y_track += self.volume * self.speed.y;
        }

        // Height at destination
        let h2 = if world.map.oob_f(self.pos) {
            cell_height - 0.002
        } else {
            world.map.height_f(self.pos)
        };

        // Mass transfer (erosion/deposition)
        let cell_discharge_val = world.map.discharge(ipos.x, ipos.y);
        let c_eq = (1.0 + Self::ENTRAINMENT * cell_discharge_val) * (cell_height - h2);
        let c_eq = c_eq.max(0.0);
        let cdiff = c_eq - self.sediment;

        if let Some(cell_mut) = world.map.get_cell_mut(Vec2::new(ipos.x as f32, ipos.y as f32)) {
            self.sediment += eff_d * cdiff;
            cell_mut.height -= eff_d * cdiff;
        }

        // Evaporation (mass-conservative)
        self.sediment /= 1.0 - Self::EVAP_RATE;
        self.volume *= 1.0 - Self::EVAP_RATE;

        // Out of bounds check
        if world.map.oob_f(self.pos) {
            return false;
        }

        // Cascade settling
        world.cascade(self.pos);

        self.age += 1;
        true
    }
}
