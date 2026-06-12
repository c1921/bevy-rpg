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
    /// Pre-allocated per-thread delta buffers (avoid per-frame allocation).
    thread_bufs: Vec<ThreadBuf>,
}

/// One set of thread-local delta arrays.
struct ThreadBuf {
    height_delta: Vec<f32>,
    discharge_track: Vec<f32>,
    momentum_x_track: Vec<f32>,
    momentum_y_track: Vec<f32>,
}

impl ThreadBuf {
    fn zero(&mut self) {
        self.height_delta.fill(0.0);
        self.discharge_track.fill(0.0);
        self.momentum_x_track.fill(0.0);
        self.momentum_y_track.fill(0.0);
    }
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
            thread_bufs: Vec::new(),
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
                    let d = 0.1 + 0.5 * (1.0 + crate::sim::cell::erf_approx(2.0 * scale_val));
                    cell.height = d * ((cell.height - min) / range);
                }
            });

        println!("... height generation complete");

        // Precompute normals for rendering
        self.map.recompute_normals();
    }

    /// Run `cycles` erosion iterations over the whole map (parallel).
    /// Uses thread-local delta buffers: each thread processes its own droplet batch
    /// on independent delta arrays, then results are merged back.
    pub fn erode(&mut self, cycles: usize) {
        let num_threads = rayon::current_num_threads().min(cycles.max(1));
        let per_thread = cycles / num_threads;

        // Ensure pre-allocated buffers exist and are zeroed
        self.ensure_bufs(num_threads);
        // Zero in parallel (each buffer is independent)
        self.thread_bufs[..num_threads]
            .par_iter_mut()
            .for_each(|buf| buf.zero());

        let base_seed = self.seed as u64 ^ (self.frame.wrapping_mul(0x9E3779B97F4A7C15));

        // Split borrows: map is read-only, bufs are thread-local mutable
        let map = &self.map;
        let bufs = &mut self.thread_bufs;

        // === Phase 1: Parallel droplet execution with thread-local deltas ===
        bufs[..num_threads].par_iter_mut().enumerate().for_each(|(t, buf)| {
            let thread_seed = base_seed ^ ((t as u64).wrapping_mul(0xDEADBEEF));
            let mut rng = rand::rngs::StdRng::seed_from_u64(thread_seed);

            let n = if t == num_threads - 1 {
                cycles - per_thread * (num_threads - 1)
            } else {
                per_thread
            };

            let height_delta = &mut buf.height_delta;
            let discharge_track = &mut buf.discharge_track;
            let momentum_x_track = &mut buf.momentum_x_track;
            let momentum_y_track = &mut buf.momentum_y_track;

            for _ in 0..n {
                let rx = rng.gen_range(0..TILE_SIZE as i32);
                let ry = rng.gen_range(0..TILE_SIZE as i32);
                let newpos = Vec2::new(rx as f32, ry as f32);

                if map.height_f_delta(height_delta, newpos) < 0.1 {
                    continue;
                }

                let mut drop = Drop::new(newpos);
                while drop.descend_delta(
                    map,
                    height_delta,
                    discharge_track,
                    momentum_x_track,
                    momentum_y_track,
                ) {
                    map.cascade_delta(height_delta, drop.pos);
                }
            }
        });

        // === Phase 2: Merge thread results + leak track into display (single pass) ===
        let nf = num_threads as f32;
        let bufs = &self.thread_bufs; // immutable borrow for reading
        self.map.cells.par_iter_mut().enumerate().for_each(|(i, cell)| {
            let mut h_sum = 0.0f32;
            let mut dt_sum = 0.0f32;
            let mut mxt_sum = 0.0f32;
            let mut myt_sum = 0.0f32;
            for t in 0..num_threads {
                let buf = &bufs[t];
                h_sum += buf.height_delta[i];
                dt_sum += buf.discharge_track[i];
                mxt_sum += buf.momentum_x_track[i];
                myt_sum += buf.momentum_y_track[i];
            }
            cell.height += h_sum / nf;
            cell.discharge = (1.0 - Self::LRATE) * cell.discharge + Self::LRATE * dt_sum;
            cell.momentum_x = (1.0 - Self::LRATE) * cell.momentum_x + Self::LRATE * mxt_sum;
            cell.momentum_y = (1.0 - Self::LRATE) * cell.momentum_y + Self::LRATE * myt_sum;
        });

        // Refresh cached normals after height change
        self.map.recompute_normals();

        self.frame += 1;
    }

    /// Ensure `thread_bufs` has at least `n` allocated entries.
    fn ensure_bufs(&mut self, n: usize) {
        while self.thread_bufs.len() < n {
            self.thread_bufs.push(ThreadBuf {
                height_delta: vec![0.0f32; WORLD_AREA],
                discharge_track: vec![0.0f32; WORLD_AREA],
                momentum_x_track: vec![0.0f32; WORLD_AREA],
                momentum_y_track: vec![0.0f32; WORLD_AREA],
            });
        }
    }
}

