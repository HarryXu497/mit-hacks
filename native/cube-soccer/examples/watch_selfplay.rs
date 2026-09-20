//! Watch the trained PPO policy play itself.
//!
//! Both teams are driven by the SAME exported policy (`assets/policy.json`),
//! each conditioned on its own tactic — so this is real model self-play, not the
//! heuristic AI. Press `T` to cycle Orange's tactic, `Y` to cycle Blue's, and
//! watch the shape change.
//!
//! The deterministic mean looks passive (the checkpoints are diffuse,
//! `exp(log_std) ≈ 2.5`); pass `--sample` to sample the livelier behaviour that
//! actually scored.
//!
//! Run with:
//!   cargo run --release --example watch_selfplay
//!   cargo run --release --example watch_selfplay -- --sample
//!   cargo run --release --example watch_selfplay -- path/to/policy.json --sample

use bevy::prelude::*;
use cube_soccer::input::keyboard::keyboard_input_system;
use cube_soccer::input::{apply_ai_actions, AIActions};
use cube_soccer::rl::{apply_policy_actions, PolicyController, PolicyNet, PolicyTeams};
use cube_soccer::systems::heuristic_ai::{Tactic, TeamDirective, TeamTactics};
use cube_soccer::CubeSoccerPlugin;

/// Training ran at 15 decisions/sec (`ACTION_REPEAT=2` @ 30 Hz physics).
const DECISION_HZ: f32 = 15.0;

/// Tracks the current named preset per team so we can cycle and print it.
#[derive(Resource)]
struct DemoTactic {
    orange: Tactic,
    blue: Tactic,
}
impl Default for DemoTactic {
    fn default() -> Self {
        Self { orange: Tactic::Balanced, blue: Tactic::Balanced }
    }
}

fn main() {
    // Args: an optional policy.json path (first positional) and `--sample`.
    let mut sample = false;
    let mut policy_path: Option<String> = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--sample" => sample = true,
            _ => policy_path = Some(arg),
        }
    }
    let policy_path = policy_path
        .unwrap_or_else(|| format!("{}/assets/policy.json", env!("CARGO_MANIFEST_DIR")));

    let net = PolicyNet::load(&policy_path)
        .unwrap_or_else(|e| panic!("failed to load policy from {policy_path}: {e}"));
    println!(
        "loaded policy: obs_dim={} act_dim={}  mode={}",
        net.obs_dim,
        net.act_dim,
        if sample { "stochastic (--sample)" } else { "deterministic (mean)" },
    );
    println!("controls: [T] cycle Orange tactic   [Y] cycle Blue tactic");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Cube Soccer 3D - Model Self-Play".to_string(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CubeSoccerPlugin)
        .init_resource::<TeamTactics>()
        .init_resource::<AIActions>()
        .init_resource::<DemoTactic>()
        .insert_resource(PolicyTeams { orange: true, blue: true })
        .insert_resource(PolicyController::new(net, sample, DECISION_HZ))
        // Decide, then apply — after the keyboard so the policy owns every cube.
        .add_systems(
            Update,
            (apply_policy_actions, apply_ai_actions)
                .chain()
                .after(keyboard_input_system),
        )
        .add_systems(Update, switch_tactics)
        .run();
}

fn switch_tactics(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut demo: ResMut<DemoTactic>,
    mut tactics: ResMut<TeamTactics>,
) {
    if keyboard.just_pressed(KeyCode::KeyT) {
        demo.orange = demo.orange.next();
        tactics.orange = TeamDirective::uniform(demo.orange.params());
        println!("Orange tactic: {}", demo.orange.name());
    }
    if keyboard.just_pressed(KeyCode::KeyY) {
        demo.blue = demo.blue.next();
        tactics.blue = TeamDirective::uniform(demo.blue.params());
        println!("Blue tactic: {}", demo.blue.name());
    }
}
