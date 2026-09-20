use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use super::state::{GameState, MatchState};
use super::events::{GoalScoredEvent, GameOverEvent, ResetGameEvent, BallTouchedEvent};
use crate::entities::{spawn_arena, spawn_wall_scoreboard, spawn_field, spawn_goals, spawn_players, spawn_ball};
use crate::entities::character::{
    animate_player_visual, reveal_loaded_characters, wear_characters, WornCharacters,
};
use crate::systems::{
    camera::{setup_camera, update_camera},
    movement::{apply_player_movement, clamp_velocities},
    status_effects::{tick_status_effects, apply_status_forces, ImpulseEvent},
    physics::configure_physics,
    scoring::{detect_goals, handle_goal_scored, update_timers},
    reset::{reset_after_goal, reset_after_round, check_reset_timer, ResetTimer},
    possession::{Possession, tick_cooldowns, update_possession, clear_possession},
    display::update_wall_scoreboard,
    eyes::animate_googly_eyes,
    trail::TrailSpawnTimer,
    superpowers::{tick_superpower_cooldowns, activate_superpowers},
    power_vfx::{animate_power_fx, load_power_fx, reset_power_fx_glow, spawn_power_fx, PowerFired},
    power_auras::{load_auras, pulse_status_auras, sync_status_auras},
    power_loadout::{deal_superpowers, superpower_keys},
};
use crate::input::keyboard::keyboard_input_system;
use crate::rendering::lighting::setup_lighting;
use crate::ui::goal_banner::{
    animate_goal_banner, dress_the_podium, keep_the_podium_on_its_layer, raise_the_goal_banner,
    setup_goal_banner, GoalCelebration,
};
use crate::ui::hud::setup_ui;
use crate::ui::powers::{fill_power_rail, setup_power_hud, update_power_hud, PowerHudSide};
use crate::ui::scoreboard::update_ui;

pub struct CubeSoccerPlugin;

impl Plugin for CubeSoccerPlugin {
    fn build(&self, app: &mut App) {
        crate::rendering::stylized::register_shader(app);
        if crate::rendering::stylized::is_rendering(app) {
            // Only where there is a renderer to use it: headless apps have no `Assets<Shader>`,
            // and `stylize` no-ops for the same reason.
            app.add_plugins(
                MaterialPlugin::<crate::rendering::stylized::JungleMaterial>::default(),
            );
        }

        app
            // Four-sample anti-aliasing shades every pixel four times over.
            // Once the static props are batched the frame becomes fill bound,
            // and on integrated graphics that setting costs about a fifth of it
            // (measured 39 fps at 4x against 46 with it off).
            //
            // Off is too far: the goal netting is built from 0.035-wide beams
            // that fall below a pixel at broadcast distance, and with no
            // coverage sampling the mesh breaks into sparkle. Two samples hold
            // the net and the touchlines together for roughly the price of
            // none, so that is where this sits.
            .insert_resource(Msaa::Sample2)

            // States
            .insert_state(MatchState::Playing)

            // Resources
            .init_resource::<GameState>()
            .init_resource::<ResetTimer>()
            .init_resource::<TrailSpawnTimer>()
            .init_resource::<Possession>()
            .init_resource::<WornCharacters>()
            .init_resource::<PowerHudSide>()
            .init_resource::<GoalCelebration>()

            // Events
            .add_event::<GoalScoredEvent>()
            .add_event::<GameOverEvent>()
            .add_event::<ResetGameEvent>()
            .add_event::<BallTouchedEvent>()
            .add_event::<ImpulseEvent>()
            .add_event::<PowerFired>()
            // Once the roster exists, so a match has powers to fire at all.
            .add_systems(PostStartup, deal_superpowers)

            // Physics
            .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
            // Debug render disabled for cleaner visuals
            // .add_plugins(RapierDebugRenderPlugin::default())

            // Startup systems
            .add_systems(Startup, (
                configure_physics,
                spawn_arena,
                spawn_wall_scoreboard,
                spawn_field,
                spawn_goals,
                spawn_players,
                spawn_ball,
                setup_camera,
                setup_lighting,
                setup_ui,
                setup_power_hud,
                setup_goal_banner,
                load_power_fx,
                load_auras,
            ))

            // The jungle is built after the pitch exists, then the static props are merged into a
            // handful of draws and the cel-lit material is swapped in over the result. The order
            // matters: batching has to see every prop, and stylising has to see the batches.
            .add_systems(PostStartup, (
                crate::jungle::build_jungle,
                crate::rendering::batching::merge_static_draws,
                crate::rendering::stylized::keep_characters_unstylised,
                crate::rendering::stylized::stylize,
            ).chain())

            .add_systems(Update, (
                crate::jungle::animate_jungle,
                crate::jungle::animate_water,
            ))

            // The broadcast camera follows the ball. It runs in PostUpdate, ahead of transform
            // propagation, so the frame it renders is the one it just aimed at rather than the
            // previous one.
            .add_systems(PostUpdate, update_camera
                .before(bevy::transform::TransformSystem::TransformPropagate))

            // Update systems during playing
            .add_systems(Update, (
                keyboard_input_system,
                superpower_keys,
                tick_superpower_cooldowns,
                activate_superpowers,
                tick_status_effects,
                apply_player_movement,
                apply_status_forces,
                clamp_velocities,
                (tick_cooldowns, update_possession).chain(),
                detect_goals,
                handle_goal_scored,
                update_timers,
                update_ui,
                update_wall_scoreboard,
            ).chain().run_if(in_state(MatchState::Playing)))

            // What a power looks like. Ordered after the cast that raises the event so a burst
            // appears on the same frame the power lands, and the glow reset runs first so the
            // frame's pieces bid up from black rather than from the last cast's brightness.
            .add_systems(Update, (
                reset_power_fx_glow,
                spawn_power_fx.after(activate_superpowers),
                animate_power_fx,
                // The consequence, not the cast: a shell for as long as the effect lasts, so a
                // frozen player reads as frozen once the beam is gone.
                sync_status_auras,
                pulse_status_auras,
            ).chain())
            .add_systems(Update, (fill_power_rail, update_power_hud).chain())

            // The goal banner. Deliberately *not* gated on `MatchState::Playing`: a goal moves
            // the match into `GoalScored`, which is precisely when the banner has to be running.
            // The stands are kept dressed at all times so the portrait is ready the instant one
            // goes in, rather than loading a model while the banner is already up.
            .add_systems(Update, (
                dress_the_podium,
                keep_the_podium_on_its_layer,
                raise_the_goal_banner,
                animate_goal_banner,
            ).chain())

            // Reset after goal (with 1 second delay)
            .add_systems(OnEnter(MatchState::GoalScored), (reset_after_goal, clear_possession))
            .add_systems(Update, check_reset_timer.run_if(in_state(MatchState::GoalScored)))

            // Reset after round timeout (immediate)
            .add_systems(OnEnter(MatchState::RoundOver), (reset_after_round, clear_possession))

            // Animate effects (always running).
            //
            // `animate_player_visual` is what makes the characters move: it reads each body's
            // velocity, fire input and knockback events and writes only the visual child's
            // transform, so it can never perturb the simulation.
            //
            // Speed trails are deliberately *not* scheduled here. The effect was not wanted in the
            // jungle presentation, and it cost a freshly allocated mesh and material per particle,
            // thirty-three times a second per moving player, in a scene that is already fill
            // bound. `systems::trail` is still built, and `TrailSpawnTimer` still registered, so
            // the other binaries that do schedule it are unaffected.
            .add_systems(Update, (
                animate_googly_eyes,
                animate_player_visual,
                // What each side is wearing, and showing it once it has loaded. Chained because
                // the reveal has to see the request the same frame it is made, and both are
                // cheap no-ops once everyone is dressed.
                (wear_characters, reveal_loaded_characters).chain(),
            ));
    }
}
