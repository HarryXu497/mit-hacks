import { tacticalOutputSchema, type TacticalOutput } from "../src/domain/interpret";
import { replaySession } from "../src/domain/replay";
import { initialBoard } from "../src/domain/session";
import {
  downstreamValueFor,
  TACTIC_TAXONOMY,
  TACTIC_TAXONOMY_VERSION,
  type SemanticInterpretation,
} from "../src/domain/tactics";
import type {
  AnnotationAddedEvent,
  MoveEvent,
  RawSessionEvent,
  Session,
  TranscriptSegment,
} from "../src/domain/types";

export type EvidenceEvent = MoveEvent | AnnotationAddedEvent;

export interface InterpretationInput {
  inputSchemaVersion: "1.0";
  session: { id: string; title: string; durationMs: number };
  boardContext: {
    format: "fixed-5v5";
    coordinateSystem: {
      normalized: true;
      x: "0=left, 1=right";
      y: "0=top goal, 1=bottom goal";
    };
    teams: Array<{
      id: "red" | "yellow";
      playerIds: number[];
      defends: "top" | "bottom";
      attacks: "top" | "bottom";
    }>;
  };
  taxonomy: typeof TACTIC_TAXONOMY;
  initialState: typeof initialBoard;
  boardEvents: EvidenceEvent[];
  transcriptSegments: TranscriptSegment[];
}

export function buildInterpretationInput(session: Session): InterpretationInput {
  const replay = replaySession(session.events);
  const activeAnnotationIds = new Set(replay.board.annotations.map((annotation) => annotation.id));
  const boardEvents = replay.effectiveEvents.filter((event): event is EvidenceEvent => {
    if (event.type === "entity_moved") return true;
    return event.type === "annotation_added" && activeAnnotationIds.has(event.annotation.id);
  });

  return {
    inputSchemaVersion: "1.0",
    session: { id: session.id, title: session.title, durationMs: session.elapsedMs },
    boardContext: {
      format: "fixed-5v5",
      coordinateSystem: {
        normalized: true,
        x: "0=left, 1=right",
        y: "0=top goal, 1=bottom goal",
      },
      teams: [
        { id: "red", playerIds: [1, 2, 3, 4, 5], defends: "top", attacks: "bottom" },
        { id: "yellow", playerIds: [6, 7, 8, 9, 10], defends: "bottom", attacks: "top" },
      ],
    },
    taxonomy: TACTIC_TAXONOMY,
    initialState: initialBoard,
    boardEvents,
    transcriptSegments: replay.transcripts,
  };
}

export function assembleTacticalOutput(
  session: Session,
  input: InterpretationInput,
  semantic: SemanticInterpretation,
): TacticalOutput {
  const events = new Map(input.boardEvents.map((event) => [event.id, event]));
  const transcripts = new Map(input.transcriptSegments.map((segment) => [segment.id, segment]));
  const steps = semantic.phases.map((phase, index) => {
    const citedEvents = phase.evidence.eventIds.map((id) => {
      const event = events.get(id);
      if (!event) throw new GroundingError(`Unknown event evidence ID: ${id}`);
      return event;
    });
    const citedTranscripts = phase.evidence.transcriptSegmentIds.map((id) => {
      const segment = transcripts.get(id);
      if (!segment) throw new GroundingError(`Unknown transcript evidence ID: ${id}`);
      return segment;
    });
    if (citedEvents.length === 0 && citedTranscripts.length === 0) {
      throw new GroundingError(`Phase ${phase.id} has no evidence.`);
    }

    const starts = [
      ...citedEvents.map((event) => event.startedAtMs),
      ...citedTranscripts.map((segment) => segment.startMs),
    ];
    const ends = [
      ...citedEvents.map((event) => event.timestampMs),
      ...citedTranscripts.map((segment) => segment.endMs),
    ];
    const movements = citedEvents
      .filter((event): event is MoveEvent => event.type === "entity_moved")
      .map((event) => ({
        entityType: event.entity.kind,
        entityId: event.entity.kind === "player" ? event.entity.id : ("ball" as const),
        from: event.from,
        to: event.to,
        path: event.path,
      }));

    return {
      id: phase.id || `phase-${index + 1}`,
      startMs: Math.min(...starts),
      endMs: Math.max(...ends),
      primaryTactic: phase.primaryTactic,
      instruction: phase.instruction,
      objective: phase.objective,
      actors: [...new Set(phase.actors)],
      movements,
      annotationIds: citedEvents
        .filter((event): event is AnnotationAddedEvent => event.type === "annotation_added")
        .map((event) => event.annotation.id),
      evidence: {
        eventIds: citedEvents.map((event) => event.id),
        transcriptSegmentIds: citedTranscripts.map((segment) => segment.id),
      },
    };
  });

  if (steps.length === 0 && (input.boardEvents.length > 0 || input.transcriptSegments.length > 0)) {
    throw new GroundingError("The interpretation omitted all available evidence.");
  }

  const classification = {
    ...semantic.classification,
    secondaryTraits: uniqueWithoutPrimary(
      semantic.classification.secondaryTraits,
      semantic.classification.primaryTactic,
    ),
    alternativeTactics: uniqueWithoutPrimary(
      semantic.classification.alternativeTactics,
      semantic.classification.primaryTactic,
    ),
  };
  const replay = replaySession(session.events);

  return tacticalOutputSchema.parse({
    schemaVersion: "1.0",
    taxonomyVersion: TACTIC_TAXONOMY_VERSION,
    interpretationMode: "model-backed",
    session: input.session,
    teams: input.boardContext.teams.map(({ id, playerIds }) => ({ id, playerIds })),
    classification,
    summary: semantic.summary,
    steps,
    finalState: replay.board,
    rlSelection: {
      schemaVersion: "1.0",
      taxonomyVersion: TACTIC_TAXONOMY_VERSION,
      sessionId: session.id,
      primaryTactic: classification.primaryTactic,
      downstreamValue: downstreamValueFor(classification.primaryTactic),
      selectionReason: classification.selectionReason,
      evidenceStrength: classification.evidenceStrength,
    },
  });
}

function uniqueWithoutPrimary<T extends string>(values: T[], primary: T): T[] {
  return [...new Set(values)].filter((value) => value !== primary);
}

export class GroundingError extends Error {
  readonly code = "INVALID_GROUNDING";
}

export function isEvidenceEvent(event: RawSessionEvent): event is EvidenceEvent {
  return event.type === "entity_moved" || event.type === "annotation_added";
}
