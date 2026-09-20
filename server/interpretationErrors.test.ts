import OpenAI from "openai";
import { afterEach, describe, expect, it, vi } from "vitest";
import { apiErrorResponse } from "./app";
import { describeInterpretationFailure, interpretSessionWithOpenAI } from "./openaiInterpretation";
import { createSession } from "../src/domain/session";
import { modelClassificationSchema } from "../src/domain/tactics";

afterEach(() => vi.unstubAllEnvs());

describe("interpretation diagnostics", () => {
  it.each([
    [401, "OPENAI_AUTH_FAILED"], [403, "OPENAI_ACCESS_DENIED"],
    [404, "OPENAI_MODEL_NOT_FOUND"], [429, "OPENAI_RATE_LIMIT_OR_QUOTA"],
    [400, "OPENAI_REQUEST_FAILED"],
  ])("preserves the reason and request ID for HTTP %s", (status, code) => {
    const error = new OpenAI.APIError(Number(status), { message: "Specific provider explanation", code: "provider_code" }, undefined,
      new Headers({ "x-request-id": "req-test" }));
    const diagnostic = describeInterpretationFailure(error);
    expect(diagnostic.code).toBe(code);
    expect(diagnostic.message).toContain("Specific provider explanation");
    expect(diagnostic.message).toContain(`HTTP ${status}`);
    expect(apiErrorResponse(diagnostic)?.body.requestId).toBe("req-test");
  });

  it("redacts credentials from provider diagnostics", () => {
    vi.stubEnv("OPENAI_API_KEY", "test-secret-credential");
    const error = new OpenAI.APIError(401, { message: "Bad key test-secret-credential or sk-partial-key" }, undefined, new Headers());
    const diagnostic = describeInterpretationFailure(error);
    expect(diagnostic.message).not.toContain("test-secret-credential");
    expect(diagnostic.message).not.toContain("sk-partial-key");
    expect(diagnostic.message).toContain("[redacted]");
  });

  it("distinguishes timeouts from connection failures", () => {
    expect(describeInterpretationFailure(new OpenAI.APIConnectionTimeoutError()).code).toBe("OPENAI_TIMEOUT");
    expect(describeInterpretationFailure(new OpenAI.APIConnectionError({ message: "DNS lookup failed" })).message).toContain("DNS lookup failed");
  });

  it("identifies the missing server configuration", async () => {
    vi.stubEnv("OPENAI_API_KEY", "");
    vi.stubEnv("OPENAI_MODEL", "configured-model");
    await expect(interpretSessionWithOpenAI(createSession())).rejects.toMatchObject({
      code: "OPENAI_NOT_CONFIGURED", message: expect.stringContaining("Missing server configuration: OPENAI_API_KEY."),
    });
  });

  it("rejects empty coaching instead of inventing a Balanced result", async () => {
    vi.stubEnv("OPENAI_API_KEY", "test-key");
    vi.stubEnv("OPENAI_MODEL", "test-model");
    await expect(interpretSessionWithOpenAI(createSession())).rejects.toMatchObject({ code: "NO_COACHING_EVIDENCE" });
  });

  it("requires a best-match choice while retaining honest weak evidence", () => {
    const classification = { primaryTactic: "wingplay", secondaryTraits: [], alternativeTactics: [],
      selectionReason: "best_match", evidenceStrength: "weak", explanation: "Closest supported approximation." };
    expect(modelClassificationSchema.safeParse(classification).success).toBe(true);
    const invalid = modelClassificationSchema.safeParse({ ...classification, selectionReason: "uncertain_fallback" });
    expect(invalid.success).toBe(false);
    if (!invalid.success) expect(describeInterpretationFailure(invalid.error).code).toBe("INVALID_MODEL_OUTPUT");
  });
});
