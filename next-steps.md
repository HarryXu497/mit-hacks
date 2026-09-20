> Historical planning notes: the taxonomy and coaching-to-game handoff below
> predate the tactical AI integration. See [AGENT-README.md](./AGENT-README.md)
> for current functionality, versions, and remaining gaps.

# Model-backed tactical interpretation

The app now sends recorded sessions to a server-only `POST /api/interpret`
route. The server resolves undo and transcript edits, calls the OpenAI Responses
API with strict Structured Outputs, validates evidence references, and then
combines model-derived tactics with deterministic board facts.

## Current contract

- Taxonomy `tactics-v1` supports `balanced`, `high_press`, `low_block`, and
  `wide`, with a deterministic mapping to the downstream tactic enum.
- One session receives an overall primary tactic and can contain multiple
  timestamped tactical phases.
- The model supplies semantic classification and evidence IDs only. The server
  supplies coordinates, movements, timestamps, annotations, teams, and final
  board state from the recorded session.
- Missing credentials, refusals, timeouts, request failures, and invalid output
  are explicit API errors. The native client preserves the session and shows
  Retry interpretation / Continue anyway (Balanced), with the latter requiring
  an explicit user click.
- Telemetry contains status, model, latency, token counts, and request ID; it
  does not contain transcript text.

## Verification

`npm test` is deterministic and offline. `npm run test:live` is opt-in and makes
exactly two real API calls: an unambiguous high-press service test and a
low-block HTTP-route test. The live tests validate classification, schema
adherence, evidence grounding, RL mapping, and preservation of recorded facts.

## Later hardening

- Replace provisional tactic descriptions with definitions owned by the RL
  system and increment the taxonomy version when their meaning changes.
- Add request cancellation and a retry control to the result rail.
- Build a reviewed evaluation set before treating generated classifications as
  training labels.
- Add a production deployment adapter for the API server and retention policy
  appropriate to the eventual hosting environment.
