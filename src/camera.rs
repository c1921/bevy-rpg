use bevy::prelude::*;

/// Resource tracking camera drag state.
#[derive(Resource, Default)]
pub struct CameraDrag {
    pub dragging: bool,
    pub last_mouse: Option<Vec2>,
}

/// Pan + zoom for a 2-D camera.
pub fn camera_control(
    mut drag: ResMut<CameraDrag>,
    mut q_camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
    mouse_btn: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    window: Query<&Window>,
) {
    let Ok((mut transform, mut projection)) = q_camera.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = projection.as_mut() else {
        return;
    };
    let Ok(window) = window.single() else {
        return;
    };
    let cursor = window.cursor_position();

    // ── middle-button drag pan ────────────────────────────────
    if mouse_btn.just_pressed(MouseButton::Middle) {
        drag.dragging = true;
        drag.last_mouse = cursor;
    }
    if mouse_btn.just_released(MouseButton::Middle) {
        drag.dragging = false;
    }
    if drag.dragging {
        if let (Some(pos), Some(last)) = (cursor, drag.last_mouse) {
            let delta = pos - last;
            transform.translation.x -= delta.x * ortho.scale;
            transform.translation.y -= delta.y * ortho.scale;
        }
        drag.last_mouse = cursor;
    }

    // ── scroll-wheel zoom (centered on cursor) ────────────────
    let old_scale = ortho.scale;
    if scroll.delta.y.abs() > 1e-6 {
        ortho.scale *= 1.0 - scroll.delta.y * 0.1;
        ortho.scale = ortho.scale.clamp(1.0, 500.0);
    }
    if (ortho.scale - old_scale).abs() > 1e-9 {
        if let Some(cursor) = cursor {
            let win_size = Vec2::new(window.width(), window.height());
            let center = win_size / 2.0;
            let pre_world = transform.translation.truncate() + (cursor - center) * old_scale;
            let post_world =
                transform.translation.truncate() + (cursor - center) * ortho.scale;
            transform.translation += (pre_world - post_world).extend(0.0);
        }
    }

    // ── WASD / arrow keyboard pan ─────────────────────────────
    let pan_speed = 300.0 * ortho.scale;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        transform.translation.y += pan_speed;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        transform.translation.y -= pan_speed;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        transform.translation.x -= pan_speed;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        transform.translation.x += pan_speed;
    }
}
