// ── World / rendering constants ──────────────────────────────────

/// Width of the sampled heightmap, in cells.
pub const GRID_COLS: usize = 800;

/// Height of the sampled heightmap, in cells.
pub const GRID_ROWS: usize = 800;

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

/// Padding cells added around the erosion heightmap so edge mountains
/// have space to erode outward.  The visible region is cropped back to
/// the original GRID_COLS×GRID_ROWS after erosion.
pub const EROSION_PADDING: usize = 64;

/// Downscale factor for the particle-erosion grid (1 = full resolution).
/// Set to 2 for a half-resolution particle grid (≈4× faster erosion).
pub const PARTICLE_SCALE: usize = 1;
