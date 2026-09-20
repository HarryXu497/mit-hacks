use bevy::prelude::*;
use crate::game::{GameState, config::*};
use crate::entities::arena::ScoreboardDigits;

/// Component for a single segment in a 7-segment display
#[derive(Component)]
pub struct DigitSegment;

/// Component to identify score digits for orange team
#[derive(Component)]
pub struct OrangeScoreDigit {
    pub digit_index: usize, // 0 = tens, 1 = units
}

/// Component to identify score digits for blue team
#[derive(Component)]
pub struct BlueScoreDigit {
    pub digit_index: usize,
}

/// Component to identify timer digits
#[derive(Component)]
pub struct TimerDigit {
    pub digit_index: usize, // 0 = tens of seconds, 1 = units of seconds
}

/// 7-segment patterns for digits 0-9
/// Each segment: [top, top-right, bottom-right, bottom, bottom-left, top-left, middle]
const SEGMENT_PATTERNS: [[bool; 7]; 10] = [
    [true, true, true, true, true, true, false],     // 0
    [false, true, true, false, false, false, false], // 1
    [true, true, false, true, true, false, true],    // 2
    [true, true, true, true, false, false, true],    // 3
    [false, true, true, false, false, true, true],   // 4
    [true, false, true, true, false, true, true],    // 5
    [true, false, true, true, true, true, true],     // 6
    [true, true, true, false, false, false, false],  // 7
    [true, true, true, true, true, true, true],      // 8
    [true, true, true, true, false, true, true],     // 9
];

/// Spawn a 7-segment digit display at the given position with an initial digit value
pub fn spawn_digit(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    position: Vec3,
    color: Color,
    scale: f32,
    initial_digit: u32,
) -> Vec<Entity> {
    let segment_thickness = 0.08 * scale;
    let segment_length = 0.4 * scale;
    let digit_height = 1.0 * scale;
    let digit_width = 0.6 * scale;

    let horizontal_mesh = meshes.add(Cuboid::new(segment_length, segment_thickness, 0.05));
    let vertical_mesh = meshes.add(Cuboid::new(segment_thickness, segment_length * 0.8, 0.05));

    let mut entities = Vec::new();

    // Segment positions relative to center:
    // 0: top horizontal
    // 1: top-right vertical
    // 2: bottom-right vertical
    // 3: bottom horizontal
    // 4: bottom-left vertical
    // 5: top-left vertical
    // 6: middle horizontal

    let segment_positions = [
        (Vec3::new(0.0, digit_height / 2.0, 0.0), true),           // 0: top
        (Vec3::new(digit_width / 2.0, digit_height / 4.0, 0.0), false),  // 1: top-right
        (Vec3::new(digit_width / 2.0, -digit_height / 4.0, 0.0), false), // 2: bottom-right
        (Vec3::new(0.0, -digit_height / 2.0, 0.0), true),          // 3: bottom
        (Vec3::new(-digit_width / 2.0, -digit_height / 4.0, 0.0), false),// 4: bottom-left
        (Vec3::new(-digit_width / 2.0, digit_height / 4.0, 0.0), false), // 5: top-left
        (Vec3::new(0.0, 0.0, 0.0), true),                          // 6: middle
    ];

    // Use the initial digit pattern
    let initial_pattern = SEGMENT_PATTERNS[(initial_digit % 10) as usize];

    for (i, (offset, is_horizontal)) in segment_positions.iter().enumerate() {
        let mesh = if *is_horizontal { horizontal_mesh.clone() } else { vertical_mesh.clone() };

        // IMPORTANT: Each segment needs its own unique material!
        // Don't clone handles - create new materials for each segment
        let segment_material = if initial_pattern[i] {
            materials.add(StandardMaterial {
                base_color: color,
                emissive: color * 3.0,
                ..default()
            })
        } else {
            materials.add(StandardMaterial {
                base_color: Color::rgb(0.15, 0.15, 0.15),
                emissive: Color::BLACK,
                ..default()
            })
        };

        let entity = commands.spawn((
            PbrBundle {
                mesh,
                material: segment_material,
                transform: Transform::from_translation(position + *offset),
                ..default()
            },
            DigitSegment,
        )).id();

        entities.push(entity);
    }

    entities
}

/// Update segment materials based on digit value
fn update_digit_segments_internal(
    segment_entities: &[Entity],
    digit: u32,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    query: &Query<&Handle<StandardMaterial>, With<DigitSegment>>,
    color: Color,
) {
    let digit = (digit % 10) as usize;
    let pattern = SEGMENT_PATTERNS[digit];

    for (i, &entity) in segment_entities.iter().enumerate() {
        if let Ok(material_handle) = query.get(entity) {
            if let Some(material) = materials.get_mut(material_handle) {
                if pattern[i] {
                    material.base_color = color;
                    material.emissive = color * 3.0;
                } else {
                    material.base_color = Color::rgb(0.15, 0.15, 0.15);
                    material.emissive = Color::BLACK;
                }
            }
        }
    }
}

/// System to update the wall scoreboard digits based on game state
pub fn update_wall_scoreboard(
    game_state: Res<GameState>,
    scoreboard: Option<Res<ScoreboardDigits>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<&Handle<StandardMaterial>, With<DigitSegment>>,
) {
    let Some(scoreboard) = scoreboard else { return };

    // Update orange score (left)
    let orange_score = game_state.score[0];
    if scoreboard.orange_digits.len() >= 2 {
        update_digit_segments_internal(
            &scoreboard.orange_digits[0],
            orange_score / 10,
            &mut materials,
            &query,
            CUBE_ORANGE_COLOR,
        );
        update_digit_segments_internal(
            &scoreboard.orange_digits[1],
            orange_score % 10,
            &mut materials,
            &query,
            CUBE_ORANGE_COLOR,
        );
    }

    // Update blue score (right)
    let blue_score = game_state.score[1];
    if scoreboard.blue_digits.len() >= 2 {
        update_digit_segments_internal(
            &scoreboard.blue_digits[0],
            blue_score / 10,
            &mut materials,
            &query,
            CUBE_BLUE_COLOR,
        );
        update_digit_segments_internal(
            &scoreboard.blue_digits[1],
            blue_score % 10,
            &mut materials,
            &query,
            CUBE_BLUE_COLOR,
        );
    }

    // Update timer (round timer)
    let timer_secs = game_state.round_timer.ceil() as u32;
    if scoreboard.timer_digits.len() >= 2 {
        update_digit_segments_internal(
            &scoreboard.timer_digits[0],
            timer_secs / 10,
            &mut materials,
            &query,
            Color::WHITE,
        );
        update_digit_segments_internal(
            &scoreboard.timer_digits[1],
            timer_secs % 10,
            &mut materials,
            &query,
            Color::WHITE,
        );
    }
}
