//! Generated character skins for the players.
//!
//! Players are simulated as cubes and always will be: the collider, mass and locked axes in
//! `cube_player.rs` are what the physics and the trained policies depend on. This module only
//! changes what a player *looks* like, by parenting a generated glTF scene to that same body.
//!
//! Keeping the two apart is deliberate and is what makes this safe to land:
//!
//! * The collider stays `Collider::cuboid(CUBE_SIZE/2, ..)`. No contact behaviour changes.
//! * `OBSERVATION_SIZE` and the observation vector are untouched, so every existing PPO
//!   checkpoint stays valid.
//! * With no model file present the game falls back to the original cube, so a checkout without
//!   assets still runs.
//!
//! Models are produced by MonkeyForge from a player's drawing and exported one unit tall standing
//! on the origin, so the only scaling needed here is by `CUBE_SIZE`.

use bevy::prelude::*;

use crate::game::config::{Team, CUBE_SIZE};

/// Path to each team's character model, relative to `assets/`.
///
/// `None` means "no model for this team": the player keeps the procedural cube. That is the
/// default state of a fresh checkout, since the models are generated rather than committed
/// wholesale.
pub fn skin_path(team: Team) -> Option<&'static str> {
    match team {
        Team::Orange => Some("characters/orange.glb#Scene0"),
        Team::Blue => Some("characters/blue.glb#Scene0"),
    }
}

/// Marker for the visual model hanging off a player body.
#[derive(Component)]
pub struct CharacterSkin;

/// Attach the generated model to a player, returning whether one was attached.
///
/// The caller uses the answer to decide whether to also spawn the googly eyes: a generated monkey
/// has its own eyes painted into its texture, and adding spheres on top of them looks like a bug.
pub fn spawn_skin(parent: &mut ChildBuilder, asset_server: &AssetServer, team: Team) -> bool {
    let Some(path) = skin_path(team) else {
        return false;
    };

    parent.spawn((
        SceneBundle {
            scene: asset_server.load(path),
            // The model is exported standing on z=0 one unit tall, while the body it hangs from is
            // centred on its own origin -- so drop it half a cube to put its feet at the cube's
            // bottom face rather than at its middle.
            transform: Transform::from_xyz(0.0, -CUBE_SIZE / 2.0, 0.0)
                .with_scale(Vec3::splat(CUBE_SIZE)),
            ..default()
        },
        CharacterSkin,
    ));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_team_resolves_to_a_distinct_model() {
        let orange = skin_path(Team::Orange);
        let blue = skin_path(Team::Blue);
        assert_ne!(orange, blue, "teams must be telling apart on the pitch");
    }

    #[test]
    fn skin_paths_name_a_scene_inside_the_gltf() {
        for team in [Team::Orange, Team::Blue] {
            if let Some(path) = skin_path(team) {
                assert!(path.ends_with("#Scene0"), "bevy needs the scene label: {path}");
                assert!(path.starts_with("characters/"), "models live in assets/characters");
            }
        }
    }
}
