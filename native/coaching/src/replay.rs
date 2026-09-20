use crate::model::{BoardState, EntityRef, RawSessionEvent, TranscriptSegment};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayState {
    pub board: BoardState,
    pub transcripts: Vec<TranscriptSegment>,
    pub effective_event_ids: Vec<String>,
}

pub fn effective_events(events: &[RawSessionEvent]) -> Vec<&RawSessionEvent> {
    let undone: HashSet<&str> = events
        .iter()
        .filter_map(|event| match event {
            RawSessionEvent::Undo {
                target_event_id, ..
            } => Some(target_event_id.as_str()),
            _ => None,
        })
        .collect();

    events
        .iter()
        .filter(|event| {
            matches!(event, RawSessionEvent::Undo { .. }) || !undone.contains(event.id())
        })
        .collect()
}

pub fn find_undo_target(events: &[RawSessionEvent]) -> Option<&RawSessionEvent> {
    let already_undone: HashSet<&str> = events
        .iter()
        .filter_map(|event| match event {
            RawSessionEvent::Undo {
                target_event_id, ..
            } => Some(target_event_id.as_str()),
            _ => None,
        })
        .collect();

    effective_events(events)
        .into_iter()
        .rev()
        .find(|event| event.is_undoable_action() && !already_undone.contains(event.id()))
}

pub fn replay_session(events: &[RawSessionEvent], until_ms: Option<u64>) -> ReplayState {
    let mut board = BoardState::default();
    let mut transcripts = HashMap::<String, TranscriptSegment>::new();
    let scoped = events
        .iter()
        .filter(|event| until_ms.is_none_or(|limit| event.timestamp_ms() <= limit))
        .cloned()
        .collect::<Vec<_>>();
    let effective = effective_events(&scoped);

    for event in &effective {
        match event {
            RawSessionEvent::EntityMoved { entity, to, .. } => match entity {
                EntityRef::Ball => board.ball = *to,
                EntityRef::Player { id } => {
                    if let Some(player) = board.players.iter_mut().find(|player| player.id == *id) {
                        player.position = *to;
                    }
                }
            },
            RawSessionEvent::AnnotationAdded { annotation, .. } => {
                board.annotations.push(annotation.clone());
            }
            RawSessionEvent::AnnotationRemoved { annotation_id, .. } => {
                board
                    .annotations
                    .retain(|annotation| annotation.id != *annotation_id);
            }
            RawSessionEvent::TranscriptAdded { segment, .. } => {
                transcripts.insert(segment.id.clone(), segment.clone());
            }
            RawSessionEvent::TranscriptEdited {
                segment_id, text, ..
            } => {
                if let Some(segment) = transcripts.get_mut(segment_id) {
                    segment.text.clone_from(text);
                }
            }
            _ => {}
        }
    }

    let mut transcripts = transcripts.into_values().collect::<Vec<_>>();
    transcripts.sort_by_key(|segment| segment.start_ms);
    ReplayState {
        board,
        transcripts,
        effective_event_ids: effective
            .into_iter()
            .map(|event| event.id().to_owned())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Point, RawSessionEvent};

    #[test]
    fn movement_and_immutable_undo_match_typescript() {
        let events = vec![
            RawSessionEvent::EntityMoved {
                id: "move-1".into(),
                timestamp_ms: 1_000,
                entity: EntityRef::Player { id: 3 },
                started_at_ms: 800,
                from: Point { x: 0.7, y: 0.23 },
                to: Point { x: 0.55, y: 0.4 },
                path: vec![Point { x: 0.7, y: 0.23 }, Point { x: 0.55, y: 0.4 }],
            },
            RawSessionEvent::Undo {
                id: "undo-1".into(),
                timestamp_ms: 1_200,
                target_event_id: "move-1".into(),
            },
        ];

        assert_eq!(
            replay_session(&events, None).board.players[2].position,
            Point { x: 0.7, y: 0.23 }
        );
        assert_eq!(
            replay_session(&events, None).effective_event_ids,
            vec!["undo-1"]
        );
    }
}
