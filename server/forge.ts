/**
 * Turning a drawn superpower into a real one.
 *
 * The player paints a superpower at the easel; MonkeyForge's classifier decides which of the
 * game's four powers it is. That classifier is Python (CLIP ViT-B/32, small enough to run on CPU
 * in a couple of seconds), so this route shells out to it and returns its answer.
 *
 * It lives on the server rather than in the game for the same reason transcription does: this
 * process already owns the machine-local tooling and the credentials, and a joining machine
 * redirects its API calls to the host, so a LAN guest gets the host's toolchain for free without
 * needing a Python environment of its own.
 *
 * The badge PNGs are *not* rendered here. All four are committed under `assets/icons/powers/`, so
 * the game maps the answer to an icon it already has -- which means the HUD works offline and the
 * round trip stays a classification rather than an image build.
 */

import { Router } from "express";
import { spawn } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";

const here = path.dirname(fileURLToPath(import.meta.url));
/** Resolved from this file, not the working directory, so it holds however the server is started. */
const repoRoot = path.resolve(here, "..");
const monkeyforge = path.join(repoRoot, "tools", "monkeyforge");

/** The four powers the game actually has. Mirrors `SuperpowerKind`, which pins this same order. */
export const POWERS = ["beam_blast", "freeze_ray", "boost", "slow"] as const;
export type Power = (typeof POWERS)[number];

/** How long to let the classifier run before giving up. First call loads CLIP, so it is slow. */
const CLASSIFY_TIMEOUT_MS = 90_000;

const forgePowerRequestSchema = z
  .object({
    /** The painted superpower, base64-encoded PNG. */
    drawing: z.string().min(1).optional(),
    /** Anything the player typed about it. The classifier is markedly better with both. */
    description: z.string().max(500).optional(),
  })
  .refine((body) => body.drawing || body.description, {
    message: "give a drawing, a description, or both",
  });

/** What the Python classifier prints with `--json`. Only the fields this route passes on. */
const classifierOutputSchema = z.object({
  power: z.enum(POWERS),
  confidence: z.number(),
  runner_up: z.enum(POWERS).nullable().optional(),
  unsure: z.boolean().optional(),
  scores: z.record(z.string(), z.number()).optional(),
  motif: z.string().nullable().optional(),
});

export class ForgeUnavailableError extends Error {
  readonly code = "FORGE_UNAVAILABLE";
}

/**
 * The Python that can run MonkeyForge.
 *
 * Its virtualenv, not the system interpreter: torch and transformers live only there.
 * `MONKEYFORGE_PYTHON` overrides it for a machine that keeps them elsewhere.
 */
function pythonExecutable(): string {
  if (process.env.MONKEYFORGE_PYTHON) return process.env.MONKEYFORGE_PYTHON;
  return process.platform === "win32"
    ? path.join(monkeyforge, ".venv", "Scripts", "python.exe")
    : path.join(monkeyforge, ".venv", "bin", "python");
}

/** Run the classifier and parse what it says. */
async function classify(sketchPath: string | null, description: string): Promise<unknown> {
  const args = [path.join("scripts", "classify_sketch.py"), "--json"];
  if (sketchPath) args.push(sketchPath);
  if (description) args.push("--description", description);

  return await new Promise((resolve, reject) => {
    const child = spawn(pythonExecutable(), args, {
      cwd: monkeyforge,
      // The classifier writes progress and Hugging Face notices to stderr; only stdout is data.
      stdio: ["ignore", "pipe", "pipe"],
      env: {
        ...process.env,
        HF_HUB_DISABLE_XET: "1",
        // Pin the *vendored* package, so a machine that also has MonkeyForge checked out
        // elsewhere (and a virtualenv with it installed editable) runs this repo's copy rather
        // than that one. Byte-identical today; a silent divergence later would be very hard to see.
        PYTHONPATH: [path.join(monkeyforge, "src"), process.env.PYTHONPATH].filter(Boolean).join(path.delimiter),
      },
    });

    let stdout = "";
    let stderr = "";
    let settled = false;
    const timer = setTimeout(() => {
      settled = true;
      child.kill();
      reject(new ForgeUnavailableError(`The classifier did not answer within ${CLASSIFY_TIMEOUT_MS / 1000}s.`));
    }, CLASSIFY_TIMEOUT_MS);

    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));

    child.on("error", (error) => {
      if (settled) return;
      clearTimeout(timer);
      settled = true;
      // Overwhelmingly the missing virtualenv, which is worth saying plainly.
      reject(
        new ForgeUnavailableError(
          `Cannot run ${pythonExecutable()}: ${error.message}. Is tools/monkeyforge/.venv set up?`,
        ),
      );
    });

    child.on("close", (code) => {
      if (settled) return;
      clearTimeout(timer);
      settled = true;
      if (code !== 0) {
        reject(new ForgeUnavailableError(`The classifier exited ${code}: ${stderr.trim().slice(-400)}`));
        return;
      }
      try {
        // The JSON is the last object printed; anything before it is progress noise.
        const start = stdout.indexOf("{");
        resolve(JSON.parse(start >= 0 ? stdout.slice(start) : stdout));
      } catch (error) {
        reject(new ForgeUnavailableError(`The classifier printed something unreadable: ${String(error)}`));
      }
    });
  });
}

export function forgeRouter(): Router {
  const router = Router();

  router.post("/api/forge/power", async (request, response) => {
    let scratch: string | null = null;
    try {
      const body = forgePowerRequestSchema.parse(request.body);

      let sketchPath: string | null = null;
      if (body.drawing) {
        const png = Buffer.from(body.drawing, "base64");
        // Magic bytes, as the artifact upload checks: a corrupted base64 should fail here rather
        // than deep inside the classifier.
        if (png.length < 8 || !png.subarray(0, 8).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))) {
          response.status(400).json({ code: "INVALID_DRAWING", message: "The drawing is not a valid PNG." });
          return;
        }
        scratch = await mkdtemp(path.join(tmpdir(), "forge-power-"));
        sketchPath = path.join(scratch, "superpower.png");
        await writeFile(sketchPath, png);
      }

      const raw = await classify(sketchPath, body.description ?? "");
      const guess = classifierOutputSchema.parse(raw);
      response.json({
        power: guess.power,
        confidence: guess.confidence,
        runnerUp: guess.runner_up ?? null,
        unsure: guess.unsure ?? false,
        motif: guess.motif ?? null,
        scores: guess.scores ?? {},
      });
    } catch (error) {
      if (error instanceof ForgeUnavailableError) {
        // Not a server fault and not fatal to a match: the game keeps whatever power it had.
        console.warn(JSON.stringify({ event: "forge_unavailable", message: error.message }));
        response.status(503).json({ code: error.code, message: error.message });
        return;
      }
      if (error instanceof z.ZodError) {
        response.status(400).json({
          code: "INVALID_FORGE_REQUEST",
          message: error.issues.map((issue) => `${issue.path.join(".")}: ${issue.message}`).join("; "),
        });
        return;
      }
      console.error("Unexpected forge error", error);
      response.status(500).json({ code: "INTERNAL_ERROR", message: "Forging the power failed unexpectedly." });
    } finally {
      if (scratch) await rm(scratch, { recursive: true, force: true }).catch(() => {});
    }
  });

  return router;
}
