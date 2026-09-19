use crate::model::{
    create_id, Annotation, AnnotationKind, EntityRef, Point, RawSessionEvent, SessionStatus, TeamId,
    Tool,
};
use crate::replay::{find_undo_target, replay_session};
use crate::session::CoachingSession;
use bevy::prelude::*;
use bevy::render::camera::{ScalingMode, Viewport};
use bevy::sprite::{ColorMaterial, MaterialMesh2dBundle};
use bevy::window::PrimaryWindow;
use bevy_egui::egui;

const TOKEN_RADIUS: f32 = 0.034;

#[derive(Component)]
pub struct CoachingOwned;

#[derive(Component)]
pub struct CoachingCamera;

#[derive(Component, Debug, Clone, Copy)]
enum BoardToken {
    Player(u8),
    Ball,
}

#[derive(Resource, Debug, Clone)]
pub struct BoardViewport {
    pub rect: egui::Rect,
    pub scale_factor: f32,
    pub visible: bool,
}

impl Default for BoardViewport {
    fn default() -> Self {
        Self {
            rect: egui::Rect::NOTHING,
            scale_factor: 1.0,
            visible: false,
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct BoardInteraction {
    pub tool: Tool,
    drag: Option<Drag>,
}

#[derive(Debug)]
enum Drag {
    Entity {
        entity: EntityRef,
        from: Point,
        current: Point,
        path: Vec<Point>,
        started_at_ms: u64,
    },
    Annotation {
        kind: AnnotationKind,
        points: Vec<Point>,
        started_at_ms: u64,
    },
}

pub fn spawn_board(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    spawn_board_entities(&mut commands, &mut meshes, &mut materials);
}

pub fn spawn_board_entities(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let projection = OrthographicProjection {
        scaling_mode: ScalingMode::Fixed {
            width: 1.12,
            height: 1.12,
        },
        ..default()
    };
    commands.spawn((
        Camera2dBundle {
            camera: Camera {
                order: 0,
                ..default()
            },
            projection,
            transform: Transform::from_xyz(0.5, 0.5, 10.0),
            ..default()
        },
        CoachingCamera,
        CoachingOwned,
    ));

    commands.spawn((
        SpriteBundle {
            sprite: Sprite {
                color: Color::rgb(0.70, 0.73, 0.74),
                custom_size: Some(Vec2::splat(1.06)),
                ..default()
            },
            transform: Transform::from_xyz(0.5, 0.5, -2.0),
            ..default()
        },
        CoachingOwned,
    ));
    commands.spawn((
        SpriteBundle {
            sprite: Sprite {
                color: Color::rgb(0.055, 0.45, 0.20),
                custom_size: Some(Vec2::ONE),
                ..default()
            },
            transform: Transform::from_xyz(0.5, 0.5, -1.0),
            ..default()
        },
        CoachingOwned,
    ));

    for player in crate::model::BoardState::default().players {
        let color = match player.team {
            TeamId::Red => Color::rgb(0.90, 0.18, 0.20),
            TeamId::Yellow => Color::rgb(0.97, 0.77, 0.12),
        };
        commands
            .spawn((
                MaterialMesh2dBundle {
                    mesh: meshes.add(Circle::new(TOKEN_RADIUS)).into(),
                    material: materials.add(ColorMaterial::from(color)),
                    transform: Transform::from_translation(to_world(player.position, 1.0)),
                    ..default()
                },
                BoardToken::Player(player.id),
                CoachingOwned,
            ))
            .with_children(|parent| {
                parent.spawn(Text2dBundle {
                    text: Text::from_section(
                        player.id.to_string(),
                        TextStyle {
                            font_size: 22.0,
                            color: if player.team == TeamId::Yellow {
                                Color::rgb(0.10, 0.14, 0.18)
                            } else {
                                Color::WHITE
                            },
                            ..default()
                        },
                    )
                    .with_justify(JustifyText::Center),
                    transform: Transform::from_xyz(0.0, -0.009, 0.1)
                        .with_scale(Vec3::splat(0.004)),
                    ..default()
                });
            });
    }

    commands.spawn((
        MaterialMesh2dBundle {
            mesh: meshes.add(Circle::new(TOKEN_RADIUS * 0.72)).into(),
            material: materials.add(ColorMaterial::from(Color::rgb(0.96, 0.96, 0.92))),
            transform: Transform::from_translation(to_world(
                crate::model::BoardState::default().ball,
                1.1,
            )),
            ..default()
        },
        BoardToken::Ball,
        CoachingOwned,
    ));
}

pub fn update_board_camera(
    viewport: Res<BoardViewport>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<CoachingCamera>>,
) {
    if !viewport.is_changed() {
        return;
    }
    let Ok(window) = windows.get_single() else {
        return;
    };
    let scale = window.scale_factor() as f32;
    let position = UVec2::new(
        (viewport.rect.min.x * scale).max(0.0) as u32,
        (viewport.rect.min.y * scale).max(0.0) as u32,
    );
    let size = UVec2::new(
        (viewport.rect.width() * scale).max(1.0) as u32,
        (viewport.rect.height() * scale).max(1.0) as u32,
    );
    for mut camera in &mut cameras {
        camera.is_active = viewport.visible;
        camera.viewport = viewport.visible.then_some(Viewport {
            physical_position: position,
            physical_size: size,
            depth: 0.0..1.0,
        });
    }
}

pub fn draw_pitch_and_annotations(
    mut gizmos: Gizmos,
    session: Res<CoachingSession>,
    interaction: Res<BoardInteraction>,
) {
    let replay_limit = if matches!(
        session.session.status,
        SessionStatus::Review | SessionStatus::Interpreted
    ) {
        Some(session.playhead_ms)
    } else {
        None
    };
    let replay = replay_session(&session.session.events, replay_limit);
    let white = Color::rgba(0.96, 0.98, 0.94, 0.92);
    gizmos.rect_2d(Vec2::splat(0.5), 0.0, Vec2::ONE, white);
    gizmos.line_2d(Vec2::new(0.0, 0.5), Vec2::new(1.0, 0.5), white);
    gizmos.circle_2d(Vec2::splat(0.5), 0.095, white);
    gizmos.circle_2d(Vec2::splat(0.5), 0.006, white);
    draw_penalty_area(&mut gizmos, 0.0, false, white);
    draw_penalty_area(&mut gizmos, 1.0, true, white);

    for annotation in &replay.board.annotations {
        draw_annotation(&mut gizmos, annotation, white);
    }
    if let Some(Drag::Annotation { kind, points, .. }) = &interaction.drag {
        draw_annotation(
            &mut gizmos,
            &Annotation {
                id: "draft".into(),
                kind: *kind,
                points: points.clone(),
            },
            white,
        );
    }
}

fn draw_penalty_area(gizmos: &mut Gizmos, goal_y: f32, bottom: bool, color: Color) {
    let direction = if bottom { -1.0 } else { 1.0 };
    let center_y = goal_y + direction * 0.085;
    gizmos.rect_2d(
        Vec2::new(0.5, center_y),
        0.0,
        Vec2::new(0.56, 0.17),
        color,
    );
    let small_y = goal_y + direction * 0.043;
    gizmos.rect_2d(
        Vec2::new(0.5, small_y),
        0.0,
        Vec2::new(0.30, 0.086),
        color,
    );
}

fn draw_annotation(gizmos: &mut Gizmos, annotation: &Annotation, color: Color) {
    for points in annotation.points.windows(2) {
        gizmos.line_2d(to_vec2(points[0]), to_vec2(points[1]), color);
    }
    if annotation.kind == AnnotationKind::Arrow && annotation.points.len() >= 2 {
        let end = to_vec2(*annotation.points.last().unwrap());
        let previous = to_vec2(annotation.points[annotation.points.len() - 2]);
        let direction = (end - previous).normalize_or_zero();
        let side = Vec2::new(-direction.y, direction.x);
        gizmos.line_2d(end, end - direction * 0.035 + side * 0.018, color);
        gizmos.line_2d(end, end - direction * 0.035 - side * 0.018, color);
    }
}

pub fn sync_tokens(
    session: Res<CoachingSession>,
    interaction: Res<BoardInteraction>,
    mut tokens: Query<(&BoardToken, &mut Transform)>,
) {
    let replay_limit = if matches!(
        session.session.status,
        SessionStatus::Review | SessionStatus::Interpreted
    ) {
        Some(session.playhead_ms)
    } else {
        None
    };
    let board = replay_session(&session.session.events, replay_limit).board;
    for (token, mut transform) in &mut tokens {
        let mut point = match token {
            BoardToken::Player(id) => board
                .players
                .iter()
                .find(|player| player.id == *id)
                .map(|player| player.position)
                .unwrap_or_default(),
            BoardToken::Ball => board.ball,
        };
        if let Some(Drag::Entity {
            entity, current, ..
        }) = &interaction.drag
        {
            let matches = match (token, entity) {
                (BoardToken::Ball, EntityRef::Ball) => true,
                (BoardToken::Player(token_id), EntityRef::Player { id }) => token_id == id,
                _ => false,
            };
            if matches {
                point = *current;
            }
        }
        transform.translation = to_world(point, transform.translation.z);
    }
}

pub fn handle_board_input(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    viewport: Res<BoardViewport>,
    mut interaction: ResMut<BoardInteraction>,
    mut session: ResMut<CoachingSession>,
) {
    if session.session.status != SessionStatus::Recording || !viewport.visible {
        interaction.drag = None;
        return;
    }
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Some(point) = point_in_viewport(cursor, viewport.rect) else {
        if buttons.just_released(MouseButton::Left) {
            finish_drag(&mut interaction, &mut session);
        }
        return;
    };
    let board = replay_session(&session.session.events, None).board;

    if buttons.just_pressed(MouseButton::Left) {
        let now = session.elapsed_now();
        match interaction.tool {
            Tool::Select => {
                if let Some((entity, from)) = hit_entity(point, &board) {
                    interaction.drag = Some(Drag::Entity {
                        entity,
                        from,
                        current: from,
                        path: vec![from],
                        started_at_ms: now,
                    });
                }
            }
            Tool::Arrow | Tool::Draw => {
                interaction.drag = Some(Drag::Annotation {
                    kind: if interaction.tool == Tool::Arrow {
                        AnnotationKind::Arrow
                    } else {
                        AnnotationKind::Freehand
                    },
                    points: vec![point],
                    started_at_ms: now,
                });
            }
            Tool::Erase => {
                if let Some(annotation) = board
                    .annotations
                    .iter()
                    .find(|annotation| annotation_hit(point, annotation))
                {
                    session.append(RawSessionEvent::AnnotationRemoved {
                        id: create_id("event"),
                        timestamp_ms: now,
                        annotation_id: annotation.id.clone(),
                    });
                }
            }
        }
    } else if buttons.pressed(MouseButton::Left) {
        if let Some(drag) = &mut interaction.drag {
            let points = match drag {
                Drag::Entity {
                    current, path, ..
                } => {
                    *current = point;
                    path
                }
                Drag::Annotation { points, .. } => points,
            };
            if points
                .last()
                .map_or(true, |previous| previous.distance(point) >= 0.008)
            {
                points.push(point);
            }
        }
    }

    if buttons.just_released(MouseButton::Left) {
        finish_drag(&mut interaction, &mut session);
    }
}

pub fn append_undo(session: &mut CoachingSession) {
    let Some(target_id) = find_undo_target(&session.session.events).map(|event| event.id().to_owned())
    else {
        return;
    };
    session.append(RawSessionEvent::Undo {
        id: create_id("event"),
        timestamp_ms: session.elapsed_now(),
        target_event_id: target_id,
    });
}

fn finish_drag(interaction: &mut BoardInteraction, session: &mut CoachingSession) {
    let Some(drag) = interaction.drag.take() else {
        return;
    };
    let timestamp_ms = session.elapsed_now();
    match drag {
        Drag::Entity {
            entity,
            from,
            current,
            path,
            started_at_ms,
        } if from.distance(current) > 0.005 => {
            session.append(RawSessionEvent::EntityMoved {
                id: create_id("event"),
                timestamp_ms,
                entity,
                started_at_ms,
                from,
                to: current,
                path,
            });
        }
        Drag::Annotation {
            kind,
            points,
            started_at_ms,
        } if points.len() > 1 => {
            session.append(RawSessionEvent::AnnotationAdded {
                id: create_id("event"),
                timestamp_ms,
                annotation: Annotation {
                    id: create_id("annotation"),
                    kind,
                    points,
                },
                started_at_ms,
            });
        }
        _ => {}
    }
}

fn point_in_viewport(cursor: Vec2, rect: egui::Rect) -> Option<Point> {
    if !rect.contains(egui::pos2(cursor.x, cursor.y)) {
        return None;
    }
    Some(
        Point {
            x: (cursor.x - rect.min.x) / rect.width(),
            y: (cursor.y - rect.min.y) / rect.height(),
        }
        .clamped(),
    )
}

fn hit_entity(point: Point, board: &crate::model::BoardState) -> Option<(EntityRef, Point)> {
    if point.distance(board.ball) <= TOKEN_RADIUS * 1.25 {
        return Some((EntityRef::Ball, board.ball));
    }
    board
        .players
        .iter()
        .find(|player| point.distance(player.position) <= TOKEN_RADIUS * 1.4)
        .map(|player| (EntityRef::Player { id: player.id }, player.position))
}

fn annotation_hit(point: Point, annotation: &Annotation) -> bool {
    annotation
        .points
        .windows(2)
        .any(|segment| distance_to_segment(point, segment[0], segment[1]) < 0.025)
}

fn distance_to_segment(point: Point, a: Point, b: Point) -> f32 {
    let p = Vec2::new(point.x, point.y);
    let a = Vec2::new(a.x, a.y);
    let b = Vec2::new(b.x, b.y);
    let length_squared = (b - a).length_squared();
    if length_squared == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(b - a) / length_squared).clamp(0.0, 1.0);
    p.distance(a + t * (b - a))
}

fn to_world(point: Point, z: f32) -> Vec3 {
    Vec3::new(point.x, 1.0 - point.y, z)
}

fn to_vec2(point: Point) -> Vec2 {
    Vec2::new(point.x, 1.0 - point.y)
}
