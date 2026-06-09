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
    let mut g = Gradient {
        real: vec![0.0_f64; n],
        imag: vec![0.0_f64; n],
        width: w,
        height: h,
    };
    simple_gradient_into(hm, &mut g);
    g
}

/// Fill `out` with the gradient of `hm` — reuses caller‑supplied buffers.
pub fn simple_gradient_into(hm: &Heightmap, out: &mut Gradient) {
    let w = hm.width;
    let h = hm.height;

    // Write directly into pre‑allocated buffers via parallel chunks.
    out.real
        .par_chunks_mut(w)
        .zip(out.imag.par_chunks_mut(w))
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
}



/// Fill `dst` with advected values — reuses caller‑supplied buffer.
pub fn displace_into(
    src: &[f64],
    grad_real: &[f64],
    grad_imag: &[f64],
    w: usize,
    h: usize,
    dst: &mut [f64],
) {
    dst.par_iter_mut()
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
            let w00 = wgt(grad_real[s00], grad_imag[s00], 1, 1);
            let w01 = wgt(grad_real[s01], grad_imag[s01], 0, 1);
            let w02 = wgt(grad_real[s02], grad_imag[s02], -1, 1);
            let w10 = wgt(grad_real[s10], grad_imag[s10], 1, 0);
            let w11 = wgt(grad_real[s11], grad_imag[s11], 0, 0);
            let w12 = wgt(grad_real[s12], grad_imag[s12], -1, 0);
            let w20 = wgt(grad_real[s20], grad_imag[s20], 1, -1);
            let w21 = wgt(grad_real[s21], grad_imag[s21], 0, -1);
            let w22 = wgt(grad_real[s22], grad_imag[s22], -1, -1);

            let mut accum = 0.0;
            accum += w00 * src[s00];
            accum += w01 * src[s01];
            accum += w02 * src[s02];
            accum += w10 * src[s10];
            accum += w11 * src[s11];
            accum += w12 * src[s12];
            accum += w20 * src[s20];
            accum += w21 * src[s21];
            accum += w22 * src[s22];

            *val = accum;
        });
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

/// Fused downhill sample + delta: `out[i] = hm[i] - hm.sample_bilinear(col+grad_real, row+grad_imag)`.
///
/// Eliminates the intermediate `off_real` / `off_imag` / neighbour arrays.
pub fn sample_downhill_delta(
    hm: &Heightmap,
    grad_real: &[f64],
    grad_imag: &[f64],
    out: &mut [f64],
) {
    let w = hm.width;
    out.par_iter_mut()
        .enumerate()
        .for_each(|(idx, d)| {
            let y = idx / w;
            let x = idx % w;
            // Downhill = follow gradient: sample at (col + grad_real, row + grad_imag).
            let sx = x as f64 + grad_real[idx];
            let sy = y as f64 + grad_imag[idx];
            *d = hm.data[idx] - hm.sample_bilinear(sx, sy);
        });
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

        // ── Pre‑allocated scratch buffers (reused every iteration) ──
        let mut grad = simple_gradient(terrain); // allocate once, reuse
        let mut height_delta = vec![0.0_f64; n];
        let mut sediment_cap = vec![0.0_f64; n];
        let mut deposited = vec![0.0_f64; n];
        let mut sediment_tmp = vec![0.0_f64; n];
        let mut water_tmp = vec![0.0_f64; n];

        for _iter in 0..cfg.iterations {
            // ── 1. Rain ────────────────────────────────────────
            water
                .par_iter_mut()
                .for_each(|v| *v += rand::random::<f64>() * cfg.rain_rate * cell_area);

            // ── 2. Gradient + normalise ───────────────────────
            simple_gradient_into(terrain, &mut grad);

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

            // ── 3. Height delta (fused: no off_real/off_imag/neighbour) ─
            sample_downhill_delta(terrain, &grad.real, &grad.imag, &mut height_delta);

            // ── 4. Sediment capacity ──────────────────────────
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
            displace_into(&sediment, &grad.real, &grad.imag, w, h, &mut sediment_tmp);
            std::mem::swap(&mut sediment, &mut sediment_tmp);
            displace_into(&water, &grad.real, &grad.imag, w, h, &mut water_tmp);
            std::mem::swap(&mut water, &mut water_tmp);

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
