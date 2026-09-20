//! The tactics table: coaching as a place on the same island as the easel.
//!
//! A carved stone table stands a few paces from the painter's easel, with the
//! pitch laid into its top, ten tokens and a ball on it, and a standing timber
//! sign beside it. The coach drags tokens, draws arrows and talks; everything
//! lands on one timestamped log, exactly as the 2D coaching screen does. When
//! they are done the camera leaves the island and flies to the match.
//!
//! What this module owns is the *table*: the board, the event log, and the
//! sign that reports them. What it deliberately does not own is speech and
//! interpretation — those need a microphone and a network service, which live
//! outside this crate. Both arrive through [`Transcript`] and [`Interpretation`],
//! which a host fills in; the table works without either, and says so.

pub mod board;
pub mod scene;
pub mod sign;
#[cfg(feature = "speech")]
pub mod speech;

use crate::creation::CreationPhase;
use bevy::prelude::*;

/// Where the table stands, in the clearing's frame: a few paces past the easel,
/// so the coach turns from the painting to the board without leaving the island.
pub const TABLE_LOCAL: Vec3 = Vec3::new(0.0, 0., -7.0);
/// Height of the plinth's shelf above the island's turf.
pub const TABLE_TOP: f32 = 1.06;
/// The board's face, in world units. Portrait, like a coach's board: the two
/// goals are at the top and the bottom, not left and right.
pub const BOARD_W: f32 = 3.1;
pub const BOARD_H: f32 = 4.0;
/// How far the board leans back off vertical, as a board on a stand does.
pub const BOARD_LEAN: f32 = 0.22;
/// How far a token stands off the face. Enough to read as a magnet on a board.
pub const TOKEN_PROUD: f32 = 0.07;
/// Where the sign stands, angled back toward the coach.
pub const SIGN_LOCAL: Vec3 = Vec3::new(4.0, 0., -8.8);
pub const SIGN_YAW: f32 = -0.36;
/// The sign's face, in world units.
pub const SIGN_W: f32 = 3.4;
pub const SIGN_H: f32 = 4.0;
/// Height of the face's underside above the turf.
pub const SIGN_FOOT: f32 = 0.7;

/// A point on the board, normalised 0..1 across its width and depth. The same
/// convention the tactical JSON uses, so nothing has to be converted later.
pub type Point = Vec2;

/// Who moved: one of the ten fixed players, or the ball.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityRef {
    Player(u8),
    Ball,
}

impl EntityRef {
    pub fn label(self) -> String {
        match self {
            Self::Player(id) => id.to_string(),
            Self::Ball => "ball".to_owned(),
        }
    }
}

/// One thing that happened, on one clock. Mirrors the 2D screen's event log:
/// raw record only, with no tactical meaning attached until interpretation.
#[derive(Debug, Clone)]
pub enum RawEvent {
    RecordingStarted { at_ms: u64 },
    RecordingStopped { at_ms: u64 },
    EntityMoved { entity: EntityRef, from: Point, to: Point, started_at_ms: u64, at_ms: u64 },
    /// `arrow` distinguishes the Arrow tool's stroke from the Pen's. The board knew which
    /// it had drawn but was not recording it, so a plain line and an arrow were
    /// indistinguishable in the log -- and the sign called both of them arrows.
    AnnotationAdded { id: u32, points: Vec<Point>, arrow: bool, at_ms: u64 },
    AnnotationRemoved { id: u32, at_ms: u64 },
    TranscriptAdded { id: u32, text: String, at_ms: u64 },
}

impl RawEvent {
    pub fn at_ms(&self) -> u64 {
        match *self {
            Self::RecordingStarted { at_ms }
            | Self::RecordingStopped { at_ms }
            | Self::EntityMoved { at_ms, .. }
            | Self::AnnotationAdded { at_ms, .. }
            | Self::AnnotationRemoved { at_ms, .. }
            | Self::TranscriptAdded { at_ms, .. } => at_ms,
        }
    }

    /// One line for the sign's timeline, in the coach's words rather than the
    /// log's: "moved 7", not "EntityMoved".
    pub fn summary(&self) -> String {
        match self {
            Self::RecordingStarted { .. } => "started recording".to_owned(),
            Self::RecordingStopped { .. } => "stopped".to_owned(),
            Self::EntityMoved { entity, .. } => format!("moved {}", entity.label()),
            Self::AnnotationAdded { arrow, .. } => {
                if *arrow { "drew an arrow".to_owned() } else { "drew a line".to_owned() }
            }
            Self::AnnotationRemoved { .. } => "removed an arrow".to_owned(),
            Self::TranscriptAdded { text, .. } => format!("said \u{201c}{text}\u{201d}"),
        }
    }
}

/// The live session: the clock, the log, and whether the clock is running.
#[derive(Resource, Default)]
pub struct Session {
    pub events: Vec<RawEvent>,
    pub elapsed_ms: u64,
    next_id: u32,
}

impl Session {
    pub fn append(&mut self, event: RawEvent) {
        self.events.push(event);
    }

    pub fn next_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Movements and annotations only — what the coach actually did, as opposed
    /// to the start and stop of the clock around it.
    pub fn action_count(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, RawEvent::EntityMoved { .. } | RawEvent::AnnotationAdded { .. }))
            .count()
    }

    pub fn transcript_count(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, RawEvent::TranscriptAdded { .. }))
            .count()
    }
}

/// Live speech, filled in by whatever is listening.
///
/// This crate has no microphone and no network. A host that does — the combined
/// coaching app, which already streams audio to a transcription service — pushes
/// finished sentences here and they land on the session clock like any other
/// event. Empty means nothing is listening, and the sign says so rather than
/// pretending the feature is present.
#[derive(Resource, Default)]
pub struct Transcript {
    /// Sentences not yet folded into the log.
    pub pending: Vec<String>,
    /// The partial sentence currently being spoken, if any.
    pub partial: String,
    /// True once something is listening, whether or not words have arrived.
    pub live: bool,
    /// A line for the sign about the microphone itself.
    pub status: String,
}

/// The result of interpreting the session, filled in by whatever can do it.
///
/// Same reasoning as [`Transcript`]: turning a session into tactical JSON is a
/// model call behind a local service, so the table asks for it and displays
/// whatever comes back.
#[derive(Resource, Default)]
pub struct Interpretation {
    pub requested: bool,
    pub summary: Option<String>,
    pub tactic: Option<String>,
    pub error: Option<String>,
}

/// Where the replay has got to, and whether it is running.
#[derive(Resource, Default)]
pub struct Playhead {
    pub at_ms: u64,
    pub running: bool,
}

/// Fired when the board is cleared, so the scene can put the tokens back.
#[derive(Event, Debug, Clone, Copy)]
pub struct ResetBoard;

/// Fired when the coach asks for tactical JSON. A host listening for this does
/// the work and writes the answer into [`Interpretation`].
#[derive(Event, Debug, Clone, Copy)]
pub struct RequestInterpretation;

/// What the table is doing.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TableState {
    /// Arranging the board before the clock starts.
    #[default]
    Setup,
    /// The clock is running and everything is being logged.
    Recording,
    /// Stopped, with the session ready to interpret or to take to the match.
    Stopped,
}

/// Which tool the coach's hand is holding. The same four the 2D screen offers.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Tool {
    /// Drag tokens and the ball.
    #[default]
    Move,
    /// Drag to draw an arrow, with a head at the end.
    Arrow,
    /// Drag to draw a plain line.
    Pen,
    /// Click a mark to take it off.
    Erase,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Arrow => "arrow",
            Self::Pen => "draw",
            Self::Erase => "erase",
        }
    }
}

pub struct TacticsPlugin;

impl Plugin for TacticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<TableState>()
            .init_resource::<Session>()
            .init_resource::<Transcript>()
            .init_resource::<Interpretation>()
            .init_resource::<Tool>()
            .init_resource::<Playhead>()
            .add_event::<RequestInterpretation>()
            .add_event::<ResetBoard>()
            .add_systems(
                Startup,
                // Chained: the sign camera writes onto a face the table has
                // to have stood up first.
                (scene::build_table, sign::build_sign)
                    .chain()
                    .after(crate::jungle::build_jungle),
            )
            .add_systems(
                Update,
                (
                    tick,
                    controls,
                    drain_transcript,
                    board::drag,
                    board::draw_mark,
                    erase,
                    playback,
                    restore_board,
                    sign::update_sign,
                )
                    .run_if(in_state(CreationPhase::Coaching)),
            );
        // The microphone follows the clock: it opens when recording starts and
        // closes when it stops, so speech covers exactly the recorded stretch.
        //
        // Deliberately *not* gated on `CreationPhase::Coaching`. Stopping the clock only asks
        // the worker for the last sentence; the sentence and the status that clears
        // `Finalizing` arrive over a channel some frames later. Gating the drain on being at
        // the table meant that leaving it -- which is exactly what a coach does after
        // speaking -- stopped `receive_speech` before those replies landed, so the transcript
        // sat on "Finishing the last sentence..." forever and the closing words were dropped.
        // Both systems are cheap no-ops when the microphone is closed.
        #[cfg(feature = "speech")]
        speech::register(app);
    }
}

/// The session clock. Runs only while recording, so the timestamps on the log
/// are time spent coaching rather than time spent sitting on the screen.
fn tick(time: Res<Time>, mut session: ResMut<Session>, state: Res<State<TableState>>) {
    if *state.get() == TableState::Recording {
        session.elapsed_ms += (time.delta_seconds() * 1000.0) as u64;
    }
}

/// Keys at the table. Deliberately few, and all of them named on the sign.
#[allow(clippy::too_many_arguments)]
fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<TableState>>,
    mut next_state: ResMut<NextState<TableState>>,
    mut session: ResMut<Session>,
    mut tool: ResMut<Tool>,
    mut interpretation: ResMut<Interpretation>,
    mut request: EventWriter<RequestInterpretation>,
    mut reset: EventWriter<ResetBoard>,
    mut playhead: ResMut<Playhead>,
    mut phase: ResMut<NextState<CreationPhase>>,
    mut c: Commands,
    cameras: Query<&Transform, With<crate::creation::camera::CreationCamera>>,
) {
    for (key, picked) in [
        (KeyCode::KeyM, Tool::Move),
        (KeyCode::KeyA, Tool::Arrow),
        (KeyCode::KeyD, Tool::Pen),
        (KeyCode::KeyE, Tool::Erase),
    ] {
        if keys.just_pressed(key) {
            *tool = picked;
        }
    }

    if keys.just_pressed(KeyCode::KeyR) {
        match state.get() {
            TableState::Recording => {
                let at_ms = session.elapsed_ms;
                session.append(RawEvent::RecordingStopped { at_ms });
                next_state.set(TableState::Stopped);
            }
            _ => {
                let at_ms = session.elapsed_ms;
                session.append(RawEvent::RecordingStarted { at_ms });
                next_state.set(TableState::Recording);
            }
        }
    }

    // Undo lifts the last mark, the only annotation the table draws.
    if (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
        && keys.just_pressed(KeyCode::KeyZ)
    {
        let last = session.events.iter().rev().find_map(|e| match e {
            RawEvent::AnnotationAdded { id, .. } => Some(*id),
            _ => None,
        });
        let already: Vec<u32> = session
            .events
            .iter()
            .filter_map(|e| match e {
                RawEvent::AnnotationRemoved { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        if let Some(id) = last.filter(|id| !already.contains(id)) {
            let at_ms = session.elapsed_ms;
            session.append(RawEvent::AnnotationRemoved { id, at_ms });
        }
    }

    // Backspace clears the session and puts the board back as it was found.
    if keys.just_pressed(KeyCode::Backspace) {
        *session = Session::default();
        *interpretation = Interpretation::default();
        next_state.set(TableState::Setup);
        reset.send(ResetBoard);
    }

    // Space replays what has been recorded so far.
    if keys.just_pressed(KeyCode::Space) && !session.events.is_empty() {
        playhead.running = !playhead.running;
        if playhead.at_ms >= session.elapsed_ms {
            playhead.at_ms = 0;
        }
    }

    if keys.just_pressed(KeyCode::KeyG) && session.action_count() > 0 {
        interpretation.requested = true;
        interpretation.error = None;
        request.send(RequestInterpretation);
    }

    // Enter leaves the island for the match, the same key that moved the
    // painting flow along.
    if keys.just_pressed(KeyCode::Enter) && *state.get() != TableState::Recording {
        let Ok(from) = cameras.get_single() else {
            return;
        };
        c.insert_resource(crate::creation::camera::Flight {
            elapsed: 0.,
            duration: crate::creation::FLIGHT_SECONDS,
            from: *from,
            to: crate::creation::camera::broadcast_view(),
        });
        phase.set(CreationPhase::Departing);
    }
}

/// Erases a mark the coach clicks on, when the eraser is in hand.
pub fn erase(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<crate::creation::camera::CreationCamera>>,
    plane: Option<Res<board::BoardPlane>>,
    marks: Res<board::Marks>,
    buttons: Res<ButtonInput<MouseButton>>,
    tool: Res<Tool>,
    mut session: ResMut<Session>,
) {
    if *tool != Tool::Erase || !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let (Some(plane), Ok(window), Ok((camera, camera_tf))) =
        (plane, windows.get_single(), cameras.get_single())
    else {
        return;
    };
    let Some(at) = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world(camera_tf, cursor))
        .and_then(|ray| plane.hit(ray.origin, *ray.direction))
    else {
        return;
    };
    // Nearest mark to the click, by its closest point.
    let hit = marks
        .committed
        .iter()
        .filter_map(|(id, points, _)| {
            let near = points.iter().map(|p| (*p - at).length()).fold(f32::MAX, f32::min);
            (near < 0.05).then_some((near, *id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, id)) = hit {
        let at_ms = session.elapsed_ms;
        session.append(RawEvent::AnnotationRemoved { id, at_ms });
    }
}

/// Runs the playhead, and puts the tokens where the log says they were.
pub fn playback(
    time: Res<Time>,
    mut playhead: ResMut<Playhead>,
    session: Res<Session>,
    plane: Option<Res<board::BoardPlane>>,
    mut tokens: Query<(&mut board::Token, &mut Transform)>,
) {
    if !playhead.running {
        return;
    }
    let Some(plane) = plane else {
        return;
    };
    playhead.at_ms += (time.delta_seconds() * 1000.0) as u64;
    if playhead.at_ms >= session.elapsed_ms {
        playhead.at_ms = session.elapsed_ms;
        playhead.running = false;
    }
    // Fold the log up to the playhead. Cheap enough at these lengths, and it
    // means playback reads the same record the payload will.
    for (mut token, mut transform) in &mut tokens {
        let mut at = None;
        for event in &session.events {
            if event.at_ms() > playhead.at_ms {
                break;
            }
            if let RawEvent::EntityMoved { entity, to, .. } = event {
                if *entity == token.entity {
                    at = Some(*to);
                }
            }
        }
        if let Some(at) = at {
            token.at = at;
            transform.translation = plane.world(at) + plane.normal() * TOKEN_PROUD;
        }
    }
}

/// Folds finished sentences from the speech seam onto the session clock.
fn drain_transcript(mut transcript: ResMut<Transcript>, mut session: ResMut<Session>) {
    if transcript.pending.is_empty() {
        return;
    }
    for text in std::mem::take(&mut transcript.pending) {
        let id = session.next_id();
        let at_ms = session.elapsed_ms;
        session.append(RawEvent::TranscriptAdded { id, text, at_ms });
    }
}

/// Puts the tokens back where they started after a reset.
pub fn restore_board(
    mut resets: EventReader<ResetBoard>,
    plane: Option<Res<board::BoardPlane>>,
    surfaces: Query<&Handle<StandardMaterial>, With<scene::BoardSurface>>,
    mats: Res<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut marks: ResMut<board::Marks>,
    mut tokens: Query<(&mut board::Token, &mut Transform)>,
) {
    if resets.read().next().is_none() {
        return;
    }
    let Some(plane) = plane else {
        return;
    };
    for (mut token, mut transform) in &mut tokens {
        if let Some(home) = scene::start_position(token.entity) {
            token.at = home;
            transform.translation = plane.world(home) + plane.normal() * TOKEN_PROUD;
        }
    }
    marks.committed.clear();
    marks.drawing.clear();
    if let Some(image) = surfaces
        .get_single()
        .ok()
        .and_then(|h| mats.get(h))
        .and_then(|m| m.base_color_texture.clone())
        .and_then(|t| images.get_mut(&t))
    {
        board::repaint(image, &marks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_and_the_sign_stand_on_the_island() {
        use crate::creation::{ISLAND_RX, ISLAND_RZ};
        let room = ISLAND_RX.min(ISLAND_RZ);
        assert!(TABLE_LOCAL.length() + BOARD_W * 0.5 < room, "the table fits");
        assert!(SIGN_LOCAL.length() < room, "the sign fits");
    }

    #[test]
    fn the_table_does_not_stand_on_the_easel() {
        use crate::creation::EASEL_LOCAL;
        let gap = (TABLE_LOCAL - EASEL_LOCAL).length();
        assert!(gap > 4.0, "the table is a few paces from the easel, not on it");
    }

    #[test]
    fn the_board_stands_taller_than_it_is_wide() {
        assert!(BOARD_H > BOARD_W, "a coach's board is portrait");
    }

    #[test]
    fn the_log_keeps_actions_and_clock_events_apart() {
        let mut session = Session::default();
        session.append(RawEvent::RecordingStarted { at_ms: 0 });
        session.append(RawEvent::EntityMoved {
            entity: EntityRef::Player(7),
            from: Point::new(0.1, 0.1),
            to: Point::new(0.5, 0.5),
            started_at_ms: 10,
            at_ms: 400,
        });
        session.append(RawEvent::TranscriptAdded { id: 1, text: "push up".into(), at_ms: 500 });
        assert_eq!(session.action_count(), 1, "the clock's own events are not actions");
        assert_eq!(session.transcript_count(), 1);
    }

    #[test]
    fn events_read_back_in_the_coachs_words() {
        let moved = RawEvent::EntityMoved {
            entity: EntityRef::Player(3),
            from: Point::ZERO,
            to: Point::ONE,
            started_at_ms: 0,
            at_ms: 1,
        };
        assert_eq!(moved.summary(), "moved 3");
        assert_eq!(EntityRef::Ball.label(), "ball");
    }

    #[test]
    fn every_tool_names_itself_for_the_sign() {
        for tool in [Tool::Move, Tool::Arrow, Tool::Pen, Tool::Erase] {
            assert!(!tool.label().is_empty(), "{tool:?}");
        }
    }

    #[test]
    fn spoken_sentences_land_on_the_session_clock() {
        let mut app = App::new();
        app.init_resource::<Session>()
            .init_resource::<Transcript>()
            .add_systems(Update, drain_transcript);
        app.world.resource_mut::<Session>().elapsed_ms = 4_200;
        app.world.resource_mut::<Transcript>().pending.push("press high".to_owned());
        app.update();

        let session = app.world.resource::<Session>();
        assert_eq!(session.transcript_count(), 1);
        match session.events.first().expect("one event") {
            RawEvent::TranscriptAdded { text, at_ms, .. } => {
                assert_eq!(text, "press high");
                assert_eq!(*at_ms, 4_200, "timestamped against the coaching clock");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(app.world.resource::<Transcript>().pending.is_empty(), "drained once");
    }
}
