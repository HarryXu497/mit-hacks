/**
 * Forging a player's drawing into the character their team wears.
 *
 * This is the slow half of MonkeyForge -- around a minute against the classifier's few seconds --
 * so it is built as a job rather than a request. Nothing waits on it: the caller gets a job id
 * immediately, the team plays in the committed base monkey meanwhile, and the forged model
 * replaces it the moment it has genuinely loaded.
 *
 * # Why it is never allowed to block
 *
 * The flow is Creation -> Coaching -> VS screen -> kickoff. Start the job when the appearance
 * drawing is submitted and it finishes during coaching, which is a minute of real player
 * engagement, so the VS screen is a deadline the pipeline beats rather than a wait anyone sees.
 *
 * But "usually finishes in time" is not "always", so every wait here is escapable:
 *
 * - **Skip is not cancel.** A skipped job keeps running. The GPU has already done most of the
 *   work, the game can re-dress a side mid-match, so a model that misses the VS screen simply
 *   arrives during the first round instead of being thrown away.
 * - **A failed or unreachable GPU is not an error anybody sees.** The job ends `failed`, the
 *   base monkey plays, and the match is unaffected.
 * - Every stage has its own timeout, so a hung model cannot strand a job in `generating` forever.
 *
 * # Where the work runs
 *
 * Reading the drawing and drawing the garment happen on the GPU box, against a resident daemon
 * (`tools/monkeyforge/worker/forge_daemon.py`) that holds Qwen2-VL and SDXL in memory -- loading
 * them per request cost ~45 s of the old ~138 s. Projecting the garment and assembling the GLB
 * happen here, because they need Blender and no GPU.
 */

import { Router } from "express";
import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const monkeyforge = path.join(repoRoot, "tools", "monkeyforge");

/** The GPU box's resident forge. Unset means "no GPU": jobs fail fast and the base monkey plays. */
const DAEMON = process.env.MONKEYFORGE_DAEMON ?? "http://10.189.122.101:8601";

/** Blender assembles the GLB. Machine-local, so overridable. */
const BLENDER =
  process.env.MONKEYFORGE_BLENDER ?? "C:\\Program Files\\Blender Foundation\\Blender 5.2\\blender.exe";

/**
 * What a team wears until something better is forged, and what it falls back to if nothing is.
 * Matches `BASE_CHARACTER` in `native/cube-soccer/src/entities/character.rs`.
 */
export const FALLBACK_CHARACTER = "characters/base_rigged.glb#Scene0";

export const TEAMS = ["orange", "blue"] as const;
export type Team = (typeof TEAMS)[number];

/**
 * Measured on the GX10 with the daemon warm, and used for the ETA the waiting UI shows.
 * Honest numbers beat an animated bar: the UI can say what is happening and roughly how long.
 */
const STAGE_SECONDS = { reading: 2, generating: 24, assembling: 35 } as const;
export const ESTIMATED_SECONDS =
  STAGE_SECONDS.reading + STAGE_SECONDS.generating + STAGE_SECONDS.assembling;

/** Per-stage ceilings. A stage that overruns fails the job rather than stranding it. */
const READ_TIMEOUT_MS = 60_000;
const SPRITES_TIMEOUT_MS = 240_000;
const ASSEMBLE_TIMEOUT_MS = 300_000;
const PLAN_TIMEOUT_MS = 30_000;

export type JobStatus = "queued" | "reading" | "generating" | "assembling" | "ready" | "failed";

export interface ForgeJob {
  id: string;
  team: Team;
  status: JobStatus;
  /** A sentence for the waiting UI, in the player's terms rather than a prompt. */
  label: string;
  startedAt: number;
  finishedAt: number | null;
  /** What the drawing turned out to be, once read. */
  summary: string | null;
  /** Path under `assets/`, ready for `WornCharacters`, once built. */
  asset: string | null;
  error: string | null;
  /**
   * Whether the client stopped waiting. Purely advisory: the job keeps running, because the model
   * is still worth having for the next round.
   */
  skipped: boolean;
}

const jobs = new Map<string, ForgeJob>();

/** Jobs are small, but a long session should not accumulate them forever. */
const MAX_JOBS = 64;

function remember(job: ForgeJob): void {
  jobs.set(job.id, job);
  if (jobs.size > MAX_JOBS) {
    const oldest = [...jobs.values()].sort((a, b) => a.startedAt - b.startedAt)[0];
    if (oldest) jobs.delete(oldest.id);
  }
}

function pythonExecutable(): string {
  if (process.env.MONKEYFORGE_PYTHON) return process.env.MONKEYFORGE_PYTHON;
  return process.platform === "win32"
    ? path.join(monkeyforge, ".venv", "Scripts", "python.exe")
    : path.join(monkeyforge, ".venv", "bin", "python");
}

async function daemon(route: string, payload: unknown, timeoutMs: number): Promise<any> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(`${DAEMON}${route}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(`${route} returned ${response.status}: ${(await response.text()).slice(0, 200)}`);
    }
    return await response.json();
  } finally {
    clearTimeout(timer);
  }
}

/** Run a local command, resolving with stdout. Used for the planner and for Blender. */
function run(command: string, args: string[], cwd: string, timeoutMs: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
      env: {
        ...process.env,
        HF_HUB_DISABLE_XET: "1",
        PYTHONPATH: [path.join(monkeyforge, "src"), process.env.PYTHONPATH].filter(Boolean).join(path.delimiter),
      },
    });
    let stdout = "";
    let stderr = "";
    let settled = false;
    const timer = setTimeout(() => {
      settled = true;
      child.kill();
      reject(new Error(`${path.basename(command)} exceeded ${timeoutMs / 1000}s`));
    }, timeoutMs);
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", (error) => {
      if (settled) return;
      clearTimeout(timer);
      settled = true;
      reject(new Error(`cannot run ${command}: ${error.message}`));
    });
    child.on("close", (code) => {
      if (settled) return;
      clearTimeout(timer);
      settled = true;
      if (code === 0) resolve(stdout);
      else reject(new Error(`${path.basename(command)} exited ${code}: ${stderr.slice(-600)}`));
    });
  });
}

/**
 * The whole pipeline for one drawing. Every failure leaves the job `failed` and the base monkey
 * in play; none of them reaches the player as an error.
 */
async function forge(job: ForgeJob, drawing: Buffer, seed: number): Promise<void> {
  const scratch = await mkdtemp(path.join(tmpdir(), `forge-character-${job.team}-`));
  try {
    job.status = "reading";
    job.label = "Looking at your drawing";
    const spec = await daemon("/read", { sketch: drawing.toString("base64") }, READ_TIMEOUT_MS);

    const specPath = path.join(scratch, "spec.json");
    await writeFile(specPath, JSON.stringify(spec));

    // Prompt wording is decided here rather than on the GPU box, because the garment vocabulary
    // and its tests live in this repo. The planner deliberately imports no torch.
    const planned = JSON.parse(
      await run(pythonExecutable(), [path.join("scripts", "plan_outfit.py"), "--spec", specPath, "--seed", String(seed)], monkeyforge, PLAN_TIMEOUT_MS),
    );

    job.summary = planned.summary ?? null;
    job.status = "generating";
    job.label = planned.summary ? `Drawing the ${planned.summary}` : "Drawing the outfit";

    const generated = await daemon("/sprites", { jobs: planned.jobs }, SPRITES_TIMEOUT_MS);

    const sprites: Record<string, string> = {};
    for (const [name, base64] of Object.entries(generated.sprites as Record<string, string>)) {
      const file = path.join(scratch, `${name}.png`);
      await writeFile(file, Buffer.from(base64, "base64"));
      sprites[name] = file;
    }
    if (!sprites.garment) throw new Error("the generator returned no garment sprite");

    job.status = "assembling";
    job.label = "Fitting it to the monkey";

    const outDir = path.join(scratch, "build");
    const args = [
      path.join("scripts", "dress_monkey.py"),
      "--spec", specPath,
      "--garment", sprites.garment,
      // Sleeves take their fabric from the garment's own image: diffusion draws a jacket hanging
      // with its sleeves angled down, while the base stands in a T-pose.
      "--sleeve", sprites.garment,
      "--sleeve-crop", "0.14,0.35,0.30,0.72",
      "--out-dir", outDir,
      "--shell-offset", "0.018",
      "--relief-strength", "1.0",
      "--resolution", "512",
    ];
    if (sprites.trousers) args.push("--trousers", sprites.trousers);
    await run(pythonExecutable(), args, monkeyforge, ASSEMBLE_TIMEOUT_MS);

    // Land it under assets/ with a per-team name the game already knows how to wear.
    const built = path.join(outDir, "character.glb");
    const destination = path.join(repoRoot, "assets", "characters", `forged-${job.team}.glb`);
    await mkdir(path.dirname(destination), { recursive: true });
    await copyFile(built, destination);

    job.asset = `characters/forged-${job.team}.glb#Scene0`;
    job.status = "ready";
    job.label = "Ready";
    job.finishedAt = Date.now();
    console.log(JSON.stringify({ event: "forge_character_ready", team: job.team, seconds: (job.finishedAt - job.startedAt) / 1000 }));
  } catch (error) {
    job.status = "failed";
    job.error = error instanceof Error ? error.message : String(error);
    job.label = "Using the default monkey";
    job.finishedAt = Date.now();
    // Deliberately a warning: an unreachable GPU box is an expected state, not a server fault.
    console.warn(JSON.stringify({ event: "forge_character_failed", team: job.team, message: job.error }));
  } finally {
    await rm(scratch, { recursive: true, force: true }).catch(() => {});
  }
}

const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

const startSchema = z.object({
  drawing: z.string().min(1),
  team: z.enum(TEAMS),
  seed: z.number().int().optional(),
});

function view(job: ForgeJob) {
  const elapsedMs = (job.finishedAt ?? Date.now()) - job.startedAt;
  const settled = job.status === "ready" || job.status === "failed";
  return {
    jobId: job.id,
    team: job.team,
    status: job.status,
    label: job.label,
    summary: job.summary,
    elapsedMs,
    estimatedSeconds: ESTIMATED_SECONDS,
    /** What to wear right now: the forged model once ready, the base monkey until then. */
    asset: job.asset ?? FALLBACK_CHARACTER,
    forged: job.status === "ready",
    skipped: job.skipped,
    /** The UI may stop waiting whenever it likes; the job carries on regardless. */
    canSkip: !settled,
    error: job.error,
  };
}

export function forgeCharacterRouter(): Router {
  const router = Router();

  router.post("/api/forge/character", async (request, response) => {
    try {
      const body = startSchema.parse(request.body);
      const png = Buffer.from(body.drawing, "base64");
      if (png.length < 8 || !png.subarray(0, 8).equals(PNG_MAGIC)) {
        response.status(400).json({ code: "INVALID_DRAWING", message: "The drawing is not a valid PNG." });
        return;
      }

      const job: ForgeJob = {
        id: randomUUID(),
        team: body.team,
        status: "queued",
        label: "Queued",
        startedAt: Date.now(),
        finishedAt: null,
        summary: null,
        asset: null,
        error: null,
        skipped: false,
      };
      remember(job);

      // Deliberately not awaited: the caller gets its job id now and the team plays meanwhile.
      void forge(job, png, body.seed ?? 11);

      response.status(202).json(view(job));
    } catch (error) {
      if (error instanceof z.ZodError) {
        response.status(400).json({
          code: "INVALID_FORGE_REQUEST",
          message: error.issues.map((issue) => `${issue.path.join(".")}: ${issue.message}`).join("; "),
        });
        return;
      }
      console.error("Unexpected character forge error", error);
      response.status(500).json({ code: "INTERNAL_ERROR", message: "Starting the forge failed." });
    }
  });

  router.get("/api/forge/character/:jobId", (request, response) => {
    const job = jobs.get(request.params.jobId);
    if (!job) {
      response.status(404).json({ code: "NO_SUCH_JOB", message: "That forge job is not known." });
      return;
    }
    response.json(view(job));
  });

  /**
   * Stop waiting. The job is *not* cancelled: most of its cost is already spent and the game can
   * re-dress a side mid-match, so a model that misses the VS screen still arrives in play.
   */
  router.post("/api/forge/character/:jobId/skip", (request, response) => {
    const job = jobs.get(request.params.jobId);
    if (!job) {
      response.status(404).json({ code: "NO_SUCH_JOB", message: "That forge job is not known." });
      return;
    }
    job.skipped = true;
    console.log(JSON.stringify({ event: "forge_character_skipped", team: job.team, status: job.status }));
    response.json(view(job));
  });

  /** Everything in flight, so the VS screen can wait on both sides at once. */
  router.get("/api/forge/characters", (_request, response) => {
    response.json({ jobs: [...jobs.values()].map(view) });
  });

  return router;
}
