use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::prelude::*;

use crate::config::MAX_HEIGHT;
use crate::contour::ContourLevel;
use crate::river::RiverSegment;

/// Map elevation to a colour gradient: green (low) → olive → brown → grey (high).
pub fn elevation_color(elev: f64) -> Color {
    let t = (elev / MAX_HEIGHT).clamp(0.0, 1.0) as f32;

    if t < 0.33 {
        let s = t / 0.33;
        Color::srgb(0.12 + s * 0.35, 0.45 + s * 0.2, 0.1 + s * 0.05)
    } else if t < 0.66 {
        let s = (t - 0.33) / 0.33;
        Color::srgb(0.47 + s * 0.3, 0.65 - s * 0.25, 0.15 + s * 0.05)
    } else {
        let s = (t - 0.66) / 0.34;
        Color::srgb(0.77 + s * 0.1, 0.4 + s * 0.2, 0.2 + s * 0.2)
    }
}

/// Build a triangle-list mesh from all polylines of one contour level.
///
/// Each segment becomes a quad (2 triangles) of width `line_width`.
pub fn build_contour_line_mesh(level: &ContourLevel, line_width: f32) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for poly in &level.polylines {
        if poly.len() < 2 {
            continue;
        }
        for i in 0..poly.len() - 1 {
            let a = Vec2::new(poly[i][0] as f32, poly[i][1] as f32);
            let b = Vec2::new(poly[i + 1][0] as f32, poly[i + 1][1] as f32);

            let dir = b - a;
            let len = dir.length();
            if len < 1e-6 {
                continue;
            }
            let dir = dir / len;
            let perp = Vec2::new(-dir.y, dir.x) * line_width * 0.5;

            let base = positions.len() as u32;
            positions.extend_from_slice(&[
                [a.x - perp.x, a.y - perp.y, 0.0],
                [a.x + perp.x, a.y + perp.y, 0.0],
                [b.x - perp.x, b.y - perp.y, 0.0],
                [b.x + perp.x, b.y + perp.y, 0.0],
            ]);
            indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base + 1,
                base + 3,
                base + 2,
            ]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Build a triangle-list mesh from river segments. Each segment is a quad
/// of its own width, with small square caps at both ends so variable-width
/// segments join without gaps.
pub fn build_river_mesh(rivers: &[RiverSegment]) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let push_quad = |p: [[f32; 3]; 4], positions: &mut Vec<[f32; 3]>, indices: &mut Vec<u32>| {
        let base = positions.len() as u32;
        positions.extend_from_slice(&p);
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
    };

    for seg in rivers {
        let a = Vec2::new(seg.a[0] as f32, seg.a[1] as f32);
        let b = Vec2::new(seg.b[0] as f32, seg.b[1] as f32);
        let dir = b - a;
        let len = dir.length();
        if len < 1e-6 {
            continue;
        }
        let dir = dir / len;
        let hw = seg.width * 0.5;
        let perp = Vec2::new(-dir.y, dir.x) * hw;

        push_quad(
            [
                [a.x - perp.x, a.y - perp.y, 0.0],
                [a.x + perp.x, a.y + perp.y, 0.0],
                [b.x - perp.x, b.y - perp.y, 0.0],
                [b.x + perp.x, b.y + perp.y, 0.0],
            ],
            &mut positions,
            &mut indices,
        );

        // Square cap at each endpoint to bridge width changes / direction turns.
        for (c, w) in [(a, hw), (b, hw)] {
            push_quad(
                [
                    [c.x - w, c.y - w, 0.0],
                    [c.x + w, c.y - w, 0.0],
                    [c.x - w, c.y + w, 0.0],
                    [c.x + w, c.y + w, 0.0],
                ],
                &mut positions,
                &mut indices,
            );
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
