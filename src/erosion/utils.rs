use rayon::prelude::*;
use crate::erosion::heightmap::{Gradient, Heightmap};

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
