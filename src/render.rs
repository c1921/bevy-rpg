// ── Pseudo‑3D heightmap renderer ─────────────────────────────────
// Ported from Python simple_map/rendering.py  render_realistic().
//
// Pipeline:  heightmap [0,1] → gamma‑tone → Sobel normals →
//   altitude‑gradient colour → ambient+diffuse lighting → RGBA8 buffer.

// ── Gradient stop ──────────────────────────────────────────────────

#[derive(Clone)]
struct Stop {
    pos: f32,
    r: f32,
    g: f32,
    b: f32,
}

/// Sample a gradient at `pos` (clamped to the stop range); linear interp.
fn sample_stops(stops: &[Stop], pos: f32) -> [f32; 3] {
    if stops.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    let p = pos.clamp(stops[0].pos, stops[stops.len() - 1].pos);

    // Find the interval [stops[i], stops[i+1]] that contains p.
    let mut i = 0;
    while i + 1 < stops.len() && stops[i + 1].pos < p {
        i += 1;
    }
    if i + 1 >= stops.len() {
        let s = &stops[i];
        return [s.r, s.g, s.b];
    }

    let a = &stops[i];
    let b = &stops[i + 1];
    let t = if (b.pos - a.pos).abs() > 1e-7 {
        (p - a.pos) / (b.pos - a.pos)
    } else {
        0.5
    };
    [
        a.r + t * (b.r - a.r),
        a.g + t * (b.g - a.g),
        a.b + t * (b.b - a.b),
    ]
}

/// Build the ocean_land gradient, split at `sea_level`.
///
/// Returns `(ocean_stops, land_stops)`.  The colour progression is:
///   deep‑blue → mid‑blue → light‑blue → green → yellow‑green →
///   brown → grey → white.
fn ocean_land_stops(sea: f32, snow: f32) -> (Vec<Stop>, Vec<Stop>) {
    let sea = sea.clamp(0.05, 0.95);
    let snow = snow.clamp(0.5, 1.0);
    let span = (snow - sea).max(1e-4);

    let land_pos = |rel: f32| -> f32 { (sea + rel * span).min(0.999) };

    // All stops (unsplit)
    let all: Vec<Stop> = vec![
        Stop { pos: 0.00                                                 , r: 8.0/255.0,  g: 20.0/255.0,  b: 65.0/255.0  },
        Stop { pos: (sea * 0.35).max(0.02)                              , r: 17.0/255.0, g: 46.0/255.0,  b: 110.0/255.0 },
        Stop { pos: (sea * 0.8).max(0.04)                                , r: 34.0/255.0, g: 78.0/255.0,  b: 138.0/255.0 },
        Stop { pos: sea                                                   , r: 52.0/255.0, g: 112.0/255.0, b: 64.0/255.0  },
        Stop { pos: land_pos(0.08)                                        , r: 66.0/255.0, g: 131.0/255.0, b: 62.0/255.0  },
        Stop { pos: land_pos(0.20)                                        , r: 122.0/255.0,g: 154.0/255.0, b: 60.0/255.0  },
        Stop { pos: land_pos(0.40)                                        , r: 213.0/255.0,g: 191.0/255.0, b: 101.0/255.0 },
        Stop { pos: land_pos(0.55)                                        , r: 210.0/255.0,g: 143.0/255.0, b: 65.0/255.0  },
        Stop { pos: land_pos(0.85)                                        , r: 147.0/255.0,g: 72.0/255.0,  b: 33.0/255.0  },
        Stop { pos: land_pos(0.95)                                        , r: 128.0/255.0,g: 128.0/255.0, b: 128.0/255.0 },
        Stop { pos: land_pos(0.98)                                        , r: 210.0/255.0,g: 210.0/255.0, b: 210.0/255.0 },
        Stop { pos: snow                                                   , r: 0.94,       g: 0.94,        b: 0.94         },
        Stop { pos: 1.0                                                    , r: 1.0,        g: 1.0,         b: 1.0          },
    ];

    // Split at sea_level
    let eps = 1e-4;
    let split = sea - eps;

    let ocean: Vec<Stop> = {
        let mut v: Vec<Stop> = all.iter()
            .filter(|s| s.pos <= split)
            .cloned()
            .collect();
        // Ensure a stop exactly at the split boundary
        let last_pos = v.last().map(|s| s.pos).unwrap_or(0.0);
        if (last_pos - split).abs() > 1e-7 && split > 0.0 {
            let c = sample_stops(&all, split);
            v.push(Stop { pos: split, r: c[0], g: c[1], b: c[2] });
        }
        v
    };

    let land: Vec<Stop> = {
        let mut v: Vec<Stop> = all.iter()
            .filter(|s| s.pos >= sea + eps)
            .cloned()
            .collect();
        // Ensure a stop exactly at sea_level
        let first_pos = v.first().map(|s| s.pos).unwrap_or(1.0);
        if (first_pos - sea).abs() > 1e-7 && sea < 1.0 {
            let c = sample_stops(&all, sea);
            v.insert(0, Stop { pos: sea, r: c[0], g: c[1], b: c[2] });
        }
        v
    };

    (ocean, land)
}

// ── Normals ────────────────────────────────────────────────────────

/// Sobel‑like central‑difference normals from a heightmap.
///
/// `hm` is `width * height` row‑major, row 0 = bottom.
/// Returns unit normals, same layout.
fn compute_normals(hm: &[f32], w: usize, h: usize, strength: f32) -> Vec<[f32; 3]> {
    let n = w * h;
    let mut normals = vec![[0.0f32; 3]; n];

    for y in 0..h {
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
            let idx = y * w + x;
            normals[idx] = [nx * inv_len, ny * inv_len, nz * inv_len];
        }
    }

    normals
}

// ── Lighting ───────────────────────────────────────────────────────

fn apply_lighting(
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

// ── Coastline mask ──────────────────────────────────────────────────

/// Pixels whose 4‑neighbourhood includes at least one ocean cell.
fn coastline_mask(ocean: &[bool], w: usize, h: usize) -> Vec<bool> {
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

// ── Main entry point ───────────────────────────────────────────────

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
