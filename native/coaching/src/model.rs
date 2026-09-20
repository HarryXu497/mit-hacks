use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn clamped(self) -> Self {
        Self {
            x: self.x.clamp(0.0, 1.0),
            y: self.y.clamp(0.0, 1.0),
        }
    }

    pub fn distance(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }

    pub fn is_normalized(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && (0.0..=1.0).contains(&self.x)
            && (0.0..=1.0).contains(&self.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamId {
    Red,
    Yellow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum EntityRef {
    Player { id: u8 },
    Ball,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub id: u8,
    pub team: TeamId,
    pub position: Point,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: String,
    pub kind: AnnotationKind,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationKind {
    Arrow,
    Freehand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardState {
    pub players: Vec<PlayerState>,
    pub ball: Point,
    pub annotations: Vec<Annotation>,
}

impl Default for BoardState {
    fn default() -> Self {
        Self {
            players: vec![
                player(1, TeamId::Red, 0.50, 0.08),
                player(2, TeamId::Red, 0.30, 0.23),
                player(3, TeamId::Red, 0.70, 0.23),
                player(4, TeamId::Red, 0.28, 0.40),
                player(5, TeamId::Red, 0.72, 0.40),
                player(6, TeamId::Yellow, 0.28, 0.60),
                player(7, TeamId::Yellow, 0.72, 0.60),
                player(8, TeamId::Yellow, 0.30, 0.77),
                player(9, TeamId::Yellow, 0.70, 0.77),
                player(10, TeamId::Yellow, 0.50, 0.92),
            ],
            ball: Point { x: 0.5, y: 0.51 },
            annotations: Vec::new(),
        }
    }
}

fn player(id: u8, team: TeamId, x: f32, y: f32) -> PlayerState {
    PlayerState {
        id,
        team,
        position: Point { x, y },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Ready,
    Recording,
    Review,
    Interpreted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    Select,
    #[default]
    Arrow,
    Draw,
    Erase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptSource {
    Speech,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub source: TranscriptSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RawSessionEvent {
    RecordingStarted {
        id: String,
        timestamp_ms: u64,
    },
    RecordingResumed {
        id: String,
        timestamp_ms: u64,
    },
    RecordingStopped {
        id: String,
        timestamp_ms: u64,
    },
    EntityMoved {
        id: String,
        timestamp_ms: u64,
        entity: EntityRef,
        started_at_ms: u64,
        from: Point,
        to: Point,
        path: Vec<Point>,
    },
    AnnotationAdded {
        id: String,
        timestamp_ms: u64,
        annotation: Annotation,
        started_at_ms: u64,
    },
    AnnotationRemoved {
        id: String,
        timestamp_ms: u64,
        annotation_id: String,
    },
    Undo {
        id: String,
        timestamp_ms: u64,
        target_event_id: String,
    },
    TranscriptAdded {
        id: String,
        timestamp_ms: u64,
        segment: TranscriptSegment,
    },
    TranscriptEdited {
        id: String,
        timestamp_ms: u64,
        segment_id: String,
        text: String,
    },
}

impl RawSessionEvent {
    pub fn id(&self) -> &str {
        match self {
            Self::RecordingStarted { id, .. }
            | Self::RecordingResumed { id, .. }
            | Self::RecordingStopped { id, .. }
            | Self::EntityMoved { id, .. }
            | Self::AnnotationAdded { id, .. }
            | Self::AnnotationRemoved { id, .. }
            | Self::Undo { id, .. }
            | Self::TranscriptAdded { id, .. }
            | Self::TranscriptEdited { id, .. } => id,
        }
    }

    pub fn timestamp_ms(&self) -> u64 {
        match self {
            Self::RecordingStarted { timestamp_ms, .. }
            | Self::RecordingResumed { timestamp_ms, .. }
            | Self::RecordingStopped { timestamp_ms, .. }
            | Self::EntityMoved { timestamp_ms, .. }
            | Self::AnnotationAdded { timestamp_ms, .. }
            | Self::AnnotationRemoved { timestamp_ms, .. }
            | Self::Undo { timestamp_ms, .. }
            | Self::TranscriptAdded { timestamp_ms, .. }
            | Self::TranscriptEdited { timestamp_ms, .. } => *timestamp_ms,
        }
    }

    pub fn is_undoable_action(&self) -> bool {
        matches!(
            self,
            Self::EntityMoved { .. }
                | Self::AnnotationAdded { .. }
                | Self::AnnotationRemoved { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub schema_version: u8,
    pub id: String,
    pub title: String,
    pub status: SessionStatus,
    pub elapsed_ms: u64,
    pub events: Vec<RawSessionEvent>,
    pub created_at: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            schema_version: 1,
            id: create_id("session"),
            title: "Build-up pattern".to_owned(),
            status: SessionStatus::Ready,
            elapsed_ms: 0,
            events: Vec::new(),
            created_at: iso_timestamp(),
        }
    }
}

impl Session {
    pub fn validate_contract(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported session schema version".into());
        }
        for event in &self.events {
            match event {
                RawSessionEvent::EntityMoved {
                    entity,
                    from,
                    to,
                    path,
                    ..
                } => {
                    if matches!(entity, EntityRef::Player { id } if !(1..=10).contains(id)) {
                        return Err("player IDs must be between 1 and 10".into());
                    }
                    if !from.is_normalized()
                        || !to.is_normalized()
                        || path.iter().any(|point| !point.is_normalized())
                    {
                        return Err("movement coordinates must be normalized".into());
                    }
                }
                RawSessionEvent::AnnotationAdded { annotation, .. }
                    if annotation.points.iter().any(|point| !point.is_normalized()) =>
                {
                    return Err("annotation coordinates must be normalized".into());
                }
                _ => {}
            }
        }
        Ok(())
    }
}

pub fn create_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

fn iso_timestamp() -> String {
    humantime::format_rfc3339(SystemTime::now()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_is_clamped_to_normalized_field() {
        assert_eq!(
            Point { x: -0.2, y: 1.4 }.clamped(),
            Point { x: 0.0, y: 1.0 }
        );
    }

    #[test]
    fn session_json_uses_existing_camel_case_contract() {
        let json = serde_json::to_value(Session::default()).unwrap();
        assert_eq!(json["schemaVersion"], 1);
        assert!(json.get("elapsedMs").is_some());
    }

    #[test]
    fn shared_typescript_fixture_round_trips() {
        let fixture = include_str!("../../../fixtures/session-v1.json");
        let session: Session = serde_json::from_str(fixture).unwrap();
        assert_eq!(session.id, "session-fixture");
        assert_eq!(session.events.len(), 6);
        let encoded = serde_json::to_value(session).unwrap();
        assert_eq!(encoded["events"][1]["type"], "entity_moved");
        assert_eq!(encoded["events"][3]["segment"]["startMs"], 1_000);
    }

    #[test]
    fn contract_validation_rejects_out_of_range_values() {
        let session = Session {
            events: vec![RawSessionEvent::EntityMoved {
                id: "move".into(),
                timestamp_ms: 0,
                entity: EntityRef::Player { id: 11 },
                started_at_ms: 0,
                from: Point { x: 0.0, y: 0.0 },
                to: Point { x: 2.0, y: 0.0 },
                path: Vec::new(),
            }],
            ..Session::default()
        };
        assert!(session.validate_contract().is_err());
    }
}
