import { initialBoard } from "./session";
import type {
  Annotation,
  BoardState,
  RawSessionEvent,
  ReplayState,
  TranscriptSegment,
} from "./types";

const actionTypes = new Set([
  "entity_moved",
  "annotation_added",
  "annotation_removed",
]);

export function effectiveEvents(events: RawSessionEvent[]): RawSessionEvent[] {
  const undone = new Set(
    events.filter((event) => event.type === "undo").map((event) => event.targetEventId),
  );
  return events.filter((event) => event.type === "undo" || !undone.has(event.id));
}

export function findUndoTarget(events: RawSessionEvent[]): RawSessionEvent | undefined {
  const effective = effectiveEvents(events);
  const alreadyUndone = new Set(
    events.filter((event) => event.type === "undo").map((event) => event.targetEventId),
  );
  return [...effective]
    .reverse()
    .find((event) => actionTypes.has(event.type) && !alreadyUndone.has(event.id));
}

export function replaySession(events: RawSessionEvent[], untilMs = Infinity): ReplayState {
  const board: BoardState = {
    players: initialBoard.players.map((player) => ({ ...player, position: { ...player.position } })),
    ball: { ...initialBoard.ball },
    annotations: [],
  };
  const transcripts = new Map<string, TranscriptSegment>();
  const scoped = events.filter((event) => event.timestampMs <= untilMs);
  const effective = effectiveEvents(scoped);

  for (const event of effective) {
    switch (event.type) {
      case "entity_moved":
        if (event.entity.kind === "ball") {
          board.ball = { ...event.to };
        } else {
          const playerId = event.entity.id;
          board.players = board.players.map((player) =>
            player.id === playerId ? { ...player, position: { ...event.to } } : player,
          );
        }
        break;
      case "annotation_added":
        board.annotations = [...board.annotations, cloneAnnotation(event.annotation)];
        break;
      case "annotation_removed":
        board.annotations = board.annotations.filter(
          (annotation) => annotation.id !== event.annotationId,
        );
        break;
      case "transcript_added":
        transcripts.set(event.segment.id, { ...event.segment });
        break;
      case "transcript_edited": {
        const previous = transcripts.get(event.segmentId);
        if (previous) transcripts.set(event.segmentId, { ...previous, text: event.text });
        break;
      }
      default:
        break;
    }
  }

  return {
    board,
    transcripts: [...transcripts.values()].sort((a, b) => a.startMs - b.startMs),
    effectiveEvents: effective,
  };
}

function cloneAnnotation(annotation: Annotation): Annotation {
  return { ...annotation, points: annotation.points.map((point) => ({ ...point })) };
}
