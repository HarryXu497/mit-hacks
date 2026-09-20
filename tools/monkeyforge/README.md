# MonkeyForge

MonkeyForge is a constraint-aware character compiler for stylized soccer monkeys. It keeps a
hand-authored, animation-safe monkey body and generates only bounded customization: morphs,
palette, hats, backpacks, and held props. The result is a game-ready bundle rather than an
unvalidated raw mesh.

```text
sketch + description
        -> CharacterSpec
        -> generated prop candidate(s)
        -> validation and ranking
        -> Blender compilation
        -> character.glb + manifest.json + metrics.json
```

The default development mode is fully local and uses a procedural low-poly helmet. The real 3D
generator is deliberately isolated behind an HTTP provider so Pixal3D or TRELLIS.2 can run in a
separate Linux/CUDA image on a rented GPU.

## Repository status

This scaffold includes:

- FastAPI upload and job-status endpoints.
- Versioned Pydantic contracts for character, equipment, and ability specifications.
- File-backed job storage and content-addressable request IDs.
- A working mock geometry provider with a procedural helmet.
- An HTTP contract for a remote image-to-3D worker.
- Passthrough and headless-Blender compiler adapters.
- A Blender script for normalization, decimation, socket placement, and GLB export.
- Hard validation rules and a trainable pairwise style ranker.
- An original rigged MonkeyForge body with balanced, runner, defender, and goalkeeper profiles.
- Unit tests for the schema and deterministic pipeline.

Generated binary bases and prototype references remain local/ignored; the build script can
recreate the original bases from source.

## Quick start

Install Python 3.11 or 3.12, then from this directory:

```powershell
python -m venv .venv
.venv\Scripts\Activate.ps1
python -m pip install --upgrade pip
python -m pip install -e ".[dev]"
Copy-Item .env.example .env
python -m uvicorn monkeyforge.api:app --reload
```

Open `http://127.0.0.1:8000/docs` and submit `POST /v1/characters` with a sketch and description.
Poll `GET /v1/jobs/{job_id}` until the job is complete.

Run checks with:

```powershell
python -m pytest
python -m ruff check .
```

## Development request

```powershell
curl.exe -X POST http://127.0.0.1:8000/v1/characters `
  -F "description=fast goalkeeper with a star-shaped aviator helmet" `
  -F "seed=42" `
  -F "sketch=@C:\path\to\your\sketch.png"
```

In mock mode, any small PNG/JPEG can be used. The response returns a stable job ID derived from
the request content, which also prevents duplicate GPU work.

## Connecting a GPU worker

Set:

```dotenv
MONKEYFORGE_PROVIDER=http
MONKEYFORGE_GPU_ENDPOINT=https://your-worker.example/v1/image-to-3d
MONKEYFORGE_GPU_TOKEN=replace-me
```

The worker contract is documented in [docs/gpu-worker-contract.md](docs/gpu-worker-contract.md).
Keep the model server in its own environment; both Pixal3D and TRELLIS.2 compile CUDA extensions
that should not be mixed into the lightweight API environment.

## Adding the base monkey

The local build produces these canonical rigged bodies:

```text
assets/base/monkeyforge_balanced.glb
assets/base/monkeyforge_runner.glb
assets/base/monkeyforge_defender.glb
assets/base/monkeyforge_goalkeeper.glb
```

Its attachment nodes should use these names:

```text
SOCKET_HEAD_TOP
SOCKET_FACE
SOCKET_BACK
SOCKET_WAIST
SOCKET_HAND_LEFT
SOCKET_HAND_RIGHT
SOCKET_TAIL_TIP
```

After Blender is installed, switch to:

```dotenv
MONKEYFORGE_COMPILER=blender
MONKEYFORGE_BLENDER_PATH=C:\Program Files\Blender Foundation\Blender 4.5\blender.exe
```

To rebuild the original bases:

```powershell
$blender = "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe"
foreach ($profile in @("balanced", "runner", "defender", "goalkeeper")) {
  & $blender --background --python scripts\blender_build_original_base.py -- `
    --profile $profile `
    --blend-output "assets\work\monkeyforge_$profile.blend" `
    --glb-output "assets\base\monkeyforge_$profile.glb"
}
```

The style ranker can be trained locally without a GPU while the real visual encoder remains frozen:

```powershell
python scripts\train_style_ranker.py `
  --dataset training\dataset.example.jsonl `
  --output output\training\style-ranker.json
```

See [docs/architecture.md](docs/architecture.md) for the production pipeline and
[docs/training.md](docs/training.md) for the recommended narrow training strategy. Hardware,
latency, and rental guidance lives in [docs/compute.md](docs/compute.md). The current prototype
base-model workflow is in [docs/base-model-prep.md](docs/base-model-prep.md).
