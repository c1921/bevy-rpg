/// Stitch a set of unordered segments into connected polylines.
pub(crate) fn chain_segments(segments: &[([f64; 2], [f64; 2])]) -> Vec<Vec<[f64; 2]>> {
    if segments.is_empty() {
        return Vec::new();
    }

    // Build adjacency: for each quantised key, list of (neighbour, seg_idx).
    // We quantise to avoid fp mismatch on shared edges.
    let n = segments.len();
    let mut adj: std::collections::HashMap<Key, Vec<(Key, usize)>> =
        std::collections::HashMap::with_capacity(n * 2);

    for (idx, &(a, b)) in segments.iter().enumerate() {
        let ka = key(a);
        let kb = key(b);
        adj.entry(ka).or_default().push((kb, idx));
        adj.entry(kb).or_default().push((ka, idx));
    }

    let mut used = vec![false; n];
    let mut result = Vec::new();

    for start_idx in 0..n {
        if used[start_idx] {
            continue;
        }
        used[start_idx] = true;

        let (mut head, mut tail) = segments[start_idx];

        // start the polyline
        let mut poly = vec![head, tail];

        // extend forward from `tail`
        loop {
            let k = key(tail);
            if let Some(neighbors) = adj.get(&k) {
                let mut extended = false;
                for &(_nk, ni) in neighbors {
                    if !used[ni] {
                        used[ni] = true;
                        let seg = segments[ni];
                        // which end connects?
                        if key(seg.0) == k {
                            tail = seg.1;
                        } else {
                            tail = seg.0;
                        }
                        poly.push(tail);
                        extended = true;
                        break;
                    }
                }
                if !extended {
                    break;
                }
            } else {
                break;
            }
        }

        // extend backward from `head`
        loop {
            let k = key(head);
            if let Some(neighbors) = adj.get(&k) {
                let mut extended = false;
                for &(_nk, ni) in neighbors {
                    if !used[ni] {
                        used[ni] = true;
                        let seg = segments[ni];
                        if key(seg.0) == k {
                            head = seg.1;
                        } else {
                            head = seg.0;
                        }
                        poly.insert(0, head);
                        extended = true;
                        break;
                    }
                }
                if !extended {
                    break;
                }
            } else {
                break;
            }
        }

        result.push(poly);
    }

    result
}

/// Quantise a point so that adjacent-cell identical edge-crossings collide.
fn key(p: [f64; 2]) -> Key {
    // 0.01 m tolerance – well below the 25 m grid spacing.
    let x = (p[0] * 100.0).round() as i64;
    let y = (p[1] * 100.0).round() as i64;
    Key { x, y }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    x: i64,
    y: i64,
}
