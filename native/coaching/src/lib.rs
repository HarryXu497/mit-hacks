pub mod board;
pub mod game;
pub mod game_handoff;
pub mod game_stream;
pub mod interpretation;
pub mod model;
pub mod network;
pub mod persistence;
pub mod phase;
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
use network::{MatchReady, NetworkEndpoint, NetworkRole};
use persistence::{autosave_session, load_recovery, AutosaveTracker, PersistenceStatus};
use phase::AppPhase;
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

#[derive(Event, Debug, Clone, Copy)]
pub struct EnterGame;

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
            .init_resource::<NetworkRole>()
            .init_resource::<NetworkEndpoint>()
            .add_event::<RequestInterpretation>()
            .add_event::<SetCoachingActive>()
            .add_event::<EnterGame>()
            .add_event::<MatchReady>()
            .add_systems(Startup, configure_egui)
            .add_systems(OnEnter(AppPhase::Coaching), spawn_board)
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
                    .run_if(coaching_is_active)
                    .run_if(in_state(AppPhase::Coaching)),
            )
            .add_systems(Update, (update_lifecycle, handle_enter_game).chain());
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
                let red = game_handoff::CoachedTeam::from_output_for_team(
                    output,
                    &session.session.id,
                    game_handoff::TeamSide::Red,
                )?;
                // Yellow is not yet coached over the network (Phase B); default it
                // until a real second-machine tactical output is merged in.
                let yellow = game_handoff::CoachedTeam::balanced_default(
                    &session.session.id,
                    game_handoff::TeamSide::Yellow,
                );
                Ok::<_, anyhow::Error>(game_handoff::MatchHandoff { red, yellow })
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
