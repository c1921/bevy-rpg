use num_complex::Complex64;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rustfft::FftPlanner;

/// FFT-based power-law noise, matching Python `util.fbm(shape, p)`.
///
/// Returns a `rows × cols` grid of `f64` values normalized to `[0, 1]`.
pub fn fbm_fft(rows: usize, cols: usize, exponent: f64, seed: u64) -> Vec<f64> {
    let n = rows * cols;
    let mut rng = StdRng::seed_from_u64(seed);

    // ── 1. Random phase noise: exp(2πi · uniform(0,1)) ────────────
    let mut data: Vec<Complex64> = (0..n)
        .map(|_| {
            let angle = rng.r#gen::<f64>() * 2.0 * std::f64::consts::PI;
            Complex64::new(angle.cos(), angle.sin())
        })
        .collect();

    // ── 2. Forward 2-D FFT (rows → columns) ───────────────────────
    fft_2d(&mut data, rows, cols, FftDirection::Forward);

    // ── 3. Multiply by power-law envelope in frequency domain ─────
    let half_rows = rows as isize / 2;
    let half_cols = cols as isize / 2;
    for y in 0..rows {
        let fy = if (y as isize) <= half_rows {
            y as f64
        } else {
            y as f64 - rows as f64
        };
        for x in 0..cols {
            // Standard FFT order: [0..n/2, -n/2..-1]
            let fx = if (x as isize) <= half_cols {
                x as f64
            } else {
                x as f64 - cols as f64
            };
            let r = (fx * fx + fy * fy).sqrt();
            let idx = y * cols + x;
            let env = if r == 0.0 { 0.0 } else { r.powf(exponent) };
            data[idx] *= Complex64::new(env, 0.0);
        }
    }

    // ── 4. Inverse 2-D FFT ───────────────────────────────────────
    fft_2d(&mut data, rows, cols, FftDirection::Inverse);

    // ── 5. Take real part, normalize to [0, 1] ────────────────────
    let inv_n = 1.0 / n as f64;
    let mut out: Vec<f64> = data.iter().map(|c| c.re * inv_n).collect();
    let h_min = out.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let h_max = out.iter().copied().reduce(f64::max).unwrap_or(1.0);
    let range = if (h_max - h_min) < 1e-12 {
        1.0
    } else {
        h_max - h_min
    };
    for v in &mut out {
        *v = (*v - h_min) / range;
    }
    out
}

enum FftDirection {
    Forward,
    Inverse,
}

/// In‑place 2-D FFT (rows first, then columns, with transpose in between).
fn fft_2d(data: &mut [Complex64], rows: usize, cols: usize, dir: FftDirection) {
    let mut planner = FftPlanner::new();

    // Plan for rows (size = cols) and columns (size = rows).
    let fft_row = match dir {
        FftDirection::Forward => planner.plan_fft_forward(cols),
        FftDirection::Inverse => planner.plan_fft_inverse(cols),
    };
    let fft_col = match dir {
        FftDirection::Forward => planner.plan_fft_forward(rows),
        FftDirection::Inverse => planner.plan_fft_inverse(rows),
    };

    // ── FFT each row in-place ──────────────────────────────────
    for row in 0..rows {
        let start = row * cols;
        fft_row.process(&mut data[start..start + cols]);
    }

    // ── Transpose ──────────────────────────────────────────────
    let mut transposed = vec![Complex64::new(0.0, 0.0); rows * cols];
    for y in 0..rows {
        for x in 0..cols {
            transposed[x * rows + y] = data[y * cols + x];
        }
    }
    data.copy_from_slice(&transposed);

    // ── FFT each (transposed) row in-place ─────────────────────
    for col in 0..cols {
        let start = col * rows;
        fft_col.process(&mut data[start..start + rows]);
    }

    // ── Transpose back ─────────────────────────────────────────
    for y in 0..rows {
        for x in 0..cols {
            transposed[y * cols + x] = data[x * rows + y];
        }
    }
    data.copy_from_slice(&transposed);
}
