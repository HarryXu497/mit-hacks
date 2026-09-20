use crate::game_handoff::TeamSide;
use crate::model::{AnnotationKind, EntityRef, RawSessionEvent, Session, SessionStatus, TeamId};
use crate::network::{MatchReady, NetworkRole};
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
    Failed,
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

#[derive(Event, Clone, Copy)]
pub enum RequestInterpretation {
    Generate,
    ContinueBalanced,
}

struct InterpretationReply {
    generation: u64,
    revision: u64,
    session_id: String,
    outcome: Result<Value, String>,
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
    mut session: ResMut<CoachingSession>,
    mut result: ResMut<TacticalResult>,
    runtime: Res<InterpretationRuntime>,
    role: Res<NetworkRole>,
    mut match_ready: EventWriter<MatchReady>,
) {
    let Some(request) = requests.read().next().copied() else {
        return;
    };
    if session.session.status == SessionStatus::Recording
        || result.state == InterpretationState::Generating
    {
        return;
    }
    if matches!(request, RequestInterpretation::ContinueBalanced) {
        if result.state != InterpretationState::Failed {
            return;
        }
        let diagnostic = result
            .notice
            .clone()
            .unwrap_or_else(|| "Unknown interpretation error".into());
        let team_id = match role.coached_side() {
            TeamSide::Red => "red",
            TeamSide::Yellow => "yellow",
        };
        let output =
            deterministic_interpretation(&session.session, "deterministic-fallback", team_id);
        accept_interpretation(
            &mut session,
            &mut result,
            output,
            Some(format!(
                "You chose to continue with Balanced. Interpretation failure: {diagnostic}"
            )),
        );
        match_ready.send(MatchReady);
        return;
    }
    result.state = InterpretationState::Generating;
    result.output = None;
    result.notice = None;
    result.session_path = None;
    result.tactics_path = None;
    result.requested_revision = Some(session.revision);
    let generation = session.generation;
    let revision = session.revision;
    let snapshot = session.session.clone();
    let sender = runtime.sender.clone();
    let team_side = role.coached_side();
    drop(std::thread::spawn(move || {
        let endpoint = std::env::var("TACTIC_LAB_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8787".to_owned());
        let outcome = fetch_interpretation(&endpoint, &snapshot, team_side);
        let _ = sender.send(InterpretationReply {
            generation,
            revision,
            session_id: snapshot.id,
            outcome,
        });
    }));
}

fn fetch_interpretation(endpoint: &str, session: &Session, team_side: TeamSide) -> Result<Value, String> {
    let team_id = match team_side {
        TeamSide::Red => "red",
        TeamSide::Yellow => "yellow",
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(transport_diagnostic)?;
    let response = client
        .post(format!("{}/api/interpret", endpoint.trim_end_matches('/')))
        .json(&json!({ "schemaVersion": "1.0", "session": session, "teamId": team_id }))
        .send()
        .map_err(transport_diagnostic)?;
    let status = response.status().as_u16();
    // Read the error body before inspecting status: it contains the actionable reason.
    let body = response.text().map_err(transport_diagnostic)?;
    decode_interpretation_response(status, &body, &session.id, team_side)
}

fn transport_diagnostic(error: reqwest::Error) -> String {
    let guidance = if error.is_timeout() {
        "INTERPRETATION_SERVER_TIMEOUT: The local server did not respond within 30 seconds. Check its logs and retry."
    } else if error.is_connect() {
        "INTERPRETATION_SERVER_UNREACHABLE: Cannot connect to the interpretation server. Start npm run dev:api and check TACTIC_LAB_API_URL."
    } else {
        "INTERPRETATION_TRANSPORT_ERROR: The request or response could not be transferred. Check the server logs."
    };
    format!("{guidance} Details: {}", error.without_url())
}

fn decode_interpretation_response(
    status: u16,
    body: &str,
    session_id: &str,
    team_side: TeamSide,
) -> Result<Value, String> {
    let output: Value = serde_json::from_str(body)
        .map_err(|error| format!("INVALID_SERVER_RESPONSE (HTTP {status}): Expected JSON, but could not decode the response: {error}. Check the API server logs."))?;
    if !(200..300).contains(&status) {
        let code = output["code"]
            .as_str()
            .unwrap_or("INTERPRETATION_HTTP_ERROR");
        let message = output["message"]
            .as_str()
            .unwrap_or("The server returned no error explanation; check its logs.");
        let request_id = output["requestId"]
            .as_str()
            .map(|id| format!(" Request ID: {id}"))
            .unwrap_or_default();
        return Err(format!("{code} (HTTP {status}): {message}{request_id}"));
    }
    if output["interpretationMode"].as_str() != Some("model-backed") {
        return Err("NON_MODEL_INTERPRETATION: The server returned a preview or fallback instead of a model interpretation. Restart the updated API server and retry; no fallback tactic was applied.".into());
    }
    if output["rlSelection"]["selectionReason"].as_str() != Some("best_match") {
        return Err("NON_BEST_MATCH_SELECTION: The server did not return a supported best-match interpretation. Restart the updated API server and retry.".into());
    }
    crate::game_handoff::CoachedTeam::from_output_for_team(&output, session_id, team_side)
        .map_err(|error| format!("INVALID_TACTICAL_PAYLOAD: {error:#}"))?;
    Ok(output)
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
        result.requested_revision = None;
        let output = match reply.outcome {
            Ok(output) => output,
            Err(diagnostic) => {
                eprintln!("Interpretation failed: {diagnostic}");
                result.output = None;
                result.state = InterpretationState::Failed;
                result.notice = Some(diagnostic);
                session.session.status = SessionStatus::Review;
                continue;
            }
        };
        let notice = if output["classification"]["evidenceStrength"].as_str() == Some("weak") {
            Some(format!(
                "Low-confidence best match: {}. {}",
                output["rlSelection"]["downstreamValue"]
                    .as_str()
                    .unwrap_or("unknown"),
                output["classification"]["explanation"]
                    .as_str()
                    .unwrap_or("Review the interpretation before starting the game.")
            ))
        } else {
            None
        };
        accept_interpretation(&mut session, &mut result, output, notice);
    }
}

fn accept_interpretation(
    session: &mut CoachingSession,
    result: &mut TacticalResult,
    output: Value,
    notice: Option<String>,
) {
    result.output = Some(output);
    result.state = InterpretationState::Ready;
    result.notice = notice;
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
                let prefix = result.notice.as_deref().unwrap_or("");
                result.notice = Some(format!(
                    "{prefix} Output generated, but saving failed: {error:#}"
                ));
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

pub fn deterministic_interpretation(session: &Session, mode: &str, team_id: &str) -> Value {
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
            "teamId": team_id,
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
    fn http_failure_preserves_server_reason_and_request_id() {
        let error = decode_interpretation_response(429,
            r#"{"code":"OPENAI_RATE_LIMIT_OR_QUOTA","message":"Quota exhausted","requestId":"req-test"}"#, "s", TeamSide::Red).unwrap_err();
        assert!(error.contains("OPENAI_RATE_LIMIT_OR_QUOTA"));
        assert!(error.contains("Quota exhausted"));
        assert!(error.contains("req-test"));
        assert!(error.contains("HTTP 429"));
    }

    #[test]
    fn rejects_invalid_response_and_automatic_server_fallback() {
        assert!(
            decode_interpretation_response(502, "<html>Gateway error</html>", "s", TeamSide::Red)
                .unwrap_err()
                .contains("HTTP 502")
        );
        let session = Session::default();
        let output = deterministic_interpretation(&session, "deterministic-fallback", "red");
        assert!(
            decode_interpretation_response(200, &output.to_string(), &session.id, TeamSide::Red)
                .unwrap_err()
                .contains("NON_MODEL_INTERPRETATION")
        );
    }

    #[test]
    fn failure_waits_for_explicit_continue_and_then_emits_balanced_game_handoff() {
        let mut app = App::new();
        app.init_resource::<CoachingSession>()
            .init_resource::<TacticalResult>()
            .init_resource::<InterpretationRuntime>()
            .init_resource::<NetworkRole>()
            .add_event::<RequestInterpretation>()
            .add_event::<MatchReady>()
            .add_systems(
                Update,
                (receive_interpretation, request_interpretation).chain(),
            );
        let session = app.world.resource::<CoachingSession>();
        let reply = InterpretationReply {
            generation: session.generation,
            revision: session.revision,
            session_id: session.session.id.clone(),
            outcome: Err("OPENAI_AUTH_FAILED: invalid API key".into()),
        };
        app.world
            .resource::<InterpretationRuntime>()
            .sender
            .send(reply)
            .unwrap();
        app.update();
        let result = app.world.resource::<TacticalResult>();
        assert_eq!(result.state, InterpretationState::Failed);
        assert!(result.output.is_none());
        assert!(result
            .notice
            .as_deref()
            .unwrap()
            .contains("invalid API key"));
        assert!(app.world.resource::<Events<MatchReady>>().is_empty());
        app.world
            .send_event(RequestInterpretation::ContinueBalanced);
        app.update();
        let result = app.world.resource::<TacticalResult>();
        assert_eq!(result.state, InterpretationState::Ready);
        assert_eq!(
            result.output.as_ref().unwrap()["rlSelection"]["downstreamValue"],
            "balanced"
        );
        assert!(result
            .notice
            .as_deref()
            .unwrap()
            .contains("You chose to continue"));
        assert!(result
            .notice
            .as_deref()
            .unwrap()
            .contains("invalid API key"));
        assert!(!app.world.resource::<Events<MatchReady>>().is_empty());
    }

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
        let output = deterministic_interpretation(&session, "deterministic-preview", "red");
        assert_eq!(output["steps"][0]["movements"][0]["entityId"], "ball");
        assert_eq!(output["schemaVersion"], "2.0");
        let handoff = crate::game_handoff::CoachedTeam::from_output_for_team(
            &output,
            &session.id,
            crate::game_handoff::TeamSide::Red,
        )
        .unwrap();
        assert_eq!(handoff.tactic, cube_soccer::systems::Tactic::Balanced);
        assert!(handoff.overrides.is_empty());
    }
}
