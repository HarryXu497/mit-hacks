//! The prompt plaque.
//!
//! The one piece of the clearing that is not an object in the world: a small
//! timber plaque along the bottom edge that says what this step is and which
//! key moves on. The wording is the 2D screen's, so both versions ask the
//! player the same questions. It is styled like the stadium — dark timber, an
//! ink line, ivory lettering — rather than like an application panel.

use super::paint::{Brush, Tool};
use super::{CreationPhase, Notice};
use bevy::prelude::*;

const INK: Color = Color::rgb(0.05, 0.04, 0.06);
const TIMBER: Color = Color::rgb(0.30, 0.15, 0.065);
const IVORY: Color = Color::rgb(0.98, 0.94, 0.74);
const MUTED: Color = Color::rgb(0.80, 0.74, 0.54);
const ORANGE: Color = Color::rgb(0.95, 0.50, 0.18);
const LEAF: Color = Color::rgb(0.55, 0.80, 0.25);

#[derive(Component)]
pub struct Plaque;
#[derive(Component)]
pub struct TitleLine;
#[derive(Component)]
pub struct HintLine;

fn line(text: &str, size: f32, color: Color) -> TextBundle {
    TextBundle::from_section(text, TextStyle { font_size: size, color, ..default() })
}

pub fn spawn(mut c: Commands) {
    c.spawn((
        NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Px(24.),
                bottom: Val::Px(22.),
                padding: UiRect::axes(Val::Px(18.), Val::Px(11.)),
                border: UiRect::all(Val::Px(3.)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.),
                ..default()
            },
            background_color: TIMBER.into(),
            border_color: INK.into(),
            ..default()
        },
        Plaque,
    ))
    .with_children(|plaque| {
        plaque.spawn((line("", 22., IVORY), TitleLine));
        plaque.spawn((line("", 16., MUTED), HintLine));
    });
}

/// The step's title and question, then its keys. Copy shared with the 2D flow.
fn copy(phase: CreationPhase) -> (&'static str, &'static str, &'static str) {
    match phase {
        CreationPhase::PaintingAppearance => (
            "Draw your player",
            "What does this player look like?",
            "Enter: save & draw superpower",
        ),
        CreationPhase::PaintingSuperpower => (
            "Draw the superpower",
            "What can this player do that nobody else can?",
            "Enter: save & review",
        ),
        _ => (
            "Review your player",
            "Check both drawings before moving on.",
            "Enter: continue to coaching    1: edit appearance    2: edit superpower",
        ),
    }
}

pub fn update(
    phase: Res<State<CreationPhase>>,
    notice: Res<Notice>,
    brush: Res<Brush>,
    mut titles: Query<&mut Text, (With<TitleLine>, Without<HintLine>)>,
    mut hints: Query<&mut Text, (With<HintLine>, Without<TitleLine>)>,
) {
    let (title, question, keys) = copy(*phase.get());
    if let Ok(mut text) = titles.get_single_mut() {
        text.sections[0].value = format!("{title}  -  {question}");
    }
    if let Ok(mut text) = hints.get_single_mut() {
        let section = &mut text.sections[0];
        if notice.text.is_empty() {
            let tool = match brush.tool {
                Tool::Pigment(_) => "brush",
                Tool::Rag => "rag (eraser)",
            };
            section.value = if phase.get().slot().is_some() {
                format!("{keys}    Ctrl+Z: undo    E / B: rag / brush    [ ]: size    now: {tool}")
            } else {
                keys.to_owned()
            };
            section.style.color = MUTED;
        } else {
            section.value = notice.text.clone();
            section.style.color = if notice.is_error { ORANGE } else { LEAF };
        }
    }
}

/// The plaque leaves with the clearing, so the flight is unbroken picture.
pub fn hide(mut plaques: Query<&mut Visibility, With<Plaque>>) {
    for mut visibility in &mut plaques {
        *visibility = Visibility::Hidden;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_tells_the_player_which_key_moves_on() {
        for phase in [
            CreationPhase::PaintingAppearance,
            CreationPhase::PaintingSuperpower,
            CreationPhase::Review,
        ] {
            assert!(copy(phase).2.starts_with("Enter:"), "{phase:?}");
        }
    }
}
