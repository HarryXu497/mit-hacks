use crate::entities::Ball;
use bevy::prelude::*;

#[derive(Component)]
pub struct CameraRig {
    target: Vec3,
}

fn broadcast_transform(target: Vec3) -> Transform {
    Transform::from_translation(target + Vec3::new(0., 30., 42.)).looking_at(target, Vec3::Y)
}

#[derive(Component)]
pub struct MainCamera;

pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3dBundle {
            transform: broadcast_transform(Vec3::ZERO),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: 38.0_f32.to_radians(),
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
        CameraRig { target: Vec3::ZERO },
    ));
}

/// Presentation-only tracking: never writes to simulation entities.
pub fn update_camera(
    time: Res<Time>,
    ball: Query<&Transform, (With<Ball>, Without<MainCamera>)>,
    mut cameras: Query<(&mut Transform, &mut CameraRig), With<MainCamera>>,
) {
    for (mut transform, mut rig) in &mut cameras {
        if let Ok(ball) = ball.get_single() {
            let desired = Vec3::new(
                ball.translation.x.clamp(-12., 12.) * 0.6,
                0.,
                ball.translation.z.clamp(-9., 9.) * 0.35,
            );
            let alpha = 1. - (-3.0 * time.delta_seconds()).exp();
            rig.target = rig.target.lerp(desired, alpha);
        }
        *transform = broadcast_transform(rig.target);
    }
}
