use std::f32::consts::PI;

use bevy::{input::mouse::MouseMotion, prelude::*};

use crate::{AppState, LoadingState, Sun};

pub struct FirstPersonPlugin {
    pub transform: Transform,
    pub camera_rotation_speed: f32,
    pub camera_movement_speed: f32,
}

impl Plugin for FirstPersonPlugin {
    fn build(&self, app: &mut App) {
        let transform = self.transform;
        let rot_speed = self.camera_rotation_speed;
        let move_speed = self.camera_movement_speed;
        app.add_systems(
            OnEnter(LoadingState::Camera),
            move |mut commands: Commands, mut next: ResMut<NextState<LoadingState>>| {
                commands.spawn(FirstPersonCamera {
                    rotation_speed: CameraRotationSpeed(rot_speed),
                    movement_speed: CameraMovementSpeed(move_speed),
                    transform,
                    camera: Camera3d::default(),
                });
                next.set(LoadingState::Textures);
            },
        )
        .add_systems(
            Update,
            (camera_rotation, camera_movement).run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
struct CameraRotationSpeed(f32);

#[derive(Component)]
struct CameraMovementSpeed(f32);

#[derive(Bundle)]
struct FirstPersonCamera {
    rotation_speed: CameraRotationSpeed,
    movement_speed: CameraMovementSpeed,
    transform: Transform,
    #[bundle()]
    camera: Camera3d,
}

fn camera_rotation(
    mut camera: Query<(&mut Transform, &CameraRotationSpeed), With<Camera>>,
    mut mouse_motion_events: MessageReader<MouseMotion>,
) {
    let (mut transform, rot_speed) = camera
        .single_mut()
        .expect("must have camera with the use of this system");

    for event in mouse_motion_events.read() {
        let yaw_rotation = Quat::from_rotation_y((-event.delta.x * rot_speed.0).to_radians());
        let mut pitch_delta = (-event.delta.y * rot_speed.0).to_radians();
        // Clamp to up and down
        if pitch_delta > 0. {
            pitch_delta = pitch_delta.min(transform.forward().angle_between(Vec3::Y));
        } else {
            pitch_delta = pitch_delta.max(-transform.forward().angle_between(-Vec3::Y));
        }
        transform.rotation = yaw_rotation * transform.rotation * Quat::from_rotation_x(pitch_delta);
    }
}

#[allow(clippy::type_complexity)]
fn camera_movement(
    mut params: ParamSet<(
        Query<&mut Transform, With<Sun>>,
        Query<(&mut Transform, &CameraMovementSpeed), With<Camera>>,
    )>,
    key: Res<ButtonInput<KeyCode>>,
) {
    let mut camera = params.p1();
    let (mut transform, movement_speed) = camera
        .single_mut()
        .expect("must have camera with the use of this system");

    if key.pressed(KeyCode::KeyW) {
        let dot = transform.forward().dot(Vec3::Y).abs();
        let forward = if ((1. - 0.1)..=(1. + 0.1)).contains(&dot) {
            Vec3::from(transform.right()).rotate_y(PI / 2.).with_y(0.)
        } else {
            Vec3::from(transform.forward()).with_y(0.)
        }
        .normalize();
        transform.translation += forward * movement_speed.0;
    }
    if key.pressed(KeyCode::KeyS) {
        let dot = transform.back().dot(Vec3::Y).abs();
        let back = if ((1. - 0.1)..=(1. + 0.1)).contains(&dot) {
            Vec3::from(transform.right()).rotate_y(-PI / 2.).with_y(0.)
        } else {
            Vec3::from(transform.back()).with_y(0.)
        }
        .normalize();
        transform.translation += back * movement_speed.0;
    }
    if key.pressed(KeyCode::KeyA) {
        let v = Vec3::from(transform.left()).with_y(0.);
        transform.translation += v.normalize() * movement_speed.0;
    }
    if key.pressed(KeyCode::KeyD) {
        let v = Vec3::from(transform.right()).with_y(0.);
        transform.translation += v.normalize() * movement_speed.0;
    }
    if key.pressed(KeyCode::Space) {
        transform.translation += Vec3::new(0.0, movement_speed.0, 0.0);
    }
    if key.pressed(KeyCode::ShiftLeft) {
        transform.translation += Vec3::new(0.0, -movement_speed.0, 0.0);
    }
    let current = transform.translation;
    let mut sun = params.p0();
    let mut sun = sun.single_mut().unwrap();
    *sun = sun.looking_at(current, Vec3::Y);
}
