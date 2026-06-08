use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

/// Continuous height field for a 50km×50km world.
pub struct Terrain {
    fbm: Fbm<Perlin>,
}

impl Terrain {
    /// Create a new terrain with the given seed.
    /// Wavelengths: base ~10 km, 6 octaves → finest ~312 m.
    pub fn new(seed: u32) -> Self {
        let fbm = Fbm::<Perlin>::new(seed)
            .set_octaves(6)
            .set_lacunarity(2.0)
            .set_persistence(0.5)
            .set_frequency(0.0001); // 1 / 10 000 m

        Terrain { fbm }
    }

    /// Height at world coordinate `(x, y)` in metres, roughly 0–2000.
    pub fn height(&self, x: f64, y: f64) -> f64 {
        let h = self.fbm.get([x, y]);
        // fBm output is ≈ [-1, 1]; map to [0, MAX_HEIGHT].
        ((h + 1.0) * 0.5).clamp(0.0, 1.0) * MAX_HEIGHT
    }
}

/// Maximum elevation in metres.
pub const MAX_HEIGHT: f64 = 2000.0;

/// Half the world extent (25 km).
pub const WORLD_HALF: f64 = 25_000.0;
