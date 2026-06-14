use bevy::prelude::*;

use crate::generation;
use crate::resources::{
    Background, ContourEntities, GenerationTask, IntermediateView,
    OverlayMode, ParticleErosionState, RegenerateRequest, RegenerateStatus, RenderMode,
    ViewKind, ViewMode, ViewSprites,
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
    mut particle_erosion: ResMut<ParticleErosionState>,
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

    // Reset particle erosion state.
    particle_erosion.world = None;
    particle_erosion.paused = true;
    particle_erosion.frame_count = 0;

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
    mut view_sprites: ResMut<ViewSprites>,
    mut particle_erosion: ResMut<ParticleErosionState>,
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
            &mut view_sprites,
            &mut particle_erosion,
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

// ── Particle erosion systems ───────────────────────────────────────

use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Toggle particle erosion paused state on key E; cycle overlay on key M.
pub fn particle_erosion_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<ParticleErosionState>,
) {
    if keys.just_pressed(KeyCode::KeyE) {
        // Lazy-init on first unpause.
        if state.paused && state.world.is_none() {
            if let Some(ref hm) = state.post_erosion_hm {
                let scale = crate::config::PARTICLE_SCALE;
                let pw = (state.hm_width + scale - 1) / scale;
                let ph = (state.hm_height + scale - 1) / scale;
                let mut pworld = crate::particle::ParticleWorld::new(
                    pw,
                    ph,
                    rand::random::<u32>(),
                );
                pworld.init_from_heightmap_scaled(hm, state.hm_width, state.hm_height, scale);
                state.world = Some(pworld);
                info!(
                    "Particle erosion initialised ({}×{} particle, scale={}) — unpaused",
                    pw, ph, scale
                );
            }
        }
        state.paused = !state.paused;
        info!("Particle erosion paused: {}", state.paused);
    }

    if keys.just_pressed(KeyCode::KeyM) {
        use OverlayMode::*;
        state.overlay = match state.overlay {
            None => Discharge,
            Discharge => Momentum,
            Momentum => DischargeOnly,
            DischargeOnly => None,
        };
        info!("Overlay: {:?}", state.overlay);
    }

    // Reset particle erosion to initial state.
    if keys.just_pressed(KeyCode::KeyR) {
        if let Some(ref hm) = state.post_erosion_hm {
            let scale = crate::config::PARTICLE_SCALE;
            let pw = (state.hm_width + scale - 1) / scale;
            let ph = (state.hm_height + scale - 1) / scale;
            let mut pworld = crate::particle::ParticleWorld::new(pw, ph, rand::random::<u32>());
            pworld.init_from_heightmap_scaled(hm, state.hm_width, state.hm_height, scale);
            state.world = Some(pworld);
            state.frame_count = 0;
            state.paused = true;
            info!("Particle erosion reset — paused");
        }
    }
}

/// Run one particle-erosion cycle per frame and update the background texture.
pub fn particle_erosion_step(
    mut state: ResMut<ParticleErosionState>,
    mut images: ResMut<Assets<Image>>,
    bg_query: Query<&Sprite, With<Background>>,
) {
    if state.paused {
        return;
    }

    let overlay = state.overlay;
    let scale = crate::config::PARTICLE_SCALE;
    let display_w = state.hm_width;
    let display_h = state.hm_height;
    let fc = state.frame_count;

    // Scoped mutable borrow: run erosion, extract heights.
    let heights = {
        let Some(world) = state.world.as_mut() else {
            return;
        };
        let t0 = std::time::Instant::now();
        let cycles = world.map.width.min(world.map.height);
        world.erode(cycles);
        let elapsed = t0.elapsed();

        let heights = if scale == 1 {
            world.extract_heights()
        } else {
            world.extract_heights_scaled(display_w, display_h)
        };

        // Performance log every 60 frames.
        if fc % 60 == 0 {
            info!(
                "Particle erode: {}µs ({}×{} grid, {} droplets)",
                elapsed.as_micros(),
                world.map.width,
                world.map.height,
                cycles,
            );
        }

        heights
    };
    state.frame_count += 1;

    // Diagnostic: log height range every 60 frames.
    if state.frame_count % 60 == 1 {
        let h_min = heights.iter().copied().reduce(f32::min).unwrap_or(0.0);
        let h_max = heights.iter().copied().reduce(f32::max).unwrap_or(1.0);
        info!(
            "Particle frame {}: height [{:.6}, {:.6}], paused={}, overlay={:?}",
            state.frame_count, h_min, h_max, state.paused, overlay
        );
    }

    // Rebuild display texture from current heights (always at display resolution).
    let mut pixels = crate::render::render_heightmap(
        &heights,
        display_w,
        display_h,
        -0.2,              // sea_level
        0.9,               // snow_level
        [-0.2, -0.5, 0.7], // light_dir
        0.35,              // ambient
        6.0,               // normal_strength
    );

    // Apply flow-data overlay if active (only when scale=1, else resolution mismatch).
    if overlay != OverlayMode::None && scale == 1 {
        if let Some(ref world) = state.world {
            crate::render::blend_overlay(&mut pixels, &world.map, overlay);
        }
    }

    let new_image = Image::new(
        Extent3d {
            width: display_w as u32,
            height: display_h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );

    // Update the background sprite's texture.
    for sprite in bg_query.iter() {
        if let Some(existing) = images.get_mut(&sprite.image) {
            *existing = new_image;
            break;
        }
    }
}
