use bevy::prelude::*;

use crate::resources::{RegenerateRequest, RegenerateStatus, RenderMode, ViewMode};
use crate::ui::{GenerateButton, StatusText, Toggle3DButton, ToggleContourButton, ToggleRiverButton, ViewButton};

/// When the Generate button is pressed, set the request flag and show status text.
pub fn regenerate_button(
    mut request: ResMut<RegenerateRequest>,
    mut status: ResMut<RegenerateStatus>,
    q_button: Query<&Interaction, (Changed<Interaction>, With<GenerateButton>)>,
) {
    if q_button.iter().any(|i| *i == Interaction::Pressed) {
        status.label = "Generating...".into();
        status.remaining = 0.5;
        request.0 = true;
    }
}

/// Handle clicks on the render-mode toggle buttons.
pub fn toggle_render_mode(
    mut render_mode: ResMut<RenderMode>,
    q_3d: Query<&Interaction, (Changed<Interaction>, With<Toggle3DButton>)>,
    q_contour: Query<&Interaction, (Changed<Interaction>, With<ToggleContourButton>)>,
    q_river: Query<&Interaction, (Changed<Interaction>, With<ToggleRiverButton>)>,
    mut q_btn: Query<(&mut BackgroundColor, &Children), Or<(With<Toggle3DButton>, With<ToggleContourButton>, With<ToggleRiverButton>)>>,
    mut q_text: Query<&mut Text>,
) {
    let mut changed = false;

    if q_3d.iter().any(|i| *i == Interaction::Pressed) {
        render_mode.show_3d = !render_mode.show_3d;
        changed = true;
    }
    if q_contour.iter().any(|i| *i == Interaction::Pressed) {
        render_mode.show_contours = !render_mode.show_contours;
        changed = true;
    }
    if q_river.iter().any(|i| *i == Interaction::Pressed) {
        render_mode.show_rivers = !render_mode.show_rivers;
        changed = true;
    }

    if changed {
        // Update button colours and text
        for (mut bg, children) in q_btn.iter_mut() {
            // Determine which button by text (safe enough for a small UI)
            for child in children.iter() {
                if let Ok(mut text) = q_text.get_mut(child) {
                    match text.as_str() {
                        t if t.starts_with("3D Terrain:") => {
                            if render_mode.show_3d {
                                *bg = BackgroundColor(Color::srgb(0.12, 0.45, 0.25));
                                **text = "3D Terrain: ON".into();
                            } else {
                                *bg = BackgroundColor(Color::srgb(0.45, 0.15, 0.15));
                                **text = "3D Terrain: OFF".into();
                            }
                        }
                        t if t.starts_with("Contours:") => {
                            if render_mode.show_contours {
                                *bg = BackgroundColor(Color::srgb(0.12, 0.45, 0.25));
                                **text = "Contours: ON".into();
                            } else {
                                *bg = BackgroundColor(Color::srgb(0.45, 0.15, 0.15));
                                **text = "Contours: OFF".into();
                            }
                        }
                        t if t.starts_with("Rivers:") => {
                            if render_mode.show_rivers {
                                *bg = BackgroundColor(Color::srgb(0.12, 0.45, 0.25));
                                **text = "Rivers: ON".into();
                            } else {
                                *bg = BackgroundColor(Color::srgb(0.45, 0.15, 0.15));
                                **text = "Rivers: OFF".into();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Tick the countdown; refresh status text from the resource label.
pub fn update_status(
    time: Res<Time>,
    mut status: ResMut<RegenerateStatus>,
    mut q_text: Query<&mut Text, With<StatusText>>,
) {
    if status.remaining > 0.0 {
        status.remaining -= time.delta_secs();
    }
    if let Ok(mut text) = q_text.single_mut() {
        if status.remaining > 0.0 {
            **text = status.label.clone();
        } else {
            **text = "".into();
        }
    }
}

/// Handle view-selection radio buttons: clicked button sets the active view.
/// Highlights the active button with a distinct background.
pub fn select_view_mode(
    mut view_mode: ResMut<ViewMode>,
    q_clicked: Query<(&Interaction, &ViewButton)>,
    mut q_btn: Query<(&mut BackgroundColor, &ViewButton)>,
) {
    // Detect which button was pressed.
    for (interaction, vb) in q_clicked.iter() {
        if *interaction == Interaction::Pressed {
            view_mode.kind = vb.0;
        }
    }
    // Update button highlights.
    let active = view_mode.kind;
    for (mut bg, vb) in q_btn.iter_mut() {
        if vb.0 == active {
            *bg = BackgroundColor(Color::srgb(0.15, 0.35, 0.55));
        } else {
            *bg = BackgroundColor(Color::srgb(0.15, 0.15, 0.15));
        }
    }
}
