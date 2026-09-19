import "dotenv/config";
import { createApp } from "./app";

const port = Number(process.env.API_PORT ?? 8787);
const app = createApp();

app.listen(port, "127.0.0.1", () => {
  console.info(`Tactic Lab API listening on http://127.0.0.1:${port}`);
});
