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
    // A vector in log order, with an index beside it -- deliberately not a `HashMap` whose
    // values are collected at the end. `HashMap` seeds its iteration order per instance and a
    // fresh one was built on every call, so the stable sort below had a different starting
    // order every frame. Two sentences sharing a timestamp therefore swapped rows on every
    // repaint, which is one of the ways the transcript panel appeared to flicker.
    let mut transcripts: Vec<TranscriptSegment> = Vec::new();
    let mut position = HashMap::<String, usize>::new();
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
                match position.get(&segment.id) {
                    // Re-added under an id already seen: replace in place, so it keeps the row
                    // it already had rather than jumping to the end of the list.
                    Some(at) => transcripts[*at] = segment.clone(),
                    None => {
                        position.insert(segment.id.clone(), transcripts.len());
                        transcripts.push(segment.clone());
                    }
                }
            }
            RawSessionEvent::TranscriptEdited {
                segment_id, text, ..
            } => {
                if let Some(at) = position.get(segment_id) {
                    transcripts[*at].text.clone_from(text);
                }
            }
            _ => {}
        }
    }

    // Stable, over a vector already in log order, so sentences sharing a timestamp keep the
    // order they were said in -- the same order on every call.
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
    use crate::model::{Point, RawSessionEvent, TranscriptSource};

    /// Two sentences on the same millisecond must not swap rows between repaints.
    ///
    /// `drain_transcript` stamps every sentence it folds in with the clock reading it began
    /// on, and two can share one. The transcript used to be accumulated in a `HashMap` and
    /// collected at the end, and a `HashMap` seeds its iteration order per instance -- so the
    /// stable sort started from a different order on every call and the rows visibly swapped.
    /// Looped, because a single call would pass by luck.
    #[test]
    fn sentences_sharing_a_timestamp_keep_the_order_they_were_said_in() {
        let events: Vec<RawSessionEvent> = ["first", "second", "third", "fourth", "fifth"]
            .iter()
            .enumerate()
            .map(|(index, text)| RawSessionEvent::TranscriptAdded {
                id: format!("ev-{index}"),
                timestamp_ms: 4_200,
                segment: TranscriptSegment {
                    id: format!("seg-{index}"),
                    start_ms: 4_200,
                    end_ms: 4_200,
                    text: (*text).to_owned(),
                    source: TranscriptSource::Speech,
                },
            })
            .collect();
        let expected: Vec<String> = ["first", "second", "third", "fourth", "fifth"]
            .iter()
            .map(|text| (*text).to_owned())
            .collect();
        for _ in 0..50 {
            let said: Vec<String> = replay_session(&events, None)
                .transcripts
                .into_iter()
                .map(|segment| segment.text)
                .collect();
            assert_eq!(said, expected, "the rows must not reshuffle between repaints");
        }
    }

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
