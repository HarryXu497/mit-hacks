/**
 * Finding the Python that can run MonkeyForge.
 *
 * Its virtualenv, not the system interpreter: torch and transformers live only there, and the
 * environment is ~1 GB, so it is deliberately not committed.
 *
 * This resolves rather than assumes, because the alternative bit during a demo. The server was
 * started with `MONKEYFORGE_PYTHON` pointing at a sibling checkout's virtualenv; anyone running a
 * plain `npm run dev:api` got `FORGE_UNAVAILABLE` from both forge routes with nothing to suggest
 * why. Checking the obvious places costs one `existsSync` per candidate at startup and removes a
 * step from the setup nobody would remember.
 */

import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");

/** Where a virtualenv keeps its interpreter, per platform. */
function interpreterIn(virtualenv: string): string {
  return process.platform === "win32"
    ? path.join(virtualenv, "Scripts", "python.exe")
    : path.join(virtualenv, "bin", "python");
}

/**
 * In preference order:
 *
 * 1. `MONKEYFORGE_PYTHON`, for a machine that keeps the environment somewhere of its own.
 * 2. The vendored toolchain's own virtualenv, which is what a fresh setup creates.
 * 3. A MonkeyForge checkout beside this repo — the common case on the machine the pipeline was
 *    built on, where the environment already exists and duplicating a gigabyte would be silly.
 */
export function monkeyforgeCandidates(): string[] {
  const candidates: string[] = [];
  if (process.env.MONKEYFORGE_PYTHON) candidates.push(process.env.MONKEYFORGE_PYTHON);
  candidates.push(interpreterIn(path.join(repoRoot, "tools", "monkeyforge", ".venv")));
  candidates.push(interpreterIn(path.join(repoRoot, "..", "MonkeyForge", ".venv")));
  return candidates;
}

/**
 * The first candidate that exists, or the first candidate at all.
 *
 * Returning something unusable rather than throwing is deliberate: the caller turns a spawn
 * failure into `FORGE_UNAVAILABLE`, which is already handled as "play in the base monkey", and a
 * route that throws at import time would take the whole server down with it.
 */
export function monkeyforgePython(): string {
  const candidates = monkeyforgeCandidates();
  return candidates.find((candidate) => existsSync(candidate)) ?? candidates[0];
}
