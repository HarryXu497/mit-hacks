//! The sign beside the table.
//!
//! Everything the 2D coaching screen kept in side panels — the recording state,
//! the live transcript, the session timeline and the tactical result — is
//! carved onto one standing board in the world instead. It is a real object:
//! lit, outlined, and still there when the camera leaves.
//!
//! Its face is a camera's render target. A second camera photographs a small
//! 2D layout of text and bars and writes the result straight onto the sign's
//! texture, which is how a 3D board comes to carry live words without any of
//! it being drawn over the top of the world.

use super::scene::SignFace;
use super::{Interpretation, Session, TableState, Tool, Transcript};
use bevy::render::camera::{ClearColorConfig, RenderTarget};
use bevy::render::render_resource::{
    Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::view::RenderLayers;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::{Text2dBounds, Text2dBundle};

/// The sign's face, in pixels. Matches the timber's 3.4 x 4.4 proportions.
const SIGN_PX: UVec2 = UVec2::new(768, 904);
/// Only the sign camera draws this layer, and it draws nothing else.
const SIGN_LAYER: u8 = 1;

const PANEL: Color = Color::rgb(0.93, 0.88, 0.71);
const IVORY: Color = Color::rgb(0.06, 0.05, 0.07);
const MUTED: Color = Color::rgb(0.42, 0.31, 0.18);
const GOLD: Color = Color::rgb(0.72, 0.30, 0.05);
const ORANGE: Color = Color::rgb(0.80, 0.24, 0.03);
const LEAF: Color = Color::rgb(0.10, 0.31, 0.16);

#[derive(Component)]
pub struct StatusLine;
#[derive(Component)]
pub struct TranscriptLines;
#[derive(Component)]
pub struct TimelineLines;
#[derive(Component)]
pub struct ResultLines;
#[derive(Component)]
pub struct KeysLine;
/// One mark on the timeline bar. Repositioned rather than respawned.
#[derive(Component)]
pub struct Tick(usize);

/// How many marks the bar can show at once.
const TICKS: usize = 48;

fn heading(text: &str, y: f32) -> (Text2dBundle, RenderLayers) {
    (
        Text2dBundle {
            text: Text::from_section(
                text,
                TextStyle { font_size: 25.0, color: MUTED, ..default() },
            ),
            text_anchor: Anchor::TopLeft,
            transform: Transform::from_xyz(-336., y, 1.),
            ..default()
        },
        RenderLayers::layer(SIGN_LAYER),
    )
}

fn body(y: f32, height: f32, size: f32, color: Color) -> (Text2dBundle, RenderLayers) {
    (
        Text2dBundle {
            text: Text::from_section("", TextStyle { font_size: size, color, ..default() })
                .with_justify(JustifyText::Left),
            text_anchor: Anchor::TopLeft,
            text_2d_bounds: Text2dBounds { size: Vec2::new(672., height) },
            transform: Transform::from_xyz(-336., y, 1.),
            ..default()
        },
        RenderLayers::layer(SIGN_LAYER),
    )
}

/// Builds the sign's face and the camera that keeps it up to date.
pub fn build_sign(
    mut c: Commands,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    faces: Query<&Handle<StandardMaterial>, With<SignFace>>,
) {
    let size = Extent3d { width: SIGN_PX.x, height: SIGN_PX.y, depth_or_array_layers: 1 };
    let mut target = Image {
        texture_descriptor: TextureDescriptor {
            label: Some("tactics-sign"),
            size,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        ..default()
    };
    target.resize(size);
    let handle = images.add(target);

    // Hand the rendered face to the timber that scene.rs already stood up.
    let Ok(material) = faces.get_single() else {
        warn!("the sign has no face to write on");
        return;
    };
    if let Some(material) = mats.get_mut(material) {
        // The timber's placeholder colour would otherwise multiply the whole
        // rendered face down to near-black.
        material.base_color = Color::WHITE;
        material.base_color_texture = Some(handle.clone());
        material.unlit = true;
    }

    c.spawn((
        Camera2dBundle {
            camera: Camera {
                // Before the main camera, so the face is ready in the same frame.
                order: -1,
                target: RenderTarget::Image(handle),
                clear_color: ClearColorConfig::Custom(PANEL),
                ..default()
            },
            // The default filmic curve crushed the sign's own palette: ivory
            // came out grey and gold came out olive. The face is authored art,
            // like every other surface here, so it is delivered as authored.
            tonemapping: bevy::core_pipeline::tonemapping::Tonemapping::None,
            deband_dither: bevy::core_pipeline::tonemapping::DebandDither::Disabled,
            ..default()
        },
        RenderLayers::layer(SIGN_LAYER),
    ));

    // The painted face itself. Behind every other layer on the sign.
    c.spawn((
        SpriteBundle {
            sprite: Sprite {
                color: PANEL,
                custom_size: Some(Vec2::new(SIGN_PX.x as f32, SIGN_PX.y as f32)),
                ..default()
            },
            transform: Transform::from_xyz(0., 0., -10.),
            ..default()
        },
        RenderLayers::layer(SIGN_LAYER),
    ));

    c.spawn((
        Text2dBundle {
            text: Text::from_section(
                "TACTIC LAB",
                TextStyle { font_size: 40.0, color: GOLD, ..default() },
            ),
            text_anchor: Anchor::TopLeft,
            transform: Transform::from_xyz(-336., 418., 1.),
            ..default()
        },
        RenderLayers::layer(SIGN_LAYER),
    ));
    c.spawn((body(366., 60., 29., IVORY), StatusLine));
    c.spawn(heading("LIVE TRANSCRIPT", 304.));
    c.spawn((body(268., 226., 23., IVORY), TranscriptLines));
    c.spawn(heading("SESSION TIMELINE", 24.));
    c.spawn((body(-56., 186., 21., MUTED), TimelineLines));
    c.spawn(heading("TACTICAL JSON", -234.));
    c.spawn((body(-270., 96., 23., LEAF), ResultLines));
    c.spawn(heading("COMMANDS", -348.));
    c.spawn((body(-382., 120., 20., MUTED), KeysLine));

    // The timeline bar, and the marks that ride on it.
    c.spawn((
        SpriteBundle {
            sprite: Sprite { color: Color::rgb(0.72, 0.66, 0.50), custom_size: Some(Vec2::new(672., 10.)), ..default() },
            transform: Transform::from_xyz(0., -14., 0.),
            ..default()
        },
        RenderLayers::layer(SIGN_LAYER),
    ));
    for index in 0..TICKS {
        c.spawn((
            SpriteBundle {
                sprite: Sprite { color: GOLD, custom_size: Some(Vec2::new(4., 22.)), ..default() },
                transform: Transform::from_xyz(0., -14., 1.),
                visibility: Visibility::Hidden,
                ..default()
            },
            Tick(index),
            RenderLayers::layer(SIGN_LAYER),
        ));
    }
}

fn clock(ms: u64) -> String {
    format!("{:02}:{:02}", ms / 60_000, (ms % 60_000) / 1000)
}

/// Writes the session onto the sign.
#[allow(clippy::too_many_arguments)]
pub fn update_sign(
    session: Res<Session>,
    state: Res<State<TableState>>,
    transcript: Res<Transcript>,
    interpretation: Res<Interpretation>,
    tool: Res<Tool>,
    mut texts: ParamSet<(
        Query<&mut Text, With<StatusLine>>,
        Query<&mut Text, With<TranscriptLines>>,
        Query<&mut Text, With<TimelineLines>>,
        Query<&mut Text, With<ResultLines>>,
        Query<&mut Text, With<KeysLine>>,
    )>,
    mut ticks: Query<(&Tick, &mut Transform, &mut Visibility)>,
) {
    if let Ok(mut text) = texts.p0().get_single_mut() {
        let section = &mut text.sections[0];
        let (label, colour) = match state.get() {
            TableState::Setup => ("Ready  -  arrange the board".to_owned(), MUTED),
            TableState::Recording => (format!("Recording  {}", clock(session.elapsed_ms)), ORANGE),
            TableState::Stopped => (format!("Stopped  {}", clock(session.elapsed_ms)), IVORY),
        };
        let hand = tool.label();
        section.value = format!("{label}      hand: {hand}");
        section.style.color = colour;
    }

    if let Ok(mut text) = texts.p1().get_single_mut() {
        let mut lines: Vec<String> = session
            .events
            .iter()
            .rev()
            .filter_map(|e| match e {
                super::RawEvent::TranscriptAdded { text, .. } => Some(text.clone()),
                _ => None,
            })
            .take(6)
            .collect();
        lines.reverse();
        if !transcript.partial.is_empty() {
            lines.push(format!("{}...", transcript.partial));
        }
        let section = &mut text.sections[0];
        if lines.is_empty() {
            // Say plainly that nothing is listening, rather than showing an
            // empty panel that looks like a feature waiting to work.
            section.value = if transcript.live {
                "Listening...".to_owned()
            } else {
                "No microphone attached to this build.\nSpeech arrives from the coaching app.".to_owned()
            };
            section.style.color = MUTED;
        } else {
            section.value = lines.join("\n");
            section.style.color = IVORY;
        }
    }

    if let Ok(mut text) = texts.p2().get_single_mut() {
        let mut lines: Vec<String> = session
            .events
            .iter()
            .rev()
            .take(7)
            .map(|e| format!("{}  {}", clock(e.at_ms()), e.summary()))
            .collect();
        lines.reverse();
        text.sections[0].value = if lines.is_empty() {
            "Nothing recorded yet.".to_owned()
        } else {
            lines.join("\n")
        };
    }

    if let Ok(mut text) = texts.p3().get_single_mut() {
        let section = &mut text.sections[0];
        if let Some(error) = &interpretation.error {
            section.value = error.clone();
            section.style.color = ORANGE;
        } else if let Some(summary) = &interpretation.summary {
            let tactic = interpretation.tactic.clone().unwrap_or_else(|| "-".to_owned());
            section.value = format!("{tactic}\n{summary}");
            section.style.color = LEAF;
        } else if interpretation.requested {
            section.value = "Interpreting...".to_owned();
            section.style.color = MUTED;
        } else {
            section.value = format!(
                "{} actions, {} spoken.\nPress G to generate.",
                session.action_count(),
                session.transcript_count()
            );
            section.style.color = MUTED;
        }
    }

    if let Ok(mut text) = texts.p4().get_single_mut() {
        // Two short columns rather than one long line, which ran off the board.
        text.sections[0].value = concat!(
            "M move      A arrow     D draw      E erase
",
            "R record/stop           Space replay
",
            "Ctrl+Z undo             Backspace reset
",
            "G generate JSON         Enter to the match",
        )
        .to_owned();
    }

    // Marks along the bar, one per logged event, oldest at the left.
    let span = session.elapsed_ms.max(1) as f32;
    let shown = session.events.len().saturating_sub(TICKS);
    for (tick, mut transform, mut visibility) in &mut ticks {
        match session.events.get(shown + tick.0) {
            Some(event) => {
                let t = event.at_ms() as f32 / span;
                transform.translation.x = -336. + t.clamp(0., 1.) * 672.;
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_as_minutes_and_seconds() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(9_000), "00:09");
        assert_eq!(clock(61_000), "01:01");
        assert_eq!(clock(605_000), "10:05");
    }

    #[test]
    fn the_signs_face_matches_the_timber_it_is_mounted_on() {
        let texture = SIGN_PX.x as f32 / SIGN_PX.y as f32;
        let timber = crate::tactics::SIGN_W / crate::tactics::SIGN_H;
        assert!((texture - timber).abs() < 0.02, "{texture} vs {timber}: the face would stretch");
    }
}
