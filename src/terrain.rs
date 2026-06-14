use crate::config::MAX_HEIGHT;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

/// Continuous height field for a 50km×50km world.
pub struct Terrain {
    fbm: Fbm<Perlin>,
}

impl Terrain {
    /// Create a new terrain with the given seed.
    pub fn new(seed: u32) -> Self {
        let fbm = Fbm::<Perlin>::new(seed)
            .set_octaves(10)
            .set_lacunarity(2.0)
            .set_persistence(0.55)
            .set_frequency(0.00003);

        Terrain { fbm }
    }

    /// Height at world coordinate `(x, y)` in metres, roughly 0–2000.
    pub fn height(&self, x: f64, y: f64) -> f64 {
        let h = self.fbm.get([x, y]);
        // fBm output is ≈ [-1, 1]; map to [0, MAX_HEIGHT].
        ((h + 1.0) * 0.5).clamp(0.0, 1.0) * MAX_HEIGHT
    }
}
