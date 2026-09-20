//! Music and sound effects.
//!
//! Two looping tracks and seven one-shots, all rendered by `tools/compose.py`
//! rather than recorded. The score is code, so changing the arrangement is a
//! diff and not a binary blob swap.
//!
//! The music is swapped on [`AppState`]: the lobby theme covers the whole front
//! end and the match theme covers play, with a short crossfade between them so
//! the drop into kickoff is a change of energy rather than a cut. Both tracks
//! share a tonal centre for the same reason.

use bevy::audio::{PlaybackMode, Volume};
use bevy::prelude::*;

use crate::game::events::GoalScoredEvent;
use crate::intro::AppState;

/// Loaded once at startup and held for the run. Every clip is small enough that
/// keeping them resident costs less than a stutter the first time one fires.
#[derive(Resource)]
pub struct Sounds {
    pub lobby: Handle<AudioSource>,
    pub match_theme: Handle<AudioSource>,
    pub menu_move: Handle<AudioSource>,
    pub menu_select: Handle<AudioSource>,
    pub menu_back: Handle<AudioSource>,
    pub kick: Handle<AudioSource>,
    pub whistle: Handle<AudioSource>,
    pub goal: Handle<AudioSource>,
    pub whoosh: Handle<AudioSource>,
}

/// Marks a music player so it can be faded and replaced.
#[derive(Component)]
pub struct Music {
    /// Where this track's volume is heading. Reaching zero despawns it.
    target: f32,
    current: f32,
}

/// Which track the music systems believe is playing.
#[derive(Resource, Default, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Playing {
    #[default]
    Nothing,
    Lobby,
    Match,
}

/// Loudness of the two beds. Low on purpose, and lower than it first sounds
/// right at: this plays under a game people talk over, and the coaching side
/// records what they say through the machine's own microphone. Anything coming
/// out of the speakers is going back into that microphone.
const MUSIC_LEVEL: f32 = 0.22;
const FADE_PER_SECOND: f32 = 1.4;

/// What everything drops to while the microphone is open.
///
/// There is no echo cancellation here, so the only real defence against the
/// music turning up in the transcript is not playing it loudly while someone is
/// talking. A quarter is quiet enough to stay out of the way and loud enough
/// that the game does not seem to have crashed.
const DUCKED: f32 = 0.25;

/// True while speech is being captured, so music and effects can get out of the
/// way. Reads the speech runtime if there is one; a build without the coaching
/// side simply never ducks.
fn microphone_open(speech: &Option<Res<crate::tactics::speech::SpeechRuntime>>) -> bool {
    use crate::tactics::speech::SpeechStatus;
    speech.as_ref().is_some_and(|s| {
        matches!(
            s.status,
            SpeechStatus::Connecting | SpeechStatus::Listening | SpeechStatus::Finalizing
        )
    })
}

fn load(assets: &AssetServer, name: &str) -> Handle<AudioSource> {
    assets.load(format!("audio/{name}.wav"))
}

pub fn load_sounds(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Sounds {
        lobby: load(&assets, "music-lobby"),
        match_theme: load(&assets, "music-match"),
        menu_move: load(&assets, "sfx-menu-move"),
        menu_select: load(&assets, "sfx-menu-select"),
        menu_back: load(&assets, "sfx-menu-back"),
        kick: load(&assets, "sfx-kick"),
        whistle: load(&assets, "sfx-whistle"),
        goal: load(&assets, "sfx-goal"),
        whoosh: load(&assets, "sfx-whoosh"),
    });
}

/// Which track a given state wants behind it.
fn wanted(state: AppState) -> Playing {
    match state {
        // The drop-in keeps the lobby theme running. Swapping on the way down
        // would start the match music over a shot that is still the menu's.
        AppState::ColdOpen | AppState::Title | AppState::Menu | AppState::DropIn => Playing::Lobby,
        AppState::Playing => Playing::Match,
    }
}

/// Starts the track the current state asks for and fades out the other.
///
/// Both players exist at once during the crossfade, which is the whole point:
/// cutting one off and starting the next leaves a hole exactly where the game
/// is trying to build momentum.
pub fn steer_music(
    mut commands: Commands,
    state: Res<State<AppState>>,
    sounds: Option<Res<Sounds>>,
    mut playing: ResMut<Playing>,
    mut tracks: Query<(Entity, &mut Music)>,
    time: Res<Time>,
) {
    let Some(sounds) = sounds else {
        return;
    };

    let want = wanted(*state.get());
    if want != *playing {
        *playing = want;
        // Everything already sounding is now on its way out.
        for (_, mut music) in &mut tracks {
            music.target = 0.0;
        }
        let source = match want {
            Playing::Lobby => sounds.lobby.clone(),
            Playing::Match => sounds.match_theme.clone(),
            Playing::Nothing => return,
        };
        commands.spawn((
            AudioBundle {
                source,
                settings: PlaybackSettings {
                    mode: PlaybackMode::Loop,
                    volume: Volume::new(0.0),
                    ..default()
                },
            },
            Music {
                target: MUSIC_LEVEL,
                current: 0.0,
            },
        ));
    }

    let step = FADE_PER_SECOND * time.delta_seconds();
    for (entity, mut music) in &mut tracks {
        let delta = music.target - music.current;
        music.current += delta.clamp(-step, step);
        if music.current <= 0.001 && music.target == 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// Pushes the faded value onto the sinks.
///
/// Separate from [`steer_music`] because the sink is not available on the frame
/// an `AudioBundle` is spawned -- it appears once the asset has decoded, and
/// querying for it in the same system would silently skip the first frames of
/// every fade.
pub fn apply_music_volume(
    tracks: Query<(&Music, &AudioSink)>,
    speech: Option<Res<crate::tactics::speech::SpeechRuntime>>,
) {
    let duck = if microphone_open(&speech) { DUCKED } else { 1.0 };
    for (music, sink) in &tracks {
        sink.set_volume(music.current * duck);
    }
}

/// A one-shot, despawned by the engine when it finishes.
///
/// `duck` is passed in rather than read here because a one-shot's volume is
/// fixed when it starts -- unlike the music, it has no sink to turn down later.
fn play(commands: &mut Commands, source: Handle<AudioSource>, volume: f32, duck: f32) {
    commands.spawn(AudioBundle {
        source,
        settings: PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::new(volume * duck),
            ..default()
        },
    });
}

/// Menu movement, confirmation and backing out.
pub fn menu_sounds(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    menu: Res<crate::intro::ui::Menu>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    speech: Option<Res<crate::tactics::speech::SpeechRuntime>>,
) {
    let Some(sounds) = sounds else {
        return;
    };
    let duck = if microphone_open(&speech) { DUCKED } else { 1.0 };
    if *state.get() != AppState::Menu {
        return;
    }
    if menu.is_changed() && !menu.is_added() {
        play(&mut commands, sounds.menu_move.clone(), 0.5, duck);
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        play(&mut commands, sounds.menu_select.clone(), 0.6, duck);
    }
    if keys.just_pressed(KeyCode::Escape) {
        play(&mut commands, sounds.menu_back.clone(), 0.5, duck);
    }
}

/// The camera's flight down to the pitch.
pub fn drop_in_sounds(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    speech: Option<Res<crate::tactics::speech::SpeechRuntime>>,
) {
    let duck = if microphone_open(&speech) { DUCKED } else { 1.0 };
    if let Some(sounds) = sounds {
        play(&mut commands, sounds.whoosh.clone(), 0.55, duck);
    }
}

/// Kickoff.
pub fn kickoff_whistle(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    speech: Option<Res<crate::tactics::speech::SpeechRuntime>>,
) {
    let duck = if microphone_open(&speech) { DUCKED } else { 1.0 };
    if let Some(sounds) = sounds {
        play(&mut commands, sounds.whistle.clone(), 0.5, duck);
    }
}

pub fn goal_sounds(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    mut scored: EventReader<GoalScoredEvent>,
    speech: Option<Res<crate::tactics::speech::SpeechRuntime>>,
) {
    let duck = if microphone_open(&speech) { DUCKED } else { 1.0 };
    let Some(sounds) = sounds else {
        scored.clear();
        return;
    };
    for _ in scored.read() {
        play(&mut commands, sounds.goal.clone(), 0.85, duck);
    }
}

/// A thud whenever a player is touching the ball and moving into it.
///
/// Driven off proximity and speed rather than a collision event, because the
/// physics layer reports contacts for the whole scene and the ball resting
/// against a foot would retrigger every frame. A cooldown does the rest.
#[derive(Resource, Default)]
pub struct KickCooldown(f32);

pub fn ball_sounds(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    time: Res<Time>,
    mut cooldown: ResMut<KickCooldown>,
    balls: Query<
        (&Transform, &bevy_rapier3d::prelude::Velocity),
        (With<crate::entities::Ball>, Without<crate::entities::CubePlayer>),
    >,
    players: Query<&Transform, With<crate::entities::CubePlayer>>,
    speech: Option<Res<crate::tactics::speech::SpeechRuntime>>,
) {
    let Some(sounds) = sounds else {
        return;
    };
    let duck = if microphone_open(&speech) { DUCKED } else { 1.0 };
    cooldown.0 = (cooldown.0 - time.delta_seconds()).max(0.0);
    if cooldown.0 > 0.0 {
        return;
    }
    let Ok((ball, velocity)) = balls.get_single() else {
        return;
    };
    // Only a struck ball, not a rolling one.
    let speed = velocity.linvel.length();
    if speed < 8.0 {
        return;
    }
    let touching = players
        .iter()
        .any(|p| p.translation.distance(ball.translation) < 2.2);
    if touching {
        let loudness = ((speed - 8.0) / 22.0).clamp(0.25, 1.0);
        play(&mut commands, sounds.kick.clone(), loudness * 0.8, duck);
        cooldown.0 = 0.18;
    }
}

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Playing>()
            .init_resource::<KickCooldown>()
            .add_systems(Startup, load_sounds)
            .add_systems(OnEnter(AppState::DropIn), drop_in_sounds)
            .add_systems(OnEnter(AppState::Playing), kickoff_whistle)
            .add_systems(
                Update,
                (
                    steer_music,
                    apply_music_volume,
                    menu_sounds,
                    goal_sounds,
                    ball_sounds.run_if(crate::intro::playing),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whole_front_end_shares_one_track() {
        // A menu that restarts its music on every screen is the thing this
        // mapping exists to prevent.
        for state in [AppState::ColdOpen, AppState::Title, AppState::Menu] {
            assert_eq!(wanted(state), Playing::Lobby, "{state:?}");
        }
    }

    #[test]
    fn the_dive_keeps_the_lobby_theme_until_the_whistle() {
        // The drop-in is still the menu's shot; the match theme belongs to play.
        assert_eq!(wanted(AppState::DropIn), Playing::Lobby);
        assert_eq!(wanted(AppState::Playing), Playing::Match);
    }

    #[test]
    fn the_music_sits_under_conversation_even_undicked() {
        // This plays while people talk over it and while a microphone is
        // recording them. Loud here is a bug, not a preference.
        assert!(MUSIC_LEVEL <= 0.25, "music bed is too loud: {MUSIC_LEVEL}");
        assert!(MUSIC_LEVEL * DUCKED <= 0.06, "ducked music is still audible in a transcript");
    }

    #[test]
    fn every_clip_this_module_asks_for_exists_on_disk() {
        // The asset server logs a missing file and carries on, so a typo here
        // is silence rather than a crash -- which is exactly the kind of fault
        // that survives to a demo.
        for name in [
            "music-lobby",
            "music-match",
            "sfx-menu-move",
            "sfx-menu-select",
            "sfx-menu-back",
            "sfx-kick",
            "sfx-whistle",
            "sfx-goal",
            "sfx-whoosh",
        ] {
            let path = format!("assets/audio/{name}.wav");
            assert!(
                std::path::Path::new(&path).exists(),
                "{path} is missing -- run `python tools/compose.py`"
            );
        }
    }
}
