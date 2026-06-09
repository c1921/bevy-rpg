use rayon::prelude::*;
use crate::erosion::heightmap::{Gradient, Heightmap};

// ── Utility functions ────────────────────────────────────────────

/// Central finite‑difference gradient with periodic wrap.
///
/// For each cell `(i, j)` (row i, col j):
///   `imag[i,j]` = 0.5 * (a[i-1, j] - a[i+1, j])   ← Python `dx`
///   `real[i,j]` = 0.5 * (a[i, j-1] - a[i, j+1])   ← Python `dy`
pub fn simple_gradient(hm: &Heightmap) -> Gradient {
    let w = hm.width;
    let h = hm.height;
    let n = w * h;
    let mut real = vec![0.0_f64; n];
    let mut imag = vec![0.0_f64; n];

    // Write directly into pre‑allocated buffers via parallel chunks.
    real.par_chunks_mut(w)
        .zip(imag.par_chunks_mut(w))
        .enumerate()
        .for_each(|(y, (r_row, i_row))| {
            let ym1 = ((y as i64 - 1).rem_euclid(h as i64)) as usize;
            let yp1 = (y + 1) % h;
            for x in 0..w {
                let xm1 = ((x as i64 - 1).rem_euclid(w as i64)) as usize;
                let xp1 = (x + 1) % w;
                // dx = 0.5*(a[i-1, j] - a[i+1, j])  → imag
                i_row[x] = 0.5 * (hm.get(x, ym1) - hm.get(x, yp1));
                // dy = 0.5*(a[i, j-1] - a[i, j+1])  → real
                r_row[x] = 0.5 * (hm.get(xm1, y) - hm.get(xp1, y));
            }
        });

    Gradient { real, imag, width: w, height: h }
}

/// Bilinear sample every cell at offset `(off_real, off_imag)` from
/// its own position.  Returns a new `Heightmap`.
///
/// `off_real[j + i * w]` is subtracted from the column coordinate,
/// `off_imag[j + i * w]` from the row coordinate.
pub fn sample(hm: &Heightmap, off_real: &[f64], off_imag: &[f64]) -> Heightmap {
    let w = hm.width;
    let h = hm.height;
    let mut out = Heightmap::new(w, h, 0.0);

    out.data
        .par_iter_mut()
        .enumerate()
        .for_each(|(idx, val)| {
            let y = idx / w;
            let x = idx % w;
            let sx = x as f64 - off_real[idx];
            let sy = y as f64 - off_imag[idx];
            *val = hm.sample_bilinear(sx, sy);
        });

    out
}

/// Semi‑Lagrangian advection: each output cell gathers from its 8
/// neighbours weighted by the source cell's gradient direction.
///
/// `grad_real`, `grad_imag` should be a **unit** vector field (as
/// produced by normalising `Gradient`).
pub fn displace(source: &Heightmap, grad_real: &[f64], grad_imag: &[f64]) -> Heightmap {
    let w = source.width;
    let h = source.height;
    let mut out = Heightmap::new(w, h, 0.0);

    out.data
        .par_iter_mut()
        .enumerate()
        .for_each(|(idx, val)| {
            let i = idx / w; // row (y)
            let j = idx % w; // col (x)

            // Pre‑compute neighbour row/col indices with wrap.
            let ii_neg = ((i as i64 - 1).rem_euclid(h as i64)) as usize;
            let ii_zer = i;
            let ii_pos = (i + 1) % h;
            let jj_neg = ((j as i64 - 1).rem_euclid(w as i64)) as usize;
            let jj_zer = j;
            let jj_pos = (j + 1) % w;

            // Source neighbour indices (row-major).
            let s00 = ii_neg * w + jj_neg;
            let s01 = ii_neg * w + jj_zer;
            let s02 = ii_neg * w + jj_pos;
            let s10 = ii_zer * w + jj_neg;
            let s11 = ii_zer * w + jj_zer;
            let s12 = ii_zer * w + jj_pos;
            let s20 = ii_pos * w + jj_neg;
            let s21 = ii_pos * w + jj_zer;
            let s22 = ii_pos * w + jj_pos;

            // Weights: for each source cell, compute from its gradient.
            // Flow direction is FROM source TOWARD target (i,j).
            let w00 = wgt(grad_real[s00], grad_imag[s00],  1,  1);
            let w01 = wgt(grad_real[s01], grad_imag[s01],  0,  1);
            let w02 = wgt(grad_real[s02], grad_imag[s02], -1,  1);
            let w10 = wgt(grad_real[s10], grad_imag[s10],  1,  0);
            let w11 = wgt(grad_real[s11], grad_imag[s11],  0,  0);
            let w12 = wgt(grad_real[s12], grad_imag[s12], -1,  0);
            let w20 = wgt(grad_real[s20], grad_imag[s20],  1, -1);
            let w21 = wgt(grad_real[s21], grad_imag[s21],  0, -1);
            let w22 = wgt(grad_real[s22], grad_imag[s22], -1, -1);

            let mut accum = 0.0;
            accum += w00 * source.data[s00];
            accum += w01 * source.data[s01];
            accum += w02 * source.data[s02];
            accum += w10 * source.data[s10];
            accum += w11 * source.data[s11];
            accum += w12 * source.data[s12];
            accum += w20 * source.data[s20];
            accum += w21 * source.data[s21];
            accum += w22 * source.data[s22];

            *val = accum;
        });

    out
}

/// Weight for flow direction `(dx, dy)` given gradient `(v_real, v_imag)`.
#[inline]
fn wgt(v_real: f64, v_imag: f64, dx: i64, dy: i64) -> f64 {
    let wx = match dx {
        -1 => (-v_real).max(0.0),
        0 => 1.0 - v_real.abs(),
        _ => v_real.max(0.0),
    };
    let wy = match dy {
        -1 => (-v_imag).max(0.0),
        0 => 1.0 - v_imag.abs(),
        _ => v_imag.max(0.0),
    };
    wx * wy
}

/// Precompute a 1‑D Gaussian kernel with periodic wrap.
pub fn gaussian_kernel_1d(sigma: f64, radius: usize) -> Vec<f64> {
    let two_s2 = 2.0 * sigma * sigma;
    let denom = 1.0 / (sigma * (2.0 * std::f64::consts::PI).sqrt());
    let mut k: Vec<f64> = (0..=2 * radius)
        .map(|i| {
            let x = i as isize - radius as isize;
            denom * (-(x as f64).powi(2) / two_s2).exp()
        })
        .collect();
    let sum: f64 = k.iter().sum();
    for v in &mut k {
        *v /= sum;
    }
    k
}

/// Separable 2‑pass spatial Gaussian blur with periodic wrap.
/// Approximation of `scipy.ndimage.gaussian_filter`.
pub fn gaussian_blur(hm: &Heightmap, sigma: f64) -> Heightmap {
    let radius = (3.0 * sigma).ceil() as usize;
    let kernel = gaussian_kernel_1d(sigma, radius);
    let r = radius;
    let w = hm.width;
    let h = hm.height;

    // ── horizontal pass ───────────────────────────────────────
    let mut tmp = Heightmap::new(w, h, 0.0);
    tmp.data
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row_out)| {
            let y_off = y * w;
            for x in 0..w {
                let mut sum = 0.0;
                for k in 0..=2 * r {
                    let sx = ((x as i64 + k as i64 - r as i64).rem_euclid(w as i64)) as usize;
                    sum += kernel[k] * hm.data[y_off + sx];
                }
                row_out[x] = sum;
            }
        });

    // ── vertical pass ─────────────────────────────────────────
    let mut out = Heightmap::new(w, h, 0.0);
    out.data
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row_out)| {
            for x in 0..w {
                let mut sum = 0.0;
                for k in 0..=2 * r {
                    let sy =
                        ((y as i64 + k as i64 - r as i64).rem_euclid(h as i64)) as usize;
                    sum += kernel[k] * tmp.data[sy * w + x];
                }
                row_out[x] = sum;
            }
        });

    out
}

// ── Erosion Config ───────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ErosionConfig {
    pub iterations: usize,
    pub rain_rate: f64,
    pub evaporation_rate: f64,
    pub min_height_delta: f64,
    pub repose_slope: f64,
    pub gravity: f64,
    pub sediment_capacity_constant: f64,
    pub dissolving_rate: f64,
    pub deposition_rate: f64,
    pub cell_width: f64,
    /// Run slope slippage (gaussian blur) every N iterations.
    /// 1 = every iteration, 3 = every 3rd iteration. Default: 3.
    pub slippage_interval: usize,
}

impl Default for ErosionConfig {
    fn default() -> Self {
        Self {
            iterations: 200,
            rain_rate: 0.0015,
            evaporation_rate: 0.0005,
            min_height_delta: 0.05,
            repose_slope: 0.03,
            gravity: 50.0,
            sediment_capacity_constant: 80.0,
            dissolving_rate: 0.4,
            deposition_rate: 0.0005,
            cell_width: 1.0,
            slippage_interval: 3,
        }
    }
}

// ── Erosion Simulator ────────────────────────────────────────────

pub struct ErosionSimulator {
    config: ErosionConfig,
}

impl ErosionSimulator {
    pub fn new(config: ErosionConfig) -> Self {
        Self { config }
    }

    /// Run the full hydraulic erosion loop on `terrain`, modifying it
    /// in‑place.
    pub fn simulate(&self, terrain: &mut Heightmap) {
        let cfg = &self.config;
        let cell_area = cfg.cell_width * cfg.cell_width;
        let w = terrain.width;
        let h = terrain.height;
        let n = w * h;

        // Persistent state arrays
        let mut sediment = vec![0.0f64; n];
        let mut water = vec![0.0f64; n];
        let mut velocity = vec![0.0f64; n];

        for _iter in 0..cfg.iterations {
            // ── 1. Rain ────────────────────────────────────────
            water
                .par_iter_mut()
                .for_each(|v| *v += rand::random::<f64>() * cfg.rain_rate * cell_area);

            // ── 2. Gradient (original sign convention) ────────
            let mut grad = simple_gradient(terrain);

            // Normalise to unit vectors; zero vectors → random direction
            let two_pi = 2.0 * std::f64::consts::PI;
            grad.real
                .par_iter_mut()
                .zip(grad.imag.par_iter_mut())
                .for_each(|(r, i)| {
                    let mag_sq = *r * *r + *i * *i;
                    if mag_sq < 1e-20 {
                        let angle = rand::random::<f64>() * two_pi;
                        *r = angle.cos();
                        *i = angle.sin();
                    } else {
                        let inv_mag = 1.0 / mag_sq.sqrt();
                        *r *= inv_mag;
                        *i *= inv_mag;
                    }
                });

            // ── 3. Height delta — sample downhill (-gradient) ─
            let off_real: Vec<f64> = grad.real.par_iter().map(|v| -v).collect();
            let off_imag: Vec<f64> = grad.imag.par_iter().map(|v| -v).collect();
            let neighbour = sample(terrain, &off_real, &off_imag);

            // height_delta = terrain - neighbour  (positive = downhill)
            let mut height_delta = vec![0.0_f64; n];
            height_delta
                .par_iter_mut()
                .zip(terrain.data.par_iter())
                .zip(neighbour.data.par_iter())
                .for_each(|((hd, t), n)| *hd = *t - n);

            // ── 4. Sediment capacity ──────────────────────────
            let mut sediment_cap = vec![0.0_f64; n];
            sediment_cap
                .par_iter_mut()
                .zip(height_delta.par_iter())
                .zip(velocity.par_iter())
                .zip(water.par_iter())
                .for_each(|(((cap, hd), vel), wat)| {
                    let hd = hd.max(cfg.min_height_delta);
                    *cap = hd / cfg.cell_width * *vel * *wat * cfg.sediment_capacity_constant;
                });

            // ── 5. Deposit / erode ────────────────────────────
            let mut deposited = vec![0.0f64; n];
            deposited
                .par_iter_mut()
                .zip(height_delta.par_iter())
                .zip(sediment.par_iter())
                .zip(sediment_cap.par_iter())
                .for_each(|(((dep, hd), sed), cap)| {
                    let amount = if *hd < 0.0 {
                        // uphill → deposit
                        hd.min(*sed) // negative or zero
                    } else if *sed > *cap {
                        // excess sediment → deposit
                        cfg.deposition_rate * (*sed - *cap)
                    } else {
                        // below capacity → erode (negative)
                        cfg.dissolving_rate * (*sed - *cap)
                    };
                    // clamp: cannot erode more than the height drop
                    *dep = amount.max(-(*hd));
                });

            // ── 6. Update terrain & sediment ──────────────────
            sediment
                .par_iter_mut()
                .zip(deposited.par_iter())
                .for_each(|(s, d)| *s -= d);
            terrain
                .data
                .par_iter_mut()
                .zip(deposited.par_iter())
                .for_each(|(t, d)| *t += d);

            // ── 7. Advect sediment & water along gradient ─────
            sediment = displace(
                &Heightmap {
                    data: sediment,
                    width: w,
                    height: h,
                },
                &grad.real,
                &grad.imag,
            )
            .data;
            water = displace(
                &Heightmap {
                    data: water,
                    width: w,
                    height: h,
                },
                &grad.real,
                &grad.imag,
            )
            .data;

            // ── 8. Slope slippage (thermal erosion) ───────────
            if _iter % cfg.slippage_interval == 0 {
                apply_slippage(terrain, cfg);
            }

            // ── 9. Update velocity ────────────────────────────
            velocity
                .par_iter_mut()
                .zip(height_delta.par_iter())
                .for_each(|(v, hd)| {
                    *v = cfg.gravity * hd.abs() / cfg.cell_width;
                });

            // ── 10. Evaporation ───────────────────────────────
            let evap_factor = 1.0 - cfg.evaporation_rate;
            water.par_iter_mut().for_each(|w| *w *= evap_factor);
        }
    }
}

/// In‑place slope slippage: cells whose gradient exceeds
/// `repose_slope` are replaced by a Gaussian‑blurred version.
fn apply_slippage(
    terrain: &mut Heightmap,
    cfg: &ErosionConfig,
) {
    // Compute slope magnitude: |grad| / cell_width
    let raw = simple_gradient(terrain);

    let w = terrain.width;
    let h = terrain.height;
    let n = w * h;

    // Magnitude of raw gradient per cell
    let mut slope_mag = vec![0.0f64; n];
    slope_mag
        .par_iter_mut()
        .zip(raw.real.par_iter())
        .zip(raw.imag.par_iter())
        .for_each(|((mag, r), i)| {
            *mag = (r * r + i * i).sqrt() / cfg.cell_width;
        });

    // Gaussian blur the terrain
    let smoothed = gaussian_blur(terrain, 1.5);

    // Replace steep cells with blurred values
    terrain
        .data
        .par_iter_mut()
        .zip(slope_mag.par_iter())
        .zip(smoothed.data.par_iter())
        .for_each(|((t, slope), s)| {
            if *slope > cfg.repose_slope {
                *t = *s;
            }
        });
}
