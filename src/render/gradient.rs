// ── Gradient stop ──────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct Stop {
    pub(crate) pos: f32,
    pub(crate) r: f32,
    pub(crate) g: f32,
    pub(crate) b: f32,
}

/// Sample a gradient at `pos` (clamped to the stop range); linear interp.
/// Uses binary search to find the interval — stops are sorted by position.
pub(crate) fn sample_stops(stops: &[Stop], pos: f32) -> [f32; 3] {
    if stops.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    let p = pos.clamp(stops[0].pos, stops[stops.len() - 1].pos);

    // Binary search for the right interval.
    let i = match stops.binary_search_by(|s| s.pos.partial_cmp(&p).unwrap_or(std::cmp::Ordering::Equal)) {
        Ok(idx) => idx.min(stops.len() - 2), // exact hit — use as left bound (unless last)
        Err(0) => 0,                          // before first — clamp to first interval
        Err(idx) => idx - 1,                  // between idx-1 and idx
    };
    let i = i.min(stops.len().saturating_sub(2));

    let a = &stops[i];
    let b = &stops[i + 1];
    let t = if (b.pos - a.pos).abs() > 1e-7 {
        (p - a.pos) / (b.pos - a.pos)
    } else {
        0.5
    };
    [
        a.r + t * (b.r - a.r),
        a.g + t * (b.g - a.g),
        a.b + t * (b.b - a.b),
    ]
}

/// Build the ocean_land gradient, split at `sea_level`.
///
/// Returns `(ocean_stops, land_stops)`.  The colour progression is:
///   deep‑blue → mid‑blue → light‑blue → green → yellow‑green →
///   brown → grey → white.
pub(crate) fn ocean_land_stops(sea: f32, snow: f32) -> (Vec<Stop>, Vec<Stop>) {
    let sea = sea.clamp(0.05, 0.95);
    let snow = snow.clamp(0.5, 1.0);
    let span = (snow - sea).max(1e-4);

    let land_pos = |rel: f32| -> f32 { (sea + rel * span).min(0.999) };

    // All stops (unsplit)
    let all: Vec<Stop> = vec![
        Stop { pos: 0.00                                                 , r: 8.0/255.0,  g: 20.0/255.0,  b: 65.0/255.0  },
        Stop { pos: (sea * 0.35).max(0.02)                              , r: 17.0/255.0, g: 46.0/255.0,  b: 110.0/255.0 },
        Stop { pos: (sea * 0.8).max(0.04)                                , r: 34.0/255.0, g: 78.0/255.0,  b: 138.0/255.0 },
        Stop { pos: sea                                                   , r: 52.0/255.0, g: 112.0/255.0, b: 64.0/255.0  },
        Stop { pos: land_pos(0.08)                                        , r: 66.0/255.0, g: 131.0/255.0, b: 62.0/255.0  },
        Stop { pos: land_pos(0.20)                                        , r: 122.0/255.0,g: 154.0/255.0, b: 60.0/255.0  },
        Stop { pos: land_pos(0.40)                                        , r: 213.0/255.0,g: 191.0/255.0, b: 101.0/255.0 },
        Stop { pos: land_pos(0.55)                                        , r: 210.0/255.0,g: 143.0/255.0, b: 65.0/255.0  },
        Stop { pos: land_pos(0.85)                                        , r: 147.0/255.0,g: 72.0/255.0,  b: 33.0/255.0  },
        Stop { pos: land_pos(0.95)                                        , r: 128.0/255.0,g: 128.0/255.0, b: 128.0/255.0 },
        Stop { pos: land_pos(0.98)                                        , r: 210.0/255.0,g: 210.0/255.0, b: 210.0/255.0 },
        Stop { pos: snow                                                   , r: 0.94,       g: 0.94,        b: 0.94         },
        Stop { pos: 1.0                                                    , r: 1.0,        g: 1.0,         b: 1.0          },
    ];

    // Split at sea_level
    let eps = 1e-4;
    let split = sea - eps;

    let ocean: Vec<Stop> = {
        let mut v: Vec<Stop> = all.iter()
            .filter(|s| s.pos <= split)
            .cloned()
            .collect();
        // Ensure a stop exactly at the split boundary
        let last_pos = v.last().map(|s| s.pos).unwrap_or(0.0);
        if (last_pos - split).abs() > 1e-7 && split > 0.0 {
            let c = sample_stops(&all, split);
            v.push(Stop { pos: split, r: c[0], g: c[1], b: c[2] });
        }
        v
    };

    let land: Vec<Stop> = {
        let mut v: Vec<Stop> = all.iter()
            .filter(|s| s.pos >= sea + eps)
            .cloned()
            .collect();
        // Ensure a stop exactly at sea_level
        let first_pos = v.first().map(|s| s.pos).unwrap_or(1.0);
        if (first_pos - sea).abs() > 1e-7 && sea < 1.0 {
            let c = sample_stops(&all, sea);
            v.insert(0, Stop { pos: sea, r: c[0], g: c[1], b: c[2] });
        }
        v
    };

    (ocean, land)
}
