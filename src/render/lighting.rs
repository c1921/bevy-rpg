pub(crate) fn apply_lighting(
    pixels: &mut [[f32; 3]],
    normals: &[[f32; 3]],
    light_dir: [f32; 3],
    ambient: f32,
) {
    // Normalise light direction
    let inv = 1.0 / (light_dir[0].powi(2) + light_dir[1].powi(2) + light_dir[2].powi(2)).sqrt();
    let lx = light_dir[0] * inv;
    let ly = light_dir[1] * inv;
    let lz = light_dir[2] * inv;

    let amb = ambient.clamp(0.0, 1.0);

    for (px, n) in pixels.iter_mut().zip(normals.iter()) {
        let diffuse = (n[0] * lx + n[1] * ly + n[2] * lz).max(0.0);
        let shading = amb + (1.0 - amb) * diffuse;
        px[0] = (px[0] * shading).clamp(0.0, 1.0);
        px[1] = (px[1] * shading).clamp(0.0, 1.0);
        px[2] = (px[2] * shading).clamp(0.0, 1.0);
    }
}

/// Pixels whose 4‑neighbourhood includes at least one ocean cell.
pub(crate) fn coastline_mask(ocean: &[bool], w: usize, h: usize) -> Vec<bool> {
    let n = w * h;
    let mut coast = vec![false; n];
    for y in 0..h {
        let ym1 = if y == 0 { h - 1 } else { y - 1 };
        let yp1 = if y + 1 == h { 0 } else { y + 1 };
        for x in 0..w {
            let xm1 = if x == 0 { w - 1 } else { x - 1 };
            let xp1 = if x + 1 == w { 0 } else { x + 1 };
            let idx = y * w + x;
            if ocean[idx] {
                continue;
            }
            // land pixel — check 4 neighbours
            if ocean[ym1 * w + x] || ocean[yp1 * w + x] || ocean[y * w + xm1] || ocean[y * w + xp1] {
                coast[idx] = true;
            }
        }
    }
    coast
}
