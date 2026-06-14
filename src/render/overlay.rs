// overlay.rs — Blend discharge / momentum flow data onto rendered RGBA8 pixels.
//
// Logic matches the reference project's OverlayMode rendering in render.rs.

use crate::particle::cell::erf_approx;
use crate::particle::cell::WorldMap;
use crate::resources::OverlayMode;

/// Blend a discharge or momentum overlay into an RGBA8 pixel buffer in-place.
///
/// `pixels` must be `width * height * 4` bytes, row-major, top-row-first
/// (matching the output of `render_heightmap`). `map` provides per-cell
/// discharge / momentum values at the same resolution.
pub fn blend_overlay(pixels: &mut [u8], map: &WorldMap, mode: OverlayMode) {
    match mode {
        OverlayMode::None => { /* nothing to do */ }
        OverlayMode::Discharge => blend_discharge(pixels, map),
        OverlayMode::Momentum => blend_momentum(pixels, map),
        OverlayMode::DischargeOnly => blend_discharge_only(pixels, map),
    }
}

fn blend_discharge(pixels: &mut [u8], map: &WorldMap) {
    let w = map.width;
    let h = map.height;

    for y in 0..h {
        let src_y = h - 1 - y;
        let row_start = y * w;

        for x in 0..w {
            let cell = &map.cells[src_y * w + x];
            let d = (erf_approx(0.4 * cell.discharge) * 0.6).clamp(0.0, 1.0);

            let off = (row_start + x) * 4;
            let r = pixels[off] as f32 / 255.0;
            let g = pixels[off + 1] as f32 / 255.0;
            let b = pixels[off + 2] as f32 / 255.0;

            // Blend toward cyan-white (more visible than blue-white).
            pixels[off] = lerp_u8(r, 0.3, d);
            pixels[off + 1] = lerp_u8(g, 1.0, d);
            pixels[off + 2] = lerp_u8(b, 1.0, d);
        }
    }
}

fn blend_momentum(pixels: &mut [u8], map: &WorldMap) {
    let w = map.width;
    let h = map.height;
    let alpha = 0.8;

    for y in 0..h {
        let src_y = h - 1 - y;
        let row_start = y * w;

        for x in 0..w {
            let cell = &map.cells[src_y * w + x];
            let mx = (0.5 * (1.0 + erf_approx(cell.momentum_x))).clamp(0.0, 1.0);
            let my = (0.5 * (1.0 + erf_approx(cell.momentum_y))).clamp(0.0, 1.0);

            let off = (row_start + x) * 4;
            let r = pixels[off] as f32 / 255.0;
            let g = pixels[off + 1] as f32 / 255.0;
            let b = pixels[off + 2] as f32 / 255.0;

            pixels[off] = lerp_u8(r, mx, alpha);
            pixels[off + 1] = lerp_u8(g, my, alpha);
            pixels[off + 2] = lerp_u8(b, 0.2, alpha);
        }
    }
}

/// Pure discharge on black background — standalone river-map view.
fn blend_discharge_only(pixels: &mut [u8], map: &WorldMap) {
    let w = map.width;
    let h = map.height;

    for y in 0..h {
        let src_y = h - 1 - y;
        let row_start = y * w;

        for x in 0..w {
            let cell = &map.cells[src_y * w + x];
            // Discharge intensity: map erf(d) to [0, 1]
            let d = (erf_approx(0.4 * cell.discharge) * 0.7).clamp(0.0, 1.0);

            let off = (row_start + x) * 4;
            // Black background, discharge → cyan → white
            pixels[off] = (d * 0.3 * 255.0) as u8;
            pixels[off + 1] = (d * 1.0 * 255.0) as u8;
            pixels[off + 2] = (d * 1.0 * 255.0) as u8;
            pixels[off + 3] = 255;
        }
    }
}

#[inline]
fn lerp_u8(a: f32, b: f32, t: f32) -> u8 {
    ((a + (b - a) * t) * 255.0).round() as u8
}
