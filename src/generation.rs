use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, Mesh2d, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite_render::{ColorMaterial, MeshMaterial2d};

use crate::config::{CONTOUR_INTERVAL, GRID_COLS, GRID_ROWS, LINE_WIDTH, MAX_HEIGHT, WORLD_HALF};
use crate::contour::{marching_squares_from_flat, ContourLevel};
use crate::resources::{ContourData, ContourEntities, RenderMode};
use crate::terrain::Terrain;

/// Generate a new terrain heightmap, background image, and contour data.
pub fn generate(seed: u32, images: &mut Assets<Image>) -> (ContourData, Handle<Image>) {
    let t_total = std::time::Instant::now();

    let terrain = Terrain::new(seed);

    let dx = (WORLD_HALF - (-WORLD_HALF)) / GRID_COLS as f64;
    let dy = (WORLD_HALF - (-WORLD_HALF)) / GRID_ROWS as f64;
    let rows = GRID_ROWS + 1;
    let cols = GRID_COLS + 1;

    // Sample the continuous terrain into a discrete heightmap.
    let t_noise = std::time::Instant::now();
    let mut hm = crate::erosion::Heightmap::new(cols, rows, 0.0);
    use rayon::prelude::*;
    hm.data.par_iter_mut().enumerate().for_each(|(idx, v)| {
        let r = idx / cols;
        let c = idx % cols;
        let wx = -WORLD_HALF + c as f64 * dx;
        let wy = -WORLD_HALF + r as f64 * dy;
        *v = terrain.height(wx, wy);
    });
    let noise_ms = t_noise.elapsed().as_millis();

    // Normalize to [0, 1] — erosion parameters are tuned for this range.
    let h_min = hm.data.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let h_max = hm.data.iter().copied().reduce(f64::max).unwrap_or(1.0);
    let h_range = if (h_max - h_min) < 1e-12 { 1.0 } else { h_max - h_min };
    // Normalize to [0,1] then remap [0, 0.45] → [0.4, 0.45] to compress
    // underwater relief before erosion, so erosion works on the remapped data.
    hm.data.iter_mut().for_each(|v| {
        let n = (*v - h_min) / h_range;
        *v = if n <= 0.45 { 0.4 + n * (0.05 / 0.45) } else { n };
    });

    // Hydraulic erosion.
    let t_erosion = std::time::Instant::now();
    let config = crate::erosion::ErosionConfig::default();
    let simulator = crate::erosion::ErosionSimulator::new(config);
    simulator.simulate(&mut hm);
    let erosion_ms = t_erosion.elapsed().as_millis();

    // Re-normalize after erosion to strict [0,1].
    let h2_min = hm.data.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let h2_max = hm.data.iter().copied().reduce(f64::max).unwrap_or(1.0);
    let h2_range = if (h2_max - h2_min) < 1e-12 { 1.0 } else { h2_max - h2_min };

    // Clone a [0,1] f32 copy for background rendering.
    let bg_f32: Vec<f32> = hm
        .data
        .iter()
        .map(|&v| ((v - h2_min) / h2_range).clamp(0.0, 1.0) as f32)
        .collect();

    // Render pseudo-3D background image.
    let t_render = std::time::Instant::now();
    let bg_pixels = crate::render::render_heightmap(
        &bg_f32,
        cols,
        rows,
        -0.2,              // sea_level
        0.9,               // snow_level
        [-0.2, -0.5, 0.7], // light_dir
        0.35,              // ambient
        6.0,               // normal_strength
    );
    let bg_image = Image::new(
        Extent3d {
            width: cols as u32,
            height: rows as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bg_pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let bg_handle = images.add(bg_image);
    let render_ms = t_render.elapsed().as_millis();

    // Scale to [0, MAX_HEIGHT] for contour extraction.
    hm.data
        .iter_mut()
        .for_each(|v| *v = (*v - h2_min) / h2_range * MAX_HEIGHT);

    // Use flat heightmap data directly for contour extraction (no Vec<Vec<f64>> conversion).
    let t_contour = std::time::Instant::now();
    let levels = marching_squares_from_flat(
        &hm.data,
        cols,
        rows,
        -WORLD_HALF,
        -WORLD_HALF,
        dx,
        dy,
        CONTOUR_INTERVAL,
    );
    let contour_ms = t_contour.elapsed().as_millis();

    let total_ms = t_total.elapsed().as_millis();
    info!(
        "generate seed={}: noise={}ms  erosion={}ms  render={}ms  contour={}ms  total={}ms",
        seed, noise_ms, erosion_ms, render_ms, contour_ms, total_ms
    );

    (ContourData { levels }, bg_handle)
}

/// Spawn contour mesh entities for each elevation level.
pub fn spawn_contour_meshes(
    data: &ContourData,
    render_mode: &RenderMode,
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    contour_entities: &mut ResMut<ContourEntities>,
) {
    let vis = if render_mode.show_contours {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for level in &data.levels {
        let color = elevation_color(level.elevation);
        let mesh = build_contour_line_mesh(level, LINE_WIDTH);
        let material = materials.add(ColorMaterial::from_color(color));
        let entity = commands
            .spawn((Mesh2d(meshes.add(mesh)), MeshMaterial2d(material), vis))
            .id();
        contour_entities.0.push(entity);
    }
}

/// Map elevation to a colour gradient: green (low) → olive → brown → grey (high).
fn elevation_color(elev: f64) -> Color {
    let t = (elev / MAX_HEIGHT).clamp(0.0, 1.0) as f32;

    if t < 0.33 {
        let s = t / 0.33;
        Color::srgb(0.12 + s * 0.35, 0.45 + s * 0.2, 0.1 + s * 0.05)
    } else if t < 0.66 {
        let s = (t - 0.33) / 0.33;
        Color::srgb(0.47 + s * 0.3, 0.65 - s * 0.25, 0.15 + s * 0.05)
    } else {
        let s = (t - 0.66) / 0.34;
        Color::srgb(0.77 + s * 0.1, 0.4 + s * 0.2, 0.2 + s * 0.2)
    }
}

/// Build a triangle-list mesh from all polylines of one contour level.
///
/// Each segment becomes a quad (2 triangles) of width `line_width`.
fn build_contour_line_mesh(level: &ContourLevel, line_width: f32) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for poly in &level.polylines {
        if poly.len() < 2 {
            continue;
        }
        for i in 0..poly.len() - 1 {
            let a = Vec2::new(poly[i][0] as f32, poly[i][1] as f32);
            let b = Vec2::new(poly[i + 1][0] as f32, poly[i + 1][1] as f32);

            let dir = b - a;
            let len = dir.length();
            if len < 1e-6 {
                continue;
            }
            let dir = dir / len;
            let perp = Vec2::new(-dir.y, dir.x) * line_width * 0.5;

            let base = positions.len() as u32;
            positions.extend_from_slice(&[
                [a.x - perp.x, a.y - perp.y, 0.0],
                [a.x + perp.x, a.y + perp.y, 0.0],
                [b.x - perp.x, b.y - perp.y, 0.0],
                [b.x + perp.x, b.y + perp.y, 0.0],
            ]);
            indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base + 1,
                base + 3,
                base + 2,
            ]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
