//! Title card and mode menu.
//!
//! Text is the only thing this module draws. Everything behind it is the live
//! stadium, so the front end costs one extra camera path and a handful of text
//! nodes rather than a second scene.

use bevy::prelude::*;

use super::director::Shot;
use super::AppState;

pub const TITLE_FONT: &str = "fonts/MPLUSRounded1c-Black.ttf";
pub const UI_FONT: &str = "fonts/MPLUSRounded1c-Bold.ttf";

/// Warm off-white, not pure white: the sky is already near-white at the top of
/// frame and pure white type disappears into it.
const PARCHMENT: Color = Color::rgb(0.99, 0.97, 0.90);
const BANANA: Color = Color::rgb(0.99, 0.84, 0.28);
const INK: Color = Color::rgb(0.09, 0.07, 0.11);
const LEAF: Color = Color::rgb(0.64, 0.86, 0.62);

/// One entry in the mode list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuItem {
    Play,
    Team,
    Settings,
    Quit,
}

impl MenuItem {
    pub const ALL: [MenuItem; 4] = [
        MenuItem::Play,
        MenuItem::Team,
        MenuItem::Settings,
        MenuItem::Quit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MenuItem::Play => "PLAY",
            MenuItem::Team => "MY TEAM",
            MenuItem::Settings => "SETTINGS",
            MenuItem::Quit => "QUIT",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            MenuItem::Play => "Five a side. First to ten.",
            MenuItem::Team => "Meet the squad.",
            MenuItem::Settings => "Sound, controls, display.",
            MenuItem::Quit => "Back to the canopy.",
        }
    }

    /// Where the camera parks while this entry is highlighted. Each one frames
    /// what the entry is about, so moving down the list is a tour of the
    /// stadium rather than a list scroll.
    pub fn shot(self) -> Shot {
        match self {
            // Square on to the pitch, pushed in near the centre circle.
            MenuItem::Play => Shot::new(Vec3::new(0., 19., 47.), Vec3::new(0., 4., -2.), 44.),
            // Down at the touchline, close enough to read a monkey's face.
            MenuItem::Team => Shot::new(Vec3::new(-17., 7., 23.), Vec3::new(-9., 3.5, 2.), 38.),
            // Looking up the pitch at the scoreboard shrine.
            MenuItem::Settings => Shot::new(Vec3::new(0., 13., 16.), Vec3::new(0., 8.5, -21.), 42.),
            // Pulled back and high, the whole archipelago in frame.
            MenuItem::Quit => Shot::new(Vec3::new(6., 52., 96.), Vec3::new(0., 8., 0.), 48.),
        }
    }
}

/// Which mode entry is highlighted. Lives outside the UI entities so the camera
/// director can read it without touching the UI tree.
#[derive(Resource, Default)]
pub struct Menu {
    pub index: usize,
}

impl Menu {
    pub fn selected(&self) -> MenuItem {
        MenuItem::ALL[self.index % MenuItem::ALL.len()]
    }

    pub fn step(&mut self, delta: isize) {
        let count = MenuItem::ALL.len() as isize;
        self.index = (((self.index as isize + delta) % count + count) % count) as usize;
    }
}

/// Despawned when leaving the state that spawned it.
#[derive(Component)]
pub struct FrontEndUi;

/// The logo group, so the slam-in can scale it.
#[derive(Component)]
pub struct TitleCard {
    pub age: f32,
}

/// The "press any key" line, so it can pulse.
#[derive(Component)]
pub struct Prompt;

/// A row in the mode list, tagged with its position.
#[derive(Component)]
pub struct MenuRow(pub usize);

/// Offsets used to fake a text outline. Bevy 0.13 has no text stroke, so the
/// string is drawn eight times in ink behind one copy in the fill colour. Eight
/// directions rather than four because at this weight a four-way outline leaves
/// visible notches on the diagonals of letters like A and Y.
const OUTLINE: [(f32, f32); 8] = [
    (-1., -1.),
    (0., -1.),
    (1., -1.),
    (-1., 0.),
    (1., 0.),
    (-1., 1.),
    (0., 1.),
    (1., 1.),
];

/// Spawns `text` with a faked outline, returning nothing: callers position it
/// with the surrounding layout.
///
/// The fill copy is the only child in normal flow, so it alone gives the
/// wrapper its size; the ink copies are absolutely positioned and therefore
/// invisible to layout. Spawn order puts the ink behind the fill.
fn outlined(
    parent: &mut ChildBuilder,
    text: &str,
    font: Handle<Font>,
    size: f32,
    fill: Color,
    weight: f32,
    marker: impl Bundle,
) {
    let mut wrapper = parent.spawn((
        NodeBundle {
            style: Style {
                position_type: PositionType::Relative,
                // Shrink-wrap the text. A column parent stretches its children
                // to full width by default, and the selected row is scaled up
                // about its own centre -- on a 1600px-wide row that throws the
                // first letter off the left of the screen.
                align_self: AlignSelf::FlexStart,
                ..default()
            },
            ..default()
        },
        marker,
    ));
    wrapper.with_children(|stack| {
        for (dx, dy) in OUTLINE {
            stack.spawn(TextBundle {
                text: Text::from_section(
                    text,
                    TextStyle {
                        font: font.clone(),
                        font_size: size,
                        color: INK,
                    },
                ),
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(dx * weight),
                    top: Val::Px(dy * weight),
                    ..default()
                },
                ..default()
            });
        }
        stack.spawn(TextBundle::from_section(
            text,
            TextStyle {
                font,
                font_size: size,
                color: fill,
            },
        ));
    });
}

fn root() -> NodeBundle {
    NodeBundle {
        style: Style {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        ..default()
    }
}

pub fn spawn_title(mut commands: Commands, assets: Res<AssetServer>) {
    let display = assets.load(TITLE_FONT);
    let body = assets.load(UI_FONT);

    commands
        .spawn((root(), FrontEndUi))
        .with_children(|screen| {
            // Logo block, pushed left and down off centre so it sits over the
            // stands rather than over the pitch, which stays readable.
            screen
                .spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        margin: UiRect {
                            left: Val::Percent(8.),
                            top: Val::Percent(11.),
                            ..default()
                        },
                        row_gap: Val::Px(6.),
                        ..default()
                    },
                    ..default()
                })
                .insert(TitleCard { age: 0. })
                .with_children(|block| {
                    outlined(block, "CANOPY", display.clone(), 128., BANANA, 5., ());
                    outlined(block, "CLASH", display, 128., BANANA, 5., ());
                    // Manual spacing: Bevy 0.13 has no letter-spacing control.
                    block.spawn(NodeBundle {
                        style: Style {
                            margin: UiRect::top(Val::Px(10.)),
                            ..default()
                        },
                        ..default()
                    })
                    .with_children(|sub| {
                        outlined(sub, "J U N G L E   S O C C E R", body.clone(), 30., LEAF, 3., ());
                    });
                });

            // Spacer pushes the prompt to the bottom of the screen.
            screen.spawn(NodeBundle {
                style: Style {
                    flex_grow: 1.,
                    ..default()
                },
                ..default()
            });

            screen
                .spawn(NodeBundle {
                    style: Style {
                        width: Val::Percent(100.),
                        justify_content: JustifyContent::Center,
                        margin: UiRect::bottom(Val::Percent(7.)),
                        ..default()
                    },
                    ..default()
                })
                .with_children(|footer| {
                    outlined(footer, "PRESS ANY KEY", body, 34., PARCHMENT, 3., Prompt);
                });
        });
}

pub fn spawn_menu(mut commands: Commands, assets: Res<AssetServer>, menu: Res<Menu>) {
    let display = assets.load(TITLE_FONT);
    let body = assets.load(UI_FONT);

    commands
        .spawn((root(), FrontEndUi))
        .with_children(|screen| {
            // Small wordmark, so the menu still reads as the same screen.
            screen
                .spawn(NodeBundle {
                    style: Style {
                        margin: UiRect {
                            left: Val::Percent(6.),
                            top: Val::Percent(7.),
                            ..default()
                        },
                        ..default()
                    },
                    ..default()
                })
                .with_children(|corner| {
                    outlined(corner, "CANOPY CLASH", display, 42., BANANA, 3., ());
                });

            screen
                .spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        margin: UiRect {
                            left: Val::Percent(6.),
                            top: Val::Percent(6.),
                            ..default()
                        },
                        row_gap: Val::Px(14.),
                        ..default()
                    },
                    ..default()
                })
                .with_children(|list| {
                    for (i, item) in MenuItem::ALL.iter().enumerate() {
                        let chosen = i == menu.index;
                        outlined(
                            list,
                            item.label(),
                            body.clone(),
                            54.,
                            if chosen { BANANA } else { PARCHMENT },
                            4.,
                            MenuRow(i),
                        );
                    }
                });

            screen.spawn(NodeBundle {
                style: Style {
                    flex_grow: 1.,
                    ..default()
                },
                ..default()
            });

            screen
                .spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        margin: UiRect {
                            left: Val::Percent(6.),
                            bottom: Val::Percent(6.),
                            ..default()
                        },
                        row_gap: Val::Px(8.),
                        ..default()
                    },
                    ..default()
                })
                .with_children(|footer| {
                    outlined(footer, menu.selected().blurb(), body.clone(), 26., LEAF, 2., Blurb);
                    outlined(
                        footer,
                        "W / S  or  ARROWS   move      ENTER  select      ESC  back",
                        body,
                        20.,
                        Color::rgb(0.72, 0.80, 0.74),
                        2.,
                        (),
                    );
                });
        });
}

/// The one-line description under the mode list.
#[derive(Component)]
pub struct Blurb;

pub fn despawn_front_end_ui(mut commands: Commands, roots: Query<Entity, With<FrontEndUi>>) {
    for root in &roots {
        commands.entity(root).despawn_recursive();
    }
}

/// Overshoot-and-settle on the logo. Games in this genre never fade type in,
/// they throw it at the screen, so the curve deliberately passes 1.0 and comes
/// back rather than easing up to it.
pub fn animate_title(time: Res<Time>, mut cards: Query<(&mut Transform, &mut TitleCard)>) {
    for (mut transform, mut card) in &mut cards {
        card.age += time.delta_seconds();
        let t = (card.age / 0.55).clamp(0., 1.);
        // Damped spring, evaluated open-loop so it is deterministic.
        let overshoot = 1. - (1. - t).powi(3) * (t * 9.4).cos();
        let scale = 0.55 + 0.45 * overshoot;
        transform.scale = Vec3::splat(scale);
        transform.rotation = Quat::from_rotation_z((1. - t) * 0.06);
    }
}

pub fn pulse_prompt(time: Res<Time>, mut prompts: Query<&mut Transform, With<Prompt>>) {
    let pulse = 1. + (time.elapsed_seconds() * 3.2).sin() * 0.035;
    for mut transform in &mut prompts {
        transform.scale = Vec3::splat(pulse);
    }
}

/// Grows the highlighted row and recolours every fill copy. The ink copies keep
/// their colour, so the outline stays constant while the fill changes.
pub fn highlight_menu_row(
    menu: Res<Menu>,
    mut rows: Query<(&MenuRow, &mut Transform, &Children)>,
    mut text: Query<&mut Text>,
    time: Res<Time>,
) {
    let alpha = 1. - (-14. * time.delta_seconds()).exp();
    for (row, mut transform, children) in &mut rows {
        let chosen = row.0 == menu.index;
        let goal = if chosen { 1.14 } else { 1.0 };
        let scale = transform.scale.x + (goal - transform.scale.x) * alpha;
        transform.scale = Vec3::splat(scale);
        // The fill copy is the last child; the ones before it are the outline.
        if let Some(&fill) = children.last() {
            if let Ok(mut text) = text.get_mut(fill) {
                for section in &mut text.sections {
                    section.style.color = if chosen { BANANA } else { PARCHMENT };
                }
            }
        }
    }
}

pub fn update_blurb(menu: Res<Menu>, blurbs: Query<&Children, With<Blurb>>, mut text: Query<&mut Text>) {
    if !menu.is_changed() {
        return;
    }
    for children in &blurbs {
        for &child in children.iter() {
            if let Ok(mut text) = text.get_mut(child) {
                for section in &mut text.sections {
                    section.value = menu.selected().blurb().to_string();
                }
            }
        }
    }
}

/// Hides the front end while the camera flies into the match.
pub fn fade_out_ui(
    time: Res<Time>,
    state: Res<State<AppState>>,
    mut roots: Query<&mut Style, With<FrontEndUi>>,
) {
    if *state.get() != AppState::DropIn {
        return;
    }
    let _ = time;
    for mut style in &mut roots {
        style.display = Display::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_wraps_in_both_directions() {
        let mut menu = Menu::default();
        assert_eq!(menu.selected(), MenuItem::Play);
        menu.step(-1);
        assert_eq!(menu.selected(), MenuItem::Quit);
        menu.step(1);
        assert_eq!(menu.selected(), MenuItem::Play);
        for _ in 0..MenuItem::ALL.len() {
            menu.step(1);
        }
        assert_eq!(menu.selected(), MenuItem::Play);
    }

    #[test]
    fn every_entry_has_its_own_camera_shot() {
        // Two entries sharing a shot means moving between them looks broken.
        for (i, a) in MenuItem::ALL.iter().enumerate() {
            for b in MenuItem::ALL.iter().skip(i + 1) {
                assert!(
                    a.shot().eye.distance(b.shot().eye) > 1.,
                    "{a:?} and {b:?} park the camera in the same place"
                );
            }
        }
    }

    #[test]
    fn title_animation_overshoots_then_settles() {
        // The slam is the whole character of the entrance; if this ever eases
        // monotonically to 1.0 the logo will look like it faded in.
        let mut peak: f32 = 0.;
        let mut t = 0.;
        while t < 0.55 {
            let progress: f32 = t / 0.55;
            let overshoot = 1. - (1. - progress).powi(3) * (progress * 9.4).cos();
            peak = peak.max(0.55 + 0.45 * overshoot);
            t += 1. / 120.;
        }
        assert!(peak > 1.02, "no overshoot, peak was {peak}");
        let settled = 0.55 + 0.45 * (1. - 0f32.powi(3) * 1.);
        assert!((settled - 1.).abs() < 1e-5, "does not settle at 1.0: {settled}");
    }
}
