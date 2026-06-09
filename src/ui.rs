use bevy::prelude::*;

use crate::{RegenerateRequest, RenderMode};

/// Resource controlling how long the status text stays visible (seconds).
#[derive(Resource)]
pub struct RegenerateStatus {
    remaining: f32,
}

impl Default for RegenerateStatus {
    fn default() -> Self {
        Self { remaining: 0.0 }
    }
}

/// Marker component for the status text entity.
#[derive(Component)]
pub(crate) struct StatusText;

/// Marker for the "Generate" button.
#[derive(Component)]
pub(crate) struct GenerateButton;

/// Marker for the 3D terrain toggle button.
#[derive(Component)]
pub(crate) struct Toggle3DButton;

/// Marker for the contour lines toggle button.
#[derive(Component)]
pub(crate) struct ToggleContourButton;

/// Spawn the UI: Generate button, render-mode toggles, and a status text.
pub fn spawn_ui(mut commands: Commands) {
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::FlexEnd,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
            padding: UiRect::bottom(Val::Px(32.0)),
            position_type: PositionType::Absolute,
            ..default()
        })
        .with_children(|parent| {
            // ── Generate button ──────────────────────────────────
            parent
                .spawn((
                    Button,
                    GenerateButton,
                    Node {
                        padding: UiRect::all(Val::Px(12.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.18)),
                ))
                .with_child((
                    Text::new("Generate"),
                    TextFont::from_font_size(24.0),
                    TextColor(Color::WHITE),
                ));

            // ── toggle row ───────────────────────────────────────
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    margin: UiRect::top(Val::Px(8.0)),
                    ..default()
                })
                .with_children(|row| {
                    // 3D Terrain toggle
                    row.spawn((
                        Button,
                        Toggle3DButton,
                        Node {
                            padding: UiRect::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.12, 0.45, 0.25)),
                    ))
                    .with_child((
                        Text::new("3D Terrain: ON"),
                        TextFont::from_font_size(16.0),
                        TextColor(Color::WHITE),
                    ));

                    // Contour Lines toggle
                    row.spawn((
                        Button,
                        ToggleContourButton,
                        Node {
                            padding: UiRect::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.12, 0.45, 0.25)),
                    ))
                    .with_child((
                        Text::new("Contours: ON"),
                        TextFont::from_font_size(16.0),
                        TextColor(Color::WHITE),
                    ));
                });

            // ── status text ──────────────────────────────────────
            parent.spawn((
                StatusText,
                Text::new(""),
                TextFont::from_font_size(16.0),
                TextColor(Color::srgb(0.9, 0.9, 0.4)),
                Node {
                    margin: UiRect::top(Val::Px(8.0)),
                    ..default()
                },
            ));
        });
}

/// When the Generate button is pressed, set the request flag and show status text.
pub fn regenerate_button(
    mut request: ResMut<RegenerateRequest>,
    mut status: ResMut<RegenerateStatus>,
    mut q_text: Query<&mut Text, With<StatusText>>,
    q_button: Query<&Interaction, (Changed<Interaction>, With<GenerateButton>)>,
) {
    if q_button.iter().any(|i| *i == Interaction::Pressed) {
        if let Ok(mut text) = q_text.single_mut() {
            **text = "Generating...".into();
        }
        status.remaining = 0.5;
        request.0 = true;
    }
}

/// Handle clicks on the render-mode toggle buttons.
pub fn toggle_render_mode(
    mut render_mode: ResMut<RenderMode>,
    q_3d: Query<&Interaction, (Changed<Interaction>, With<Toggle3DButton>)>,
    q_contour: Query<&Interaction, (Changed<Interaction>, With<ToggleContourButton>)>,
    mut q_btn: Query<(&mut BackgroundColor, &Children), Or<(With<Toggle3DButton>, With<ToggleContourButton>)>>,
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
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Tick the countdown; clear the status text when it expires.
pub fn update_status(
    time: Res<Time>,
    mut status: ResMut<RegenerateStatus>,
    mut q_text: Query<&mut Text, With<StatusText>>,
) {
    if status.remaining > 0.0 {
        status.remaining -= time.delta_secs();
        if status.remaining <= 0.0 {
            if let Ok(mut text) = q_text.single_mut() {
                **text = "".into();
            }
        }
    }
}
