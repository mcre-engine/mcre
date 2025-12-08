use bevy::{input::mouse::MouseMotion, prelude::*};

use crate::{AppState, LoadingState};

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
        let yaw_quat = Quat::from_axis_angle(Vec3::Y, (-event.delta.x * rot_speed.0).to_radians());
        let pitch_quat =
            Quat::from_axis_angle(Vec3::X, (-event.delta.y * rot_speed.0).to_radians());
        transform.rotation = yaw_quat * transform.rotation * pitch_quat;
    }
}

fn camera_movement(
    mut camera: Query<(&mut Transform, &CameraMovementSpeed), With<Camera>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    let (mut transform, movement_speed) = camera
        .single_mut()
        .expect("must have camera with the use of this system");
    if key.pressed(KeyCode::KeyW) {
        let mut v = Vec3::from(transform.forward());
        v.y = 0.0;
        transform.translation += v.normalize() * movement_speed.0;
    }
    if key.pressed(KeyCode::KeyS) {
        let mut v = Vec3::from(transform.back());
        v.y = 0.0;
        transform.translation += v.normalize() * movement_speed.0;
    }
    if key.pressed(KeyCode::KeyA) {
        let mut v = Vec3::from(transform.left());
        v.y = 0.0;
        transform.translation += v.normalize() * movement_speed.0;
    }
    if key.pressed(KeyCode::KeyD) {
        let mut v = Vec3::from(transform.right());
        v.y = 0.0;
        transform.translation += v.normalize() * movement_speed.0;
    }
    if key.pressed(KeyCode::Space) {
        transform.translation += Vec3::new(0.0, movement_speed.0, 0.0);
    }
    if key.pressed(KeyCode::ShiftLeft) {
        transform.translation += Vec3::new(0.0, -movement_speed.0, 0.0);
    }
}
