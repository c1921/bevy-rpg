mod camera;
mod config;
mod contour;
mod erosion;
mod generation;
mod render;
mod resources;
mod systems;
mod terrain;
mod ui;

use bevy::prelude::*;
use camera::{camera_control, CameraDrag};
use resources::{
    ContourEntities, GenerationTask, RegenerateRequest, RegenerateStatus, RenderMode,
};
use systems::{
    maintain_generation_label, poll_generation, regenerate_on_request, setup,
    sync_background_visibility, sync_contour_visibility,
};
use ui::{regenerate_button, spawn_ui, toggle_render_mode, update_status};

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
        .init_resource::<RenderMode>()
        .init_resource::<GenerationTask>()
        .add_systems(Startup, (setup, spawn_ui))
        .add_systems(
            Update,
            (
                camera_control,
                regenerate_on_request,
                regenerate_button,
                toggle_render_mode,
                poll_generation,
                maintain_generation_label,
                update_status,
                sync_background_visibility,
            ),
        )
        .add_systems(Update, (sync_contour_visibility,))
        .run();
}
