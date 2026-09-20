//! Music and sound effects.
//!
//! Deliberately ignorant of what screen the game is on. The engine crate has no
//! idea whether it is running inside the coaching app, a standalone match or a
//! training harness, so this exposes two controls and lets the host drive them:
//!
//! - [`MusicTrack`], a resource naming the bed that should be playing.
//! - [`Sfx`], an event. Anything that wants a noise sends one.
//!
//! The alternative -- reading the host's state enum from in here -- would make
//! the engine depend on the app that embeds it, which is backwards.
//!
//! Music is two third-party CC0 tracks; the effects are synthesised by
//! `tools/compose.py`. See `assets/audio/CREDITS.md`.

use bevy::audio::{PlaybackMode, Volume};
use bevy::prelude::*;

use crate::game::events::GoalScoredEvent;

/// Which bed should be playing. Set by whatever owns the screen flow.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicTrack {
    #[default]
    Silence,
    /// Menus, character creation, the tactics board: everything before kickoff.
    Lobby,
    /// A match in progress.
    Match,
}

/// Every noise the game can make.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sfx {
    MenuMove,
    MenuSelect,
    MenuBack,
    Kick,
    Whistle,
    Goal,
    Whoosh,
}

/// Set while a microphone is recording, so everything gets out of the way.
///
/// The coaching app drives this from its speech status. There is no echo
/// cancellation anywhere in this stack, so whatever leaves the speakers arrives
/// in the transcript; turning the game down is the only defence available.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct MicrophoneOpen(pub bool);

/// Loaded once and held. Every clip is small enough that keeping them resident
/// beats a stutter the first time one fires.
#[derive(Resource)]
pub struct Sounds {
    lobby: Handle<AudioSource>,
    match_theme: Handle<AudioSource>,
    menu_move: Handle<AudioSource>,
    menu_select: Handle<AudioSource>,
    menu_back: Handle<AudioSource>,
    kick: Handle<AudioSource>,
    whistle: Handle<AudioSource>,
    goal: Handle<AudioSource>,
    whoosh: Handle<AudioSource>,
}

impl Sounds {
    fn for_effect(&self, sfx: Sfx) -> (Handle<AudioSource>, f32) {
        match sfx {
            Sfx::MenuMove => (self.menu_move.clone(), 0.50),
            Sfx::MenuSelect => (self.menu_select.clone(), 0.60),
            Sfx::MenuBack => (self.menu_back.clone(), 0.50),
            Sfx::Kick => (self.kick.clone(), 0.70),
            Sfx::Whistle => (self.whistle.clone(), 0.50),
            Sfx::Goal => (self.goal.clone(), 0.85),
            Sfx::Whoosh => (self.whoosh.clone(), 0.55),
        }
    }
}

#[derive(Component)]
struct Music {
    target: f32,
    current: f32,
}

/// What the current bed is, so a change of track can be noticed.
#[derive(Resource, Default)]
struct Sounding(MusicTrack);

/// Quiet on purpose. This plays under a game people talk over, and under a
/// microphone that is recording them.
const MUSIC_LEVEL: f32 = 0.22;
const FADE_PER_SECOND: f32 = 1.4;

/// What everything drops to while the microphone is open.
const DUCKED: f32 = 0.25;

pub fn load_sounds(mut commands: Commands, assets: Res<AssetServer>) {
    // Extensions are explicit: the music is Vorbis and the effects are WAV.
    commands.insert_resource(Sounds {
        lobby: assets.load("audio/music-lobby.ogg"),
        match_theme: assets.load("audio/music-match.ogg"),
        menu_move: assets.load("audio/sfx-menu-move.wav"),
        menu_select: assets.load("audio/sfx-menu-select.wav"),
        menu_back: assets.load("audio/sfx-menu-back.wav"),
        kick: assets.load("audio/sfx-kick.wav"),
        whistle: assets.load("audio/sfx-whistle.wav"),
        goal: assets.load("audio/sfx-goal.wav"),
        whoosh: assets.load("audio/sfx-whoosh.wav"),
    });
}

/// Starts the requested bed and fades out whatever was playing.
///
/// Both players exist at once during the crossfade, which is the point: cutting
/// one off and starting the next leaves a hole exactly where the game is trying
/// to build momentum.
fn steer_music(
    mut commands: Commands,
    wanted: Res<MusicTrack>,
    sounds: Option<Res<Sounds>>,
    mut sounding: ResMut<Sounding>,
    mut tracks: Query<(Entity, &mut Music)>,
    time: Res<Time>,
) {
    let Some(sounds) = sounds else {
        return;
    };

    if *wanted != sounding.0 {
        sounding.0 = *wanted;
        for (_, mut music) in &mut tracks {
            music.target = 0.0;
        }
        let source = match *wanted {
            MusicTrack::Lobby => sounds.lobby.clone(),
            MusicTrack::Match => sounds.match_theme.clone(),
            MusicTrack::Silence => return,
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

/// Pushes the faded value onto the sinks, ducked if a microphone is live.
///
/// Separate from [`steer_music`] because the sink does not exist on the frame an
/// `AudioBundle` is spawned -- it appears once the asset has decoded, and
/// querying for it in the same system would silently skip the start of a fade.
fn apply_music_volume(tracks: Query<(&Music, &AudioSink)>, mic: Res<MicrophoneOpen>) {
    let duck = if mic.0 { DUCKED } else { 1.0 };
    for (music, sink) in &tracks {
        sink.set_volume(music.current * duck);
    }
}

fn play_effects(
    mut commands: Commands,
    mut requests: EventReader<Sfx>,
    sounds: Option<Res<Sounds>>,
    mic: Res<MicrophoneOpen>,
) {
    let Some(sounds) = sounds else {
        requests.clear();
        return;
    };
    // A one-shot's volume is fixed when it starts; unlike the music it has no
    // sink to turn down afterwards, so ducking is applied here.
    let duck = if mic.0 { DUCKED } else { 1.0 };
    for sfx in requests.read() {
        let (source, level) = sounds.for_effect(*sfx);
        commands.spawn(AudioBundle {
            source,
            settings: PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: Volume::new(level * duck),
                ..default()
            },
        });
    }
}

fn goal_sounds(mut scored: EventReader<GoalScoredEvent>, mut sfx: EventWriter<Sfx>) {
    for _ in scored.read() {
        sfx.send(Sfx::Goal);
    }
}

/// How long after a touch before the ball can thump again.
#[derive(Resource)]
struct KickCooldown(f32);

impl Default for KickCooldown {
    fn default() -> Self {
        Self(0.0)
    }
}

/// A thud when a player is on the ball and it is moving fast.
///
/// Driven off proximity and speed rather than a collision event: the physics
/// layer reports contacts for the whole scene, and a ball resting against a
/// foot would retrigger every frame.
fn ball_sounds(
    time: Res<Time>,
    mut cooldown: ResMut<KickCooldown>,
    mut sfx: EventWriter<Sfx>,
    balls: Query<
        (&Transform, &bevy_rapier3d::prelude::Velocity),
        (With<crate::entities::Ball>, Without<crate::entities::CubePlayer>),
    >,
    players: Query<&Transform, With<crate::entities::CubePlayer>>,
) {
    cooldown.0 = (cooldown.0 - time.delta_seconds()).max(0.0);
    if cooldown.0 > 0.0 {
        return;
    }
    let Ok((ball, velocity)) = balls.get_single() else {
        return;
    };
    if velocity.linvel.length() < 8.0 {
        return;
    }
    if players
        .iter()
        .any(|p| p.translation.distance(ball.translation) < 2.2)
    {
        sfx.send(Sfx::Kick);
        cooldown.0 = 0.18;
    }
}

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicTrack>()
            .init_resource::<MicrophoneOpen>()
            .init_resource::<Sounding>()
            .init_resource::<KickCooldown>()
            .add_event::<Sfx>()
            .add_systems(Startup, load_sounds)
            .add_systems(
                Update,
                (
                    steer_music,
                    apply_music_volume,
                    goal_sounds,
                    ball_sounds,
                    play_effects,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_music_sits_under_conversation_even_undicked() {
        // This plays while people talk over it and while a microphone records
        // them. Loud here is a bug, not a preference.
        assert!(MUSIC_LEVEL <= 0.25, "music bed is too loud: {MUSIC_LEVEL}");
        assert!(
            MUSIC_LEVEL * DUCKED <= 0.06,
            "ducked music is still loud enough to reach a transcript"
        );
    }

    #[test]
    fn every_clip_exists_on_disk() {
        // The asset server logs a missing file and carries on, so a wrong name
        // here is silence rather than a crash -- exactly the sort of fault that
        // survives all the way to a demo.
        for name in [
            "music-lobby.ogg",
            "music-match.ogg",
            "sfx-menu-move.wav",
            "sfx-menu-select.wav",
            "sfx-menu-back.wav",
            "sfx-kick.wav",
            "sfx-whistle.wav",
            "sfx-goal.wav",
            "sfx-whoosh.wav",
        ] {
            let path = format!("assets/audio/{name}");
            assert!(std::path::Path::new(&path).exists(), "{path} is missing");
        }
    }

    #[test]
    fn every_effect_maps_to_a_distinct_clip() {
        // A copy-paste in `for_effect` would be silent in the sense that it
        // still plays something, just the wrong thing.
        let all = [
            Sfx::MenuMove,
            Sfx::MenuSelect,
            Sfx::MenuBack,
            Sfx::Kick,
            Sfx::Whistle,
            Sfx::Goal,
            Sfx::Whoosh,
        ];
        let sounds = Sounds {
            lobby: Handle::default(),
            match_theme: Handle::default(),
            menu_move: Handle::weak_from_u128(1),
            menu_select: Handle::weak_from_u128(2),
            menu_back: Handle::weak_from_u128(3),
            kick: Handle::weak_from_u128(4),
            whistle: Handle::weak_from_u128(5),
            goal: Handle::weak_from_u128(6),
            whoosh: Handle::weak_from_u128(7),
        };
        let mut seen = Vec::new();
        for sfx in all {
            let (handle, _) = sounds.for_effect(sfx);
            assert!(!seen.contains(&handle.id()), "{sfx:?} reuses another clip");
            seen.push(handle.id());
        }
    }
}
