// main.rs — Bevy application entry point

mod render;
mod sim;

use bevy::prelude::*;
use bevy::window::Window;
use render::*;
use sim::cell::*;
use sim::vegetation::Vegetation;

fn main() {
    let mut app = App::new();

    // Parse seed from args
    let seed: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32
        });

    let sim_state = SimState::new(seed);

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("SimpleHydrology — seed {}", seed),
            ..default()
        }),
        ..default()
    }))
        .insert_resource(sim_state)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                input_system,
                erosion_system,
                vegetation_system,
                update_display_system,
                update_step_counter,
            ),
        )
        .run();
}

/// Marker for the step counter text entity
#[derive(Component)]
struct StepCounter;

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, sim_state: Res<SimState>) {
    // Spawn 2D camera
    commands.spawn(Camera2d);

    // Create initial heightmap texture
    let image = build_heightmap_image(&sim_state.world, sim_state.view_mode, sim_state.view_overlay);
    let image_handle = images.add(image);

    // Spawn sprite at world center
    commands.spawn((
        Sprite {
            image: image_handle.clone(),
            custom_size: Some(Vec2::new(WORLD_SIZE as f32, WORLD_SIZE as f32)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
        HeightmapImage,
    ));

    // Spawn step counter text (top center)
    commands.spawn((
        Text2d::new("Step: 0"),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::WHITE),
        TextLayout::new(Justify::Center, LineBreak::NoWrap),
        Transform::from_xyz(0.0, 380.0, 10.0),
        StepCounter,
    ));
}

/// Keyboard input: camera movement, pause, view toggle
///
/// Key bindings:
///   WASD   — pan camera
///   P      — toggle pause (paused by default)
///   B      — terrain view (hillshaded colormap)
///   G      — grayscale height view
///   M      — toggle discharge overlay
///   N      — toggle momentum overlay
///   Escape — clear all overlays
fn input_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut sim_state: ResMut<SimState>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
    time: Res<Time>,
) {
    let move_speed = 100.0 * time.delta_secs();

    if let Ok(mut cam_transform) = camera_query.single_mut() {
        if keys.pressed(KeyCode::KeyW) {
            cam_transform.translation.y += move_speed;
        }
        if keys.pressed(KeyCode::KeyS) {
            cam_transform.translation.y -= move_speed;
        }
        if keys.pressed(KeyCode::KeyA) {
            cam_transform.translation.x -= move_speed;
        }
        if keys.pressed(KeyCode::KeyD) {
            cam_transform.translation.x += move_speed;
        }
        // Zoom with scroll wheel is handled by Bevy's built-in Camera2d
    }

    // --- Pause toggle ---
    if keys.just_pressed(KeyCode::KeyP) {
        sim_state.paused = !sim_state.paused;
        info!("Paused: {}", sim_state.paused);
    }

    // --- Terrain view (hillshaded colormap) ---
    if keys.just_pressed(KeyCode::KeyB) {
        sim_state.view_mode = ViewMode::Terrain;
        info!("View: terrain (hillshaded)");
    }

    // --- Grayscale height view ---
    if keys.just_pressed(KeyCode::KeyG) {
        sim_state.view_mode = ViewMode::Grayscale;
        info!("View: grayscale height");
    }

    // --- Discharge overlay (mutual with momentum) ---
    if keys.just_pressed(KeyCode::KeyM) {
        sim_state.view_overlay = if sim_state.view_overlay == OverlayMode::Discharge {
            OverlayMode::None
        } else {
            OverlayMode::Discharge
        };
        info!("Overlay: {:?}", sim_state.view_overlay);
    }

    // --- Momentum overlay (mutual with discharge) ---
    if keys.just_pressed(KeyCode::KeyN) {
        sim_state.view_overlay = if sim_state.view_overlay == OverlayMode::Momentum {
            OverlayMode::None
        } else {
            OverlayMode::Momentum
        };
        info!("Overlay: {:?}", sim_state.view_overlay);
    }

    // --- Clear overlays ---
    if keys.just_pressed(KeyCode::Escape) {
        sim_state.view_overlay = OverlayMode::None;
        info!("Overlay: cleared");
    }
}

/// Run erosion cycles each frame (when not paused)
fn erosion_system(mut sim_state: ResMut<SimState>, time: Res<Time<Real>>) {
    if sim_state.paused {
        return;
    }

    sim_state.world.erode(TILE_SIZE);
    sim_state.frame_count += 1;
    sim_state.sim_time += time.delta_secs();
}

/// Run vegetation growth each frame (when not paused)
fn vegetation_system(mut sim_state: ResMut<SimState>) {
    if sim_state.paused {
        return;
    }

    let mut veg = Vegetation::new();
    let veg_seed = sim_state.world.seed as u64 ^ sim_state.frame_count;
    veg.grow(&mut sim_state.world, veg_seed);
    // Root density persists on cells; vegetation is transient.
}

/// Rebuild the display texture each frame
fn update_display_system(
    sim_state: Res<SimState>,
    mut images: ResMut<Assets<Image>>,
    mut sprite_query: Query<(&mut Sprite, &HeightmapImage)>,
) {
    let new_image =
        build_heightmap_image(&sim_state.world, sim_state.view_mode, sim_state.view_overlay);

    for (sprite, _) in sprite_query.iter_mut() {
        if let Some(existing) = images.get_mut(&sprite.image) {
            *existing = new_image;
            break;
        }
    }
}

/// Update the on-screen step counter text
fn update_step_counter(
    sim_state: Res<SimState>,
    mut text_query: Query<&mut Text2d, With<StepCounter>>,
) {
    for mut text in text_query.iter_mut() {
        text.0 = format!(
            "Step: {}  |  Time: {:.2}s  |  Paused: {}",
            sim_state.frame_count, sim_state.sim_time, sim_state.paused
        );
    }
}
