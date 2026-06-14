use bevy::prelude::*;

use crate::resources::ViewKind;

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

/// Marker for the rivers toggle button.
#[derive(Component)]
pub(crate) struct ToggleRiverButton;

/// Marker + payload for view-selection buttons (radio style).
#[derive(Component, Clone, Copy)]
pub(crate) struct ViewButton(pub ViewKind);

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

                    // Rivers toggle
                    row.spawn((
                        Button,
                        ToggleRiverButton,
                        Node {
                            padding: UiRect::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.12, 0.45, 0.25)),
                    ))
                    .with_child((
                        Text::new("Rivers: ON"),
                        TextFont::from_font_size(16.0),
                        TextColor(Color::WHITE),
                    ));
                });

            // ── view select row (radio buttons) ────────────────
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    margin: UiRect::top(Val::Px(8.0)),
                    ..default()
                })
                .with_children(|row| {
                    for (kind, label) in [
                        (ViewKind::Final, "Final"),
                        (ViewKind::DrainageField, "Drainage"),
                        (ViewKind::CompressedNorm, "Compressed N"),
                        (ViewKind::ProcessedNoise, "Compressed"),
                        (ViewKind::InitialNoise, "Raw Noise"),
                    ] {
                        row.spawn((
                            Button,
                            ViewButton(kind),
                            Node {
                                padding: UiRect::all(Val::Px(8.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                        ))
                        .with_child((
                            Text::new(label),
                            TextFont::from_font_size(15.0),
                            TextColor(Color::WHITE),
                        ));
                    }
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
