//! "The Sitting" — the character-painting clearing, as a place in the world.
//!
//! The player stands on a spur peak beside the stadium with an easel in front
//! of them and a monkey holding a pose on a stone nearby. The flow is the one
//! the 2D player-creation screen established, step for step: paint the player's
//! appearance, paint their superpower, review both, continue. Each painting is
//! saved as it is finished. When the player continues, the camera flies down to
//! the pitch. There is no menu and no moment where the world stops existing —
//! the clearing is built at startup and stays standing afterwards.
//!
//! The model does not change while you paint. He is the reference, not a
//! preview: turning a painting into a playable monkey happens downstream, and
//! this module deliberately knows nothing about it.

pub mod camera;
pub mod hud;
pub mod paint;
pub mod persistence;
pub mod scene;

use bevy::prelude::*;
use paint::Slot;

/// Where the spur stands, relative to the pitch at the origin.
///
/// Off the far right corner of the summit: close enough that the stadium is a
/// clear presence, far enough that the clearing feels like its own place.
/// Chosen to sit inside the match camera's frame, so the easel remains visible
/// on the mountainside during play.
pub const PEAK: Vec3 = Vec3::new(40.0, 0.0, -34.0);
/// Height of the flat cap above the pitch plane.
pub const PEAK_TOP: f32 = 17.6;
/// Where the easel's feet sit on the cap.
pub const EASEL_ANCHOR: Vec3 = Vec3::new(PEAK.x + 0.4, PEAK_TOP + 0.55, PEAK.z + 1.4);

/// How long the flight from the easel to the pitch takes.
const FLIGHT_SECONDS: f32 = 4.2;

/// The steps of creation, in the order the 2D screen defined them.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CreationPhase {
    #[default]
    PaintingAppearance,
    PaintingSuperpower,
    /// Both paintings side by side, with the chance to go back to either.
    Review,
    /// The camera is flying to the pitch.
    Departing,
    /// Arrived. The host app takes over from here.
    Finished,
}

impl CreationPhase {
    /// The painting being worked on, if this is a painting step.
    pub fn slot(self) -> Option<Slot> {
        match self {
            Self::PaintingAppearance => Some(Slot::Appearance),
            Self::PaintingSuperpower => Some(Slot::Superpower),
            _ => None,
        }
    }

    /// What the easel shows. In review it keeps the superpower, while the
    /// appearance hangs on the second board beside it.
    pub fn easel_slot(self) -> Option<Slot> {
        match self {
            Self::PaintingAppearance => Some(Slot::Appearance),
            _ => Some(Slot::Superpower),
        }
    }
}

/// A short message for the player: a refusal, or a confirmation of a save.
#[derive(Resource, Default)]
pub struct Notice {
    pub text: String,
    pub is_error: bool,
    seconds_left: f32,
}

impl Notice {
    pub fn say(&mut self, text: impl Into<String>) {
        *self = Self { text: text.into(), is_error: false, seconds_left: 3.5 };
    }

    pub fn refuse(&mut self, text: impl Into<String>) {
        *self = Self { text: text.into(), is_error: true, seconds_left: 6.0 };
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.seconds_left = 0.;
    }
}

/// Handles to the scene, so other passes need no name lookups.
#[derive(Resource)]
pub struct CreationScene {
    pub canvas: Entity,
}

/// Fired once the flight has landed on the match framing. Carries what the 2D
/// screen's `ContinueToCoaching` carries, so a host can treat them alike.
#[derive(Event, Debug, Clone)]
pub struct CreationComplete {
    pub session_id: String,
    pub manifest_path: Option<std::path::PathBuf>,
}

/// Adds the clearing, the painting controls, saving, review and the flight.
///
/// The host decides what "finished" means: this plugin only reports that the
/// camera has arrived, via [`CreationComplete`].
pub struct CreationPlugin;

impl Plugin for CreationPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<CreationPhase>()
            .init_resource::<paint::Brush>()
            .init_resource::<paint::StrokeState>()
            .init_resource::<persistence::CreationSession>()
            .init_resource::<Notice>()
            .add_event::<CreationComplete>()
            .add_systems(
                Startup,
                (
                    // Ordered after the jungle, which blanks every mesh that
                    // already exists when it runs. Building second keeps the
                    // clearing out of that pass whatever the scheduler decides.
                    scene::build_clearing.after(crate::jungle::build_jungle),
                    camera::spawn_camera,
                    hud::spawn,
                ),
            )
            .add_systems(
                Update,
                (
                    (paint::tools, paint::paint).chain(),
                    advance,
                    paint::show_active_sheet,
                    scene::show_review_board,
                    camera::glide,
                    breathe,
                    fade_notice,
                    hud::update,
                )
                    .run_if(not(in_state(CreationPhase::Departing)))
                    .run_if(not(in_state(CreationPhase::Finished))),
            )
            .add_systems(Update, camera::fly.run_if(in_state(CreationPhase::Departing)))
            .add_systems(OnEnter(CreationPhase::Departing), hud::hide)
            .add_systems(OnEnter(CreationPhase::Finished), announce);
    }
}

/// Enter moves the flow forward, saving on the way — the same key, and the same
/// rule, as the 2D screen: nothing advances unless the write succeeded.
#[allow(clippy::too_many_arguments)]
fn advance(
    keys: Res<ButtonInput<KeyCode>>,
    phase: Res<State<CreationPhase>>,
    mut next: ResMut<NextState<CreationPhase>>,
    paintings: Option<Res<paint::Paintings>>,
    mut session: ResMut<persistence::CreationSession>,
    mut notice: ResMut<Notice>,
    mut c: Commands,
    cameras: Query<&Transform, With<camera::CreationCamera>>,
) {
    let Some(paintings) = paintings else {
        return;
    };
    let current = *phase.get();

    // In review, 1 and 2 go back to a painting, like the 2D "Edit" buttons.
    if current == CreationPhase::Review {
        if keys.just_pressed(KeyCode::Digit1) {
            next.set(CreationPhase::PaintingAppearance);
        } else if keys.just_pressed(KeyCode::Digit2) {
            next.set(CreationPhase::PaintingSuperpower);
        }
    }
    if !keys.just_pressed(KeyCode::Enter) {
        return;
    }

    match current {
        CreationPhase::PaintingAppearance | CreationPhase::PaintingSuperpower => {
            let slot = current.slot().expect("painting phases have a slot");
            if paintings.sheet(slot).is_empty() {
                notice.refuse(format!("Paint the {} before saving.", slot.slug()));
                return;
            }
            match session.save(slot, &paintings) {
                Ok(_) => {
                    notice.say(format!("Saved the {}.", slot.slug()));
                    next.set(match current {
                        CreationPhase::PaintingAppearance => CreationPhase::PaintingSuperpower,
                        _ => CreationPhase::Review,
                    });
                }
                Err(error) => notice.refuse(format!(
                    "Could not save the {}: {error}. Check the output folder is writable, then press Enter again.",
                    slot.slug()
                )),
            }
        }
        CreationPhase::Review => match session.write_manifest(&paintings) {
            Ok(_) => {
                let Ok(from) = cameras.get_single() else {
                    return;
                };
                c.insert_resource(camera::Flight {
                    elapsed: 0.,
                    duration: FLIGHT_SECONDS,
                    from: *from,
                    to: camera::broadcast_view(),
                });
                next.set(CreationPhase::Departing);
            }
            Err(error) => notice.refuse(format!("Could not finalize the session: {error}")),
        },
        _ => {}
    }
}

/// The model's held pose, drifting very slightly.
///
/// Someone posing is still breathing and shifting their weight. Without this he
/// reads as a statue. For the superpower sitting he squares up to the painter
/// and bounces on his feet: a different pose for a different painting, and a
/// cue, without any text, that the subject has changed.
fn breathe(
    time: Res<Time>,
    phase: Res<State<CreationPhase>>,
    mut models: Query<(&mut Transform, &scene::PosingModel)>,
) {
    let t = time.elapsed_seconds();
    let powered = *phase.get() == CreationPhase::PaintingSuperpower;
    for (mut transform, model) in &mut models {
        let rise = if powered {
            (t * 5.2).sin().abs() * 0.16
        } else {
            (t * 1.05).sin() * 0.022 + (t * 0.37).sin() * 0.014
        };
        let sway = (t * 0.51).sin() * 0.012;
        let yaw = if powered { 0.12 } else { scene::MODEL_YAW };
        let target = Quat::from_rotation_y(yaw + (t * 0.43).sin() * 0.018);
        transform.translation = model.rest + Vec3::new(sway, rise, 0.);
        transform.rotation = transform.rotation.slerp(target, 0.08);
    }
}

fn fade_notice(time: Res<Time>, mut notice: ResMut<Notice>) {
    if notice.seconds_left > 0. {
        notice.seconds_left -= time.delta_seconds();
        if notice.seconds_left <= 0. {
            notice.clear();
        }
    }
}

fn announce(
    mut done: EventWriter<CreationComplete>,
    session: Res<persistence::CreationSession>,
) {
    done.send(CreationComplete {
        session_id: session.id.clone(),
        manifest_path: Some(session.directory().join(persistence::MANIFEST_FILE_NAME)),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_peak_stands_clear_of_the_pitch() {
        use crate::game::config::{FIELD_DEPTH, FIELD_WIDTH};
        assert!(PEAK.x.abs() > FIELD_WIDTH / 2., "the spur must not sit on the playing surface");
        assert!(PEAK.z.abs() > FIELD_DEPTH / 2.);
    }

    #[test]
    fn the_easel_stands_on_the_cap() {
        assert!(EASEL_ANCHOR.y > PEAK_TOP, "the easel's feet rest on top of the peak");
    }

    #[test]
    fn the_clearing_looks_down_on_the_pitch() {
        assert!(PEAK_TOP > 10.0);
    }

    #[test]
    fn creation_starts_with_the_appearance_like_the_2d_flow() {
        assert_eq!(CreationPhase::default(), CreationPhase::PaintingAppearance);
        assert_eq!(CreationPhase::default().slot(), Some(Slot::Appearance));
    }

    #[test]
    fn only_painting_steps_accept_paint() {
        assert_eq!(CreationPhase::PaintingSuperpower.slot(), Some(Slot::Superpower));
        for phase in [CreationPhase::Review, CreationPhase::Departing, CreationPhase::Finished] {
            assert_eq!(phase.slot(), None, "{phase:?} must not take strokes");
        }
    }

    #[test]
    fn review_keeps_the_superpower_on_the_easel() {
        assert_eq!(CreationPhase::Review.easel_slot(), Some(Slot::Superpower));
        assert_eq!(CreationPhase::PaintingAppearance.easel_slot(), Some(Slot::Appearance));
    }
}
