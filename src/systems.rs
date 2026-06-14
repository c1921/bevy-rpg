use bevy::prelude::*;

use crate::generation;
use crate::resources::{
    Background, ContourEntities, GenerationTask, IntermediateView, RegenerateRequest,
    RegenerateStatus, RenderMode, RiverEntities, ViewKind, ViewMode, ViewSprites,
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
    mut river_entities: ResMut<RiverEntities>,
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

    // Drop old river meshes.
    for &entity in &river_entities.0 {
        commands.entity(entity).try_despawn();
    }
    river_entities.0.clear();

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
    mut river_entities: ResMut<RiverEntities>,
    mut status: ResMut<RegenerateStatus>,
    mut view_sprites: ResMut<ViewSprites>,
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
            &mut river_entities,
            &mut view_sprites,
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

pub fn sync_contour_visibility(
    render_mode: Res<RenderMode>,
    view_mode: Res<ViewMode>,
    contour_entities: Res<ContourEntities>,
    mut vis_query: Query<&mut Visibility>,
) {
    let show = render_mode.show_contours && view_mode.kind == ViewKind::Final;
    for &entity in &contour_entities.0 {
        if let Ok(mut vis) = vis_query.get_mut(entity) {
            *vis = if show {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Sync river-mesh visibility: shown only in the Final view when enabled.
pub fn sync_river_visibility(
    render_mode: Res<RenderMode>,
    view_mode: Res<ViewMode>,
    river_entities: Res<RiverEntities>,
    mut vis_query: Query<&mut Visibility>,
) {
    let show = render_mode.show_rivers && view_mode.kind == ViewKind::Final;
    for &entity in &river_entities.0 {
        if let Ok(mut vis) = vis_query.get_mut(entity) {
            *vis = if show {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Sync intermediate-view sprite visibility with `ViewMode` and `RenderMode`.
///
/// Only one view is visible at a time; when `show_3d` is off, all are hidden.
pub fn sync_view_visibility(
    render_mode: Res<RenderMode>,
    view_mode: Res<ViewMode>,
    mut bg_query: Query<&mut Visibility, (With<Background>, Without<IntermediateView>)>,
    mut iv_query: Query<(&mut Visibility, &crate::resources::IntermediateView), Without<Background>>,
) {
    let show_any = render_mode.show_3d;
    // Background (final) sprite.
    if let Ok(mut vis) = bg_query.single_mut() {
        *vis = if show_any && view_mode.kind == ViewKind::Final {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    // Intermediate-view sprites.
    for (mut vis, iv) in iv_query.iter_mut() {
        *vis = if show_any && iv.kind == view_mode.kind {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
