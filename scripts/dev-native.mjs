import { spawn, spawnSync } from "node:child_process";
import process from "node:process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const cargoHome = path.join(process.env.HOME ?? "", ".cargo", "bin");
if (cargoHome && !process.env.PATH?.split(":").includes(cargoHome)) {
  process.env.PATH = `${cargoHome}${path.delimiter}${process.env.PATH ?? ""}`;
}
const cargoCheck = spawnSync("cargo", ["--version"], {
  cwd: root,
  encoding: "utf8",
  shell: false,
});

if (cargoCheck.error?.code === "ENOENT") {
  console.error(
    "Rust/Cargo is required for native coaching but was not found on PATH.\n" +
      "Install Rust 1.85+ from https://rustup.rs, then run:\n" +
      '  source "$HOME/.cargo/env"\n' +
      "Or add that line to ~/.zshrc so new terminals pick up Cargo automatically.",
  );
  process.exit(1);
}
if (cargoCheck.status !== 0) {
  console.error(cargoCheck.stderr || "Unable to run Cargo.");
  process.exit(cargoCheck.status ?? 1);
}

const children = new Set();
let shuttingDown = false;

function start(command, args, label, environment = process.env) {
  const child = spawn(command, args, {
    cwd: root,
    env: environment,
    stdio: "inherit",
    shell: false,
    windowsHide: false,
  });
  children.add(child);
  child.once("exit", (code, signal) => {
    children.delete(child);
    if (!shuttingDown && code !== 0) {
      console.error(`${label} exited unexpectedly (${signal ?? code}).`);
      shutdown(code ?? 1);
    }
  });
  return child;
}

async function waitForService() {
  const port = Number(process.env.API_PORT ?? 8787);
  const url = `http://127.0.0.1:${port}/api/health`;
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // The service is still starting.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Local service did not become ready at ${url}.`);
}

function shutdown(exitCode = 0) {
  if (shuttingDown) return;
  shuttingDown = true;
  for (const child of children) child.kill();
  setTimeout(() => process.exit(exitCode), 250).unref();
}

for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, () => shutdown(0));
}
process.on("exit", () => {
  for (const child of children) child.kill();
});

const tsxCli = path.join(root, "node_modules", "tsx", "dist", "cli.mjs");
start(process.execPath, [tsxCli, "server/index.ts"], "Local service");

try {
  await waitForService();
  const port = Number(process.env.API_PORT ?? 8787);
  const native = start(
    "cargo",
    [
      "run",
      "--manifest-path",
      "native/coaching/Cargo.toml",
      "--bin",
      "native-coaching",
    ],
    "Native coaching",
    {
      ...process.env,
      TACTIC_LAB_API_URL:
        process.env.TACTIC_LAB_API_URL ?? `http://127.0.0.1:${port}`,
      TACTIC_LAB_WS_URL:
        process.env.TACTIC_LAB_WS_URL ?? `ws://127.0.0.1:${port}/api/transcribe`,
    },
  );
  native.once("exit", (code) => shutdown(code ?? 0));
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  shutdown(1);
}
