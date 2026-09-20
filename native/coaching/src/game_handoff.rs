//! Validated boundary between the external coaching JSON and the game controller.
use anyhow::{bail, ensure, Context, Result};
use bevy::prelude::*;
use cube_soccer::game::Team;
use cube_soccer::systems::heuristic_ai::{Tactic, TeamDirective, TeamTactics};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

/// A single team's coached directive, parsed and grounded from that team's own
/// tactical output. Either side (red or yellow) produces one of these
/// independently — merging two of them into a match is `MatchHandoff`'s job.
#[derive(Resource, Clone, Debug)]
pub struct CoachedTeam {
    pub session_id: String,
    pub team_id: TeamSide,
    pub tactic: Tactic,
    pub overrides: Vec<(u8, Tactic)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamSide {
    Red,
    Yellow,
}

impl TeamSide {
    fn wire_label(self) -> &'static str {
        match self {
            TeamSide::Red => "red",
            TeamSide::Yellow => "yellow",
        }
    }

    fn roster(self) -> std::ops::RangeInclusive<u8> {
        match self {
            TeamSide::Red => 1..=5,
            TeamSide::Yellow => 6..=10,
        }
    }

    fn game_team(self) -> Team {
        match self {
            TeamSide::Red => Team::Orange,
            TeamSide::Yellow => Team::Blue,
        }
    }
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

impl CoachedTeam {
    /// Parses and grounds a tactical output for a specific expected team
    /// (`expected_side`). Rejects output coached for the other team.
    pub fn from_output_for_team(
        output: &Value,
        session_id: &str,
        expected_side: TeamSide,
    ) -> Result<Self> {
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
        // Legacy (v1) payloads predate multi-team coaching and are always red.
        ensure!(
            !legacy || expected_side == TeamSide::Red,
            "Legacy tactical output can only be applied to red"
        );
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
                selection.team_id.as_deref() == Some(expected_side.wire_label()),
                "Expected a {} tactical selection",
                expected_side.wire_label()
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
            let roster = expected_side.roster();
            for entry in selection
                .player_overrides
                .context("Missing playerOverrides")?
            {
                ensure!(
                    roster.contains(&entry.player_id),
                    "Overrides must refer to {} players {}-{}",
                    expected_side.wire_label(),
                    roster.start(),
                    roster.end()
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
            team_id: expected_side,
            tactic,
            overrides,
        })
    }

    /// A team that was never coached — drives the controller with a neutral default.
    pub fn balanced_default(session_id: &str, team_id: TeamSide) -> Self {
        Self {
            session_id: session_id.to_owned(),
            team_id,
            tactic: Tactic::Balanced,
            overrides: Vec::new(),
        }
    }

    pub fn directive(&self) -> TeamDirective {
        let mut directive = TeamDirective::uniform(self.tactic.params());
        let roster_start = *self.team_id.roster().start();
        for &(id, tactic) in &self.overrides {
            directive.set_player((id - roster_start) as usize, tactic.params());
        }
        directive
    }

    pub fn summary_line(&self) -> String {
        let label = match self.team_id {
            TeamSide::Red => "Red / Orange",
            TeamSide::Yellow => "Yellow / Blue",
        };
        let mut text = format!("{label}: {}", self.tactic.name());
        for (id, tactic) in &self.overrides {
            text.push_str(&format!("\nPlayer {id}: {}", tactic.name()));
        }
        text
    }
}

/// Both teams' coached directives, merged and ready to drive `apply_heuristic_ai`.
#[derive(Resource, Clone, Debug)]
pub struct MatchHandoff {
    pub red: CoachedTeam,
    pub yellow: CoachedTeam,
}

impl MatchHandoff {
    pub fn team_tactics(&self) -> TeamTactics {
        debug_assert_eq!(self.red.team_id.game_team(), Team::Orange);
        debug_assert_eq!(self.yellow.team_id.game_team(), Team::Blue);
        TeamTactics {
            orange: self.red.directive(),
            blue: self.yellow.directive(),
        }
    }

    pub fn summary(&self) -> String {
        format!("{}\n{}", self.red.summary_line(), self.yellow.summary_line())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn output(team: &str, label: &str, session_id: &str) -> Value {
        json!({"schemaVersion":"2.0", "taxonomyVersion":"tactics-v2", "session":{"id":session_id},
            "rlSelection":{"schemaVersion":"2.0", "taxonomyVersion":"tactics-v2", "sessionId":session_id,
            "teamId":team, "primaryTactic":label, "downstreamValue":label, "playerOverrides":[]}})
    }

    #[test]
    fn all_presets_reach_the_controller_for_red() {
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
            let red =
                CoachedTeam::from_output_for_team(&output("red", label, "s"), "s", TeamSide::Red)
                    .unwrap();
            let handoff = MatchHandoff {
                red,
                yellow: CoachedTeam::balanced_default("s", TeamSide::Yellow),
            };
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
    fn yellow_team_reaches_the_blue_controller() {
        let yellow = CoachedTeam::from_output_for_team(
            &output("yellow", "lowblock", "s"),
            "s",
            TeamSide::Yellow,
        )
        .unwrap();
        let handoff = MatchHandoff {
            red: CoachedTeam::balanced_default("s", TeamSide::Red),
            yellow,
        };
        assert_eq!(
            handoff.team_tactics().blue.base_params(),
            Tactic::LowBlock.params()
        );
        assert_eq!(
            handoff.team_tactics().orange.base_params(),
            Tactic::Balanced.params()
        );
    }

    #[test]
    fn rejects_output_for_the_wrong_team() {
        assert!(CoachedTeam::from_output_for_team(
            &output("yellow", "balanced", "s"),
            "s",
            TeamSide::Red
        )
        .is_err());
        assert!(CoachedTeam::from_output_for_team(
            &output("red", "balanced", "s"),
            "s",
            TeamSide::Yellow
        )
        .is_err());
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
        let mut payload = output("red", "highpress", "s");
        payload["rlSelection"]["playerOverrides"] = json!([{"playerId":2,"tactic":"lowblock","evidence":{"eventIds":["e"],"transcriptSegmentIds":[]}}]);
        let red = CoachedTeam::from_output_for_team(&payload, "s", TeamSide::Red).unwrap();
        let directive = red.directive();
        assert_eq!(directive.params_for(1), Tactic::LowBlock.params());
        assert_eq!(directive.params_for(0), Tactic::HighPress.params());
        payload["rlSelection"]["playerOverrides"][0]["playerId"] = json!(6);
        assert!(CoachedTeam::from_output_for_team(&payload, "s", TeamSide::Red).is_err());
    }

    #[test]
    fn yellow_overrides_apply_only_to_yellow_roster() {
        let mut payload = output("yellow", "highpress", "s");
        payload["rlSelection"]["playerOverrides"] = json!([{"playerId":7,"tactic":"lowblock","evidence":{"eventIds":["e"],"transcriptSegmentIds":[]}}]);
        let yellow = CoachedTeam::from_output_for_team(&payload, "s", TeamSide::Yellow).unwrap();
        let directive = yellow.directive();
        assert_eq!(directive.params_for(1), Tactic::LowBlock.params());
        assert_eq!(directive.params_for(0), Tactic::HighPress.params());
        payload["rlSelection"]["playerOverrides"][0]["playerId"] = json!(2);
        assert!(CoachedTeam::from_output_for_team(&payload, "s", TeamSide::Yellow).is_err());
    }

    #[test]
    fn legacy_wide_maps_to_wing_play() {
        let mut payload = output("red", "wide", "s");
        payload["schemaVersion"] = json!("1.0");
        payload["taxonomyVersion"] = json!("tactics-v1");
        payload["rlSelection"]["schemaVersion"] = json!("1.0");
        payload["rlSelection"]["taxonomyVersion"] = json!("tactics-v1");
        payload["rlSelection"]["downstreamValue"] = json!("Wide");
        assert_eq!(
            CoachedTeam::from_output_for_team(&payload, "s", TeamSide::Red)
                .unwrap()
                .tactic,
            Tactic::WingPlay
        );
    }

    #[test]
    fn legacy_output_cannot_be_applied_to_yellow() {
        let mut payload = output("red", "wide", "s");
        payload["schemaVersion"] = json!("1.0");
        payload["taxonomyVersion"] = json!("tactics-v1");
        payload["rlSelection"]["schemaVersion"] = json!("1.0");
        payload["rlSelection"]["taxonomyVersion"] = json!("tactics-v1");
        payload["rlSelection"]["downstreamValue"] = json!("Wide");
        assert!(CoachedTeam::from_output_for_team(&payload, "s", TeamSide::Yellow).is_err());
    }

    #[test]
    fn rejects_wrong_session_unknown_label_and_version() {
        assert!(CoachedTeam::from_output_for_team(
            &output("red", "balanced", "s"),
            "another",
            TeamSide::Red
        )
        .is_err());
        assert!(CoachedTeam::from_output_for_team(
            &output("red", "invented", "s"),
            "s",
            TeamSide::Red
        )
        .is_err());
        let mut payload = output("red", "balanced", "s");
        payload["schemaVersion"] = json!("99");
        assert!(CoachedTeam::from_output_for_team(&payload, "s", TeamSide::Red).is_err());
    }
}
