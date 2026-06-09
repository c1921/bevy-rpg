// ── Heightmap ────────────────────────────────────────────────────

/// A 2-D floating‑point grid stored in row‑major order.
#[derive(Clone)]
pub struct Heightmap {
    pub data: Vec<f64>,
    pub width: usize,
    pub height: usize,
}

impl Heightmap {
    pub fn new(width: usize, height: usize, fill: f64) -> Self {
        Self {
            data: vec![fill; width * height],
            width,
            height,
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> f64 {
        self.data[y * self.width + x]
    }

    /// Wrap an integer column index into [0, width).
    #[inline]
    pub fn wrap_x(&self, x: i64) -> usize {
        let w = self.width as i64;
        ((x % w + w) % w) as usize
    }

    /// Wrap an integer row index into [0, height).
    #[inline]
    pub fn wrap_y(&self, y: i64) -> usize {
        let h = self.height as i64;
        ((y % h + h) % h) as usize
    }

    /// Bilinear sample at a sub-pixel coordinate `(px, py)` with
    /// periodic boundary wrapping.
    pub fn sample_bilinear(&self, px: f64, py: f64) -> f64 {
        let x0f = px.floor();
        let y0f = py.floor();
        let x0 = x0f as i64;
        let y0 = y0f as i64;
        let x1 = x0 + 1;
        let y1 = y0 + 1;

        let tx = px - x0f;
        let ty = py - y0f;

        let x0w = self.wrap_x(x0);
        let x1w = self.wrap_x(x1);
        let y0w = self.wrap_y(y0);
        let y1w = self.wrap_y(y1);

        let a00 = self.get(x0w, y0w);
        let a01 = self.get(x1w, y0w);
        let a10 = self.get(x0w, y1w);
        let a11 = self.get(x1w, y1w);

        (1.0 - ty) * ((1.0 - tx) * a00 + tx * a01)
            + ty * ((1.0 - tx) * a10 + tx * a11)
    }
}

// ── Gradient ─────────────────────────────────────────────────────

/// Gradient of a heightmap: `real` = dy (axis‑1 / column difference),
/// `imag` = dx (axis‑0 / row difference).  Matches the Python complex
/// encoding `1j * dx + dy`.
#[derive(Clone)]
pub struct Gradient {
    pub real: Vec<f64>,
    pub imag: Vec<f64>,
    #[allow(dead_code)]
    pub width: usize,
    #[allow(dead_code)]
    pub height: usize,
}

impl Gradient {
    #[allow(dead_code)]
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Self {
            real: vec![0.0; n],
            imag: vec![0.0; n],
            width,
            height,
        }
    }
}
