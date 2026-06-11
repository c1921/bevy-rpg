// world.rs — World struct: noise generation, erosion cycle, cascade settling

use super::cell::*;
use super::water::Drop;
use bevy::math::{Vec2, IVec2};
use rand::Rng;
use rand::SeedableRng;

pub struct World {
    pub map: WorldMap,
    pub seed: u32,
    pub frame: u64,
}

impl World {
    // Parameters (matching original World statics)
    pub const LRATE: f32 = 0.1;
    #[allow(dead_code)]
    pub const DISCHARGE_THRESH: f32 = 0.3;
    pub const MAXDIFF: f32 = 0.01;
    pub const SETTLING: f32 = 0.8;

    pub fn new(seed: u32) -> Self {
        let mut world = Self {
            map: WorldMap::new(),
            seed,
            frame: 0,
        };
        world.generate();
        world
    }

    /// Generate heightmap using FastNoiseLite (OpenSimplex2 + FBm, 8 octaves)
    pub fn generate(&mut self) {
        use fastnoise_lite::*;

        println!("Generating New World");
        println!("Seed: {}", self.seed);

        // --- Layer 1: 8-octave FBm noise ---
        let mut noise = FastNoiseLite::with_seed(self.seed as i32);
        noise.set_noise_type(Some(NoiseType::OpenSimplex2));
        noise.set_fractal_type(Some(FractalType::FBm));

        for cell in self.map.cells.iter_mut() {
            cell.height = 0.0;
        }

        let mut frequency = 1.0f32;
        let mut scale = 0.6f32;

        for _octave in 0..8 {
            noise.set_frequency(Some(frequency));

            for y in 0..WORLD_SIZE {
                for x in 0..WORLD_SIZE {
                    let px = x as f32 / TILE_SIZE as f32;
                    let py = y as f32 / TILE_SIZE as f32;
                    let h = noise.get_noise_2d(px, py);
                    if let Some(cell) = self.map.get_mut(x as i32, y as i32) {
                        cell.height += scale * h;
                    }
                }
            }

            frequency *= 2.0;
            scale *= 0.6;
        }

        // --- Find min/max for normalization ---
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for cell in self.map.cells.iter() {
            min = min.min(cell.height);
            max = max.max(cell.height);
        }

        // --- Layer 2: Normalization noise pass ---
        let mut noise2 = FastNoiseLite::with_seed(self.seed as i32);
        noise2.set_noise_type(Some(NoiseType::OpenSimplex2));
        noise2.set_fractal_type(Some(FractalType::FBm));
        noise2.set_fractal_octaves(Some(1));
        noise2.set_fractal_lacunarity(Some(2.0));
        noise2.set_fractal_gain(Some(0.6));
        noise2.set_frequency(Some(1.0));

        for y in 0..WORLD_SIZE {
            for x in 0..WORLD_SIZE {
                let px = x as f32 / TILE_SIZE as f32;
                let py = y as f32 / TILE_SIZE as f32;
                let scale_val = noise2.get_noise_2d(px, py);
                let d = 0.1 + 0.5 * (1.0 + erf_approx(2.0 * scale_val));
                if let Some(cell) = self.map.get_mut(x as i32, y as i32) {
                    cell.height = d * ((cell.height - min) / (max - min));
                }
            }
        }

        println!("... height generation complete");
    }

    /// Run `cycles` erosion iterations over the whole map
    pub fn erode(&mut self, cycles: usize) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(
            self.seed as u64 ^ (self.frame.wrapping_mul(0x9E3779B97F4A7C15)),
        );

        // Reset track fields
        for cell in self.map.cells.iter_mut() {
            cell.discharge_track = 0.0;
            cell.momentum_x_track = 0.0;
            cell.momentum_y_track = 0.0;
        }

        // Spawn and run particles
        for _ in 0..cycles {
            let rx = rng.gen_range(0..TILE_SIZE as i32);
            let ry = rng.gen_range(0..TILE_SIZE as i32);
            let newpos = Vec2::new(rx as f32, ry as f32);

            // Skip if too low (below water level ~0.1)
            if self.map.height_f(newpos) < 0.1 {
                continue;
            }

            let mut drop = Drop::new(newpos);
            while drop.descend(self) {}
        }

        // Leak track values into display fields
        for cell in self.map.cells.iter_mut() {
            cell.discharge =
                (1.0 - Self::LRATE) * cell.discharge + Self::LRATE * cell.discharge_track;
            cell.momentum_x =
                (1.0 - Self::LRATE) * cell.momentum_x + Self::LRATE * cell.momentum_x_track;
            cell.momentum_y =
                (1.0 - Self::LRATE) * cell.momentum_y + Self::LRATE * cell.momentum_y_track;
        }

        self.frame += 1;
    }

    /// Cascade settling: propagate excess height to lower neighbors
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

        struct Point {
            pos: IVec2,
            h: f32,
            d: f32,
        }

        let mut sn: [Option<Point>; 8] = [
            None, None, None, None, None, None, None, None,
        ];
        let mut num = 0;

        for (dx, dy) in &neighbors {
            let npos = IVec2::new(ipos.x + LOD_SIZE * dx, ipos.y + LOD_SIZE * dy);

            if self.map.oob_i(npos.x, npos.y) {
                continue;
            }

            let d = Vec2::new(*dx as f32, *dy as f32).length();

            if let Some(cell) = self.map.get(npos.x, npos.y) {
                sn[num] = Some(Point {
                    pos: npos,
                    h: cell.height,
                    d,
                });
                num += 1;
            }
        }

        // Sort by height ascending
        let mut points: Vec<&Point> = sn[..num].iter().filter_map(|p| p.as_ref()).collect();
        points.sort_by(|a, b| a.h.partial_cmp(&b.h).unwrap());

        let cur_h = self.map.height_i(ipos.x, ipos.y);

        for pt in &points {
            let diff = cur_h - pt.h;
            if diff == 0.0 {
                continue;
            }

            let excess = if pt.h > 0.1 {
                diff.abs() - pt.d * Self::MAXDIFF * LOD_SIZE_F
            } else {
                diff.abs()
            };

            if excess <= 0.0 {
                continue;
            }

            let transfer = Self::SETTLING * excess / 2.0;

            if diff > 0.0 {
                if let Some(cell) = self.map.get_mut(ipos.x, ipos.y) {
                    cell.height -= transfer;
                }
                if let Some(cell) = self.map.get_mut(pt.pos.x, pt.pos.y) {
                    cell.height += transfer;
                }
            } else {
                if let Some(cell) = self.map.get_mut(ipos.x, ipos.y) {
                    cell.height += transfer;
                }
                if let Some(cell) = self.map.get_mut(pt.pos.x, pt.pos.y) {
                    cell.height -= transfer;
                }
            }
        }
    }
}

/// Cheap erf approximation (duplicated here to avoid circular dep)
#[inline]
fn erf_approx(x: f32) -> f32 {
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
