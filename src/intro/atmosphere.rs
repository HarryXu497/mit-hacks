//! What separates the front end from the match without touching the geometry:
//! bars, a vignette and a different time of day.
//!
//! The stadium on the title card is the same stadium the match is played in.
//! Rebuilding it for the menu would double the scene; relighting it costs two
//! lerps a frame and reads as a different place entirely.

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::AppState;
use crate::rendering::lighting::KeyLight;

/// How the scene is lit in a given state.
#[derive(Clone, Copy)]
struct Mood {
    key_colour: Color,
    key_illuminance: f32,
    /// Where the key light sits. Low and raking for the menu, high for play.
    key_from: Vec3,
    ambient_colour: Color,
    ambient_brightness: f32,
    /// 0 at kickoff, 1 on the title card.
    vignette: f32,
    /// Fraction of screen height covered by each bar.
    letterbox: f32,
}

/// Late afternoon. A low sun rakes across the stands and throws the long
/// shadows that make the bowl read as deep, which the midday match light
/// deliberately avoids because it would stripe the pitch.
const FRONT_OF_HOUSE: Mood = Mood {
    key_colour: Color::rgb(1.0, 0.72, 0.42),
    key_illuminance: 4100.0,
    key_from: Vec3::new(-46., 13., 26.),
    ambient_colour: Color::rgb(0.52, 0.62, 0.92),
    ambient_brightness: 15.0,
    vignette: 0.75,
    letterbox: 0.0,
};

/// Same as [`FRONT_OF_HOUSE`] but with bars in, for the opening flight.
const COLD_OPEN: Mood = Mood {
    letterbox: 0.085,
    ..FRONT_OF_HOUSE
};

/// The match lighting from [`crate::rendering::lighting`]. Duplicated here as
/// the target of the transition; the setup system remains the source of truth
/// for what a match actually looks like.
const KICKOFF: Mood = Mood {
    key_colour: Color::rgb(1.0, 0.89, 0.70),
    key_illuminance: 4800.0,
    key_from: Vec3::new(-12., 24., 8.),
    ambient_colour: Color::rgb(0.72, 0.84, 1.0),
    ambient_brightness: 22.0,
    vignette: 0.0,
    letterbox: 0.0,
};

fn mood_for(state: AppState) -> Mood {
    match state {
        AppState::ColdOpen => COLD_OPEN,
        AppState::Title | AppState::Menu => FRONT_OF_HOUSE,
        AppState::DropIn | AppState::Playing => KICKOFF,
    }
}

fn mix_colour(a: Color, b: Color, t: f32) -> Color {
    let (a, b) = (a.rgba_to_vec4(), b.rgba_to_vec4());
    let m = a.lerp(b, t);
    Color::rgba(m.x, m.y, m.z, m.w)
}

/// The lighting actually on screen, chased toward [`mood_for`]. Held as a
/// resource rather than read back off the light so the transition survives a
/// state change happening mid-fade.
#[derive(Resource)]
pub(crate) struct CurrentMood {
    now: Mood,
    /// Which state the fade is heading for, so a transition can restart it.
    toward: AppState,
    /// Set once the fade has arrived. While true nothing is written at all.
    ///
    /// This is not a micro-optimisation. Writing the key light's transform
    /// marks it changed, and a changed directional light re-renders its shadow
    /// cascade; doing that every frame forever costs about a quarter of the
    /// frame rate on integrated graphics, for a light that stopped moving
    /// seconds ago.
    settled: bool,
}

impl Default for CurrentMood {
    fn default() -> Self {
        Self {
            now: COLD_OPEN,
            toward: AppState::ColdOpen,
            settled: false,
        }
    }
}

/// How close counts as arrived. Illuminance is the coarsest of the tracked
/// values at a few thousand, so the threshold is expressed against it.
const SETTLED_EPSILON: f32 = 0.5;

impl Mood {
    fn close_enough(&self, goal: &Mood) -> bool {
        (self.key_illuminance - goal.key_illuminance).abs() < SETTLED_EPSILON
            && (self.ambient_brightness - goal.ambient_brightness).abs() < 0.01
            && self.key_from.distance(goal.key_from) < 0.05
            && (self.vignette - goal.vignette).abs() < 0.002
            && (self.letterbox - goal.letterbox).abs() < 0.0005
    }
}

#[derive(Component)]
pub(crate) struct Vignette;

#[derive(Component)]
pub(crate) struct LetterboxBar;

/// Radial darkening, generated rather than shipped as a texture.
///
/// A real post-process pass would need a custom render graph node. This is a
/// stretched 128px image over the whole screen, which on a gradient this soft
/// is indistinguishable and costs one transparent quad.
fn vignette_image() -> Image {
    const SIZE: u32 = 128;
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let u = (x as f32 / (SIZE - 1) as f32) * 2. - 1.;
            let v = (y as f32 / (SIZE - 1) as f32) * 2. - 1.;
            // Fourth power keeps the middle of frame completely clear and puts
            // the whole falloff in the outer third.
            let r = (u * u + v * v).sqrt() / std::f32::consts::SQRT_2;
            let alpha = (r.powi(4) * 255.).clamp(0., 255.) as u8;
            data.extend_from_slice(&[0, 0, 0, alpha]);
        }
    }
    Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Spawns the two bars and the vignette once, at startup. They live for the
/// whole run and are driven by opacity and height rather than being respawned,
/// so a state change cannot leave a bar stuck on screen.
pub fn setup_overlays(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let vignette = images.add(vignette_image());

    commands.spawn((
        ImageBundle {
            image: UiImage::new(vignette),
            style: Style {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                ..default()
            },
            // Behind the text, in front of the stadium.
            z_index: ZIndex::Global(-10),
            ..default()
        },
        Vignette,
    ));

    for edge in [true, false] {
        commands.spawn((
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(0.),
                    top: if edge { Val::Px(0.) } else { Val::Auto },
                    bottom: if edge { Val::Auto } else { Val::Px(0.) },
                    ..default()
                },
                background_color: Color::BLACK.into(),
                z_index: ZIndex::Global(10),
                ..default()
            },
            LetterboxBar,
        ));
    }
}

/// Chases the current mood toward the one the state asks for and writes it to
/// the light, the ambient, the bars and the vignette.
#[allow(clippy::too_many_arguments)]
pub fn blend_atmosphere(
    time: Res<Time>,
    state: Res<State<AppState>>,
    mut mood: ResMut<CurrentMood>,
    mut ambient: ResMut<AmbientLight>,
    mut key: Query<(&mut DirectionalLight, &mut Transform), With<KeyLight>>,
    mut vignettes: Query<&mut BackgroundColor, With<Vignette>>,
    mut bars: Query<&mut Style, With<LetterboxBar>>,
) {
    let state = *state.get();
    if mood.toward != state {
        mood.toward = state;
        mood.settled = false;
    }
    if mood.settled {
        return;
    }
    let goal = mood_for(state);
    // Slow enough to be felt as a change of light rather than a flicker, quick
    // enough that the drop-in has finished relighting by the whistle.
    let t = (1. - (-1.8 * time.delta_seconds()).exp()).clamp(0., 1.);

    let now = &mut mood.now;
    now.key_colour = mix_colour(now.key_colour, goal.key_colour, t);
    now.key_illuminance += (goal.key_illuminance - now.key_illuminance) * t;
    now.key_from = now.key_from.lerp(goal.key_from, t);
    now.ambient_colour = mix_colour(now.ambient_colour, goal.ambient_colour, t);
    now.ambient_brightness += (goal.ambient_brightness - now.ambient_brightness) * t;
    now.vignette += (goal.vignette - now.vignette) * t;
    now.letterbox += (goal.letterbox - now.letterbox) * t;

    // Snap on arrival rather than approaching forever, so the light can be left
    // alone from the next frame on.
    if now.close_enough(&goal) {
        *now = goal;
        mood.settled = true;
    }
    let now = &mood.now;

    ambient.color = now.ambient_colour;
    ambient.brightness = now.ambient_brightness;

    for (mut light, mut transform) in &mut key {
        light.color = now.key_colour;
        light.illuminance = now.key_illuminance;
        *transform = Transform::from_translation(now.key_from).looking_at(Vec3::ZERO, Vec3::Y);
    }

    for mut tint in &mut vignettes {
        tint.0 = Color::rgba(1., 1., 1., now.vignette);
    }

    for mut style in &mut bars {
        style.height = Val::Percent(now.letterbox * 100.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_cold_open_wears_bars() {
        assert!(mood_for(AppState::ColdOpen).letterbox > 0.);
        for state in [
            AppState::Title,
            AppState::Menu,
            AppState::DropIn,
            AppState::Playing,
        ] {
            assert_eq!(mood_for(state).letterbox, 0., "{state:?} has bars");
        }
    }

    #[test]
    fn play_is_lit_exactly_as_the_match_lighting_sets_it_up() {
        // If these drift apart the pitch will change colour a second after
        // kickoff, which looks like a bug rather than a transition.
        let kickoff = mood_for(AppState::Playing);
        assert_eq!(kickoff.key_illuminance, 4800.0);
        assert_eq!(kickoff.ambient_brightness, 22.0);
        assert_eq!(kickoff.vignette, 0.);
    }

    #[test]
    fn vignette_is_clear_in_the_middle_and_dark_in_the_corners() {
        let image = vignette_image();
        let size = 128usize;
        let alpha_at = |x: usize, y: usize| image.data[(y * size + x) * 4 + 3];
        assert!(alpha_at(size / 2, size / 2) < 6, "centre is not clear");
        assert!(alpha_at(0, 0) > 200, "corner is not dark");
        assert!(alpha_at(size / 2, 0) < alpha_at(0, 0), "edge darker than corner");
    }
}
