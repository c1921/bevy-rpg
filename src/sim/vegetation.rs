// vegetation.rs — Plant particles and Vegetation container

use super::cell::*;
use super::world::World;
use bevy::math::Vec2;
use rand::Rng;
use rand::SeedableRng;

pub struct Plant {
    pub pos: Vec2,
    pub size: f32,
}

impl Plant {
    pub const MAX_SIZE: f32 = 1.5;
    pub const GROW_RATE: f32 = 0.05;
    pub const MAX_STEEP: f32 = 0.8;
    pub const MAX_DISCHARGE: f32 = 0.3;
    pub const MAX_TREE_HEIGHT: f32 = 0.8;

    pub fn new(pos: Vec2) -> Self {
        Self { pos, size: 0.0 }
    }

    pub fn grow(&mut self) {
        self.size += Self::GROW_RATE * (Self::MAX_SIZE - self.size);
    }

    /// Check if this plant should die
    pub fn die(&self, world: &World, rng: &mut impl Rng) -> bool {
        if world.map.discharge_f(self.pos) >= Self::MAX_DISCHARGE {
            return true;
        }
        if world.map.height_f(self.pos) >= Self::MAX_TREE_HEIGHT {
            return true;
        }
        // Random death: 1 in 1000 chance
        rng.gen_range(0..1000) == 0
    }

    /// Check if a new plant can spawn at this position
    pub fn can_spawn(pos: Vec2, world: &World) -> bool {
        if world.map.discharge_f(pos) >= Self::MAX_DISCHARGE {
            return false;
        }
        let ipos = pos.as_ivec2();
        if let Some(cell) = world.map.get(ipos.x, ipos.y) {
            if cell.cached_normal.y < Self::MAX_STEEP {
                return false;
            }
        }
        if world.map.height_f(pos) >= Self::MAX_TREE_HEIGHT {
            return false;
        }
        true
    }

    /// Modify root_density in surrounding cells
    #[allow(dead_code)]
    pub fn root(&self, world: &mut World, factor: f32) {
        let offsets: [(f32, f32, f32); 9] = [
            (0.0, 0.0, 1.0),
            (1.0, 0.0, 0.6),
            (-1.0, 0.0, 0.6),
            (0.0, 1.0, 0.6),
            (0.0, -1.0, 0.6),
            (-1.0, -1.0, 0.4),
            (1.0, -1.0, 0.4),
            (-1.0, 1.0, 0.4),
            (1.0, 1.0, 0.4),
        ];

        for (dx, dy, weight) in &offsets {
            let p = self.pos + Vec2::new(*dx, *dy);
            if let Some(cell) = world.map.get_cell_mut(p) {
                cell.root_density += factor * weight;
            }
        }
    }
}

pub struct Vegetation {
    pub plants: Vec<Plant>,
}

impl Vegetation {
    pub fn new() -> Self {
        Self { plants: Vec::new() }
    }

    /// One vegetation growth cycle (matching original Vegetation::grow)
    pub fn grow(&mut self, world: &mut World, seed: u64) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

        // Try to spawn a new random plant
        {
            let rx = rng.gen_range(0..WORLD_SIZE);
            let ry = rng.gen_range(0..WORLD_SIZE);
            let pos = Vec2::new(rx as f32, ry as f32);

            if Plant::can_spawn(pos, world) {
                let plant = Plant::new(pos);
                // Temporarily add to root
                self.root_at(world, pos, 1.0);
                self.plants.push(plant);
            }
        }

        let mut i = 0;
        while i < self.plants.len() {
            // Grow
            self.plants[i].grow();

            // Die?
            if self.plants[i].die(world, &mut rng) {
                // Remove root influence
                let pos = self.plants[i].pos;
                self.root_at(world, pos, -1.0);
                self.plants.swap_remove(i);
                continue;
            }

            // Chance to reproduce (1 in 20)
            if rng.gen_range(0..20) == 0 {
                let offset_x = rng.gen_range(-4..5) as f32;
                let offset_y = rng.gen_range(-4..5) as f32;
                let npos = self.plants[i].pos + Vec2::new(offset_x, offset_y);

                if world.map.oob_f(npos) {
                    i += 1;
                    continue;
                }

                if world.map.discharge_f(npos) >= Plant::MAX_DISCHARGE {
                    i += 1;
                    continue;
                }

                // Check root density at new position
                if let Some(cell) = world.map.get_cell(npos) {
                    if rng.gen_range(0..1000) as f32 / 1000.0 <= cell.root_density {
                        i += 1;
                        continue;
                    }
                }

                let ipos = npos.as_ivec2();
                let normal_y = world.map.get(ipos.x, ipos.y)
                    .map(|c| c.cached_normal.y)
                    .unwrap_or(0.0);
                if normal_y <= Plant::MAX_STEEP {
                    i += 1;
                    continue;
                }

                let plant = Plant::new(npos);
                self.root_at(world, npos, 1.0);
                self.plants.push(plant);
            }

            i += 1;
        }
    }

    fn root_at(&self, world: &mut World, pos: Vec2, factor: f32) {
        let offsets: [(f32, f32, f32); 9] = [
            (0.0, 0.0, 1.0),
            (1.0, 0.0, 0.6),
            (-1.0, 0.0, 0.6),
            (0.0, 1.0, 0.6),
            (0.0, -1.0, 0.6),
            (-1.0, -1.0, 0.4),
            (1.0, -1.0, 0.4),
            (-1.0, 1.0, 0.4),
            (1.0, 1.0, 0.4),
        ];

        for (dx, dy, weight) in &offsets {
            let p = pos + Vec2::new(*dx, *dy);
            if let Some(cell) = world.map.get_cell_mut(p) {
                cell.root_density += factor * weight;
            }
        }
    }
}
