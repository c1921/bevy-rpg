mod camera;
mod contour;
mod erosion;
mod terrain;
mod ui;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, Mesh2d, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::{ColorMaterial, MeshMaterial2d};
use camera::{camera_control, CameraDrag};
use contour::{marching_squares_from_heights, ContourLevel};
use terrain::{Terrain, WORLD_HALF};
use ui::{regenerate_button, spawn_ui, update_status, RegenerateStatus};

// ── world constants ──────────────────────────────────────────────
const GRID_COLS: usize = 400;
const GRID_ROWS: usize = 400;
const CONTOUR_INTERVAL: f64 = 200.0; // metres
const LINE_WIDTH: f32 = 50.0; // world units ≈ 1.5 px at default zoom

/// Cached contour data.
#[derive(Resource)]
struct ContourData {
    levels: Vec<ContourLevel>,
}

/// Entities that hold the contour-line meshes (cleared on regeneration).
#[derive(Resource, Default)]
struct ContourEntities(Vec<Entity>);

/// Resource flag — set to true to request terrain regeneration.
#[derive(Resource, Default)]
pub struct RegenerateRequest(pub bool);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Bevy RPG — Contour Map".into(),
                resolution: (1920, 1080).into(),
                ..default()
            }),
            ..default()
        }))
        .init_resource::<CameraDrag>()
        .init_resource::<ContourEntities>()
        .init_resource::<RegenerateStatus>()
        .init_resource::<RegenerateRequest>()
        .add_systems(Startup, (setup, spawn_ui))
        .add_systems(Update, (camera_control, regenerate_on_request, regenerate_button, update_status))
        .run();
}

// ── startup ──────────────────────────────────────────────────────

fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut contour_entities: ResMut<ContourEntities>,
) {
    // 2-D camera
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 30.0,
            ..OrthographicProjection::default_2d()
        }),
    ));

    // initial terrain
    let seed = rand::random::<u32>();
    let data = generate(seed);
    info!(
        "seed={}  levels={}  total-segments={}",
        seed,
        data.levels.len(),
        data.levels
            .iter()
            .map(|l| l.polylines.iter().map(|p| p.len().saturating_sub(1)).sum::<usize>())
            .sum::<usize>(),
    );

    // spawn mesh entities
    spawn_contour_meshes(&data, &mut commands, &mut materials, &mut meshes, &mut contour_entities);
    commands.insert_resource(data);
}

// ── per-frame systems ────────────────────────────────────────────

/// Respond to `RegenerateRequest` flag by rebuilding the terrain.
fn regenerate_on_request(
    mut request: ResMut<RegenerateRequest>,
    mut data: ResMut<ContourData>,
    mut commands: Commands,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut contour_entities: ResMut<ContourEntities>,
) {
    if request.0 {
        request.0 = false;

        // drop old meshes
        for &entity in &contour_entities.0 {
            commands.entity(entity).try_despawn();
        }
        contour_entities.0.clear();

        let new_seed = rand::random::<u32>();
        *data = generate(new_seed);
        info!("regenerated — seed={}", new_seed);

        spawn_contour_meshes(
            &data,
            &mut commands,
            &mut materials,
            &mut meshes,
            &mut contour_entities,
        );
    }
}

// ── helpers ──────────────────────────────────────────────────────

fn generate(seed: u32) -> ContourData {
    let terrain = Terrain::new(seed);

    let dx = (WORLD_HALF - (-WORLD_HALF)) / GRID_COLS as f64;
    let dy = (WORLD_HALF - (-WORLD_HALF)) / GRID_ROWS as f64;
    let rows = GRID_ROWS + 1;
    let cols = GRID_COLS + 1;

    // Sample the continuous terrain into a discrete heightmap.
    let mut hm = erosion::Heightmap::new(cols, rows, 0.0);
    for r in 0..rows {
        for c in 0..cols {
            let wx = -WORLD_HALF + c as f64 * dx;
            let wy = -WORLD_HALF + r as f64 * dy;
            hm.set(c, r, terrain.height(wx, wy));
        }
    }

    // Normalize to [0, 1] — erosion parameters are tuned for this range.
    let h_min = hm.data.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let h_max = hm.data.iter().copied().reduce(f64::max).unwrap_or(1.0);
    let h_range = if (h_max - h_min) < 1e-12 { 1.0 } else { h_max - h_min };
    hm.data.iter_mut().for_each(|v| *v = (*v - h_min) / h_range);

    // Hydraulic erosion.
    let config = erosion::ErosionConfig::default();
    let simulator = erosion::ErosionSimulator::new(config);
    simulator.simulate(&mut hm);

    // Re‑normalize after erosion and scale back to [0, MAX_HEIGHT].
    let h2_min = hm.data.iter().copied().reduce(f64::min).unwrap_or(0.0);
    let h2_max = hm.data.iter().copied().reduce(f64::max).unwrap_or(1.0);
    let h2_range = if (h2_max - h2_min) < 1e-12 { 1.0 } else { h2_max - h2_min };
    hm.data
        .iter_mut()
        .for_each(|v| *v = (*v - h2_min) / h2_range * terrain::MAX_HEIGHT);

    // Convert back to row-major Vec<Vec<f64>> for contour extraction.
    let heights: Vec<Vec<f64>> = (0..rows)
        .map(|r| (0..cols).map(|c| hm.get(c, r)).collect())
        .collect();

    let levels = marching_squares_from_heights(
        &heights,
        -WORLD_HALF,
        -WORLD_HALF,
        dx,
        dy,
        CONTOUR_INTERVAL,
    );
    ContourData { levels }
}

fn spawn_contour_meshes(
    data: &ContourData,
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    contour_entities: &mut ResMut<ContourEntities>,
) {
    for level in &data.levels {
        let color = elevation_color(level.elevation);
        let mesh = build_contour_line_mesh(level, LINE_WIDTH);
        let material = materials.add(ColorMaterial::from_color(color));
        let entity = commands
            .spawn((Mesh2d(meshes.add(mesh)), MeshMaterial2d(material)))
            .id();
        contour_entities.0.push(entity);
    }
}

/// Map elevation to a colour gradient: green (low) → olive → brown → grey (high).
fn elevation_color(elev: f64) -> Color {
    let t = (elev / terrain::MAX_HEIGHT).clamp(0.0, 1.0) as f32;

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
