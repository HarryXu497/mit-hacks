use bevy::prelude::*;
use crate::game::config::*;

#[derive(Component)]
pub struct MainCamera;

pub fn setup_camera(mut commands: Commands) {
    // Isometric view, slightly elevated
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 20.0, 25.0)
                .looking_at(Vec3::new(0.0, FIELD_HEIGHT, 0.0), Vec3::Y),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: 45.0_f32.to_radians(),
                ..default()
            }),
            ..default()
        },
        MainCamera,
    ));
}
