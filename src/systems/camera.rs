use bevy::prelude::*;

#[derive(Component)]
pub struct MainCamera;

pub fn setup_camera(mut commands: Commands) {
    // Isometric view, slightly elevated
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 32.0, 36.0)
                .looking_at(Vec3::new(0.0, 3.8, -5.0), Vec3::Y),
            projection: Projection::Orthographic(OrthographicProjection {
                scaling_mode: bevy::render::camera::ScalingMode::FixedHorizontal(46.0),
                ..default()
            }),
            ..default()
        },
        bevy::pbr::FogSettings {
            color: Color::rgb(0.53, 0.75, 0.78),
            falloff: bevy::pbr::FogFalloff::Linear {
                start: 55.0,
                end: 110.0,
            },
            ..default()
        },
        MainCamera,
    ));
}
