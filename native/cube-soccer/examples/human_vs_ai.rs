//! Human vs AI gameplay example
//!
//! Controls:
//! - Orange player index 0 (Human): WASD + Space
//! - Superpowers (fire from the human cube, aimed by its facing / movement dir):
//!     1 = Beam blast (knock back opponents in front)
//!     2 = Freeze ray (freeze the nearest opponent in front)
//!     3 = Boost (1.5x speed/accel on self)
//!     4 = Slow (slow the nearest opponent)
//! - All Blue players and Orange teammates: built-in heuristic AI
//!
//! Run with: cargo run --release --example human_vs_ai

use bevy::prelude::*;
use cube_soccer::entities::{CubePlayer, PlayerInput};
use cube_soccer::game::Team;
use cube_soccer::input::keyboard::keyboard_input_system;
use cube_soccer::systems::heuristic_ai::{apply_heuristic_ai, AiControlled};
use cube_soccer::systems::superpowers::{activate_superpowers, Superpower, SuperpowerKind};
use cube_soccer::CubeSoccerPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Cube Soccer 3D - Human vs AI".to_string(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CubeSoccerPlugin)
        .add_systems(PostStartup, tag_ai_players)
        .add_systems(Update, apply_heuristic_ai.after(keyboard_input_system))
        // Debug: let the human fire each superpower with number keys. Must run
        // before activation so the requested fire lands the same frame.
        .add_systems(Update, debug_fire_keys.before(activate_superpowers))
        .run();
}

/// Human controls Orange index 0. Everything else (all Blue, Orange teammates)
/// is AI-controlled. The human cube also gets a `Superpower` so the number keys
/// can swap + fire it (the AI cubes have none, so they don't fire).
fn tag_ai_players(mut commands: Commands, query: Query<(Entity, &CubePlayer)>) {
    for (entity, player) in query.iter() {
        let is_human = player.team == Team::Orange && player.index == 0;
        if is_human {
            commands.entity(entity).insert(Superpower::new(SuperpowerKind::BeamBlast));
        } else {
            commands.entity(entity).insert(AiControlled);
        }
    }
}

/// Keys 1-4: set the human cube's superpower to that kind, ready it, and request
/// a fire this frame. Any other frame clears the fire request (one shot per press).
fn debug_fire_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut PlayerInput, &mut Superpower, &CubePlayer)>,
) {
    for (mut input, mut power, player) in query.iter_mut() {
        if player.team != Team::Orange || player.index != 0 {
            continue;
        }
        let kind = if keyboard.just_pressed(KeyCode::Digit1) {
            Some(SuperpowerKind::BeamBlast)
        } else if keyboard.just_pressed(KeyCode::Digit2) {
            Some(SuperpowerKind::FreezeRay)
        } else if keyboard.just_pressed(KeyCode::Digit3) {
            Some(SuperpowerKind::Boost)
        } else if keyboard.just_pressed(KeyCode::Digit4) {
            Some(SuperpowerKind::Slow)
        } else {
            None
        };

        match kind {
            Some(k) => {
                power.kind = k;
                power.cooldown_remaining = 0.0; // debug: always ready on keypress
                input.fire = true;
                println!("fire superpower: {:?}", k);
            }
            None => {
                input.fire = false;
            }
        }
    }
}
