# Architecture

## Boundary between learned and deterministic systems

MonkeyForge uses learned models for semantic interpretation, concept generation, mesh generation,
and aesthetic ranking. It uses deterministic code for everything that can break gameplay:
coordinate systems, triangle budgets, sockets, scale, collisions, ability limits, file formats,
and validation.

## Services

### API/orchestrator

Runs on CPU. It accepts the sketch, validates the request, creates a deterministic job ID, invokes
providers, records state transitions, and assembles the final character bundle.

### Specification provider

Converts the drawing and description into `CharacterSpec`. The initial implementation is
rule-based so the repository is usable without an LLM. A production provider can call one VLM per
new character, but its response must validate against `schemas/character_spec.schema.json`.

### Geometry provider

Accepts an isolated reference image, prompt, seed, part schema, and triangle target. The development
provider writes a procedural OBJ. The HTTP provider sends the same request to a dedicated CUDA
worker and expects a GLB response.

### Compiler

The compiler selects the canonical role silhouette (`balanced`, `runner`, `defender`, or
`goalkeeper`), imports its rigged body and generated accessory, reduces the accessory to the target
triangle budget, normalizes its dimensions, attaches it to a named socket, and exports a GLB. The
compiler must never silently accept a missing socket or missing mesh.

## State machine

```text
queued -> parsing -> generating -> compiling -> validating -> complete
                                               \-> failed
```

Every transition is persisted to `data/jobs/<job_id>/job.json`. A failed job retains its inputs and
error information for debugging.

## Character bundle

```text
data/jobs/<job_id>/output/
├── character.glb or accessory.obj
├── manifest.json
└── metrics.json
```

The manifest is the engine-facing contract. Game code should not infer abilities or attachment
behavior from filenames.

## Production deployment

```text
Browser/game client
       |
       v
CPU API/orchestrator ----- object storage/cache
       |
       v
GPU image-to-3D worker --- Pixal3D/TRELLIS.2
       |
       v
Blender compilation worker
```

### Training boundary

The first trainable component is a six-feature pairwise style ranker. It learns from team-owned
preferred/rejected renders while geometry generation remains frozen. This makes iteration cheap and
keeps gameplay-critical scale, sockets, triangle limits, and provenance deterministic. The JSONL
contract and local training command are documented in [training.md](training.md).

For a hackathon, the API and Blender compiler can share one CPU container. Keep the CUDA worker
separate so it can be swapped between a local GPU, RunPod, Modal, or another provider.
