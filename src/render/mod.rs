mod gradient;
mod lighting;
mod normals;

use gradient::{ocean_land_stops, sample_stops};
use lighting::{apply_lighting, coastline_mask};
use normals::compute_normals;

/// Render a normalised heightmap `[0, 1]` into an RGBA8 pixel buffer.
///
/// * `hm`        — `width * height` row‑major; row 0 = bottom of world.
/// * `sea_level` — height below which is ocean (e.g. 0.45).
/// * `snow_level`— height above which is snow (e.g. 0.9).
/// * `light_dir` — vector toward the light, e.g. `[-0.2, -0.5, 0.7]`.
/// * `ambient`   — ambient light fraction (0–1), e.g. 0.35.
/// * `strength`  — normal exaggeration, e.g. 6.0.
///
/// Returns `Vec<u8>` with `width * height * 4` bytes, top row first
/// (ready for `bevy::render::render_resource::Image`).
pub fn render_heightmap(
    hm: &[f32],
    width: usize,
    height: usize,
    sea_level: f32,
    snow_level: f32,
    light_dir: [f32; 3],
    ambient: f32,
    strength: f32,
) -> Vec<u8> {
    let n = width * height;
    assert_eq!(hm.len(), n, "heightmap size mismatch");

    // ── 1. Gamma tone: power 1.3 boosts contrast ──────────────
    let toned: Vec<f32> = hm.iter().map(|&v| v.clamp(0.0, 1.0).powf(1.3)).collect();

    // ── 2. Sobel normals on the toned map ─────────────────────
    let normals = compute_normals(&toned, width, height, strength);

    // ── 3. Altitude gradient ──────────────────────────────────
    let (ocean_stops, land_stops) = ocean_land_stops(sea_level, snow_level);

    // Ocean mask based on ORIGINAL height (before toning)
    let ocean_mask: Vec<bool> = hm.iter().map(|&v| v <= sea_level).collect();

    // Coastline for emphasis
    let coast = coastline_mask(&ocean_mask, width, height);

    // Pick a coastline highlight colour
    let coast_color = if !ocean_stops.is_empty() {
        sample_stops(&ocean_stops, (sea_level - 0.02).max(0.0))
    } else if !land_stops.is_empty() {
        sample_stops(&land_stops, sea_level)
    } else {
        [0.2, 0.55, 0.25]
    };

    // ── 4. Colour every pixel ─────────────────────────────────
    let mut pixels: Vec<[f32; 3]> = vec![[0.0; 3]; n];
    for i in 0..n {
        let t = toned[i];
        if ocean_mask[i] {
            // Ocean
            if !ocean_stops.is_empty() {
                let v = t.clamp(ocean_stops[0].pos, ocean_stops[ocean_stops.len() - 1].pos);
                pixels[i] = sample_stops(&ocean_stops, v);
            }
        } else {
            // Land
            if !land_stops.is_empty() {
                let v = t.clamp(land_stops[0].pos, land_stops[land_stops.len() - 1].pos);
                pixels[i] = sample_stops(&land_stops, v);
            }
        }
    }

    // Coastline blend
    for i in 0..n {
        if coast[i] {
            let blend = 0.65;
            pixels[i][0] = (blend * coast_color[0] + (1.0 - blend) * pixels[i][0]).clamp(0.0, 1.0);
            pixels[i][1] = (blend * coast_color[1] + (1.0 - blend) * pixels[i][1]).clamp(0.0, 1.0);
            pixels[i][2] = (blend * coast_color[2] + (1.0 - blend) * pixels[i][2]).clamp(0.0, 1.0);
        }
    }

    // ── 5. Lighting ───────────────────────────────────────────
    apply_lighting(&mut pixels, &normals, light_dir, ambient);

    // ── 6. Pack into RGBA8, flip vertically ───────────────────
    //   Input:  row 0 = bottom of world.
    //   Output: row 0 = top    of image (Bevy convention).
    let mut buf = vec![0u8; n * 4];
    for src_y in 0..height {
        let dst_y = height - 1 - src_y; // flip
        for x in 0..width {
            let src = src_y * width + x;
            let dst = dst_y * width + x;
            let px = pixels[src];
            buf[dst * 4    ] = (px[0] * 255.0).round() as u8;
            buf[dst * 4 + 1] = (px[1] * 255.0).round() as u8;
            buf[dst * 4 + 2] = (px[2] * 255.0).round() as u8;
            buf[dst * 4 + 3] = 255;
        }
    }

    buf
}
