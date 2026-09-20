from __future__ import annotations

from contextlib import asynccontextmanager
from pathlib import Path

from fastapi import BackgroundTasks, FastAPI, File, Form, HTTPException, UploadFile, status

from monkeyforge.compilers import BlenderCompiler, PassthroughCompiler
from monkeyforge.config import Settings, get_settings
from monkeyforge.measurements import BlenderMeasurer
from monkeyforge.models import (
    UNKNOWN_VERSION,
    Archetype,
    BuildIdentity,
    CharacterRequest,
    CreateCharacterResponse,
    JobRecord,
    JobState,
)
from monkeyforge.pipeline import CharacterPipeline, request_digest
from monkeyforge.providers import HttpGeometryProvider, MockGeometryProvider
from monkeyforge.spec_provider import RuleBasedSpecProvider
from monkeyforge.storage import JobStore


async def build_pipeline(settings: Settings) -> tuple[JobStore, CharacterPipeline]:
    store = JobStore(settings.jobs_dir)
    if settings.provider == "http":
        if not settings.gpu_endpoint:
            raise ValueError("MONKEYFORGE_GPU_ENDPOINT is required for the HTTP provider")
        geometry_provider = HttpGeometryProvider(
            endpoint=settings.gpu_endpoint,
            token=settings.gpu_token,
            max_response_bytes=settings.max_geometry_response_mb * 1024 * 1024,
        )
    else:
        geometry_provider = MockGeometryProvider()

    measurer = None
    if settings.compiler == "blender":
        repository_root = Path(__file__).resolve().parents[2]
        measurer = BlenderMeasurer(
            blender_path=settings.blender_path,
            script_path=repository_root / "scripts" / "blender_measure_glb.py",
        )
        compiler = BlenderCompiler(
            blender_path=settings.blender_path,
            base_models={
                Archetype.BALANCED: settings.base_model,
                Archetype.PLAYMAKER: settings.base_model,
                Archetype.RUNNER: settings.runner_base_model,
                Archetype.DEFENDER: settings.defender_base_model,
                Archetype.GOALKEEPER: settings.goalkeeper_base_model,
            },
            script_path=repository_root / "scripts" / "blender_compile.py",
        )
    else:
        compiler = PassthroughCompiler()

    build = await resolve_build_identity(settings, geometry_provider, compiler)
    pipeline = CharacterPipeline(
        store=store,
        spec_provider=RuleBasedSpecProvider(),
        geometry_provider=geometry_provider,
        compiler=compiler,
        measurer=measurer,
        build=build,
    )
    return store, pipeline


async def resolve_build_identity(
    settings: Settings,
    geometry_provider: object,
    compiler: object,
) -> BuildIdentity:
    """Determine the toolchain identity that cache keys are computed against.

    Provider version resolution, in order: an explicit `MONKEYFORGE_GENERATOR_VERSION`
    pin, else whatever the worker reports from `/health`, else a provider constant,
    else `unknown`. The explicit pin wins so an operator can always force a rebuild.
    """
    provider_version = settings.generator_version or getattr(geometry_provider, "version", None)
    if provider_version is None and hasattr(geometry_provider, "probe_version"):
        provider_version = await geometry_provider.probe_version()

    return BuildIdentity(
        provider=settings.provider,
        provider_version=provider_version or UNKNOWN_VERSION,
        compiler=settings.compiler,
        compiler_version=getattr(compiler, "version", UNKNOWN_VERSION),
        base_models_version=getattr(compiler, "base_models_version", UNKNOWN_VERSION),
    )


def _should_rebuild(existing: JobRecord, build: BuildIdentity | None) -> bool:
    """Decide whether a cached job may be served, or must be built again.

    Two reasons to rebuild. The build identity has moved, so the cached bytes came
    from a toolchain that no longer exists — including the case where the cached
    job predates build tracking and its toolchain is simply unknown. Or the cached
    job failed: a transient worker error is not a result, and caching it would pin
    the failure to that request forever.
    """
    if existing.state is JobState.FAILED:
        return True
    if build is None:
        return False
    return existing.build != build


@asynccontextmanager
async def lifespan(app: FastAPI):
    settings = get_settings()
    settings.jobs_dir.mkdir(parents=True, exist_ok=True)
    app.state.settings = settings
    app.state.store, app.state.pipeline = await build_pipeline(settings)
    yield


app = FastAPI(
    title="MonkeyForge",
    version="0.1.0",
    description="Constraint-aware compiler for generated, game-ready monkey characters.",
    lifespan=lifespan,
)


@app.get("/health")
async def health() -> dict[str, str]:
    return {"status": "ok"}


@app.post(
    "/v1/characters",
    response_model=CreateCharacterResponse,
    status_code=status.HTTP_202_ACCEPTED,
)
async def create_character(
    background_tasks: BackgroundTasks,
    description: str = Form(min_length=3, max_length=1000),
    seed: int = Form(default=0, ge=0, le=2**31 - 1),
    sketch: UploadFile = File(),
) -> CreateCharacterResponse:
    settings: Settings = app.state.settings
    sketch_bytes = await sketch.read(settings.max_upload_mb * 1024 * 1024 + 1)
    if len(sketch_bytes) > settings.max_upload_mb * 1024 * 1024:
        raise HTTPException(status_code=413, detail="sketch exceeds upload limit")
    if not sketch_bytes:
        raise HTTPException(status_code=400, detail="sketch is empty")

    pipeline: CharacterPipeline = app.state.pipeline
    build = pipeline.build
    job_id = request_digest(description, seed, sketch_bytes, build)
    store: JobStore = app.state.store
    existing = store.get(job_id)
    if existing is not None and not _should_rebuild(existing, build):
        return CreateCharacterResponse(
            job_id=job_id,
            state=existing.state,
            status_url=f"/v1/jobs/{job_id}",
        )

    suffix = Path(sketch.filename or "sketch.png").suffix.lower()
    if suffix not in {".png", ".jpg", ".jpeg", ".webp"}:
        suffix = ".bin"
    job_dir = store.job_dir(job_id)
    input_dir = job_dir / "input"
    input_dir.mkdir(parents=True, exist_ok=True)
    sketch_path = input_dir / f"sketch{suffix}"
    sketch_path.write_bytes(sketch_bytes)

    request = CharacterRequest(description=description, seed=seed)
    record = JobRecord(job_id=job_id, request=request, sketch_path=sketch_path, build=build)
    store.create(record)
    background_tasks.add_task(pipeline.run, job_id)
    return CreateCharacterResponse(
        job_id=job_id,
        state=JobState.QUEUED,
        status_url=f"/v1/jobs/{job_id}",
    )


@app.get("/v1/jobs/{job_id}", response_model=JobRecord)
async def get_job(job_id: str) -> JobRecord:
    store: JobStore = app.state.store
    record = store.get(job_id)
    if record is None:
        raise HTTPException(status_code=404, detail="job not found")
    return record


def run() -> None:
    import uvicorn

    uvicorn.run("monkeyforge.api:app", host="127.0.0.1", port=8000, reload=False)
