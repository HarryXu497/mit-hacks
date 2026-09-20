pub mod board;
pub mod game;
pub mod game_handoff;
pub mod forge;
pub mod game_stream;
pub mod interpretation;
pub mod menu;
pub mod model;
pub mod network;
pub mod persistence;
pub mod phase;
pub mod replay;
pub mod session;
pub mod theme;
pub mod speech;
pub mod tactics_bridge;
pub mod ui;
pub mod world;

use bevy::prelude::*;
// Only the shared board state survives here; the drawing and input systems belong to the stone
// table in the world now. See the note on `CoachingPlugin::build`.
use board::{BoardInteraction, BoardViewport};
use interpretation::{
    invalidate_stale_result, receive_interpretation, request_interpretation, InterpretationRuntime,
    RequestInterpretation, TacticalResult,
};
use network::{MatchReady, NetworkEndpoint, NetworkRole};
use persistence::{AutosaveTracker, PersistenceStatus};
use phase::AppPhase;
use session::CoachingSession;
use speech::SpeechRuntime;
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

#[derive(Event, Debug, Clone, Copy)]
pub struct EnterGame;

impl Plugin for CoachingPlugin {
    fn build(&self, app: &mut App) {
        // Deliberately never loads native/coaching/src/persistence.rs's
        // crash-recovery file: that file lives at one fixed OS path shared
        // by every process on the machine, so two multiplayer processes
        // (host + joiner) on the same Mac would otherwise load each
        // other's in-progress session at startup. Every process starts
        // blank instead; see AGENT-README.md's "4a. Multiplayer" section.
        app.insert_resource(CoachingSession::default())
            .init_resource::<CoachingLifecycle>()
            .init_resource::<BoardViewport>()
            .init_resource::<BoardInteraction>()
            .init_resource::<CoachingUiState>()
            .init_resource::<SpeechRuntime>()
            .init_resource::<TacticalResult>()
            .init_resource::<InterpretationRuntime>()
            .init_resource::<AutosaveTracker>()
            .init_resource::<PersistenceStatus>()
            .init_resource::<NetworkRole>()
            .init_resource::<NetworkEndpoint>()
            .add_event::<RequestInterpretation>()
            .add_event::<SetCoachingActive>()
            .add_event::<EnterGame>()
            .add_event::<MatchReady>()
            .add_systems(Startup, configure_egui)
            // The board is the stone table on the clearing's island now, not a 2D scene drawn
            // over a blank window, so five systems and a second camera are gone from here:
            // `spawn_board`, `update_board_camera`, `handle_board_input`, `sync_tokens` and
            // `draw_pitch_and_annotations`. `cube_soccer::tactics` draws and drives the board in
            // the world, and `world::mirror_the_table_into_the_session` folds what it records
            // into the session below. The 2D board's code stays in `board.rs` -- it is what the
            // whole session model was designed against, and it is still the reference for the
            // coordinate convention both boards share.
            //
            // `receive_speech` is gone for the same reason: the table has its own microphone now,
            // streaming to the very same `/api/transcribe` websocket this crate's `speech.rs`
            // used. Two clients on one microphone would fight over the device.
            //
            // `tick_session` is gone because the table keeps the clock: its own tick only
            // advances while recording, which is the behaviour this had.
            .add_systems(
                Update,
                (
                    coaching_ui,
                    request_interpretation,
                    receive_interpretation,
                    invalidate_stale_result,
                )
                    .chain()
                    .run_if(coaching_is_active)
                    .run_if(in_state(AppPhase::Coaching)),
            )
            .add_systems(Update, (update_lifecycle, handle_enter_game).chain());
    }
}

fn coaching_is_active(lifecycle: Res<CoachingLifecycle>) -> bool {
    lifecycle.active
}

/// Turning coaching on and off.
///
/// This used to spawn and tear down the 2D board's entities and stop its microphone. The board is
/// now a permanent fixture of the world and the microphone belongs to it, so all that is left is
/// the flag the panel's systems run on and stopping the clock -- which the table follows, through
/// `world::the_panel_drives_the_clock`.
fn update_lifecycle(
    mut events: EventReader<SetCoachingActive>,
    mut lifecycle: ResMut<CoachingLifecycle>,
    mut session: ResMut<CoachingSession>,
) {
    for event in events.read() {
        if lifecycle.active == event.0 {
            continue;
        }
        lifecycle.active = event.0;
        if !event.0 {
            session.stop();
        }
    }
}

fn handle_enter_game(
    mut enter_game: EventReader<EnterGame>,
    mut commands: Commands,
    session: Res<CoachingSession>,
    mut result: ResMut<TacticalResult>,
    mut set_active: EventWriter<SetCoachingActive>,
    mut next_phase: ResMut<NextState<AppPhase>>,
) {
    if enter_game.read().next().is_some() {
        if result.state != interpretation::InterpretationState::Ready
            || session.session.status != model::SessionStatus::Interpreted
        {
            result.notice =
                Some("Generate a current interpretation before starting the game.".into());
            return;
        }
        let handoff = result
            .output
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing interpretation"))
            .and_then(|output| {
                // Solo only: the output must belong to this process's own live
                // session. Networked play goes through `network.rs` instead,
                // where the two sides legitimately have different session ids.
                let red = game_handoff::CoachedTeam::from_local_session(
                    output,
                    &session.session.id,
                    game_handoff::TeamSide::Red,
                )?;
                let yellow = game_handoff::CoachedTeam::balanced_default(
                    &session.session.id,
                    game_handoff::TeamSide::Yellow,
                );
                Ok::<_, anyhow::Error>(game_handoff::MatchHandoff {
                    match_id: format!("solo-{}", session.session.id),
                    red,
                    yellow,
                })
            });
        match handoff {
            Ok(handoff) => {
                commands.insert_resource(handoff.team_tactics());
                commands.insert_resource(handoff);
            }
            Err(error) => {
                result.notice = Some(format!("Cannot start game: {error:#}"));
                return;
            }
        }
        set_active.send(SetCoachingActive(false));
        next_phase.set(AppPhase::Game);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::scene::ScenePlugin;
    use bevy::time::TimeUpdateStrategy;
    use cube_soccer::entities::{Ball, CubePlayer};
    use cube_soccer::game::{GameState, GoalScoredEvent, MatchState, Team};
    use cube_soccer::systems::{AiControlled, Tactic, TeamTactics};
    use std::time::Duration;

    /// Backs up whatever is at `persistence::recovery_path()` (if anything)
    /// and restores it on drop, so this test never permanently clobbers a
    /// real in-progress session on the machine it runs on.
    struct RecoveryFileGuard {
        path: std::path::PathBuf,
        original: Option<Vec<u8>>,
    }

    impl RecoveryFileGuard {
        fn new() -> Self {
            let path = persistence::recovery_path();
            let original = std::fs::read(&path).ok();
            Self { path, original }
        }

        fn write_stale_session(&self) {
            let mut stale = model::Session {
                title: "stale session from another process".into(),
                ..Default::default()
            };
            stale.events.push(model::RawSessionEvent::RecordingStarted {
                id: "stale-event".into(),
                timestamp_ms: 0,
            });
            std::fs::create_dir_all(self.path.parent().unwrap()).unwrap();
            std::fs::write(&self.path, serde_json::to_vec(&stale).unwrap()).unwrap();
        }
    }

    impl Drop for RecoveryFileGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(bytes) => {
                    let _ = std::fs::write(&self.path, bytes);
                }
                None => {
                    let _ = std::fs::remove_file(&self.path);
                }
            }
        }
    }

    #[test]
    fn coaching_plugin_never_loads_a_pre_existing_recovery_file() {
        // Regression test for the multiplayer bug where two `native-coaching`
        // processes on the same machine (host + joiner) both read/wrote the
        // same fixed OS-level recovery file, so a fresh process would load
        // whatever the other process had already recorded. CoachingPlugin
        // must always start with a blank session, independent of anything
        // left on disk by a previous run or a different process.
        let guard = RecoveryFileGuard::new();
        guard.write_stale_session();

        let mut app = App::new();
        app.add_plugins(CoachingPlugin);

        let session = app.world.resource::<CoachingSession>();
        assert!(session.session.events.is_empty());
        assert_ne!(session.session.title, "stale session from another process");
    }

    fn handoff_app() -> App {
        let mut app = App::new();
        let mut session = CoachingSession::default();
        session.session.status = model::SessionStatus::Interpreted;
        let mut result = TacticalResult::default();
        result.state = interpretation::InterpretationState::Ready;
        result.output = Some(interpretation::deterministic_interpretation(
            &session.session,
            "deterministic-fallback",
            "red",
        ));
        app.add_plugins(MinimalPlugins)
            .init_state::<AppPhase>()
            .insert_resource(session)
            .insert_resource(result)
            .add_event::<EnterGame>()
            .add_event::<SetCoachingActive>()
            .add_systems(Update, handle_enter_game);
        app
    }

    #[test]
    fn stale_and_invalid_results_do_not_enter_game() {
        let mut app = handoff_app();
        app.world.resource_mut::<CoachingSession>().mark_edited();
        app.world.send_event(EnterGame);
        app.update();
        app.update();
        assert_eq!(
            *app.world.resource::<State<AppPhase>>().get(),
            AppPhase::Lobby
        );
        assert!(!app.world.contains_resource::<game_handoff::MatchHandoff>());
        assert!(app.world.resource::<TacticalResult>().notice.is_some());

        app.world.resource_mut::<CoachingSession>().session.status =
            model::SessionStatus::Interpreted;
        app.world
            .resource_mut::<TacticalResult>()
            .output
            .as_mut()
            .unwrap()["rlSelection"]["downstreamValue"] = serde_json::json!("invented");
        app.world.send_event(EnterGame);
        app.update();
        app.update();
        assert!(!app.world.contains_resource::<game_handoff::MatchHandoff>());
    }

    #[test]
    fn coached_game_spawns_ten_ai_players_moves_and_keeps_tactics_after_resets() {
        let mut app = handoff_app();
        app.add_plugins((
            AssetPlugin::default(),
            ScenePlugin,
            bevy::transform::TransformPlugin,
            bevy::hierarchy::HierarchyPlugin,
        ))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            1.0 / 60.0,
        )))
        // The world is built at startup by `world::WorldPlugin`, not by entering the Game phase,
        // because the easel and the tactics table stand in it before the match begins and the
        // match is re-entered at every round boundary. This test exercises `GamePlugin` alone, so
        // it spawns the part of the world the match needs -- the players and the ball -- rather
        // than pulling in the jungle and the two in-world screens, which want a renderer.
        .add_systems(
            Startup,
            (
                cube_soccer::systems::physics::configure_physics,
                cube_soccer::entities::spawn_players,
                cube_soccer::entities::spawn_ball,
            )
                .chain(),
        )
        .add_plugins(game::GamePlugin);
        {
            let mut result = app.world.resource_mut::<TacticalResult>();
            let output = result.output.as_mut().unwrap();
            output["rlSelection"]["primaryTactic"] = serde_json::json!("highpress");
            output["rlSelection"]["downstreamValue"] = serde_json::json!("highpress");
        }
        app.world.send_event(EnterGame);
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            *app.world.resource::<State<AppPhase>>().get(),
            AppPhase::Game
        );
        let players: Vec<_> = app
            .world
            .query_filtered::<(Entity, &Transform), (With<CubePlayer>, With<AiControlled>)>()
            .iter(&app.world)
            .map(|(e, t)| (e, t.translation))
            .collect();
        assert_eq!(players.len(), 10);
        assert_eq!(
            app.world
                .query_filtered::<Entity, With<Ball>>()
                .iter(&app.world)
                .count(),
            1
        );
        for _ in 0..60 {
            app.update();
        }
        assert!(players.iter().any(|(entity, start)| {
            let now = app.world.get::<Transform>(*entity).unwrap().translation;
            (Vec2::new(now.x, now.z) - Vec2::new(start.x, start.z)).length() > 0.1
        }));
        app.world.send_event(GoalScoredEvent {
            scoring_team: Team::Orange,
        });
        for _ in 0..90 {
            app.update();
        }
        assert!(app.world.resource::<GameState>().score[0] >= 1);
        assert_eq!(
            *app.world.resource::<State<MatchState>>().get(),
            MatchState::Playing
        );
        app.world
            .resource_mut::<NextState<MatchState>>()
            .set(MatchState::RoundOver);
        app.update();
        app.update();
        assert_eq!(
            app.world.resource::<TeamTactics>().orange.base_params(),
            Tactic::HighPress.params()
        );
        assert_eq!(
            app.world.resource::<TeamTactics>().blue.base_params(),
            Tactic::Balanced.params()
        );
        assert_eq!(
            app.world
                .query_filtered::<Entity, With<AiControlled>>()
                .iter(&app.world)
                .count(),
            10
        );
    }
}
