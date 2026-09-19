import { describe, expect, it } from "vitest";
import { apiErrorResponse } from "./app";
import { assembleTacticalOutput, buildInterpretationInput, GroundingError } from "./normalize";
import { InterpretationServiceError } from "./openaiInterpretation";
import { createSession } from "../src/domain/session";
import type { SemanticInterpretation } from "../src/domain/tactics";

function fixtureSession() {
  const session = createSession();
  session.status = "review";
  session.elapsedMs = 3_000;
  session.events = [
    {
      id: "move-1",
      type: "entity_moved",
      entity: { kind: "player", id: 3 },
      timestampMs: 1_500,
      startedAtMs: 1_000,
      from: { x: 0.7, y: 0.23 },
      to: { x: 0.55, y: 0.4 },
      path: [{ x: 0.7, y: 0.23 }, { x: 0.55, y: 0.4 }],
    },
    {
      id: "transcript-event",
      type: "transcript_added",
      timestampMs: 1_800,
      segment: {
        id: "segment-1",
        startMs: 900,
        endMs: 1_800,
        text: "Three steps forward to press.",
        source: "manual",
      },
    },
  ];
  return session;
}

function semantic(): SemanticInterpretation {
  return {
    classification: {
      primaryTactic: "high_press",
      secondaryTraits: ["high_press", "wide", "wide"],
      alternativeTactics: ["balanced"],
      selectionReason: "best_match",
      evidenceStrength: "strong",
      explanation: "The demonstrated movement and instruction describe a press.",
    },
    summary: { name: "Press together", objective: "Recover the ball high." },
    phases: [
      {
        id: "phase-1",
        primaryTactic: "high_press",
        instruction: "Player three steps forward.",
        objective: "Apply pressure.",
        actors: [3],
        evidence: { eventIds: ["move-1"], transcriptSegmentIds: ["segment-1"] },
      },
    ],
  };
}

describe("model interpretation assembly", () => {
  it("copies immutable facts and derives phase timing from cited evidence", () => {
    const session = fixtureSession();
    const output = assembleTacticalOutput(session, buildInterpretationInput(session), semantic());

    expect(output.steps[0].startMs).toBe(900);
    expect(output.steps[0].endMs).toBe(1_800);
    expect(output.steps[0].movements[0].to).toEqual({ x: 0.55, y: 0.4 });
    expect(output.classification.secondaryTraits).toEqual(["wide"]);
    expect(output.rlSelection.downstreamValue).toBe("HighPress");
  });

  it("rejects invented evidence IDs", () => {
    const session = fixtureSession();
    const candidate = semantic();
    candidate.phases[0].evidence.eventIds = ["invented-event"];

    expect(() => assembleTacticalOutput(session, buildInterpretationInput(session), candidate)).toThrow(
      GroundingError,
    );
  });

  it("maps service failures to explicit API errors", () => {
    expect(apiErrorResponse(new InterpretationServiceError("OPENAI_TIMEOUT", "Timed out.", 504))).toEqual({
      status: 504,
      body: { code: "OPENAI_TIMEOUT", message: "Timed out." },
    });
  });
});
