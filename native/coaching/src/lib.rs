pub mod board;
pub mod interpretation;
pub mod model;
pub mod persistence;
pub mod replay;
pub mod session;
pub mod speech;
pub mod ui;

use bevy::prelude::*;
use bevy::sprite::ColorMaterial;
use board::{
    draw_pitch_and_annotations, handle_board_input, spawn_board, spawn_board_entities, sync_tokens,
    update_board_camera, BoardInteraction, BoardViewport,
};
use interpretation::{
    invalidate_stale_result, receive_interpretation, request_interpretation, InterpretationRuntime,
    RequestInterpretation, TacticalResult,
};
use persistence::{autosave_session, load_recovery, AutosaveTracker, PersistenceStatus};
use session::{tick_session, CoachingSession};
use speech::{receive_speech, SpeechRuntime};
use ui::{coaching_ui, configure_egui, CoachingUiState};

pub struct CoachingPlugin;

#[derive(Resource, Debug, Clone, Copy)]
pub struct CoachingLifecycle {
    pub active: bool,
}

impl Default for CoachingLifecycle {
    fn default() -> Self {
        Self { active: true }
    }
}

#[derive(Event, Debug, Clone, Copy)]
pub struct SetCoachingActive(pub bool);

impl Plugin for CoachingPlugin {
    fn build(&self, app: &mut App) {
        let restored = load_recovery()
            .map(CoachingSession::from_session)
            .unwrap_or_default();
        app.insert_resource(restored)
            .init_resource::<CoachingLifecycle>()
            .init_resource::<BoardViewport>()
            .init_resource::<BoardInteraction>()
            .init_resource::<CoachingUiState>()
            .init_resource::<SpeechRuntime>()
            .init_resource::<TacticalResult>()
            .init_resource::<InterpretationRuntime>()
            .init_resource::<AutosaveTracker>()
            .init_resource::<PersistenceStatus>()
            .add_event::<RequestInterpretation>()
            .add_event::<SetCoachingActive>()
            .add_systems(Startup, (configure_egui, spawn_board).chain())
            .add_systems(
                Update,
                (
                    tick_session,
                    coaching_ui,
                    update_board_camera,
                    handle_board_input,
                    sync_tokens,
                    draw_pitch_and_annotations,
                    receive_speech,
                    request_interpretation,
                    receive_interpretation,
                    invalidate_stale_result,
                    autosave_session,
                )
                    .chain()
                    .run_if(coaching_is_active),
            )
            .add_systems(Update, update_lifecycle);
    }
}

fn coaching_is_active(lifecycle: Res<CoachingLifecycle>) -> bool {
    lifecycle.active
}

fn update_lifecycle(
    mut commands: Commands,
    mut events: EventReader<SetCoachingActive>,
    mut lifecycle: ResMut<CoachingLifecycle>,
    mut session: ResMut<CoachingSession>,
    mut speech: ResMut<SpeechRuntime>,
    owned: Query<Entity, With<board::CoachingOwned>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for event in events.read() {
        if lifecycle.active == event.0 {
            continue;
        }
        lifecycle.active = event.0;
        if event.0 {
            spawn_board_entities(&mut commands, &mut meshes, &mut materials);
        } else {
            speech.stop();
            session.stop();
            for entity in &owned {
                commands.entity(entity).despawn_recursive();
            }
        }
    }
}
