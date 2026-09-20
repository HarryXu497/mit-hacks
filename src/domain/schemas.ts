import { z } from "zod";

export const pointSchema = z.object({
  x: z.number().min(0).max(1),
  y: z.number().min(0).max(1),
});

export const entityRefSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("player"), id: z.number().int().min(1).max(10) }),
  z.object({ kind: z.literal("ball") }),
]);

export const annotationSchema = z.object({
  id: z.string(),
  kind: z.enum(["arrow", "freehand"]),
  points: z.array(pointSchema),
});

export const transcriptSegmentSchema = z.object({
  id: z.string(),
  startMs: z.number().nonnegative(),
  endMs: z.number().nonnegative(),
  text: z.string(),
  source: z.enum(["speech", "manual"]),
});

const eventBase = { id: z.string(), timestampMs: z.number().nonnegative() };

export const rawSessionEventSchema = z.discriminatedUnion("type", [
  z.object({ ...eventBase, type: z.literal("recording_started") }),
  z.object({ ...eventBase, type: z.literal("recording_resumed") }),
  z.object({ ...eventBase, type: z.literal("recording_stopped") }),
  z.object({
    ...eventBase,
    type: z.literal("entity_moved"),
    entity: entityRefSchema,
    startedAtMs: z.number().nonnegative(),
    from: pointSchema,
    to: pointSchema,
    path: z.array(pointSchema),
  }),
  z.object({
    ...eventBase,
    type: z.literal("annotation_added"),
    annotation: annotationSchema,
    startedAtMs: z.number().nonnegative(),
  }),
  z.object({
    ...eventBase,
    type: z.literal("annotation_removed"),
    annotationId: z.string(),
  }),
  z.object({
    ...eventBase,
    type: z.literal("undo"),
    targetEventId: z.string(),
  }),
  z.object({
    ...eventBase,
    type: z.literal("transcript_added"),
    segment: transcriptSegmentSchema,
  }),
  z.object({
    ...eventBase,
    type: z.literal("transcript_edited"),
    segmentId: z.string(),
    text: z.string(),
  }),
]);

export const sessionSchema = z.object({
  schemaVersion: z.literal(1),
  id: z.string(),
  title: z.string(),
  status: z.enum(["ready", "recording", "review", "interpreted"]),
  elapsedMs: z.number().nonnegative(),
  events: z.array(rawSessionEventSchema),
  createdAt: z.string(),
});

export const interpretationRequestSchema = z.object({
  schemaVersion: z.literal("1.0"),
  session: sessionSchema,
  teamId: z.enum(["red", "yellow"]).default("red"),
});
