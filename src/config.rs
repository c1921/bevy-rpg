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

/// Width of rendered river lines is data‑driven (per‑segment); this is the
/// z‑offset used so rivers draw above the background but below contours.
pub const RIVER_Z: f32 = -0.5;

/// Colour of river lines (deep water blue).
pub const RIVER_COLOR: (f32, f32, f32) = (0.18, 0.40, 0.70);
