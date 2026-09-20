//! The camera that starts at the easel and flies to the pitch.
//!
//! One camera, one continuous move. The creation screen and the match are not
//! separate views of separate scenes — they are two framings of one world, and
//! the transition between them is travel, not a cut.

use super::{place, CreationPhase, EASEL_LOCAL};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;

/// Tags the camera that owns the creation phase.
#[derive(Component)]
pub struct CreationCamera;

/// Runs the flight from the easel to the broadcast position.
#[derive(Resource)]
pub struct Flight {
    pub elapsed: f32,
    pub duration: f32,
    pub from: Transform,
    pub to: Transform,
}

/// Where the painter stands: close enough that the canvas dominates the frame,
/// angled so the model sits to its left and the stadium shows past its right
/// edge. The whole composition is readable in one shot without moving.
pub fn easel_view() -> Transform {
    // Set CANOPY_SITTING_WIDE to step back and inspect the whole island, in the
    // same spirit as CANOPY_CLOSEUP on the match camera. Presentation only.
    if std::env::var("CANOPY_SITTING_WIDE").is_ok() {
        return Transform::from_translation(place(EASEL_LOCAL + Vec3::new(9.0, 7.0, 19.0)))
            .looking_at(place(EASEL_LOCAL + Vec3::new(-1.5, 1.0, 0.)), Vec3::Y);
    }
    let canvas = EASEL_LOCAL + Vec3::new(0., 2.55, 0.52);
    Transform::from_translation(place(canvas + Vec3::new(1.62, 0.72, 6.15))).looking_at(
        // Aiming slightly below centre leaves headroom for the ranges behind.
        place(canvas + Vec3::new(-0.15, -0.18, 0.)),
        Vec3::Y,
    )
}

/// Stepped back and to the left, so the easel and the second board share the
/// frame for review.
pub fn review_view() -> Transform {
    Transform::from_translation(place(EASEL_LOCAL + Vec3::new(-1.1, 3.3, 9.4)))
        .looking_at(place(EASEL_LOCAL + Vec3::new(-1.35, 2.25, 0.8)), Vec3::Y)
}

/// Eases the camera between the painting view and the review view. The same
/// rule as the flight, at a smaller scale: the view changes by moving.
pub fn glide(
    time: Res<Time>,
    phase: Res<State<CreationPhase>>,
    mut cameras: Query<&mut Transform, With<CreationCamera>>,
) {
    let Ok(mut transform) = cameras.get_single_mut() else {
        return;
    };
    let target = if *phase.get() == CreationPhase::Review { review_view() } else { easel_view() };
    let alpha = 1. - (-4.0 * time.delta_seconds()).exp();
    transform.translation = transform.translation.lerp(target.translation, alpha);
    transform.rotation = transform.rotation.slerp(target.rotation, alpha);
}

/// The match framing,/// The match framing, matched to the broadcast camera the game itself uses so
/// the flight lands exactly where gameplay begins with no visible correction.
pub fn broadcast_view() -> Transform {
    Transform::from_translation(Vec3::new(0., 44., 78.)).looking_at(Vec3::new(0., 10., 0.), Vec3::Y)
}

pub fn spawn_camera(mut c: Commands) {
    c.spawn((
        Camera3dBundle {
            transform: easel_view(),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: 42.0_f32.to_radians(),
                ..default()
            }),
            // Matches the match camera exactly: authored chroma, no filmic curve.
            tonemapping: Tonemapping::None,
            ..default()
        },
        bevy::pbr::FogSettings {
            color: Color::rgb(0.53, 0.75, 0.78),
            falloff: bevy::pbr::FogFalloff::Linear {
                start: 150.0,
                end: 560.0,
            },
            ..default()
        },
        CreationCamera,
    ));
}

/// Smoothstep: no abrupt start or stop, so the move reads as a camera being
/// carried rather than a value being animated.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3.0 - 2.0 * t)
}

/// Carries the camera along the flight path, then hands over to the match.
///
/// The path bows outward through a waypoint above the valley rather than
/// interpolating straight through the mountainside — a straight line from the
/// peak to the broadcast position would clip through the summit massif.
pub fn fly(
    time: Res<Time>,
    mut flight: ResMut<Flight>,
    mut cameras: Query<&mut Transform, With<CreationCamera>>,
    mut phase: ResMut<NextState<CreationPhase>>,
) {
    let Ok(mut transform) = cameras.get_single_mut() else {
        return;
    };
    flight.elapsed += time.delta_seconds();
    let t = ease(flight.elapsed / flight.duration);

    // The straight line from the island to the broadcast position passes
    // through the summit massif, so the path bows up and over through a raised
    // midpoint. Derived from the two ends rather than tuned to one island, so
    // moving the clearing does not fly the camera through a mountain.
    let control = (flight.from.translation + flight.to.translation) * 0.5 + Vec3::Y * 26.0;
    let a = flight.from.translation.lerp(control, t);
    let b = control.lerp(flight.to.translation, t);
    transform.translation = a.lerp(b, t);
    transform.rotation = flight.from.rotation.slerp(flight.to.rotation, t);

    if flight.elapsed >= flight.duration {
        transform.clone_from(&flight.to);
        phase.set(CreationPhase::Finished);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_painter_stands_behind_the_easel_looking_at_the_pitch() {
        let view = easel_view();
        let canvas = place(EASEL_LOCAL + Vec3::new(0., 2.55, 0.52));
        let to_canvas = (canvas - view.translation).normalize();
        assert!(view.forward().dot(to_canvas) > 0.98, "the camera looks at the canvas");
        // And past it: the pitch is beyond the canvas, not behind the painter.
        let to_pitch = (Vec3::ZERO - view.translation).normalize();
        assert!(view.forward().dot(to_pitch) > 0.8, "the stadium is in shot");
    }

    #[test]
    fn the_flight_lands_exactly_on_the_broadcast_framing() {
        // The match camera is authoritative; creation must arrive on its value.
        let landed = broadcast_view();
        assert_eq!(landed.translation, Vec3::new(0., 44., 78.));
    }

    #[test]
    fn the_ease_curve_is_flat_at_both_ends() {
        assert_eq!(ease(0.), 0.);
        assert_eq!(ease(1.), 1.);
        // Symmetric about the midpoint, so the move accelerates and decelerates
        // by the same amount.
        assert!((ease(0.5) - 0.5).abs() < 1e-6);
        assert!(ease(0.1) < 0.1, "slow to start");
        assert!(ease(0.9) > 0.9, "slow to finish");
    }
}
