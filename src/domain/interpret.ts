import { z } from "zod";
import { replaySession } from "./replay";
import { pointSchema } from "./schemas";
import {
  classificationSchema,
  downstreamValueFor,
  TACTIC_TAXONOMY_VERSION,
  tacticKeySchema,
} from "./tactics";
import type { AnnotationAddedEvent, MoveEvent, RawSessionEvent, Session } from "./types";

const movementSchema = z.object({
  entityType: z.enum(["player", "ball"]),
  entityId: z.union([z.number().int().min(1).max(10), z.literal("ball")]),
  from: pointSchema,
  to: pointSchema,
  path: z.array(pointSchema),
});

export const tacticalOutputSchema = z.object({
  schemaVersion: z.literal("1.0"),
  taxonomyVersion: z.literal(TACTIC_TAXONOMY_VERSION),
  interpretationMode: z.enum([
    "deterministic-preview",
    "model-backed",
    "deterministic-fallback",
  ]),
  session: z.object({
    id: z.string(),
    title: z.string(),
    durationMs: z.number().nonnegative(),
  }),
  teams: z.array(
    z.object({
      id: z.enum(["red", "yellow"]),
      playerIds: z.array(z.number().int().min(1).max(10)),
    }),
  ),
  classification: classificationSchema,
  summary: z.object({
    name: z.string(),
    objective: z.string(),
  }),
  steps: z.array(
    z.object({
      id: z.string(),
      startMs: z.number().nonnegative(),
      endMs: z.number().nonnegative(),
      primaryTactic: tacticKeySchema,
      instruction: z.string(),
      objective: z.string(),
      actors: z.array(z.number().int().min(1).max(10)),
      movements: z.array(movementSchema),
      annotationIds: z.array(z.string()),
      evidence: z.object({
        eventIds: z.array(z.string()),
        transcriptSegmentIds: z.array(z.string()),
      }),
    }),
  ),
  finalState: z.object({
    players: z.array(
      z.object({ id: z.number(), team: z.enum(["red", "yellow"]), position: pointSchema }),
    ),
    ball: pointSchema,
    annotations: z.array(
      z.object({
        id: z.string(),
        kind: z.enum(["arrow", "freehand"]),
        points: z.array(pointSchema),
      }),
    ),
  }),
  rlSelection: z.object({
    schemaVersion: z.literal("1.0"),
    taxonomyVersion: z.literal(TACTIC_TAXONOMY_VERSION),
    sessionId: z.string(),
    primaryTactic: tacticKeySchema,
    downstreamValue: z.enum(["Balanced", "HighPress", "LowBlock", "Wide"]),
    selectionReason: classificationSchema.shape.selectionReason,
    evidenceStrength: classificationSchema.shape.evidenceStrength,
  }),
});

export type TacticalOutput = z.infer<typeof tacticalOutputSchema>;

const isBoardAction = (
  event: RawSessionEvent,
): event is MoveEvent | AnnotationAddedEvent =>
  event.type === "entity_moved" || event.type === "annotation_added";

export function interpretSession(
  session: Session,
  mode: "deterministic-preview" | "deterministic-fallback" = "deterministic-preview",
): TacticalOutput {
  const replay = replaySession(session.events);
  const boardActions = replay.effectiveEvents.filter(isBoardAction);
  const transcriptSteps = replay.transcripts.length
    ? replay.transcripts.map((segment, index) => ({
        segment,
        startMs: index === 0 ? 0 : segment.startMs,
        endMs: replay.transcripts[index + 1]?.startMs ?? session.elapsedMs,
      }))
    : [
        {
          segment: null,
          startMs: 0,
          endMs: session.elapsedMs,
        },
      ];

  const steps = transcriptSteps.map(({ segment, startMs, endMs }, index) => {
    const actions = boardActions.filter(
      (event) => event.timestampMs >= startMs && event.timestampMs <= endMs,
    );
    const moveEvents = actions.filter(
      (event): event is MoveEvent => event.type === "entity_moved",
    );
    const annotationEvents = actions.filter(
      (event): event is AnnotationAddedEvent => event.type === "annotation_added",
    );

    return {
      id: `step-${index + 1}`,
      startMs,
      endMs: Math.max(startMs, endMs),
      primaryTactic: "balanced" as const,
      instruction: segment?.text || "Demonstrated movement sequence.",
      objective: "Preserve the demonstrated sequence without adding model-derived meaning.",
      actors: [
        ...new Set(
          moveEvents
            .filter((event) => event.entity.kind === "player")
            .map((event) => (event.entity.kind === "player" ? event.entity.id : 0)),
        ),
      ],
      movements: moveEvents.map((event) => ({
        entityType: event.entity.kind,
        entityId: event.entity.kind === "player" ? event.entity.id : ("ball" as const),
        from: event.from,
        to: event.to,
        path: event.path,
      })),
      annotationIds: annotationEvents.map((event) => event.annotation.id),
      evidence: {
        eventIds: actions.map((event) => event.id),
        transcriptSegmentIds: segment ? [segment.id] : [],
      },
    };
  });

  return tacticalOutputSchema.parse({
    schemaVersion: "1.0",
    taxonomyVersion: TACTIC_TAXONOMY_VERSION,
    interpretationMode: mode,
    session: {
      id: session.id,
      title: session.title,
      durationMs: session.elapsedMs,
    },
    teams: [
      { id: "red", playerIds: [1, 2, 3, 4, 5] },
      { id: "yellow", playerIds: [6, 7, 8, 9, 10] },
    ],
    classification: {
      primaryTactic: "balanced",
      secondaryTraits: [],
      alternativeTactics: [],
      selectionReason: "system_fallback",
      evidenceStrength: "weak",
      explanation: "Offline deterministic output; no model-backed tactic classification was performed.",
    },
    summary: {
      name: session.title,
      objective: "Coordinate a 5-v-5 build-up pattern using timed player and ball movements.",
    },
    steps,
    finalState: replay.board,
    rlSelection: {
      schemaVersion: "1.0",
      taxonomyVersion: TACTIC_TAXONOMY_VERSION,
      sessionId: session.id,
      primaryTactic: "balanced",
      downstreamValue: downstreamValueFor("balanced"),
      selectionReason: "system_fallback",
      evidenceStrength: "weak",
    },
  });
}
