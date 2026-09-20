use crate::model::{AnnotationKind, EntityRef, RawSessionEvent, Session, SessionStatus, TeamId};
use crate::persistence::save_artifacts;
use crate::replay::{effective_events, replay_session};
use crate::session::CoachingSession;
use bevy::prelude::*;
use crossbeam_channel::{unbounded, Receiver, Sender};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpretationState {
    Idle,
    Generating,
    Ready,
}

#[derive(Resource, Debug, Clone)]
pub struct TacticalResult {
    pub state: InterpretationState,
    pub output: Option<Value>,
    pub notice: Option<String>,
    pub session_path: Option<PathBuf>,
    pub tactics_path: Option<PathBuf>,
    requested_revision: Option<u64>,
}

impl Default for TacticalResult {
    fn default() -> Self {
        Self {
            state: InterpretationState::Idle,
            output: None,
            notice: None,
            session_path: None,
            tactics_path: None,
            requested_revision: None,
        }
    }
}

#[derive(Event)]
pub struct RequestInterpretation;

struct InterpretationReply {
    generation: u64,
    revision: u64,
    session_id: String,
    output: Value,
    notice: Option<String>,
}

#[derive(Resource)]
pub struct InterpretationRuntime {
    sender: Sender<InterpretationReply>,
    receiver: Receiver<InterpretationReply>,
}

impl Default for InterpretationRuntime {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        Self { sender, receiver }
    }
}

pub fn request_interpretation(
    mut requests: EventReader<RequestInterpretation>,
    session: Res<CoachingSession>,
    mut result: ResMut<TacticalResult>,
    runtime: Res<InterpretationRuntime>,
) {
    if requests.read().next().is_none()
        || session.session.status == SessionStatus::Recording
        || result.state == InterpretationState::Generating
    {
        return;
    }
    result.state = InterpretationState::Generating;
    result.output = None;
    result.notice = None;
    result.requested_revision = Some(session.revision);
    let generation = session.generation;
    let revision = session.revision;
    let snapshot = session.session.clone();
    let sender = runtime.sender.clone();
    drop(std::thread::spawn(move || {
        let endpoint = std::env::var("TACTIC_LAB_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8787".to_owned());
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(25))
            .build()
            .and_then(|client| {
                client
                    .post(format!("{endpoint}/api/interpret"))
                    .json(&json!({ "schemaVersion": "1.0", "session": snapshot }))
                    .send()?
                    .error_for_status()?
                    .json::<Value>()
            });
        let (output, notice) = match response {
            Ok(output) => (output, None),
            Err(error) => (
                deterministic_interpretation(&snapshot, "deterministic-fallback"),
                Some(format!(
                    "The model service was unavailable ({error}); using deterministic fallback."
                )),
            ),
        };
        let _ = sender.send(InterpretationReply {
            generation,
            revision,
            session_id: snapshot.id,
            output,
            notice,
        });
    }));
}

pub fn receive_interpretation(
    mut session: ResMut<CoachingSession>,
    mut result: ResMut<TacticalResult>,
    runtime: Res<InterpretationRuntime>,
) {
    while let Ok(reply) = runtime.receiver.try_recv() {
        if reply.generation != session.generation
            || reply.revision != session.revision
            || reply.session_id != session.session.id
        {
            continue;
        }
        result.notice = reply.notice;
        result.output = Some(reply.output);
        result.state = InterpretationState::Ready;
        result.requested_revision = None;
        session.session.status = SessionStatus::Interpreted;
        session.playhead_ms = session.session.elapsed_ms;
        if let Some(output) = &result.output {
            match save_artifacts(&session.session, output) {
                Ok((session_path, tactics_path)) => {
                    result.session_path = Some(session_path);
                    result.tactics_path = Some(tactics_path);
                }
                Err(error) => {
                    result.notice = Some(format!("Output generated, but saving failed: {error:#}"));
                }
            }
        }
    }
}

pub fn invalidate_stale_result(session: Res<CoachingSession>, mut result: ResMut<TacticalResult>) {
    if (session.session.status != SessionStatus::Interpreted
        && result.state == InterpretationState::Ready)
        || (result.state == InterpretationState::Generating
            && result.requested_revision != Some(session.revision))
    {
        *result = TacticalResult::default();
    }
}

pub fn deterministic_interpretation(session: &Session, mode: &str) -> Value {
    let replay = replay_session(&session.events, None);
    let effective = effective_events(&session.events);
    let board_actions = effective
        .iter()
        .copied()
        .filter(|event| {
            matches!(
                event,
                RawSessionEvent::EntityMoved { .. } | RawSessionEvent::AnnotationAdded { .. }
            )
        })
        .collect::<Vec<_>>();
    let transcript_ranges = if replay.transcripts.is_empty() {
        vec![(None, 0, session.elapsed_ms)]
    } else {
        replay
            .transcripts
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                (
                    Some(segment),
                    if index == 0 { 0 } else { segment.start_ms },
                    replay
                        .transcripts
                        .get(index + 1)
                        .map(|next| next.start_ms)
                        .unwrap_or(session.elapsed_ms),
                )
            })
            .collect()
    };

    let steps = transcript_ranges
        .into_iter()
        .enumerate()
        .map(|(index, (segment, start_ms, end_ms))| {
            let actions = board_actions
                .iter()
                .copied()
                .filter(|event| {
                    event.timestamp_ms() >= start_ms && event.timestamp_ms() <= end_ms
                })
                .collect::<Vec<_>>();
            let actors = actions
                .iter()
                .filter_map(|event| match event {
                    RawSessionEvent::EntityMoved {
                        entity: EntityRef::Player { id },
                        ..
                    } => Some(*id),
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>();
            let movements = actions
                .iter()
                .filter_map(|event| match event {
                    RawSessionEvent::EntityMoved {
                        entity,
                        from,
                        to,
                        path,
                        ..
                    } => Some(json!({
                        "entityType": match entity { EntityRef::Player { .. } => "player", EntityRef::Ball => "ball" },
                        "entityId": match entity { EntityRef::Player { id } => json!(id), EntityRef::Ball => json!("ball") },
                        "from": from,
                        "to": to,
                        "path": path,
                    })),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let annotation_ids = actions
                .iter()
                .filter_map(|event| match event {
                    RawSessionEvent::AnnotationAdded { annotation, .. } => {
                        Some(annotation.id.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            json!({
                "id": format!("step-{}", index + 1),
                "startMs": start_ms,
                "endMs": end_ms.max(start_ms),
                "primaryTactic": "balanced",
                "instruction": segment.map(|item| item.text.as_str()).unwrap_or("Demonstrated movement sequence."),
                "objective": "Preserve the demonstrated sequence without adding model-derived meaning.",
                "actors": actors,
                "movements": movements,
                "annotationIds": annotation_ids,
                "evidence": {
                    "eventIds": actions.iter().map(|event| event.id()).collect::<Vec<_>>(),
                    "transcriptSegmentIds": segment.map(|item| vec![item.id.as_str()]).unwrap_or_default(),
                }
            })
        })
        .collect::<Vec<_>>();

    json!({
        "schemaVersion": "2.0",
        "taxonomyVersion": "tactics-v2",
        "interpretationMode": mode,
        "session": {
            "id": session.id,
            "title": session.title,
            "durationMs": session.elapsed_ms,
        },
        "teams": [
            { "id": "red", "playerIds": [1, 2, 3, 4, 5] },
            { "id": "yellow", "playerIds": [6, 7, 8, 9, 10] },
        ],
        "classification": {
            "primaryTactic": "balanced",
            "secondaryTraits": [],
            "alternativeTactics": [],
            "selectionReason": "system_fallback",
            "evidenceStrength": "weak",
            "explanation": "Offline deterministic output; no model-backed tactic classification was performed.",
        },
        "summary": {
            "name": session.title,
            "objective": "Coordinate a 5-v-5 build-up pattern using timed player and ball movements.",
        },
        "steps": steps,
        "finalState": {
            "players": replay.board.players.iter().map(|player| json!({
                "id": player.id,
                "team": match player.team { TeamId::Red => "red", TeamId::Yellow => "yellow" },
                "position": player.position,
            })).collect::<Vec<_>>(),
            "ball": replay.board.ball,
            "annotations": replay.board.annotations.iter().map(|annotation| json!({
                "id": annotation.id,
                "kind": match annotation.kind { AnnotationKind::Arrow => "arrow", AnnotationKind::Freehand => "freehand" },
                "points": annotation.points,
            })).collect::<Vec<_>>(),
        },
        "rlSelection": {
            "schemaVersion": "2.0",
            "taxonomyVersion": "tactics-v2",
            "sessionId": session.id,
            "primaryTactic": "balanced",
            "downstreamValue": "balanced",
            "teamId": "red",
            "playerOverrides": [],
            "selectionReason": "system_fallback",
            "evidenceStrength": "weak",
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Point, TranscriptSegment, TranscriptSource};

    #[test]
    fn deterministic_payload_preserves_contract() {
        let session = Session {
            elapsed_ms: 3_000,
            events: vec![
                RawSessionEvent::EntityMoved {
                    id: "move-1".into(),
                    timestamp_ms: 1_200,
                    entity: EntityRef::Ball,
                    started_at_ms: 800,
                    from: Point { x: 0.5, y: 0.51 },
                    to: Point { x: 0.6, y: 0.4 },
                    path: vec![Point { x: 0.5, y: 0.51 }, Point { x: 0.6, y: 0.4 }],
                },
                RawSessionEvent::TranscriptAdded {
                    id: "transcript-1".into(),
                    timestamp_ms: 1_000,
                    segment: TranscriptSegment {
                        id: "segment-1".into(),
                        start_ms: 1_000,
                        end_ms: 2_000,
                        text: "Play into the middle.".into(),
                        source: TranscriptSource::Manual,
                    },
                },
            ],
            ..default()
        };
        let output = deterministic_interpretation(&session, "deterministic-preview");
        assert_eq!(output["steps"][0]["movements"][0]["entityId"], "ball");
        assert_eq!(output["schemaVersion"], "2.0");
    }
}
