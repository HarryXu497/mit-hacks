use crate::entities::Ball;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;

/// `Default` so a host can hand an existing camera over to the match: the creation camera flies
/// to the pitch and then gains this, rather than a second camera being spawned for the match.
#[derive(Component, Default)]
pub struct CameraRig {
    target: Vec3,
}

/// Set CANOPY_CLOSEUP to inspect the characters at model scale. Presentation
/// only: it moves the camera and nothing else, so physics and controls are
/// unaffected and the normal broadcast view is what ships.
fn closeup_transform(target: Vec3) -> Option<Transform> {
    std::env::var("CANOPY_CLOSEUP").ok().map(|_| {
        Transform::from_translation(target + Vec3::new(-2.6, 3.4, 7.0))
            .looking_at(target + Vec3::new(0., 1.4, 0.), Vec3::Y)
    })
}

fn broadcast_transform(target: Vec3) -> Transform {
    // Summit broadcast: the pitch fills the lower frame while the aim point sits
    // above the turf, so the horizon, the ranges behind it and a band of sky stay
    // in shot. Pitched much steeper than this and the mountains crop off-screen.
    Transform::from_translation(target + Vec3::new(0., 44., 78.))
        .looking_at(target + Vec3::new(0., 10., 0.), Vec3::Y)
}

#[derive(Component)]
pub struct MainCamera;

pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3dBundle {
            transform: closeup_transform(Vec3::ZERO).unwrap_or(broadcast_transform(Vec3::ZERO)),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: 42.0_f32.to_radians(),
                ..default()
            }),
            // The default filmic curve (TonyMcMapface) desaturates and rolls off
            // highlights for photographic realism. A flat cartoon palette wants
            // its authored chroma delivered intact, so the transform is skipped.
            tonemapping: Tonemapping::None,
            ..default()
        },
        bevy::pbr::FogSettings {
            color: Color::rgb(0.53, 0.75, 0.78),
            // Reaches far enough that the back ranges keep their silhouettes;
            // depth comes from their own colour wash, not from fog erasing them.
            falloff: bevy::pbr::FogFalloff::Linear {
                start: 150.0,
                end: 560.0,
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
    if std::env::var("TACTIC_LAB_AUTOPLAY").is_ok() {
        static TICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n % 120 == 0 {
            eprintln!("DIAG update_camera tick {n}; cameras matched = {}", cameras.iter().count());
        }
    }
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
        *transform = closeup_transform(rig.target).unwrap_or(broadcast_transform(rig.target));
    }
}
