use bevy::prelude::*;

/// Cached contour data.
#[derive(Resource)]
pub struct ContourData {
    pub levels: Vec<crate::contour::ContourLevel>,
}

/// Entities that hold the contour-line meshes (cleared on regeneration).
#[derive(Resource, Default)]
pub struct ContourEntities(pub Vec<Entity>);

/// Resource flag — set to true to request terrain regeneration.
#[derive(Resource, Default)]
pub struct RegenerateRequest(pub bool);

/// Resource controlling which render layers are visible.
#[derive(Resource)]
pub struct RenderMode {
    pub show_3d: bool,
    pub show_contours: bool,
}

impl Default for RenderMode {
    fn default() -> Self {
        Self {
            show_3d: true,
            show_contours: true,
        }
    }
}

/// Resource controlling how long the status text stays visible (seconds).
#[derive(Resource)]
pub struct RegenerateStatus {
    pub remaining: f32,
}

impl Default for RegenerateStatus {
    fn default() -> Self {
        Self { remaining: 0.0 }
    }
}

/// Marker for the background heightmap sprite.
#[derive(Component)]
pub struct Background;
