use bevy::prelude::*;
use crate::game::config::*;

/// Material presets for the game
#[derive(Resource)]
pub struct GameMaterials {
    pub wall: Handle<StandardMaterial>,
    pub field: Handle<StandardMaterial>,
    pub grid_line: Handle<StandardMaterial>,
    pub orange: Handle<StandardMaterial>,
    pub blue: Handle<StandardMaterial>,
    pub ball: Handle<StandardMaterial>,
}

pub fn setup_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let game_materials = GameMaterials {
        wall: materials.add(StandardMaterial {
            base_color: WALL_COLOR,
            perceptual_roughness: 0.9,
            metallic: 0.0,
            ..default()
        }),
        field: materials.add(StandardMaterial {
            base_color: FIELD_COLOR,
            perceptual_roughness: 0.7,
            metallic: 0.0,
            ..default()
        }),
        grid_line: materials.add(StandardMaterial {
            base_color: Color::rgba(1.0, 1.0, 1.0, 0.5),
            perceptual_roughness: 0.5,
            metallic: 0.0,
            ..default()
        }),
        orange: materials.add(StandardMaterial {
            base_color: CUBE_ORANGE_COLOR,
            perceptual_roughness: 0.4,
            metallic: 0.5,
            ..default()
        }),
        blue: materials.add(StandardMaterial {
            base_color: CUBE_BLUE_COLOR,
            perceptual_roughness: 0.4,
            metallic: 0.5,
            ..default()
        }),
        ball: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.8,
            metallic: 0.1,
            ..default()
        }),
    };

    commands.insert_resource(game_materials);
}
