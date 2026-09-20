import { describe, expect, it } from "vitest";
import { apiErrorResponse } from "./app";
import { assembleTacticalOutput, buildInterpretationInput, GroundingError } from "./normalize";
import { InterpretationServiceError } from "./openaiInterpretation";
import { createSession } from "../src/domain/session";
import { tacticKeys, type SemanticInterpretation } from "../src/domain/tactics";

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
      primaryTactic: "highpress",
      secondaryTraits: ["highpress", "wingplay", "wingplay"],
      alternativeTactics: ["balanced"],
      selectionReason: "best_match",
      evidenceStrength: "strong",
      explanation: "The demonstrated movement and instruction describe a press.",
    },
    summary: { name: "Press together", objective: "Recover the ball high." },
    playerOverrides: [],
    phases: [
      {
        id: "phase-1",
        primaryTactic: "highpress",
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
    expect(output.classification.secondaryTraits).toEqual(["wingplay"]);
    expect(output.rlSelection.downstreamValue).toBe("highpress");
  });

  it("rejects invented evidence IDs", () => {
    const session = fixtureSession();
    const candidate = semantic();
    candidate.phases[0].evidence.eventIds = ["invented-event"];

    expect(() => assembleTacticalOutput(session, buildInterpretationInput(session), candidate)).toThrow(
      GroundingError,
    );
  });

  it.each(tacticKeys)("emits a controller-compatible %s selection", (tactic) => {
    const session = fixtureSession();
    const candidate = semantic();
    candidate.classification.primaryTactic = tactic;
    const output = assembleTacticalOutput(session, buildInterpretationInput(session), candidate);
    expect(output.schemaVersion).toBe("2.0");
    expect(output.rlSelection).toMatchObject({ downstreamValue: tactic, teamId: "red", playerOverrides: [] });
  });

  it("preserves evidence-backed player overrides", () => {
    const session = fixtureSession();
    const candidate = semantic();
    candidate.playerOverrides = [{ playerId: 3, tactic: "lowblock", evidence: { eventIds: ["move-1"], transcriptSegmentIds: [] } }];
    const output = assembleTacticalOutput(session, buildInterpretationInput(session), candidate);
    expect(output.rlSelection.playerOverrides).toEqual(candidate.playerOverrides);
  });

  it("rejects duplicate, opponent, unsupported and ungrounded overrides", () => {
    const session = fixtureSession();
    const valid = { playerId: 3, tactic: "lowblock" as const, evidence: { eventIds: ["move-1"], transcriptSegmentIds: [] } };
    for (const overrides of [
      [valid, valid],
      [{ ...valid, playerId: 6 }],
      [{ ...valid, tactic: "invented" }],
      [{ ...valid, evidence: { eventIds: [], transcriptSegmentIds: [] } }],
      [{ ...valid, evidence: { eventIds: ["invented"], transcriptSegmentIds: [] } }],
      [{ ...valid, evidence: { eventIds: [], transcriptSegmentIds: ["invented"] } }],
    ]) {
      const candidate = { ...semantic(), playerOverrides: overrides } as SemanticInterpretation;
      expect(() => assembleTacticalOutput(session, buildInterpretationInput(session), candidate)).toThrow();
    }
  });

  it("assembles a yellow-team selection with overrides scoped to players 6-10", () => {
    const session = fixtureSession();
    session.events[0] = {
      ...session.events[0],
      entity: { kind: "player", id: 8 },
    } as typeof session.events[0];
    const candidate = semantic();
    candidate.playerOverrides = [{ playerId: 8, tactic: "lowblock", evidence: { eventIds: ["move-1"], transcriptSegmentIds: [] } }];
    const input = buildInterpretationInput(session, "yellow");
    const output = assembleTacticalOutput(session, input, candidate);
    expect(output.rlSelection.teamId).toBe("yellow");
    expect(output.rlSelection.playerOverrides).toEqual(candidate.playerOverrides);
  });

  it("rejects a yellow-team override for a red player", () => {
    const session = fixtureSession();
    const candidate = semantic();
    candidate.playerOverrides = [{ playerId: 3, tactic: "lowblock", evidence: { eventIds: ["move-1"], transcriptSegmentIds: [] } }];
    const input = buildInterpretationInput(session, "yellow");
    expect(() => assembleTacticalOutput(session, input, candidate)).toThrow(GroundingError);
  });

  it("maps service failures to explicit API errors", () => {
    expect(apiErrorResponse(new InterpretationServiceError("OPENAI_TIMEOUT", "Timed out.", 504))).toEqual({
      status: 504,
      body: { code: "OPENAI_TIMEOUT", message: "Timed out." },
    });
  });
});
