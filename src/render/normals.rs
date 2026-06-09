use rayon::prelude::*;

/// Sobel‑like central‑difference normals from a heightmap.
///
/// `hm` is `width * height` row‑major, row 0 = bottom.
/// Returns unit normals, same layout.
pub(crate) fn compute_normals(hm: &[f32], w: usize, h: usize, strength: f32) -> Vec<[f32; 3]> {
    let n = w * h;
    let mut normals = vec![[0.0f32; 3]; n];

    normals
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row_out)| {
            let ym1 = if y == 0 { h - 1 } else { y - 1 };
            let yp1 = if y + 1 == h { 0 } else { y + 1 };
            for x in 0..w {
                let xm1 = if x == 0 { w - 1 } else { x - 1 };
                let xp1 = if x + 1 == w { 0 } else { x + 1 };

                let dx = 0.5 * (hm[yp1 * w + x] - hm[ym1 * w + x]);
                let dy = 0.5 * (hm[y * w + xp1] - hm[y * w + xm1]);

                let nx = -dx * strength;
                let ny = -dy * strength;
                let nz = 1.0;

                let inv_len = 1.0 / (nx * nx + ny * ny + nz * nz).sqrt();
                row_out[x] = [nx * inv_len, ny * inv_len, nz * inv_len];
            }
        });

    normals
}
