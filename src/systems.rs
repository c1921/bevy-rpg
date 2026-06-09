use bevy::prelude::*;

use crate::generation;
use crate::resources::{
    Background, ContourEntities, GenerationTask, RegenerateRequest, RegenerateStatus, RenderMode,
};

/// Startup system: spawn camera + UI, then kick off async terrain generation.
pub fn setup(
    mut commands: Commands,
    mut gen_task: ResMut<GenerationTask>,
) {
    // 2-D camera
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 30.0,
            ..OrthographicProjection::default_2d()
        }),
    ));

    // Kick off background generation.
    let seed = rand::random::<u32>();
    let cell = std::sync::Arc::new(std::sync::Mutex::new(None));
    let cell2 = cell.clone();
    std::thread::spawn(move || {
        let result = generation::compute_raw(seed);
        *cell2.lock().unwrap() = Some(result);
    });
    gen_task.cell = Some(cell);
    info!("generation started (seed={})", seed);
}

/// Respond to `RegenerateRequest` flag, cleaning up old assets and
/// starting a new generation on a background thread.
pub fn regenerate_on_request(
    mut request: ResMut<RegenerateRequest>,
    mut commands: Commands,
    mut contour_entities: ResMut<ContourEntities>,
    mut gen_task: ResMut<GenerationTask>,
    bg_query: Query<Entity, With<Background>>,
) {
    if !request.0 {
        return;
    }
    request.0 = false;

    // Drop old contour meshes.
    for &entity in &contour_entities.0 {
        commands.entity(entity).try_despawn();
    }
    contour_entities.0.clear();

    // Drop old background.
    if let Ok(entity) = bg_query.single() {
        commands.entity(entity).despawn();
    }

    // Kick off new generation on a background thread.
    let new_seed = rand::random::<u32>();
    let cell = std::sync::Arc::new(std::sync::Mutex::new(None));
    let cell2 = cell.clone();
    std::thread::spawn(move || {
        let result = generation::compute_raw(new_seed);
        *cell2.lock().unwrap() = Some(result);
    });
    gen_task.cell = Some(cell);
    info!("regeneration started — seed={}", new_seed);
}

/// Poll the background generation task every frame.
/// When the result arrives, create Bevy assets on the main thread.
pub fn poll_generation(
    mut gen_task: ResMut<GenerationTask>,
    render_mode: Res<RenderMode>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut contour_entities: ResMut<ContourEntities>,
    mut status: ResMut<RegenerateStatus>,
) {
    let cell = match gen_task.cell.take() {
        Some(c) => c,
        None => return,
    };

    let mut guard = cell.lock().unwrap();
    if let Some(result) = guard.take() {
        drop(guard);
        generation::apply_result(
            result,
            &render_mode,
            &mut commands,
            &mut images,
            &mut materials,
            &mut meshes,
            &mut contour_entities,
        );
        status.remaining = 0.0;
        status.label.clear();
        // cell dropped → no more polling.
    } else {
        // Still computing — put the Arc back.
        drop(guard);
        gen_task.cell = Some(cell);
    }
}

/// Keep the "Generating..." label alive while a task is pending.
pub fn maintain_generation_label(
    gen_task: Res<GenerationTask>,
    mut status: ResMut<RegenerateStatus>,
) {
    if gen_task.cell.is_some() {
        status.remaining = 0.5;
        status.label = "Generating...".into();
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
