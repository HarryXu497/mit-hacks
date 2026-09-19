import "dotenv/config";
import { createServer } from "node:http";
import { createApp } from "./app";
import { attachTranscriptionWebSocket } from "./transcription";

const port = Number(process.env.API_PORT ?? 8787);
const app = createApp();
const server = createServer(app);
attachTranscriptionWebSocket(server);

server.listen(port, "127.0.0.1", () => {
  console.info(`Tactic Lab API listening on http://127.0.0.1:${port}`);
});
