use bevy::prelude::*;

use crate::RegenerateRequest;

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

/// Spawn the UI: a "Generate" button and a status text, centered at the bottom.
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
            parent
                .spawn((
                    Button,
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

/// When the button is pressed, set the request flag and show status text.
pub fn regenerate_button(
    mut request: ResMut<RegenerateRequest>,
    mut status: ResMut<RegenerateStatus>,
    mut q_text: Query<&mut Text, With<StatusText>>,
    q_button: Query<&Interaction, (Changed<Interaction>, With<Button>)>,
) {
    if q_button.iter().any(|i| *i == Interaction::Pressed) {
        if let Ok(mut text) = q_text.single_mut() {
            **text = "Generating...".into();
        }
        status.remaining = 0.5;
        request.0 = true;
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
