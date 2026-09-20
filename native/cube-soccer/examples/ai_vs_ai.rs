//! AI vs AI gameplay example
//!
//! Watch two AI teams play. Every cube runs the built-in team AI.
//! Press `T` to cycle Orange's tactic, `Y` to cycle Blue's.
//!
//! Run with: cargo run --release --example ai_vs_ai

use bevy::prelude::*;
use cube_soccer::entities::CubePlayer;
use cube_soccer::input::keyboard::keyboard_input_system;
use cube_soccer::systems::heuristic_ai::{AiControlled, Tactic, TeamDirective, TeamTactics};
use cube_soccer::systems::kick::{apply_kicks, tick_kick_cooldowns, KickCooldowns};
use cube_soccer::systems::movement::apply_player_movement;
use cube_soccer::systems::possession::update_possession;
use cube_soccer::systems::soccer_ai::{apply_soccer_ai, PlayMemory};
use cube_soccer::CubeSoccerPlugin;

/// Tracks the current named preset per team so we can cycle and print it.
#[derive(Resource)]
struct DemoTactic {
    orange: Tactic,
    blue: Tactic,
}
impl Default for DemoTactic {
    fn default() -> Self {
        Self { orange: Tactic::Balanced, blue: Tactic::Balanced }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Cube Soccer 3D - AI vs AI".to_string(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CubeSoccerPlugin)
        .init_resource::<TeamTactics>()
        .init_resource::<KickCooldowns>()
        .init_resource::<PlayMemory>()
        .init_resource::<DemoTactic>()
        .add_systems(PostStartup, tag_all_ai)
        // Run after the keyboard system so the AI fully controls every cube. Kicks are struck
        // after the players have moved and before possession is resolved, as in the game.
        .add_systems(Update, apply_soccer_ai.after(keyboard_input_system))
        .add_systems(
            Update,
            (tick_kick_cooldowns, apply_kicks)
                .chain()
                .after(apply_player_movement)
                .before(update_possession),
        )
        .add_systems(Update, switch_tactics)
        .run();
}

fn tag_all_ai(mut commands: Commands, query: Query<Entity, With<CubePlayer>>) {
    for entity in query.iter() {
        commands.entity(entity).insert(AiControlled);
    }
}

fn switch_tactics(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut demo: ResMut<DemoTactic>,
    mut tactics: ResMut<TeamTactics>,
) {
    if keyboard.just_pressed(KeyCode::KeyT) {
        demo.orange = demo.orange.next();
        tactics.orange = TeamDirective::uniform(demo.orange.params());
        println!("Orange tactic: {}", demo.orange.name());
    }
    if keyboard.just_pressed(KeyCode::KeyY) {
        demo.blue = demo.blue.next();
        tactics.blue = TeamDirective::uniform(demo.blue.params());
        println!("Blue tactic: {}", demo.blue.name());
    }
}
