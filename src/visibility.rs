use bevy::prelude::*;

use crate::resources::{
    Background, ContourEntities, IntermediateView, RenderMode, RiverEntities, ViewKind, ViewMode,
};

/// Sync contour-mesh visibility: shown only in the Final view when enabled.
pub fn sync_contour_visibility(
    render_mode: Res<RenderMode>,
    view_mode: Res<ViewMode>,
    contour_entities: Res<ContourEntities>,
    mut vis_query: Query<&mut Visibility>,
) {
    let show = render_mode.show_contours && view_mode.kind == ViewKind::Final;
    for &entity in &contour_entities.0 {
        if let Ok(mut vis) = vis_query.get_mut(entity) {
            *vis = if show {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Sync river-mesh visibility: shown only in the Final view when enabled.
pub fn sync_river_visibility(
    render_mode: Res<RenderMode>,
    view_mode: Res<ViewMode>,
    river_entities: Res<RiverEntities>,
    mut vis_query: Query<&mut Visibility>,
) {
    let show = render_mode.show_rivers && view_mode.kind == ViewKind::Final;
    for &entity in &river_entities.0 {
        if let Ok(mut vis) = vis_query.get_mut(entity) {
            *vis = if show {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Sync intermediate-view sprite visibility with `ViewMode` and `RenderMode`.
///
/// Only one view is visible at a time; when `show_3d` is off, all are hidden.
pub fn sync_view_visibility(
    render_mode: Res<RenderMode>,
    view_mode: Res<ViewMode>,
    mut bg_query: Query<&mut Visibility, (With<Background>, Without<IntermediateView>)>,
    mut iv_query: Query<(&mut Visibility, &crate::resources::IntermediateView), Without<Background>>,
) {
    let show_any = render_mode.show_3d;
    // Background (final) sprite.
    if let Ok(mut vis) = bg_query.single_mut() {
        *vis = if show_any && view_mode.kind == ViewKind::Final {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    // Intermediate-view sprites.
    for (mut vis, iv) in iv_query.iter_mut() {
        *vis = if show_any && iv.kind == view_mode.kind {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
