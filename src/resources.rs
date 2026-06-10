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
    pub label: String,
}

impl Default for RegenerateStatus {
    fn default() -> Self {
        Self {
            remaining: 0.0,
            label: String::new(),
        }
    }
}

/// Marker for the background heightmap sprite.
#[derive(Component)]
pub struct Background;

/// Marker component for intermediate-view sprites (initial noise, post-erosion, etc.).
#[derive(Component, Clone, Copy)]
pub struct IntermediateView {
    pub kind: ViewKind,
}

/// Which intermediate / final view is currently shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ViewKind {
    Final,
    InitialNoise,
}

/// Resource — which intermediate/final view is active.
#[derive(Resource)]
pub struct ViewMode {
    pub kind: ViewKind,
}

impl Default for ViewMode {
    fn default() -> Self {
        Self { kind: ViewKind::Final }
    }
}

/// Entities of the intermediate-view sprites, keyed by ViewKind.
#[derive(Resource, Default)]
pub struct ViewSprites {
    pub entities: std::collections::HashMap<ViewKind, Entity>,
}

// ── Async generation ───────────────────────────────────────────────

/// Fully‑computed generation data, ready for asset creation on the main thread.
///
/// All fields are `Send + 'static` so this can cross a thread boundary.
pub struct GenerationResult {
    pub seed: u32,
    pub bg_pixels: Vec<u8>,
    pub bg_cols: usize,
    pub bg_rows: usize,
    pub data: ContourData,
    /// Initial noise heightmap, normalized to [0, 1] (f32).
    pub initial_noise_hm: Vec<f32>,
}

/// State for background terrain generation.
/// Polled every frame; the `Arc<Mutex<…>>` receives results from the worker thread.
#[derive(Resource, Default)]
pub struct GenerationTask {
    pub cell: Option<std::sync::Arc<std::sync::Mutex<Option<GenerationResult>>>>,
}
