import "dotenv/config";
import request from "supertest";
import { beforeAll, describe, expect, it } from "vitest";
import { createApp } from "../../server/app";
import { buildInterpretationInput } from "../../server/normalize";
import { interpretSessionWithOpenAI } from "../../server/openaiInterpretation";
import { tacticalOutputSchema } from "../../src/domain/interpret";
import { replaySession } from "../../src/domain/replay";
import { highPressSession, lowBlockSession } from "./fixtures";

describe("live OpenAI tactical interpretation", () => {
  beforeAll(() => {
    if (!process.env.OPENAI_API_KEY || !process.env.OPENAI_MODEL) {
      throw new Error("Set OPENAI_API_KEY and OPENAI_MODEL in .env before running npm run test:live.");
    }
  });

  it("classifies an unambiguous high press with grounded structured output", async () => {
    const session = highPressSession();
    const { output, telemetry } = await interpretSessionWithOpenAI(session);
    const input = buildInterpretationInput(session);
    const validEventIds = new Set(input.boardEvents.map((event) => event.id));
    const validTranscriptIds = new Set(input.transcriptSegments.map((segment) => segment.id));

    expect(tacticalOutputSchema.safeParse(output).success).toBe(true);
    expect(output.classification.primaryTactic).toBe("high_press");
    expect(output.rlSelection.downstreamValue).toBe("HighPress");
    expect(output.steps.length).toBeGreaterThan(0);
    for (const step of output.steps) {
      expect(step.evidence.eventIds.every((id) => validEventIds.has(id))).toBe(true);
      expect(step.evidence.transcriptSegmentIds.every((id) => validTranscriptIds.has(id))).toBe(true);
    }
    expect(telemetry.inputTokens).toBeGreaterThan(0);
  });

  it("classifies a low block through the HTTP route without changing recorded facts", async () => {
    const session = lowBlockSession();
    const response = await request(createApp())
      .post("/api/interpret")
      .send({ schemaVersion: "1.0", session })
      .expect(200);
    const output = tacticalOutputSchema.parse(response.body);

    expect(output.classification.primaryTactic).toBe("low_block");
    expect(output.rlSelection.downstreamValue).toBe("LowBlock");
    expect(output.finalState).toEqual(replaySession(session.events).board);
    expect(output.steps.flatMap((step) => step.movements).map((move) => move.to)).toEqual(
      expect.arrayContaining([{ x: 0.4, y: 0.18 }, { x: 0.6, y: 0.18 }]),
    );
  });
});
