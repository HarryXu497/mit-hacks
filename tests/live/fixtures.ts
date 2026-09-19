import type { Session } from "../../src/domain/types";

export function highPressSession(): Session {
  return {
    schemaVersion: 1,
    id: "live-high-press",
    title: "Red team high press",
    status: "review",
    elapsedMs: 8_000,
    createdAt: "2026-09-19T12:00:00.000Z",
    events: [
      {
        id: "high-move-2",
        type: "entity_moved",
        entity: { kind: "player", id: 2 },
        startedAtMs: 1_000,
        timestampMs: 2_200,
        from: { x: 0.3, y: 0.23 },
        to: { x: 0.32, y: 0.67 },
        path: [{ x: 0.3, y: 0.23 }, { x: 0.32, y: 0.67 }],
      },
      {
        id: "high-move-3",
        type: "entity_moved",
        entity: { kind: "player", id: 3 },
        startedAtMs: 1_100,
        timestampMs: 2_300,
        from: { x: 0.7, y: 0.23 },
        to: { x: 0.68, y: 0.67 },
        path: [{ x: 0.7, y: 0.23 }, { x: 0.68, y: 0.67 }],
      },
      {
        id: "high-transcript-event",
        type: "transcript_added",
        timestampMs: 3_200,
        segment: {
          id: "high-transcript",
          startMs: 800,
          endMs: 3_200,
          text: "Red presses high together near the yellow goal to win the ball immediately.",
          source: "manual",
        },
      },
    ],
  };
}

export function lowBlockSession(): Session {
  return {
    schemaVersion: 1,
    id: "live-low-block",
    title: "Red team low block",
    status: "review",
    elapsedMs: 8_000,
    createdAt: "2026-09-19T12:00:00.000Z",
    events: [
      {
        id: "low-move-4",
        type: "entity_moved",
        entity: { kind: "player", id: 4 },
        startedAtMs: 1_000,
        timestampMs: 2_100,
        from: { x: 0.28, y: 0.4 },
        to: { x: 0.4, y: 0.18 },
        path: [{ x: 0.28, y: 0.4 }, { x: 0.4, y: 0.18 }],
      },
      {
        id: "low-move-5",
        type: "entity_moved",
        entity: { kind: "player", id: 5 },
        startedAtMs: 1_100,
        timestampMs: 2_200,
        from: { x: 0.72, y: 0.4 },
        to: { x: 0.6, y: 0.18 },
        path: [{ x: 0.72, y: 0.4 }, { x: 0.6, y: 0.18 }],
      },
      {
        id: "low-transcript-event",
        type: "transcript_added",
        timestampMs: 3_200,
        segment: {
          id: "low-transcript",
          startMs: 800,
          endMs: 3_200,
          text: "Red drops into a compact low block close to the top goal and protects the middle.",
          source: "manual",
        },
      },
    ],
  };
}
