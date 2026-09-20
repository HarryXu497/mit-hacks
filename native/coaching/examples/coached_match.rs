//! Render the production game with a synthetic coaching payload, without
//! microphone capture, external API calls, or touching saved coaching sessions.
//! cargo run --manifest-path native/coaching/Cargo.toml --example coached_match -- highpress
use bevy::prelude::*;
use serde_json::json;
use tactic_lab_native::{game::GamePlugin, game_handoff::GameHandoff, phase::AppPhase};

fn main() {
    let tactic = std::env::args().nth(1).unwrap_or_else(|| "highpress".into());
    let output = json!({
        "schemaVersion": "2.0", "taxonomyVersion": "tactics-v2",
        "session": { "id": "synthetic-render-smoke" },
        "rlSelection": {
            "schemaVersion": "2.0", "taxonomyVersion": "tactics-v2",
            "sessionId": "synthetic-render-smoke", "teamId": "red",
            "primaryTactic": tactic, "downstreamValue": tactic,
            "playerOverrides": [{ "playerId": 2, "tactic": "lowblock",
                "evidence": { "eventIds": [], "transcriptSegmentIds": ["synthetic-instruction"] } }]
        }
    });
    let handoff = GameHandoff::from_output(&output, "synthetic-render-smoke")
        .expect("Pass one of the ten canonical tactic labels");
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Tactic Lab — synthetic coached match".into(),
                resolution: (1280.0_f32, 800.0_f32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_state(AppPhase::Game)
        .insert_resource(handoff.team_tactics())
        .insert_resource(handoff)
        .add_plugins(GamePlugin)
        .run();
}
