use crate::entities::Ball;
use bevy::prelude::*;

#[derive(Component)]
pub struct CameraRig {
    broadcast: bool,
    target: Vec3,
}

fn overview() -> Transform {
    Transform::from_xyz(0.0, 36.0, 36.0).looking_at(Vec3::new(0.0, 2.0, -3.5), Vec3::Y)
}

fn projection(broadcast: bool) -> Projection {
    if broadcast {
        Projection::Perspective(PerspectiveProjection {
            fov: 38.0_f32.to_radians(),
            ..default()
        })
    } else {
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::render::camera::ScalingMode::FixedHorizontal(48.0),
            ..default()
        })
    }
}

#[derive(Component)]
pub struct MainCamera;

pub fn setup_camera(mut commands: Commands) {
    let broadcast = std::env::var("CANOPY_CAMERA").as_deref() == Ok("broadcast");
    commands.spawn((
        Camera3dBundle {
            transform: if broadcast {
                Transform::from_xyz(0., 30., 42.).looking_at(Vec3::ZERO, Vec3::Y)
            } else {
                overview()
            },
            projection: projection(broadcast),
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
        CameraRig {
            broadcast,
            target: Vec3::ZERO,
        },
    ));
}

/// Presentation-only tracking: never writes to simulation entities.
pub fn update_camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    ball: Query<&Transform, (With<Ball>, Without<MainCamera>)>,
    mut cameras: Query<(&mut Transform, &mut Projection, &mut CameraRig), With<MainCamera>>,
) {
    for (mut transform, mut lens, mut rig) in &mut cameras {
        if keys.just_pressed(KeyCode::KeyC) {
            rig.broadcast = !rig.broadcast;
            *lens = projection(rig.broadcast);
            if !rig.broadcast {
                *transform = overview();
            }
        }
        if rig.broadcast {
            if let Ok(ball) = ball.get_single() {
                let desired = Vec3::new(
                    ball.translation.x.clamp(-12., 12.) * 0.6,
                    0.,
                    ball.translation.z.clamp(-9., 9.) * 0.35,
                );
                let alpha = 1. - (-3.0 * time.delta_seconds()).exp();
                rig.target = rig.target.lerp(desired, alpha);
            }
            *transform = Transform::from_translation(rig.target + Vec3::new(0., 30., 42.))
                .looking_at(rig.target, Vec3::Y);
        }
    }
}
