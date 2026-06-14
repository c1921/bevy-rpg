// world.rs — ParticleWorld: erosion cycle with thread-local delta buffers
// Adapted from reference project (sim/world.rs).

use super::cell::*;
use super::water::Drop;
use bevy::math::Vec2;
use rand::Rng;
use rand::SeedableRng;
use rayon::prelude::*;

pub struct ParticleWorld {
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

impl ParticleWorld {
    pub const LRATE: f32 = 0.1;

    pub fn new(width: usize, height: usize, seed: u32) -> Self {
        Self {
            map: WorldMap::new(width, height),
            seed,
            frame: 0,
            thread_bufs: Vec::new(),
        }
    }

    /// Initialize cell heights from a flat heightmap (row-major).
    /// Heights should be normalised to [0, 1].
    /// If `scale > 1`, the heightmap is downsampled via box-filter averaging.
    pub fn init_from_heightmap_scaled(
        &mut self,
        hm: &[f32],
        hm_width: usize,
        hm_height: usize,
        scale: usize,
    ) {
        let pw = self.map.width;
        let ph = self.map.height;
        assert_eq!(pw, (hm_width + scale - 1) / scale);
        assert_eq!(ph, (hm_height + scale - 1) / scale);

        for py in 0..ph {
            for px in 0..pw {
                // Box-filter average over scale×scale block.
                let mut sum = 0.0f32;
                let mut count = 0u32;
                for dy in 0..scale {
                    let sy = py * scale + dy;
                    if sy >= hm_height { continue; }
                    for dx in 0..scale {
                        let sx = px * scale + dx;
                        if sx >= hm_width { continue; }
                        sum += hm[sy * hm_width + sx];
                        count += 1;
                    }
                }
                let idx = py * pw + px;
                self.map.cells[idx].height = if count > 0 { sum / count as f32 } else { 0.0 };
                self.map.cells[idx].discharge = 0.0;
                self.map.cells[idx].momentum_x = 0.0;
                self.map.cells[idx].momentum_y = 0.0;
                self.map.cells[idx].discharge_track = 0.0;
                self.map.cells[idx].momentum_x_track = 0.0;
                self.map.cells[idx].momentum_y_track = 0.0;
                self.map.cells[idx].root_density = 0.0;
            }
        }

        self.map.recompute_normals();
    }

    /// Extract current cell heights into a flat `Vec<f32>` (row-major, same res as grid).
    pub fn extract_heights(&self) -> Vec<f32> {
        self.map.cells.iter().map(|c| c.height).collect()
    }

    /// Extract heights upsampled to a target resolution via bilinear interpolation.
    pub fn extract_heights_scaled(&self, target_w: usize, target_h: usize) -> Vec<f32> {
        let pw = self.map.width;
        let ph = self.map.height;
        let mut out = vec![0.0f32; target_w * target_h];

        for ty in 0..target_h {
            // Map target row to source float row.
            let sy = (ty as f32 + 0.5) / target_h as f32 * ph as f32 - 0.5;
            let sy0 = sy.floor() as i32;
            let sy1 = sy0 + 1;
            let fy = sy - sy0 as f32;

            for tx in 0..target_w {
                let sx = (tx as f32 + 0.5) / target_w as f32 * pw as f32 - 0.5;
                let sx0 = sx.floor() as i32;
                let sx1 = sx0 + 1;
                let fx = sx - sx0 as f32;

                let h = |x: i32, y: i32| -> f32 {
                    if x < 0 || y < 0 || x >= pw as i32 || y >= ph as i32 {
                        return 0.0;
                    }
                    self.map.cells[(y as usize) * pw + (x as usize)].height
                };

                let h00 = h(sx0, sy0);
                let h10 = h(sx1, sy0);
                let h01 = h(sx0, sy1);
                let h11 = h(sx1, sy1);

                let h0 = h00 + (h10 - h00) * fx;
                let h1 = h01 + (h11 - h01) * fx;
                out[ty * target_w + tx] = h0 + (h1 - h0) * fy;
            }
        }

        out
    }

    /// Run one erosion cycle: spawn `cycles` droplets across the map (parallel).
    /// Uses thread-local delta buffers.
    pub fn erode(&mut self, cycles: usize) {
        let num_threads = rayon::current_num_threads().min(cycles.max(1));
        let per_thread = cycles / num_threads;

        // Ensure pre-allocated buffers exist and are zeroed.
        self.ensure_bufs(num_threads);
        self.thread_bufs[..num_threads]
            .par_iter_mut()
            .for_each(|buf| buf.zero());

        let base_seed = self.seed as u64 ^ (self.frame.wrapping_mul(0x9E3779B97F4A7C15));

        // Split borrows: map is read-only, bufs are thread-local mutable.
        let map = &self.map;
        let bufs = &mut self.thread_bufs;
        let w = map.width as i32;
        let h = map.height as i32;

        // === Phase 1: Parallel droplet execution with thread-local deltas ===
        bufs[..num_threads]
            .par_iter_mut()
            .enumerate()
            .for_each(|(t, buf)| {
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
                    let rx = rng.gen_range(0..h);
                    let ry = rng.gen_range(0..w);
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
        let bufs = &self.thread_bufs;
        self.map
            .cells
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, cell)| {
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
                cell.momentum_x =
                    (1.0 - Self::LRATE) * cell.momentum_x + Self::LRATE * mxt_sum;
                cell.momentum_y =
                    (1.0 - Self::LRATE) * cell.momentum_y + Self::LRATE * myt_sum;
            });

        // Refresh cached normals after height change.
        self.map.recompute_normals();

        self.frame += 1;
    }

    /// Ensure `thread_bufs` has at least `n` allocated entries.
    fn ensure_bufs(&mut self, n: usize) {
        let area = self.map.area();
        while self.thread_bufs.len() < n {
            self.thread_bufs.push(ThreadBuf {
                height_delta: vec![0.0f32; area],
                discharge_track: vec![0.0f32; area],
                momentum_x_track: vec![0.0f32; area],
                momentum_y_track: vec![0.0f32; area],
            });
        }
    }
}
