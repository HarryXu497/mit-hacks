import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { interpretSession, tacticalOutputSchema } from "./interpret";
import { effectiveEvents, replaySession } from "./replay";
import { sessionSchema } from "./schemas";
import { clampPoint, createSession } from "./session";
import type { RawSessionEvent } from "./types";

describe("session domain", () => {
  it("clamps normalized coordinates", () => {
    expect(clampPoint({ x: -0.2, y: 1.4 })).toEqual({ x: 0, y: 1 });
  });

  it("replays movement and immutable undo", () => {
    const events: RawSessionEvent[] = [
      {
        id: "move-1",
        type: "entity_moved",
        entity: { kind: "player", id: 3 },
        timestampMs: 1000,
        startedAtMs: 800,
        from: { x: 0.7, y: 0.23 },
        to: { x: 0.55, y: 0.4 },
        path: [
          { x: 0.7, y: 0.23 },
          { x: 0.55, y: 0.4 },
        ],
      },
      { id: "undo-1", type: "undo", targetEventId: "move-1", timestampMs: 1200 },
    ];
    expect(effectiveEvents(events).map((event) => event.id)).toEqual(["undo-1"]);
    expect(replaySession(events).board.players.find((player) => player.id === 3)?.position).toEqual({
      x: 0.7,
      y: 0.23,
    });
  });

  it("creates a schema-valid session-derived tactical payload", () => {
    const session = createSession();
    session.elapsedMs = 3000;
    session.events = [
      {
        id: "move-1",
        type: "entity_moved",
        entity: { kind: "ball" },
        timestampMs: 1200,
        startedAtMs: 800,
        from: { x: 0.5, y: 0.51 },
        to: { x: 0.6, y: 0.4 },
        path: [
          { x: 0.5, y: 0.51 },
          { x: 0.6, y: 0.4 },
        ],
      },
      {
        id: "transcript-1",
        type: "transcript_added",
        timestampMs: 1000,
        segment: {
          id: "segment-1",
          startMs: 1000,
          endMs: 2000,
          text: "Play into the middle.",
          source: "manual",
        },
      },
    ];

    const result = interpretSession(session);
    expect(tacticalOutputSchema.safeParse(result).success).toBe(true);
    expect(result.steps[0].movements[0].entityId).toBe("ball");
    expect(result.steps[0].evidence.transcriptSegmentIds).toEqual(["segment-1"]);
  });

  it("replays the shared native compatibility fixture", () => {
    const fixture = JSON.parse(
      readFileSync(new URL("../../fixtures/session-v1.json", import.meta.url), "utf8"),
    );
    const session = sessionSchema.parse(fixture);
    const replay = replaySession(session.events);

    expect(replay.board.players.find((player) => player.id === 3)?.position).toEqual({
      x: 0.55,
      y: 0.4,
    });
    expect(replay.transcripts[0]?.text).toBe("Player three moves inside to receive.");
    expect(tacticalOutputSchema.safeParse(interpretSession(session)).success).toBe(true);
  });
});
