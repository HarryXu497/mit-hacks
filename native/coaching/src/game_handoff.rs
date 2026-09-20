//! Validated boundary between the external coaching JSON and the game controller.
use anyhow::{bail, ensure, Context, Result};
use bevy::prelude::*;
use cube_soccer::game::Team;
use cube_soccer::systems::heuristic_ai::{Tactic, TeamDirective, TeamTactics};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Resource, Clone, Debug)]
pub struct GameHandoff {
    pub session_id: String,
    pub tactic: Tactic,
    pub overrides: Vec<(u8, Tactic)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selection {
    schema_version: String,
    taxonomy_version: String,
    session_id: String,
    primary_tactic: String,
    downstream_value: String,
    team_id: Option<String>,
    player_overrides: Option<Vec<PlayerOverride>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerOverride {
    player_id: u8,
    tactic: String,
    evidence: Evidence,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    event_ids: Vec<String>,
    transcript_segment_ids: Vec<String>,
}

pub fn game_identity(id: u8) -> Result<(Team, usize)> {
    match id {
        1..=5 => Ok((Team::Orange, (id - 1) as usize)),
        6..=10 => Ok((Team::Blue, (id - 6) as usize)),
        _ => bail!("Invalid coaching player ID: {id}"),
    }
}

fn canonical_tactic(label: &str) -> Result<Tactic> {
    // v2 wire labels are deliberately stricter than the human-friendly game parser.
    ensure!(
        matches!(
            label,
            "balanced"
                | "highpress"
                | "gegenpress"
                | "lowblock"
                | "parkthebus"
                | "counterattack"
                | "possession"
                | "wingplay"
                | "narrowmidblock"
                | "alloutattack"
        ),
        "Unknown tactic label: {label}"
    );
    Tactic::from_name(label).context("Unsupported game tactic")
}

impl GameHandoff {
    pub fn from_output(output: &Value, session_id: &str) -> Result<Self> {
        ensure!(
            output["session"]["id"].as_str() == Some(session_id),
            "Interpretation belongs to another session"
        );
        let selection: Selection = serde_json::from_value(output["rlSelection"].clone())
            .context("Invalid tactical selection")?;
        ensure!(
            selection.session_id == session_id,
            "Selection belongs to another session"
        );
        let legacy = match (
            output["schemaVersion"].as_str(),
            selection.schema_version.as_str(),
        ) {
            (Some("1.0"), "1.0") => true,
            (Some("2.0"), "2.0") => false,
            _ => bail!("Unsupported tactical output version"),
        };
        let expected_taxonomy = if legacy { "tactics-v1" } else { "tactics-v2" };
        ensure!(
            selection.taxonomy_version == expected_taxonomy
                && output["taxonomyVersion"].as_str() == Some(expected_taxonomy),
            "Unsupported tactic taxonomy"
        );
        let tactic = if legacy {
            let (primary, tactic) = match selection.downstream_value.as_str() {
                "Balanced" => ("balanced", Tactic::Balanced),
                "HighPress" => ("high_press", Tactic::HighPress),
                "LowBlock" => ("low_block", Tactic::LowBlock),
                "Wide" => ("wide", Tactic::WingPlay),
                other => bail!("Unknown legacy tactic: {other}"),
            };
            ensure!(
                selection.primary_tactic == primary,
                "Inconsistent legacy tactic selection"
            );
            tactic
        } else {
            ensure!(
                selection.team_id.as_deref() == Some("red"),
                "Only red can be coached in this version"
            );
            ensure!(
                selection.primary_tactic == selection.downstream_value,
                "Inconsistent tactic selection"
            );
            canonical_tactic(&selection.downstream_value)?
        };
        let mut seen = HashSet::new();
        let mut overrides = Vec::new();
        if !legacy {
            for entry in selection
                .player_overrides
                .context("Missing playerOverrides")?
            {
                ensure!(
                    (1..=5).contains(&entry.player_id),
                    "Overrides must refer to red players 1–5"
                );
                ensure!(seen.insert(entry.player_id), "Duplicate player override");
                ensure!(
                    !entry.evidence.event_ids.is_empty()
                        || !entry.evidence.transcript_segment_ids.is_empty(),
                    "Player override has no evidence"
                );
                overrides.push((entry.player_id, canonical_tactic(&entry.tactic)?));
            }
        }
        overrides.sort_by_key(|(id, _)| *id);
        Ok(Self {
            session_id: session_id.to_owned(),
            tactic,
            overrides,
        })
    }

    pub fn team_tactics(&self) -> TeamTactics {
        let mut orange = TeamDirective::uniform(self.tactic.params());
        for &(id, tactic) in &self.overrides {
            orange.set_player((id - 1) as usize, tactic.params());
        }
        TeamTactics {
            orange,
            blue: TeamDirective::default(),
        }
    }

    pub fn summary(&self) -> String {
        let mut text = format!(
            "Red / Orange: {}  |  Yellow / Blue: Balanced",
            self.tactic.name()
        );
        for (id, tactic) in &self.overrides {
            text.push_str(&format!("\nPlayer {id}: {}", tactic.name()));
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn output(label: &str) -> Value {
        json!({"schemaVersion":"2.0", "taxonomyVersion":"tactics-v2", "session":{"id":"s"},
            "rlSelection":{"schemaVersion":"2.0", "taxonomyVersion":"tactics-v2", "sessionId":"s",
            "teamId":"red", "primaryTactic":label, "downstreamValue":label, "playerOverrides":[]}})
    }

    #[test]
    fn all_presets_reach_the_controller() {
        for label in [
            "balanced",
            "highpress",
            "gegenpress",
            "lowblock",
            "parkthebus",
            "counterattack",
            "possession",
            "wingplay",
            "narrowmidblock",
            "alloutattack",
        ] {
            let handoff = GameHandoff::from_output(&output(label), "s").unwrap();
            assert_eq!(
                handoff.team_tactics().orange.base_params(),
                Tactic::from_name(label).unwrap().params()
            );
            assert_eq!(
                handoff.team_tactics().blue.base_params(),
                Tactic::Balanced.params()
            );
        }
    }

    #[test]
    fn fixed_roster_maps_without_collision() {
        for id in 1..=10 {
            let (team, index) = game_identity(id).unwrap();
            assert_eq!(
                cube_soccer::game::agent_flat_index(team, index),
                (id - 1) as usize
            );
        }
        assert!(game_identity(0).is_err());
        assert!(game_identity(11).is_err());
    }

    #[test]
    fn overrides_apply_only_to_the_selected_player() {
        let mut payload = output("highpress");
        payload["rlSelection"]["playerOverrides"] = json!([{"playerId":2,"tactic":"lowblock","evidence":{"eventIds":["e"],"transcriptSegmentIds":[]}}]);
        let tactics = GameHandoff::from_output(&payload, "s")
            .unwrap()
            .team_tactics();
        assert_eq!(tactics.orange.params_for(1), Tactic::LowBlock.params());
        assert_eq!(tactics.orange.params_for(0), Tactic::HighPress.params());
        payload["rlSelection"]["playerOverrides"][0]["playerId"] = json!(6);
        assert!(GameHandoff::from_output(&payload, "s").is_err());
    }

    #[test]
    fn legacy_wide_maps_to_wing_play() {
        let mut payload = output("wide");
        payload["schemaVersion"] = json!("1.0");
        payload["taxonomyVersion"] = json!("tactics-v1");
        payload["rlSelection"]["schemaVersion"] = json!("1.0");
        payload["rlSelection"]["taxonomyVersion"] = json!("tactics-v1");
        payload["rlSelection"]["downstreamValue"] = json!("Wide");
        assert_eq!(
            GameHandoff::from_output(&payload, "s").unwrap().tactic,
            Tactic::WingPlay
        );
    }

    #[test]
    fn rejects_wrong_session_unknown_label_and_version() {
        assert!(GameHandoff::from_output(&output("balanced"), "another").is_err());
        assert!(GameHandoff::from_output(&output("invented"), "s").is_err());
        let mut payload = output("balanced");
        payload["schemaVersion"] = json!("99");
        assert!(GameHandoff::from_output(&payload, "s").is_err());
    }
}
