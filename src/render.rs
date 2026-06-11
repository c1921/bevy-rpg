// render.rs — Bevy 2D heightmap display with hillshaded terrain colormap

use crate::sim::cell::*;
use crate::sim::world::World;
use bevy::asset::RenderAssetUsages;
use bevy::math::Vec3;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

// ============================================================
//  Colormap & shading constants (from terrain-erosion-3-ways)
// ============================================================

/// Multi-stop terrain colormap (matches _TERRAIN_CMAP from util.py)
const TERRAIN_COLORMAP: [(f32, (f32, f32, f32)); 5] = [
    (0.00, (0.15, 0.30, 0.15)), // deep green lowlands
    (0.25, (0.30, 0.45, 0.30)), // mid green
    (0.50, (0.50, 0.50, 0.35)), // olive-brown
    (0.80, (0.40, 0.36, 0.33)), // grey-brown
    (1.00, (1.00, 1.00, 1.00)), // white peaks
];

/// Light direction (approximates az=270°, alt=30° from reference project).
/// We want light coming from the left/west, slightly above.
const LIGHT_DIR: Vec3 = Vec3::new(-0.8, 0.5, -0.3);

/// Vertical exaggeration applied to normals for hillshading
const VERT_EXAG: f32 = 10.0;



// ============================================================
//  SimState resource
// ============================================================

#[derive(Resource)]
pub struct SimState {
    pub world: World,
    pub paused: bool,
    pub view_mode: ViewMode,
    pub view_overlay: OverlayMode,
    pub frame_count: u64,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ViewMode {
    Terrain,   // hillshaded colormap (default)
    Grayscale, // raw height as grayscale
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum OverlayMode {
    None,
    Discharge, // blue-white discharge map
    Momentum,  // R/G momentum map
}

impl SimState {
    pub fn new(seed: u32) -> Self {
        Self {
            world: World::new(seed),
            paused: true, // PAUSED BY DEFAULT (matching original)
            view_mode: ViewMode::Terrain,
            view_overlay: OverlayMode::None,
            frame_count: 0,
        }
    }
}

/// Marker component for the heightmap image entity
#[derive(Component)]
pub struct HeightmapImage;

// ============================================================
//  Image builder
// ============================================================

/// Build a Bevy Image from the world's cell data.
pub fn build_heightmap_image(world: &World, view_mode: ViewMode, overlay: OverlayMode) -> Image {
    let size = WORLD_SIZE as u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    let light_dir = LIGHT_DIR.normalize();

    for y in 0..WORLD_SIZE {
        for x in 0..WORLD_SIZE {
            let idx = ((y * WORLD_SIZE + x) * 4) as usize;
            let Some(cell) = world.map.get(x as i32, y as i32) else {
                continue;
            };

            let h = cell.height.clamp(0.0, 1.0);

            let (r, g, b) = match view_mode {
                ViewMode::Grayscale => {
                    // Pure grayscale height
                    let v = h;
                    (v, v, v)
                }
                ViewMode::Terrain => {
                    // --- Multi-stop colormap ---
                    let (cr, cg, cb) = sample_colormap(h);

                    // --- Hillshading via overlay blend ---
                    let n = world.map.normal(x as i32, y as i32);
                    // Exaggerate normal for stronger shading
                    let n_exag = Vec3::new(n.x, n.y * VERT_EXAG, n.z).normalize();
                    let shade = (n_exag.dot(light_dir)).max(0.0);

                    let (sr, sg, sb) = overlay_blend(shade, cr, cg, cb);
                    (sr, sg, sb)
                }
            };

            // --- Apply overlay ---
            let (r, g, b) = match overlay {
                OverlayMode::None => (r, g, b),
                OverlayMode::Discharge => {
                    // Blue-white wash based on discharge
                    let d = (erf_approx_fast(0.4 * cell.discharge) * 0.5).clamp(0.0, 1.0);
                    (
                        lerp_f32(r, 1.0, d),
                        lerp_f32(g, 1.0, d),
                        lerp_f32(b, 1.0, d * 0.7),
                    )
                }
                OverlayMode::Momentum => {
                    // Red-green momentum overlay, blended on top
                    let mx = (0.5 * (1.0 + erf_approx_fast(cell.momentum_x))).clamp(0.0, 1.0);
                    let my = (0.5 * (1.0 + erf_approx_fast(cell.momentum_y))).clamp(0.0, 1.0);
                    let alpha = 0.6;
                    (
                        lerp_f32(r, mx, alpha),
                        lerp_f32(g, my, alpha),
                        lerp_f32(b, 0.5, alpha),
                    )
                }
            };

            rgba[idx] = (r * 255.0) as u8;
            rgba[idx + 1] = (g * 255.0) as u8;
            rgba[idx + 2] = (b * 255.0) as u8;
            rgba[idx + 3] = 255;
        }
    }

    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

// ============================================================
//  Helpers
// ============================================================

/// Sample the multi-stop terrain colormap by height.
fn sample_colormap(h: f32) -> (f32, f32, f32) {
    let h = h.clamp(0.0, 1.0);
    // Find the two stops bracketing h
    for i in 1..TERRAIN_COLORMAP.len() {
        let (t0, c0) = TERRAIN_COLORMAP[i - 1];
        let (t1, c1) = TERRAIN_COLORMAP[i];
        if h <= t1 {
            let frac = if (t1 - t0) > 0.0 { (h - t0) / (t1 - t0) } else { 0.0 };
            return (
                lerp_f32(c0.0, c1.0, frac),
                lerp_f32(c0.1, c1.1, frac),
                lerp_f32(c0.2, c1.2, frac),
            );
        }
    }
    // Past last stop (shouldn't happen with clamp)
    (
        TERRAIN_COLORMAP.last().unwrap().1 .0,
        TERRAIN_COLORMAP.last().unwrap().1 .1,
        TERRAIN_COLORMAP.last().unwrap().1 .2,
    )
}

/// Overlay blend: if shade < 0.5 → 2*shade*color; else → 1 - 2*(1-shade)*(1-color)
fn overlay_blend(shade: f32, r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let blend = |c: f32| {
        if shade < 0.5 {
            2.0 * shade * c
        } else {
            1.0 - 2.0 * (1.0 - shade) * (1.0 - c)
        }
    };
    (blend(r), blend(g), blend(b))
}

#[inline]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn erf_approx_fast(x: f32) -> f32 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    sign * y
}
