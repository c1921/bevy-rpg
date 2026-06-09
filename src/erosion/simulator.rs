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
    let mut real = Vec::with_capacity(n);
    let mut imag = Vec::with_capacity(n);

    // Collect into Vec by iterating rows in parallel.
    let pairs: Vec<(Vec<f64>, Vec<f64>)> = (0..h)
        .into_par_iter()
        .map(|y| {
            let ym1 = ((y as i64 - 1).rem_euclid(h as i64)) as usize;
            let yp1 = (y + 1) % h;
            let mut r_row = vec![0.0; w];
            let mut i_row = vec![0.0; w];
            for x in 0..w {
                let xm1 = ((x as i64 - 1).rem_euclid(w as i64)) as usize;
                let xp1 = (x + 1) % w;
                // dx = 0.5*(a[i-1, j] - a[i+1, j])  → imag
                i_row[x] = 0.5 * (hm.get(x, ym1) - hm.get(x, yp1));
                // dy = 0.5*(a[i, j-1] - a[i, j+1])  → real
                r_row[x] = 0.5 * (hm.get(xm1, y) - hm.get(xp1, y));
            }
            (r_row, i_row)
        })
        .collect();

    for (r, i) in pairs {
        real.extend(r);
        imag.extend(i);
    }

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

            let mut accum = 0.0;

            // 9 neighbours: dx, dy ∈ {-1, 0, 1}
            for dx in -1i64..=1 {
                for dy in -1i64..=1 {
                    // source cell coordinates
                    let ii = ((i as i64 - dy).rem_euclid(h as i64)) as usize;
                    let jj = ((j as i64 - dx).rem_euclid(w as i64)) as usize;
                    let src_idx = ii * w + jj;

                    let v_real = grad_real[src_idx];
                    let v_imag = grad_imag[src_idx];

                    // weight in x direction
                    let wx = match dx {
                        -1 => (-v_real).max(0.0),
                        0 => 1.0 - v_real.abs(),
                        _ => v_real.max(0.0), // dx == 1
                    };

                    // weight in y direction
                    let wy = match dy {
                        -1 => (-v_imag).max(0.0),
                        0 => 1.0 - v_imag.abs(),
                        _ => v_imag.max(0.0), // dy == 1
                    };

                    let wgt = wx * wy;
                    if wgt > 0.0 {
                        accum += wgt * source.data[src_idx];
                    }
                }
            }

            *val = accum;
        });

    out
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
            let mut off_real = grad.real.clone();
            let mut off_imag = grad.imag.clone();
            off_real.par_iter_mut().for_each(|v| *v = -*v);
            off_imag.par_iter_mut().for_each(|v| *v = -*v);
            let neighbour = sample(terrain, &off_real, &off_imag);

            // height_delta = terrain - neighbour  (positive = downhill)
            let mut height_delta = terrain.data.clone();
            height_delta
                .par_iter_mut()
                .zip(neighbour.data.par_iter())
                .for_each(|(t, n)| *t -= n);

            // ── 4. Sediment capacity ──────────────────────────
            let mut sediment_cap = height_delta.clone();
            sediment_cap
                .par_iter_mut()
                .zip(velocity.par_iter())
                .zip(water.par_iter())
                .for_each(|((cap, vel), wat)| {
                    let hd = cap.max(cfg.min_height_delta);
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
            apply_slippage(terrain, cfg);

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
