# AGENTS.md

## Goal

Build a standalone soccer coaching interface. A user demonstrates a tactic on a
virtual board while speaking, and the app preserves the board actions and speech
as one synchronized session that can later become structured tactical JSON.

## Demo target

- Exactly ten movable players in a fixed 5-v-5 setup: red IDs `1` through
  `5` and yellow IDs `6` through `10`
- A movable ball
- Arrows or other tactical annotations
- Speech recording or transcription
- Timestamped, synchronized board and speech events
- Semantic interpretation into structured tactical JSON

## Architecture

- Keep board state, raw event recording, audio/transcription, synchronization,
  and tactical interpretation separate.
- Raw events describe only what happened. Do not add tactical meaning until the
  interpretation layer.
- Use normalized field coordinates from `0` to `1`.
- Optimize the model for this fixed 5-v-5 setup. Do not build team-size
  configuration, rosters, or substitutions.
- Treat the tactical JSON as an external interface. This repo does not own or
  assume anything about its consumers.

## Hackathon workflow

- Inspect the repo first, then propose a small plan for the requested phase.
- Build incrementally and keep changes scoped to that phase.
- Prefer the fastest simple solution that produces a convincing demo; avoid
  premature infrastructure and abstractions.
- You may choose the stack and add dependencies or services without asking.
- Keep the architectural boundaries above, and flag choices likely to block a
  later phase.
- Verification means the build passes and the main flow gets a quick browser
  smoke test. Automated tests are optional unless specifically requested.
