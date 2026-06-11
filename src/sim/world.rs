// world.rs — World struct: noise generation, erosion cycle, cascade settling

use super::cell::*;
use super::water::Drop;
use bevy::math::Vec2;
use rand::Rng;
use rand::SeedableRng;
use rayon::prelude::*;

pub struct World {
    pub map: WorldMap,
    pub seed: u32,
    pub frame: u64,
}

impl World {
    pub const LRATE: f32 = 0.1;
    #[allow(dead_code)]
    pub const DISCHARGE_THRESH: f32 = 0.3;

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

        // Reset heights
        self.map.cells.par_iter_mut().for_each(|c| c.height = 0.0);

        let mut frequency = 1.0f32;
        let mut scale = 0.6f32;

        for _octave in 0..8 {
            noise.set_frequency(Some(frequency));

            // Parallel XY noise sampling within each octave
            self.map.cells.par_chunks_mut(WORLD_SIZE)
                .enumerate()
                .for_each(|(y, row)| {
                    let py = y as f32 / TILE_SIZE as f32;
                    for (x, cell) in row.iter_mut().enumerate() {
                        let px = x as f32 / TILE_SIZE as f32;
                        let h = noise.get_noise_2d(px, py);
                        cell.height += scale * h;
                    }
                });

            frequency *= 2.0;
            scale *= 0.6;
        }

        // --- Find min/max for normalization (parallel reduce) ---
        let (min, max) = self.map.cells.par_iter()
            .map(|c| (c.height, c.height))
            .reduce(
                || (f32::MAX, f32::MIN),
                |(min_a, max_a), (min_b, max_b)| {
                    (min_a.min(min_b), max_a.max(max_b))
                },
            );

        // --- Layer 2: Normalization noise pass ---
        let mut noise2 = FastNoiseLite::with_seed(self.seed as i32);
        noise2.set_noise_type(Some(NoiseType::OpenSimplex2));
        noise2.set_fractal_type(Some(FractalType::FBm));
        noise2.set_fractal_octaves(Some(1));
        noise2.set_fractal_lacunarity(Some(2.0));
        noise2.set_fractal_gain(Some(0.6));
        noise2.set_frequency(Some(1.0));

        let range = max - min;
        self.map.cells.par_chunks_mut(WORLD_SIZE)
            .enumerate()
            .for_each(|(y, row)| {
                let py = y as f32 / TILE_SIZE as f32;
                for (x, cell) in row.iter_mut().enumerate() {
                    let px = x as f32 / TILE_SIZE as f32;
                    let scale_val = noise2.get_noise_2d(px, py);
                    let d = 0.1 + 0.5 * (1.0 + erf_approx(2.0 * scale_val));
                    cell.height = d * ((cell.height - min) / range);
                }
            });

        println!("... height generation complete");
    }

    /// Run `cycles` erosion iterations over the whole map (parallel).
    /// Uses thread-local delta buffers: each thread processes its own droplet batch
    /// on independent delta arrays, then results are merged back.
    pub fn erode(&mut self, cycles: usize) {
        let num_threads = rayon::current_num_threads().min(cycles.max(1));
        let per_thread = cycles / num_threads;

        let base_seed = self.seed as u64 ^ (self.frame.wrapping_mul(0x9E3779B97F4A7C15));

        // === Phase 1: Parallel droplet execution with thread-local deltas ===
        let thread_results: Vec<_> = (0..num_threads).into_par_iter().map(|t| {
            // Thread-local delta arrays
            let mut height_delta = vec![0.0f32; WORLD_AREA];
            let mut discharge_track = vec![0.0f32; WORLD_AREA];
            let mut momentum_x_track = vec![0.0f32; WORLD_AREA];
            let mut momentum_y_track = vec![0.0f32; WORLD_AREA];

            let thread_seed = base_seed ^ ((t as u64).wrapping_mul(0xDEADBEEF));
            let mut rng = rand::rngs::StdRng::seed_from_u64(thread_seed);

            let n = if t == num_threads - 1 {
                cycles - per_thread * (num_threads - 1)
            } else {
                per_thread
            };

            for _ in 0..n {
                let rx = rng.gen_range(0..TILE_SIZE as i32);
                let ry = rng.gen_range(0..TILE_SIZE as i32);
                let newpos = Vec2::new(rx as f32, ry as f32);

                // Check spawn height against base + delta
                if self.map.height_f_delta(&height_delta, newpos) < 0.1 {
                    continue;
                }

                let mut drop = Drop::new(newpos);
                while drop.descend_delta(
                    &self.map,
                    &mut height_delta,
                    &mut discharge_track,
                    &mut momentum_x_track,
                    &mut momentum_y_track,
                ) {
                    // Cascade after each step (within thread-local delta)
                    self.map.cascade_delta(&mut height_delta, drop.pos);
                }
            }

            (height_delta, discharge_track, momentum_x_track, momentum_y_track)
        }).collect();

        // === Phase 2: Reset main track fields (parallel) ===
        self.map.cells.par_iter_mut().for_each(|c| {
            c.discharge_track = 0.0;
            c.momentum_x_track = 0.0;
            c.momentum_y_track = 0.0;
        });

        // === Phase 3: Merge thread results into main grid ===
        // Height: average deltas across threads, track: sum deltas across threads
        let nf = num_threads as f32;
        self.map.cells.par_iter_mut().enumerate().for_each(|(i, cell)| {
            let mut h_sum = 0.0f32;
            let mut dt_sum = 0.0f32;
            let mut mxt_sum = 0.0f32;
            let mut myt_sum = 0.0f32;
            for result in &thread_results {
                h_sum += result.0[i];
                dt_sum += result.1[i];
                mxt_sum += result.2[i];
                myt_sum += result.3[i];
            }
            cell.height += h_sum / nf;
            cell.discharge_track = dt_sum;
            cell.momentum_x_track = mxt_sum;
            cell.momentum_y_track = myt_sum;
        });

        // === Phase 4: Leak track values into display fields (parallel) ===
        self.map.cells.par_iter_mut().for_each(|c| {
            c.discharge = (1.0 - Self::LRATE) * c.discharge + Self::LRATE * c.discharge_track;
            c.momentum_x = (1.0 - Self::LRATE) * c.momentum_x + Self::LRATE * c.momentum_x_track;
            c.momentum_y = (1.0 - Self::LRATE) * c.momentum_y + Self::LRATE * c.momentum_y_track;
        });

        self.frame += 1;
    }
}

/// Cheap erf approximation
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
