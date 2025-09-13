use crate::{CAMERA_SPEED, ZOOM_IN_SPEED, ZOOM_OUT_SPEED, map::TILE_SIZE, units::Player};
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::mouse::{MouseScrollUnit, MouseWheel},
    prelude::*,
};

#[derive(Component)]
pub struct Selected;

#[derive(Component)]
pub struct CameraMovement(pub CameraMovementKind);

#[derive(Clone, Copy)]
pub enum CameraMovementKind {
    SmoothFollowPlayer,
    DirectFollowPlayer,
    FreeCamera,
}

impl CameraMovementKind {
    pub fn next(self) -> Self {
        use CameraMovementKind::*;
        match self {
            SmoothFollowPlayer => DirectFollowPlayer,
            DirectFollowPlayer => FreeCamera,
            FreeCamera => SmoothFollowPlayer,
        }
    }
}

#[derive(Resource)]
pub struct UpsCounter {
    pub ticks: u32,
    pub last_second: f64,
    pub ups: u32,
}

pub fn display_fps_ups_system(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    mut counter: ResMut<UpsCounter>,
) {
    let now = time.elapsed_secs_f64();
    if now - counter.last_second >= 1.0 {
        // Calcule l’UPS
        counter.ups = counter.ticks;
        counter.ticks = 0;
        counter.last_second = now;

        // Récupère le FPS depuis le plugin
        if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(fps_avg) = fps.smoothed() {
                println!("FPS: {:.0} | UPS: {}", fps_avg, counter.ups);
            }
        }
    }
}

pub fn handle_camera_inputs_system(
    mut camera_query: Query<
        (&mut Transform, &mut Projection, &mut CameraMovement),
        (With<Camera>, Without<Player>),
    >,
    input: Res<ButtonInput<KeyCode>>,
    mut input_mouse_wheel: EventReader<MouseWheel>,
    player_query: Query<&Transform, With<Player>>,
    time: Res<Time>,
) {
    let Ok((mut camera_transform, mut projection, mut camera_movement)) = camera_query.single_mut()
    else {
        return;
    };

    if input.just_pressed(KeyCode::KeyM) {
        camera_movement.0 = camera_movement.0.next();
    }

    if let Ok(player_transform) = player_query.single() {
        match camera_movement.0 {
            CameraMovementKind::SmoothFollowPlayer => {
                let Vec3 { x, y, .. } = player_transform.translation;
                let direction = Vec3::new(x, y, camera_transform.translation.z);
                camera_transform
                    .translation
                    .smooth_nudge(&direction, 2., time.delta_secs());
            }
            CameraMovementKind::DirectFollowPlayer => {
                camera_transform.translation = player_transform.translation;
            }
            _ => (),
        }
    }

    match camera_movement.0 {
        CameraMovementKind::FreeCamera => {
            // free Camera movement controls
            let mut direction = Vec3::ZERO;
            if input.pressed(KeyCode::KeyW) {
                direction.y += 1.0;
            }
            if input.pressed(KeyCode::KeyS) {
                direction.y -= 1.0;
            }
            if input.pressed(KeyCode::KeyA) {
                direction.x -= 1.0;
            }
            if input.pressed(KeyCode::KeyD) {
                direction.x += 1.0;
            }

            // Récupérer le niveau de zoom actuel
            let zoom_scale = if let Projection::Orthographic(projection2d) = &*projection {
                projection2d.scale
            } else {
                1.0 // Valeur par défaut si ce n'est pas une projection orthographique
            };

            // normalizes to have constant diagonal speed
            if direction != Vec3::ZERO {
                direction = direction.normalize();
                let speed_in_pixels =
                    CAMERA_SPEED * TILE_SIZE.x * zoom_scale.powf(0.7) * time.delta_secs();
                camera_transform.translation += direction * speed_in_pixels;
            }
        }
        _ => (),
    }

    // zoom controls
    if let Projection::Orthographic(projection2d) = &mut *projection {
        for mouse_wheel_event in input_mouse_wheel.read() {
            use bevy::math::ops::powf;
            match mouse_wheel_event.unit {
                MouseScrollUnit::Line => {
                    if mouse_wheel_event.y > 0.0 {
                        projection2d.scale *= powf(ZOOM_IN_SPEED, time.delta_secs());
                    } else if mouse_wheel_event.y < 0.0 {
                        projection2d.scale *= powf(ZOOM_OUT_SPEED, time.delta_secs());
                    }
                }
                MouseScrollUnit::Pixel => {
                    if mouse_wheel_event.y > 0.0 {
                        projection2d.scale *= powf(ZOOM_IN_SPEED, time.delta_secs());
                    } else if mouse_wheel_event.y < 0.0 {
                        projection2d.scale *= powf(ZOOM_OUT_SPEED, time.delta_secs());
                    }
                }
            }
        }
    }
}
