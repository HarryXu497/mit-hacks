import cors from "cors";
import express from "express";
import { ZodError } from "zod";
import { interpretationRequestSchema } from "../src/domain/schemas";
import type { Session } from "../src/domain/types";
import { forgeRouter } from "./forge";
import { lobbyRouter } from "./lobby";
import { GroundingError } from "./normalize";
import {
  InterpretationServiceError,
  interpretSessionWithOpenAI,
  type ModelInterpretationResult,
} from "./openaiInterpretation";

type Interpreter = (session: Session, teamId?: "red" | "yellow") => Promise<ModelInterpretationResult>;

export function createApp(interpreter: Interpreter = interpretSessionWithOpenAI) {
  const app = express();
  app.disable("x-powered-by");
  // LAN-only hackathon demo: any machine on the same wifi may be the joiner,
  // so origin is intentionally unrestricted rather than a per-deploy allowlist.
  app.use(cors());
  // Artifact bundles carry two base64 1024x1024 PNGs plus a full event log, and
  // a long coaching session's /api/interpret body can also pass 1mb.
  app.use(express.json({ limit: "25mb" }));
  app.use(lobbyRouter());
  // Turns a drawn superpower into one of the game's four. Shells out to MonkeyForge, so a
  // joining machine gets the host's Python toolchain by redirecting here.
  app.use(forgeRouter());

  app.get("/api/health", (_request, response) => {
    response.json({ ok: true, openaiConfigured: Boolean(process.env.OPENAI_API_KEY && process.env.OPENAI_MODEL) });
  });

  app.post("/api/interpret", async (request, response) => {
    try {
      const { session, teamId } = interpretationRequestSchema.parse(request.body);
      const result = await interpreter(session, teamId);
      if (result.telemetry.requestId) response.setHeader("x-openai-request-id", result.telemetry.requestId);
      response.json(result.output);
    } catch (error) {
      const knownError = apiErrorResponse(error);
      if (knownError) {
        console.warn(JSON.stringify({ event: "interpretation_rejected", ...knownError.body }));
        response.status(knownError.status).json(knownError.body);
        return;
      }
      console.error("Unexpected interpretation error", error);
      response.status(500).json({ code: "INTERNAL_ERROR", message: "Interpretation failed unexpectedly." });
    }
  });

  return app;
}

export function apiErrorResponse(error: unknown): {
  status: number;
  body: { code: string; message: string; requestId?: string };
} | null {
  if (error instanceof ZodError) {
    return { status: 400, body: { code: "INVALID_SESSION", message: `Invalid session: ${error.issues.map((issue) => `${issue.path.join(".")}: ${issue.message}`).join("; ")}` } };
  }
  if (error instanceof GroundingError) {
    return { status: 422, body: { code: error.code, message: error.message } };
  }
  if (error instanceof InterpretationServiceError) {
    return { status: error.status, body: { code: error.code, message: error.message, ...(error.requestId ? { requestId: error.requestId } : {}) } };
  }
  return null;
}
