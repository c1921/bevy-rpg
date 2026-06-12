// main.rs — Bevy application entry point

mod render;
mod sim;

use bevy::prelude::*;
use bevy::window::Window;
use render::*;
use sim::cell::*;
use sim::vegetation::Vegetation;
use std::sync::Mutex;

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

    let burst_state = BurstState {
        rx: Mutex::new(None),
        total: 0,
    };

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("SimpleHydrology — seed {}", seed),
            ..default()
        }),
        ..default()
    }))
        .insert_resource(sim_state)
        .insert_resource(burst_state)
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
    let image = build_heightmap_image(sim_state.world.as_ref().unwrap(), sim_state.view_mode, sim_state.view_overlay);
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
        Text2d::new("Step: 0/1000"),
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
///   R      — toggle live preview / fast mode
fn input_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut sim_state: ResMut<SimState>,
    burst_state: Res<BurstState>,
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
        if sim_state.world.is_none() && burst_state.rx.lock().unwrap().is_some() {
            info!("Cannot pause — background burst is running");
        } else {
            sim_state.paused = !sim_state.paused;
            info!("Paused: {}", sim_state.paused);
        }
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

    // --- Live preview toggle ---
    if keys.just_pressed(KeyCode::KeyR) {
        if sim_state.world.is_none() && burst_state.rx.lock().unwrap().is_some() {
            info!("Cannot switch mode — background burst is running");
        } else {
            sim_state.live_preview = !sim_state.live_preview;
            if !sim_state.live_preview {
                // Switching to fast mode: reset finished flag so burst can run
                sim_state.finished = false;
                info!("Mode: FAST (no preview) — unpause to burst-run");
            } else {
                info!("Mode: LIVE preview — each frame visible");
            }
        }
    }
}

/// Run erosion cycles each frame.
///
/// Live-preview mode: one cycle per frame on the main thread.
/// Fast mode: spawns a background thread that runs all remaining steps;
/// the main thread polls for completion without blocking.
fn erosion_system(
    mut sim_state: ResMut<SimState>,
    mut burst_state: ResMut<BurstState>,
    time: Res<Time<Real>>,
) {
    // --- 1. Check if a background burst just finished ---
    let total = burst_state.total;
    let rx_opt = burst_state.rx.get_mut().unwrap();
    if let Some(rx) = rx_opt.as_mut() {
        // Worker thread is/was running — try to collect its result
        if let Ok((world, elapsed)) = rx.try_recv() {
            // Worker sent the World back
            *rx_opt = None; // clear the receiver
            sim_state.world = Some(world);
            sim_state.frame_count = sim_state.target_steps;
            sim_state.sim_time += elapsed;
            sim_state.paused = true;
            sim_state.finished = true;
            info!(
                "Background burst of {} steps complete in {:.2}s. Paused.",
                total, elapsed
            );
        }
        return; // burst is/was in progress — don't start a new one
    }
    // Release rx_opt borrow before potentially needing burst_state again
    let _ = rx_opt;

    if sim_state.paused {
        return;
    }

    if sim_state.live_preview {
        // --- Real-time preview: one cycle per frame ---
        let world = sim_state.world.as_mut().expect("World missing in live-preview mode");
        world.erode(TILE_SIZE);
        sim_state.frame_count += 1;
        sim_state.sim_time += time.delta_secs();

        if sim_state.frame_count >= sim_state.target_steps {
            sim_state.paused = true;
            sim_state.finished = true;
            info!("Reached target {} steps. Paused.", sim_state.target_steps);
        }
    } else {
        // --- Fast mode: spawn background thread ---
        let world = match sim_state.world.take() {
            Some(w) => w,
            None => return,
        };

        let remaining = sim_state.target_steps.saturating_sub(sim_state.frame_count);

        let (tx, rx) = std::sync::mpsc::channel::<(crate::sim::world::World, f32)>();
        {
            let rx_opt2 = burst_state.rx.get_mut().unwrap();
            *rx_opt2 = Some(rx);
        }
        burst_state.total = remaining;

        info!("Starting background burst of {} steps...", remaining);

        std::thread::spawn(move || {
            let mut w = world;
            let start = std::time::Instant::now();
            for i in 0..remaining {
                w.erode(TILE_SIZE);
                // Vegetation cycle (matching 1:1 ratio)
                {
                    let mut veg = Vegetation::new();
                    let veg_seed = w.seed as u64 ^ (i + 1);
                    veg.grow(&mut w, veg_seed);
                }
            }
            let elapsed = start.elapsed().as_secs_f32();
            println!("Background burst {} steps in {:.2}s.", remaining, elapsed);
            tx.send((w, elapsed)).ok();
        });
    }
}

/// Run vegetation growth each frame (when not paused and world is present)
fn vegetation_system(mut sim_state: ResMut<SimState>) {
    if sim_state.paused {
        return;
    }

    let frame_count = sim_state.frame_count;
    let Some(world) = sim_state.world.as_mut() else {
        return; // world is on the background thread
    };

    let mut veg = Vegetation::new();
    let veg_seed = world.seed as u64 ^ frame_count;
    veg.grow(world, veg_seed);
    // Root density persists on cells; vegetation is transient.
}

/// Rebuild the display texture each frame
fn update_display_system(
    sim_state: Res<SimState>,
    mut images: ResMut<Assets<Image>>,
    mut sprite_query: Query<(&mut Sprite, &HeightmapImage)>,
) {
    let Some(world) = sim_state.world.as_ref() else {
        return; // world is on the background thread
    };

    let new_image =
        build_heightmap_image(world, sim_state.view_mode, sim_state.view_overlay);

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
    burst_state: Res<BurstState>,
    mut text_query: Query<&mut Text2d, With<StepCounter>>,
) {
    let mode = if sim_state.live_preview { "[LIVE]" } else { "[FAST]" };
    let done = if sim_state.finished { " FINISHED" } else { "" };

    let line = if sim_state.world.is_none() && burst_state.rx.lock().unwrap().is_some() {
        // Background burst is running
        format!(
            "Computing... {} steps on bg thread  |  [FAST]",
            burst_state.total
        )
    } else {
        format!(
            "Step: {}/{}  |  Time: {:.2}s  |  Paused: {}  |  {}{}",
            sim_state.frame_count,
            sim_state.target_steps,
            sim_state.sim_time,
            sim_state.paused,
            mode,
            done,
        )
    };

    for mut text in text_query.iter_mut() {
        text.0 = line.clone();
    }
}
