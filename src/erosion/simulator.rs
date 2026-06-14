use rayon::prelude::*;
use crate::erosion::heightmap::Heightmap;
use crate::erosion::utils::{displace_into, gaussian_blur, sample_downhill_delta, simple_gradient, simple_gradient_into};

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
            iterations: 150,
            rain_rate: 0.0015,
            evaporation_rate: 0.0005,
            min_height_delta: 0.05,
            repose_slope: 0.10,
            gravity: 25.0,
            sediment_capacity_constant: 50.0,
            dissolving_rate: 0.2,
            deposition_rate: 0.001,
            cell_width: 1.0,
            slippage_interval: 6,
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
