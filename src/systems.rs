use bevy::prelude::*;

use crate::config::WORLD_SIZE;
use crate::generation;
use crate::resources::{Background, ContourData, ContourEntities, RegenerateRequest, RenderMode};

/// Startup system: spawn camera, generate initial terrain, spawn background + contours.
pub fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut contour_entities: ResMut<ContourEntities>,
    render_mode: Res<RenderMode>,
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
    let (data, bg_handle) = generation::generate(seed, &mut images);
    info!(
        "seed={}  levels={}  total-segments={}",
        seed,
        data.levels.len(),
        data.levels
            .iter()
            .map(|l| l.polylines.iter().map(|p| p.len().saturating_sub(1)).sum::<usize>())
            .sum::<usize>(),
    );

    // background heightmap sprite
    commands.spawn((
        Sprite {
            image: bg_handle,
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

    // spawn contour mesh entities
    generation::spawn_contour_meshes(&data, &render_mode, &mut commands, &mut materials, &mut meshes, &mut contour_entities);
    commands.insert_resource(data);
}

/// Respond to `RegenerateRequest` flag by rebuilding the terrain.
pub fn regenerate_on_request(
    mut request: ResMut<RegenerateRequest>,
    render_mode: Res<RenderMode>,
    mut data: ResMut<ContourData>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut contour_entities: ResMut<ContourEntities>,
    bg_query: Query<Entity, With<Background>>,
) {
    if request.0 {
        request.0 = false;

        // drop old meshes
        for &entity in &contour_entities.0 {
            commands.entity(entity).try_despawn();
        }
        contour_entities.0.clear();

        // drop old background
        if let Ok(entity) = bg_query.single() {
            commands.entity(entity).despawn();
        }

        let new_seed = rand::random::<u32>();
        let (new_data, bg_handle) = generation::generate(new_seed, &mut images);
        *data = new_data;
        info!("regenerated — seed={}", new_seed);

        // re-spawn background
        commands.spawn((
            Sprite {
                image: bg_handle,
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

        generation::spawn_contour_meshes(
            &data,
            &render_mode,
            &mut commands,
            &mut materials,
            &mut meshes,
            &mut contour_entities,
        );
    }
}

/// Sync the background sprite visibility with `RenderMode::show_3d`.
pub fn sync_background_visibility(
    render_mode: Res<RenderMode>,
    mut bg_query: Query<&mut Visibility, With<Background>>,
) {
    if render_mode.is_changed() {
        if let Ok(mut vis) = bg_query.single_mut() {
            *vis = if render_mode.show_3d {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Sync contour mesh visibility with `RenderMode::show_contours`.
pub fn sync_contour_visibility(
    render_mode: Res<RenderMode>,
    contour_entities: Res<ContourEntities>,
    mut vis_query: Query<&mut Visibility>,
) {
    if render_mode.is_changed() {
        for &entity in &contour_entities.0 {
            if let Ok(mut vis) = vis_query.get_mut(entity) {
                *vis = if render_mode.show_contours {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}
