import "dotenv/config";
import { createServer } from "node:http";
import { networkInterfaces } from "node:os";
import { createApp } from "./app";
import { attachLobbyWebSocket } from "./lobby";
import { attachTranscriptionWebSocket } from "./transcription";

const port = Number(process.env.API_PORT ?? 8787);
const app = createApp();
const server = createServer(app);
attachTranscriptionWebSocket(server);
attachLobbyWebSocket(server);

// Bound to 0.0.0.0 so a joiner on the same LAN can reach the host's server
// for /api/interpret, /api/transcribe, and the lobby/game-stream endpoints.
server.listen(port, "0.0.0.0", () => {
  console.info(`Tactic Lab API listening on http://127.0.0.1:${port}`);
  for (const address of lanAddresses()) {
    console.info(`  LAN-reachable at http://${address}:${port}`);
  }
});

function lanAddresses(): string[] {
  const interfaces = networkInterfaces();
  const addresses: string[] = [];
  for (const entries of Object.values(interfaces)) {
    for (const entry of entries ?? []) {
      if (entry.family === "IPv4" && !entry.internal) addresses.push(entry.address);
    }
  }
  return addresses;
}
