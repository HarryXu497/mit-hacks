//! Camera shots for the front end.
//!
//! The match camera in [`crate::systems::camera`] keeps running its own damped
//! follow once play starts; this module owns the camera everywhere else. Both
//! write the same component, so exactly one of them is scheduled at a time.

use bevy::prelude::*;

use super::AppState;
use crate::systems::camera::MainCamera;

/// A camera position, its aim point and the lens it is shot on.
///
/// Focal length belongs to the shot rather than sitting as a constant because
/// the cold open starts wide, which exaggerates the rush past the cliffs, and
/// closes to the broadcast lens as it settles so the stadium stops bowing at
/// the edges of frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub eye: Vec3,
    pub look: Vec3,
    pub fov_degrees: f32,
}

impl Shot {
    pub const fn new(eye: Vec3, look: Vec3, fov_degrees: f32) -> Self {
        Self {
            eye,
            look,
            fov_degrees,
        }
    }

    pub fn mix(self, other: Self, t: f32) -> Self {
        Self {
            eye: self.eye.lerp(other.eye, t),
            look: self.look.lerp(other.look, t),
            fov_degrees: self.fov_degrees + (other.fov_degrees - self.fov_degrees) * t,
        }
    }

    fn transform(self) -> Transform {
        Transform::from_translation(self.eye).looking_at(self.look, Vec3::Y)
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// Title card. Low, close and off to one side: the bowl fills the right of
/// frame with the ranges behind it, and the hillside on the left stays empty
/// for the logo to sit on. Pulled back any further and the stadium shrinks
/// into a wide band of sky.
pub const HERO: Shot = Shot::new(Vec3::new(-46., 13., 50.), Vec3::new(-3., 6., 0.), 46.);

/// The framing the match camera computes for a centred ball. The drop-in ends
/// here so play begins on the shot the menu was already flying toward.
pub const BROADCAST: Shot = Shot::new(Vec3::new(0., 44., 78.), Vec3::new(0., 10., 0.), 42.);

/// Keyframes for the opening flight, in seconds. Starts under the lip of the
/// stadium among the waterfalls, climbs the cliff face, swings out to the title
/// framing.
const COLD_OPEN: [(f32, Shot); 5] = [
    (
        0.0,
        Shot::new(Vec3::new(-70., -14., 40.), Vec3::new(-30., 6., 4.), 66.),
    ),
    (
        2.6,
        Shot::new(Vec3::new(-64., -2., 44.), Vec3::new(-26., 8., 4.), 62.),
    ),
    (
        5.2,
        Shot::new(Vec3::new(-58., 3., 47.), Vec3::new(-16., 8., 2.), 56.),
    ),
    (
        7.6,
        Shot::new(Vec3::new(-51., 9., 49.), Vec3::new(-8., 6.5, 1.), 50.),
    ),
    (9.8, HERO),
];

pub const COLD_OPEN_SECONDS: f32 = 9.8;

/// Seconds of no input on the title card before the attract tour takes over.
pub const ATTRACT_AFTER_SECONDS: f32 = 20.0;

/// Seconds the camera takes to fly from wherever the menu left it to
/// [`BROADCAST`].
pub const DROP_IN_SECONDS: f32 = 1.6;

fn cold_open_shot(elapsed: f32) -> Shot {
    let last = COLD_OPEN.len() - 1;
    for i in 0..last {
        let (start, from) = COLD_OPEN[i];
        let (end, to) = COLD_OPEN[i + 1];
        if elapsed < end {
            return from.mix(to, ease((elapsed - start) / (end - start)));
        }
    }
    COLD_OPEN[last].1
}

/// Slow orbit of the bowl, high and wide enough to show the archipelago and the
/// ranges behind it.
fn attract_shot(elapsed: f32) -> Shot {
    let angle = elapsed * 0.075;
    let radius = 88. + (elapsed * 0.21).sin() * 10.;
    Shot::new(
        Vec3::new(
            angle.sin() * radius,
            34. + (elapsed * 0.13).sin() * 9.,
            angle.cos() * radius,
        ),
        Vec3::new(0., 7., 0.),
        48.,
    )
}

/// Holds the shot actually on screen, which chases the shot the current state
/// asks for. Every state change is therefore a move rather than a cut.
#[derive(Resource)]
pub struct Director {
    pub current: Shot,
    /// Seconds in the current state, reset on every transition.
    pub elapsed: f32,
    /// Seconds since the player last touched anything.
    pub idle: f32,
    /// Where the drop-in started, so it can ease from there to [`BROADCAST`].
    pub drop_in_from: Shot,
}

impl Default for Director {
    fn default() -> Self {
        Self {
            current: COLD_OPEN[0].1,
            elapsed: 0.,
            idle: 0.,
            drop_in_from: HERO,
        }
    }
}

impl Director {
    pub fn attracting(&self) -> bool {
        self.idle >= ATTRACT_AFTER_SECONDS
    }
}

pub fn reset_shot_clock(mut director: ResMut<Director>) {
    director.elapsed = 0.;
}

/// Remembers where the camera was when the drop-in began. Without this the
/// flight would start from whichever menu anchor was live and jump.
pub fn begin_drop_in(mut director: ResMut<Director>) {
    director.drop_in_from = director.current;
}

/// The shot the front end wants this frame.
fn desired(state: AppState, director: &Director, menu_shot: Shot) -> Shot {
    match state {
        AppState::ColdOpen => cold_open_shot(director.elapsed),
        AppState::Title | AppState::Menu => {
            if director.attracting() {
                attract_shot(director.idle - ATTRACT_AFTER_SECONDS)
            } else if state == AppState::Title {
                // A slow drift keeps the title card from looking like a
                // screenshot without pulling attention off the logo.
                let t = director.elapsed;
                Shot::new(
                    HERO.eye + Vec3::new((t * 0.11).sin() * 2.4, (t * 0.09).cos() * 1.1, 0.),
                    HERO.look,
                    HERO.fov_degrees,
                )
            } else {
                menu_shot
            }
        }
        AppState::DropIn => director
            .drop_in_from
            .mix(BROADCAST, ease(director.elapsed / DROP_IN_SECONDS)),
        AppState::Playing => BROADCAST,
    }
}

/// Drives the camera in every state except [`AppState::Playing`].
///
/// The cold open and the drop-in play back at their authored speed. The title,
/// menu and attract shots are chased with exponential damping instead, so
/// changing menu item glides. The damping is frame-rate independent, which
/// matters because this scene swings between 30 and 60 fps depending on where
/// the camera is pointing.
pub fn drive_camera(
    time: Res<Time>,
    state: Res<State<AppState>>,
    menu: Res<super::ui::Menu>,
    mut director: ResMut<Director>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let state = *state.get();
    // Clamped, because the first seconds of a run are not smooth: shader
    // pipelines compile, the scenery merge runs, and any one of those frames
    // can report a multi-second delta. Advancing the shot clock by that raw
    // value skips the whole opening on a cold start, which is exactly when a
    // player is most likely to be watching it.
    let step = time.delta_seconds().min(1. / 20.);
    director.elapsed += step;
    director.idle += step;

    let target = desired(state, &director, menu.selected().shot());
    director.current = match state {
        // Authored timing: follow the path exactly.
        AppState::ColdOpen | AppState::DropIn => target,
        _ => {
            let alpha = 1. - (-2.2 * time.delta_seconds()).exp();
            director.current.mix(target, alpha)
        }
    };

    for (mut transform, mut projection) in &mut cameras {
        *transform = director.current.transform();
        if let Projection::Perspective(perspective) = projection.as_mut() {
            perspective.fov = director.current.fov_degrees.to_radians();
        }
    }
}

/// Hands the camera over at exactly the pose the match camera computes for a
/// centred ball, so the first frame of play does not jump.
pub fn hand_over_to_match(mut cameras: Query<(&mut Transform, &mut Projection), With<MainCamera>>) {
    for (mut transform, mut projection) in &mut cameras {
        *transform = BROADCAST.transform();
        if let Projection::Perspective(perspective) = projection.as_mut() {
            perspective.fov = BROADCAST.fov_degrees.to_radians();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_open_starts_below_the_pitch_and_settles_on_the_title_shot() {
        let opening = cold_open_shot(0.);
        assert!(
            opening.eye.y < 0.,
            "the flight should start under the stadium lip, got {}",
            opening.eye.y
        );
        assert_eq!(cold_open_shot(COLD_OPEN_SECONDS), HERO);
        assert_eq!(cold_open_shot(COLD_OPEN_SECONDS + 5.), HERO);
    }

    #[test]
    fn cold_open_never_jumps_between_keyframes() {
        // A discontinuity here reads as a hard cut in the middle of the flight.
        let step = 1. / 60.;
        let mut previous = cold_open_shot(0.);
        let mut t = step;
        while t <= COLD_OPEN_SECONDS {
            let shot = cold_open_shot(t);
            let jump = shot.eye.distance(previous.eye);
            assert!(jump < 2.5, "jump of {jump} at t={t}");
            previous = shot;
            t += step;
        }
    }

    #[test]
    fn drop_in_lands_exactly_on_the_broadcast_shot() {
        let director = Director {
            drop_in_from: HERO,
            elapsed: DROP_IN_SECONDS,
            ..Default::default()
        };
        let menu = super::super::ui::Menu::default();
        assert_eq!(
            desired(AppState::DropIn, &director, menu.selected().shot()),
            BROADCAST
        );
    }

    #[test]
    fn attract_tour_stays_outside_the_stands_and_above_the_water() {
        for i in 0..2000 {
            let shot = attract_shot(i as f32 * 0.3);
            let ground_distance = shot.eye.xz().length();
            assert!(
                ground_distance > 60.,
                "orbit clipped the stands: {ground_distance}"
            );
            assert!(shot.eye.y > 12., "orbit dipped into the valley: {}", shot.eye.y);
        }
    }
}
