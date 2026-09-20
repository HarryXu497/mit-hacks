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

/// The island the clearing stands on, read from the landscape's own table of
/// outcrops rather than invented here. The clearing furnishes an island the
/// world already has, so there is one definition of where that island is and
/// the stadium never grows a peak that exists only for this screen.
const OUTCROP: (f32, f32, f32, f32, f32) =
    crate::jungle::landscape::OUTCROPS[crate::jungle::landscape::CLEARING_OUTCROP];

/// Centre of the island, on the pitch's plane.
pub const PEAK: Vec3 = Vec3::new(OUTCROP.0, 0., OUTCROP.1);
/// Height of its flat grass table.
pub const PEAK_TOP: f32 = OUTCROP.2;
/// Half-extents of that table, which is how much room the clearing has.
pub const ISLAND_RX: f32 = OUTCROP.3;
pub const ISLAND_RZ: f32 = OUTCROP.4;

/// Where the easel's feet sit, in the clearing's frame.
pub const EASEL_LOCAL: Vec3 = Vec3::new(0.4, 0.55, 1.4);

/// How far the clearing's frame is turned.
///
/// The clearing is authored with -Z pointing away from the painter. Turning it
/// to this heading aims that axis at the stadium, so looking past the easel
/// means looking at the pitch, and the flight afterwards runs straight down
/// the same line instead of swinging around to find it.
pub fn clearing_yaw() -> f32 {
    PEAK.x.atan2(PEAK.z)
}

/// The clearing's rotation, for anything that must face along with it.
pub fn facing() -> Quat {
    Quat::from_rotation_y(clearing_yaw())
}

/// A point in the clearing's frame — x right, y up from the island's table,
/// z toward the painter — as a point in the world.
pub fn place(local: Vec3) -> Vec3 {
    PEAK + Vec3::Y * PEAK_TOP + facing() * local
}

/// Where the easel's feet sit, in the world.
pub fn easel_anchor() -> Vec3 {
    place(EASEL_LOCAL)
}

/// How long the flight from the island to the pitch takes. The table starts
/// it, so it is public; the duration is a property of the journey, not of who
/// happens to press the key.
pub const FLIGHT_SECONDS: f32 = 4.2;

/// The steps of creation, in the order the 2D screen defined them.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CreationPhase {
    /// Nothing is happening yet: the clearing stands, but the painter is not at the easel and no
    /// input is being read.
    ///
    /// The host decides when creation *begins*, just as it decides what "finished" means. This is
    /// the default because a host may have its own screens in front of the clearing — the combined
    /// app opens on a lobby menu — and painting systems running behind them would read strays
    /// clicks onto the canvas and draw the prompt plaque over the menu. `bin/creation.rs`, which
    /// is only the clearing, leaves this state on startup.
    #[default]
    Idle,
    PaintingAppearance,
    PaintingSuperpower,
    /// Both paintings side by side, with the chance to go back to either.
    Review,
    /// The tactics table has the floor; see the `tactics` module.
    Coaching,
    /// The camera is flying to the pitch.
    Departing,
    /// Arrived. The host app takes over from here.
    Finished,
}

impl CreationPhase {
    /// True while the painter is still at the easel, which is what the painting
    /// systems run on. The table's own phases take over afterwards.
    pub fn at_the_easel(self) -> bool {
        matches!(self, Self::PaintingAppearance | Self::PaintingSuperpower | Self::Review)
    }

    /// True before the host has started creation, when nothing should be read or drawn.
    pub fn is_idle(self) -> bool {
        matches!(self, Self::Idle)
    }

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
                    .run_if(|phase: Res<State<CreationPhase>>| phase.get().at_the_easel()),
            )
            // The camera keeps moving after the easel is finished with: it
            // crosses the island to the table, then leaves for the match.
            .add_systems(
                Update,
                (camera::glide, fade_notice).run_if(in_state(CreationPhase::Coaching)),
            )
            .add_systems(OnEnter(CreationPhase::Coaching), hud::hide)
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
                // The paintings are done; the coach turns to the table. The
                // flight to the match is the table's to start, not the easel's.
                next.set(CreationPhase::Coaching);
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
        // Turned with the clearing. Without this the pose is rebuilt in world
        // axes every frame and the model ends up facing off the island.
        let target = facing() * Quat::from_rotation_y(yaw + (t * 0.43).sin() * 0.018);
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
    fn the_clearing_stands_on_an_island_the_landscape_already_raises() {
        // The point of this: no peak exists solely to host the creation screen.
        let table = crate::jungle::landscape::OUTCROPS;
        assert!(crate::jungle::landscape::CLEARING_OUTCROP < table.len());
        let chosen = table[crate::jungle::landscape::CLEARING_OUTCROP];
        assert_eq!(PEAK.x, chosen.0);
        assert_eq!(PEAK.z, chosen.1);
        assert_eq!(PEAK_TOP, chosen.2);
    }

    #[test]
    fn the_clearing_stands_clear_of_the_pitch() {
        use crate::game::config::{FIELD_DEPTH, FIELD_WIDTH};
        assert!(PEAK.x.abs() > FIELD_WIDTH / 2., "the island is not on the playing surface");
        assert!(PEAK.z.abs() > FIELD_DEPTH / 2.);
    }

    #[test]
    fn the_clearing_faces_the_stadium() {
        // -Z in the clearing's frame must point at the pitch, or the painter
        // works with their back to the thing the flight is about to fly to.
        let forward = facing() * Vec3::NEG_Z;
        let to_pitch = (Vec3::new(-PEAK.x, 0., -PEAK.z)).normalize();
        assert!(forward.dot(to_pitch) > 0.999, "forward {forward:?} vs {to_pitch:?}");
    }

    #[test]
    fn the_easel_stands_on_the_island_not_over_its_edge() {
        assert!(easel_anchor().y > PEAK_TOP, "the easel's feet rest on the table");
        // Generous margin: the model, the shelf and the boards all sit further
        // out than the easel does, and none of them may hang off the rim.
        assert!(EASEL_LOCAL.x.abs() + 5.0 < ISLAND_RX, "room across the island");
        assert!(EASEL_LOCAL.z.abs() + 5.0 < ISLAND_RZ, "room along the island");
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
