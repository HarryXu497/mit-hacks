//! Five-a-side formation. Physics comes from the existing player bundle.
use crate::entities::CubePlayerBundle;
use crate::game::config::*;
use bevy::prelude::*;

#[derive(Component)]
pub struct FormationSlot(pub usize);

pub fn formation_position(team: Team, slot: usize) -> Vec3 {
    let side = if team == Team::Orange { -1. } else { 1. };
    let positions = [
        (0.12, 0.),
        (0.29, -0.25),
        (0.29, 0.25),
        (0.43, 0.),
        (0.16, 0.32),
    ];
    let (x, z) = positions[slot % positions.len()];
    Vec3::new(
        side * FIELD_WIDTH * x,
        FIELD_HEIGHT + CUBE_SIZE,
        FIELD_DEPTH * z,
    )
}

pub fn spawn_rosters(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for team in [Team::Orange, Team::Blue] {
        for slot in 0..5 {
            commands.spawn((
                CubePlayerBundle::new(
                    team,
                    formation_position(team, slot),
                    &mut meshes,
                    &mut materials,
                ),
                FormationSlot(slot),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_reset_restores_all_formation_slots() {
        use crate::game::{GameState, MatchState};
        use crate::systems::reset::{reset_after_goal, reset_after_round, reset_game};
        use bevy_rapier3d::prelude::Velocity;
        for mode in 0..3 {
            let mut app = App::new();
            app.init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<StandardMaterial>>()
                .init_resource::<GameState>()
                .insert_resource(NextState::<MatchState>::default());
            for team in [Team::Orange, Team::Blue] {
                for slot in 0..5 {
                    app.world.spawn((
                        crate::entities::CubePlayer {
                            team,
                            can_jump: false,
                        },
                        FormationSlot(slot),
                        Transform::from_xyz(2., 8., 3.),
                        Velocity {
                            linvel: Vec3::ONE,
                            angvel: Vec3::ONE,
                        },
                    ));
                }
            }
            match mode {
                0 => {
                    app.add_systems(Update, reset_game);
                }
                1 => {
                    app.add_systems(Update, reset_after_goal);
                }
                _ => {
                    app.add_systems(Update, reset_after_round);
                }
            }
            app.update();
            let mut query = app.world.query::<(
                &crate::entities::CubePlayer,
                &FormationSlot,
                &Transform,
                &Velocity,
            )>();
            for (player, slot, transform, velocity) in query.iter(&app.world) {
                assert_eq!(
                    transform.translation,
                    formation_position(player.team, slot.0)
                );
                assert_eq!(velocity.linvel, Vec3::ZERO);
                assert_eq!(velocity.angvel, Vec3::ZERO);
            }
        }
    }
    #[test]
    fn ten_players_have_distinct_mirrored_formation_positions() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Startup, spawn_rosters);
        app.update();
        let mut query = app
            .world
            .query::<(&crate::entities::CubePlayer, &FormationSlot, &Transform)>();
        let players: Vec<_> = query.iter(&app.world).collect();
        assert_eq!(players.len(), 10);
        for team in [Team::Orange, Team::Blue] {
            assert_eq!(players.iter().filter(|(p, _, _)| p.team == team).count(), 5);
        }
        for (i, (_, _, a)) in players.iter().enumerate() {
            for (_, _, b) in players.iter().skip(i + 1) {
                assert!(a.translation.distance(b.translation) > CUBE_SIZE * 2.);
            }
        }
    }
}
