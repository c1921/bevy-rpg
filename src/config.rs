// ── World / rendering constants ──────────────────────────────────

/// Width of the sampled heightmap, in cells.
pub const GRID_COLS: usize = 511;

/// Height of the sampled heightmap, in cells.
pub const GRID_ROWS: usize = 511;

/// Vertical spacing between contour levels (metres).
pub const CONTOUR_INTERVAL: f64 = 200.0;

/// Width of rendered contour lines in world units.
pub const LINE_WIDTH: f32 = 50.0;

/// Maximum terrain elevation in metres.
pub const MAX_HEIGHT: f64 = 2000.0;

/// Half the world extent in each axis (25 km).
pub const WORLD_HALF: f64 = 25_000.0;

/// Full world diagonal (for Sprite custom_size).
pub const WORLD_SIZE: f32 = (WORLD_HALF as f32) * 2.0;
