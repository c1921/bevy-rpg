use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, Mesh2d, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite_render::{ColorMaterial, MeshMaterial2d};

use crate::config::{CONTOUR_INTERVAL, EROSION_PADDING, GRID_COLS, GRID_ROWS, LINE_WIDTH, MAX_HEIGHT, RIVER_COLOR, RIVER_Z, WORLD_HALF, WORLD_SIZE};
use crate::contour::{marching_squares_from_flat, ContourLevel};
use crate::resources::{Background, ContourData, ContourEntities, GenerationResult, IntermediateView, RenderMode, RiverEntities, ViewKind, ViewSprites};
use crate::river::RiverSegment;
use crate::terrain::Terrain;

/// Pure computation: noise → erosion → render pixels → contour extraction.
///
/// Thread‑safe (no Bevy types), suitable for a background thread.
pub fn compute_raw(seed: u32) -> GenerationResult {
    let t_total = std::time::Instant::now();

    let terrain = Terrain::new(seed);

    let dx = (WORLD_HALF - (-WORLD_HALF)) / GRID_COLS as f64;
    let dy = (WORLD_HALF - (-WORLD_HALF)) / GRID_ROWS as f64;
    let rows = GRID_ROWS + 1;
    let cols = GRID_COLS + 1;
    let pad = EROSION_PADDING;
    let extended_rows = rows + 2 * pad;
    let extended_cols = cols + 2 * pad;

    // Sample the continuous terrain into an extended discrete heightmap
    // with padding so edge mountains have room to erode outward.
    let t_noise = std::time::Instant::now();
    let mut hm = crate::erosion::Heightmap::new(extended_cols, extended_rows, 0.0);
    use rayon::prelude::*;
    let start_x = -WORLD_HALF - pad as f64 * dx;
    let start_y = -WORLD_HALF - pad as f64 * dy;
    hm.data.par_iter_mut().enumerate().for_each(|(idx, v)| {
        let r = idx / extended_cols;
        let c = idx % extended_cols;
        let wx = start_x + c as f64 * dx;
        let wy = start_y + r as f64 * dy;
        *v = terrain.height(wx, wy);
    });
    let noise_ms = t_noise.elapsed().as_millis();

    // Normalize the extended grid to [0, 1].
    let h_min = hm.data.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let h_max = hm.data.iter().copied().reduce(f64::max).unwrap_or(1.0);
    let h_range = if (h_max - h_min) < 1e-12 {
        1.0
    } else {
        h_max - h_min
    };

    // Capture initial noise (simple [0,1] normalised, cropped to visible region).
    let initial_noise_hm: Vec<f32> = hm.crop_normalized_f32(pad, cols, rows);

    // Normalize to [0,1] then remap [0, 0.45] → [0.4, 0.45] to compress
    // underwater relief before erosion, so erosion works on the remapped data.
    hm.data.iter_mut().for_each(|v| {
        let n = (*v - h_min) / h_range;
        *v = if n <= 0.45 {
            0.4 + n * (0.05 / 0.45)
        } else {
            n
        };
    });

    // Capture processed-noise heightmap (post-compression, pre-erosion, cropped).
    let processed_noise_hm: Vec<f32> = hm.crop_normalized_f32(pad, cols, rows);

    // Re-normalize processed noise to strict [0,1] (matching Final's scale).
    let pn_min = processed_noise_hm.iter().copied().reduce(f32::min).unwrap_or(0.0);
    let pn_max = processed_noise_hm.iter().copied().reduce(f32::max).unwrap_or(1.0);
    let pn_range = if (pn_max - pn_min) < 1e-12 { 1.0 } else { pn_max - pn_min };
    let compressed_norm_hm: Vec<f32> = processed_noise_hm
        .iter()
        .map(|&v| ((v - pn_min) / pn_range).clamp(0.0, 1.0))
        .collect();

    // Hydraulic erosion on the extended grid.
    let t_erosion = std::time::Instant::now();
    let config = crate::erosion::ErosionConfig::default();
    let simulator = crate::erosion::ErosionSimulator::new(config);
    simulator.simulate(&mut hm);
    let erosion_ms = t_erosion.elapsed().as_millis();

    // Crop back to the visible region, then re-normalize to [0,1].
    let mut visible = hm.crop(pad, cols, rows);
    let h2_min = visible.data.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let h2_max = visible.data.iter().copied().reduce(f64::max).unwrap_or(1.0);
    let h2_range = if (h2_max - h2_min) < 1e-12 {
        1.0
    } else {
        h2_max - h2_min
    };

    // Clone a [0,1] f32 copy for background rendering.
    let bg_f32: Vec<f32> = visible
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
    let render_ms = t_render.elapsed().as_millis();

    // Scale to [0, MAX_HEIGHT] for contour extraction.
    visible
        .data
        .iter_mut()
        .for_each(|v| *v = (*v - h2_min) / h2_range * MAX_HEIGHT);

    // Use flat heightmap data directly for contour extraction (no Vec<Vec<f64>> conversion).
    let t_contour = std::time::Instant::now();
    let levels = marching_squares_from_flat(
        &visible.data,
        cols,
        rows,
        -WORLD_HALF,
        -WORLD_HALF,
        dx,
        dy,
        CONTOUR_INTERVAL,
    );
    let contour_ms = t_contour.elapsed().as_millis();

    // River network extraction on the eroded, cropped grid.  Flow depends
    // only on relative height, so the [0, MAX_HEIGHT] scaling above is fine.
    let t_river = std::time::Instant::now();
    let river_cfg = crate::river::RiverConfig::default();
    let river_net = crate::river::extract(
        &visible.data,
        cols,
        rows,
        -WORLD_HALF,
        -WORLD_HALF,
        dx,
        dy,
        &river_cfg,
    );
    let river_ms = t_river.elapsed().as_millis();

    let total_ms = t_total.elapsed().as_millis();
    info!(
        "generate seed={}: noise={}ms  erosion={}ms  render={}ms  contour={}ms  river={}ms({} segs)  total={}ms",
        seed, noise_ms, erosion_ms, render_ms, contour_ms, river_ms, river_net.segments.len(), total_ms
    );

    GenerationResult {
        seed,
        bg_pixels,
        bg_cols: cols,
        bg_rows: rows,
        data: ContourData { levels },
        rivers: river_net.segments,
        initial_noise_hm,
        processed_noise_hm,
        compressed_norm_hm,
        drainage_field: river_net.accum_field,
    }
}

/// On‑main‑thread: turn a `GenerationResult` into Bevy assets and spawn entities.
///
/// Returns the background `Handle<Image>`.
pub fn apply_result(
    result: GenerationResult,
    render_mode: &RenderMode,
    commands: &mut Commands,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    contour_entities: &mut ResMut<ContourEntities>,
    river_entities: &mut ResMut<RiverEntities>,
    view_sprites: &mut ResMut<ViewSprites>,
) -> Handle<Image> {
    info!(
        "apply_result seed={}  levels={}  total-segments={}",
        result.seed,
        result.data.levels.len(),
        result.data
            .levels
            .iter()
            .map(|l| l.polylines.iter().map(|p| p.len().saturating_sub(1)).sum::<usize>())
            .sum::<usize>(),
    );

    // Background image asset.
    let bg_image = Image::new(
        Extent3d {
            width: result.bg_cols as u32,
            height: result.bg_rows as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        result.bg_pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let bg_handle = images.add(bg_image);

    // Background sprite.
    commands.spawn((
        Sprite {
            image: bg_handle.clone(),
            custom_size: Some(Vec2::new(WORLD_SIZE, WORLD_SIZE)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -1.0),
        if render_mode.show_3d {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
        Background,
    ));

    // Contour mesh entities.
    spawn_contour_meshes(
        &result.data,
        render_mode,
        commands,
        materials,
        meshes,
        contour_entities,
    );

    // River mesh entities.
    spawn_river_meshes(
        &result.rivers,
        render_mode,
        commands,
        materials,
        meshes,
        river_entities,
    );

    // Store contour data as a resource for later access.
    commands.insert_resource(result.data);

    // ── Intermediate view sprites ──────────────────────────────────
    // Clear old intermediate-view sprites.
    for &entity in view_sprites.entities.values() {
        commands.entity(entity).try_despawn();
    }
    view_sprites.entities.clear();

    // Helper: create a sprite from a [0,1] f32 heightmap.
    let make_view = |hm: &[f32], kind: ViewKind,
                     commands: &mut Commands,
                     images: &mut ResMut<Assets<Image>>|
     -> Entity {
        let pixels = crate::render::render_heightmap(
            hm,
            result.bg_cols,
            result.bg_rows,
            -0.2,              // sea_level
            0.9,               // snow_level
            [-0.2, -0.5, 0.7], // light_dir
            0.35,              // ambient
            6.0,               // normal_strength
        );
        let image = Image::new(
            Extent3d {
                width: result.bg_cols as u32,
                height: result.bg_rows as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let handle = images.add(image);
        commands
            .spawn((
                Sprite {
                    image: handle,
                    custom_size: Some(Vec2::new(WORLD_SIZE, WORLD_SIZE)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, -1.0),
                Visibility::Hidden,
                IntermediateView { kind },
            ))
            .id()
    };

    let ent_init = make_view(&result.initial_noise_hm, ViewKind::InitialNoise, commands, images);
    let ent_proc = make_view(&result.processed_noise_hm, ViewKind::ProcessedNoise, commands, images);
    let ent_cnorm = make_view(&result.compressed_norm_hm, ViewKind::CompressedNorm, commands, images);
    let ent_drain = make_view(&result.drainage_field, ViewKind::DrainageField, commands, images);
    view_sprites.entities.insert(ViewKind::InitialNoise, ent_init);
    view_sprites.entities.insert(ViewKind::ProcessedNoise, ent_proc);
    view_sprites.entities.insert(ViewKind::CompressedNorm, ent_cnorm);
    view_sprites.entities.insert(ViewKind::DrainageField, ent_drain);

    bg_handle
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

/// Spawn river-line mesh entities (one mesh for the whole network).
pub fn spawn_river_meshes(
    rivers: &[RiverSegment],
    render_mode: &RenderMode,
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    river_entities: &mut ResMut<RiverEntities>,
) {
    if rivers.is_empty() {
        return;
    }
    let vis = if render_mode.show_rivers {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    let color = Color::srgb(RIVER_COLOR.0, RIVER_COLOR.1, RIVER_COLOR.2);
    let mesh = build_river_mesh(rivers);
    let material = materials.add(ColorMaterial::from_color(color));
    let entity = commands
        .spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, RIVER_Z),
            vis,
        ))
        .id();
    river_entities.0.push(entity);
}

/// Build a triangle-list mesh from river segments. Each segment is a quad
/// of its own width, with small square caps at both ends so variable-width
/// segments join without gaps.
fn build_river_mesh(rivers: &[RiverSegment]) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let push_quad = |p: [[f32; 3]; 4], positions: &mut Vec<[f32; 3]>, indices: &mut Vec<u32>| {
        let base = positions.len() as u32;
        positions.extend_from_slice(&p);
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
    };

    for seg in rivers {
        let a = Vec2::new(seg.a[0] as f32, seg.a[1] as f32);
        let b = Vec2::new(seg.b[0] as f32, seg.b[1] as f32);
        let dir = b - a;
        let len = dir.length();
        if len < 1e-6 {
            continue;
        }
        let dir = dir / len;
        let hw = seg.width * 0.5;
        let perp = Vec2::new(-dir.y, dir.x) * hw;

        push_quad(
            [
                [a.x - perp.x, a.y - perp.y, 0.0],
                [a.x + perp.x, a.y + perp.y, 0.0],
                [b.x - perp.x, b.y - perp.y, 0.0],
                [b.x + perp.x, b.y + perp.y, 0.0],
            ],
            &mut positions,
            &mut indices,
        );

        // Square cap at each endpoint to bridge width changes / direction turns.
        for (c, w) in [(a, hw), (b, hw)] {
            push_quad(
                [
                    [c.x - w, c.y - w, 0.0],
                    [c.x + w, c.y - w, 0.0],
                    [c.x - w, c.y + w, 0.0],
                    [c.x + w, c.y + w, 0.0],
                ],
                &mut positions,
                &mut indices,
            );
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
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
