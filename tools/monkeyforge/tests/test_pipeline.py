import json
from pathlib import Path

import pytest

from monkeyforge.api import _should_rebuild
from monkeyforge.compilers import BlenderCompiler, PassthroughCompiler
from monkeyforge.models import (
    Archetype,
    BuildIdentity,
    CharacterRequest,
    JobRecord,
    JobState,
)
from monkeyforge.pipeline import CharacterPipeline, request_digest
from monkeyforge.providers import MockGeometryProvider
from monkeyforge.spec_provider import RuleBasedSpecProvider
from monkeyforge.storage import JobStore


def build(**overrides: str) -> BuildIdentity:
    fields: dict[str, str] = {
        "provider": "mock",
        "provider_version": "mock-1",
        "compiler": "passthrough",
        "compiler_version": "passthrough-1",
        "base_models_version": "abc123",
    }
    fields.update(overrides)
    return BuildIdentity(**fields)


def test_request_digest_is_stable_and_input_sensitive() -> None:
    first = request_digest("fast monkey", 42, b"image")
    second = request_digest("fast monkey", 42, b"image")
    changed = request_digest("fast monkey", 43, b"image")
    assert first == second
    assert first != changed


def test_request_digest_is_stable_for_the_same_build() -> None:
    identity = build()
    assert request_digest("fast monkey", 42, b"image", identity) == request_digest(
        "fast monkey", 42, b"image", identity
    )


@pytest.mark.parametrize(
    "field,value",
    [
        # Swapping the mock provider for the real GPU worker.
        ("provider", "http"),
        # Same worker, new model behind it — the TRELLIS.2 upgrade.
        ("provider_version", "trellis-2.0"),
        ("compiler", "blender"),
        # An edit to blender_compile.py changes the compiler's behaviour.
        ("compiler_version", "blender-deadbeef01"),
        # A rebuilt base monkey changes every character built on it.
        ("base_models_version", "999fff"),
        ("pipeline_version", "2"),
    ],
)
def test_request_digest_changes_when_the_toolchain_changes(field: str, value: str) -> None:
    """The bug this guards: identical request, different toolchain, same job id.

    A matching id means `create_character` serves the cached build and never runs
    the new generator, silently.
    """
    before = request_digest("fast monkey", 42, b"image", build())
    after = request_digest("fast monkey", 42, b"image", build(**{field: value}))
    assert before != after, f"{field} must take part in the cache key"


def test_build_identity_differences_names_what_changed() -> None:
    changes = build().differences(build(provider="http", provider_version="trellis-2.0"))
    assert changes == ["provider: 'mock' -> 'http'", "provider_version: 'mock-1' -> 'trellis-2.0'"]


def record(**overrides: object) -> JobRecord:
    fields: dict[str, object] = {
        "job_id": "j",
        "request": CharacterRequest(description="fast monkey", seed=1),
        "sketch_path": Path("sketch.png"),
        "state": JobState.COMPLETE,
        "build": build(),
    }
    fields.update(overrides)
    return JobRecord(**fields)


def test_cached_job_is_served_when_the_build_is_unchanged() -> None:
    assert _should_rebuild(record(), build()) is False


def test_cached_job_is_rebuilt_when_the_generator_changed() -> None:
    assert _should_rebuild(record(), build(provider_version="trellis-2.0")) is True


def test_cached_job_is_rebuilt_when_its_toolchain_is_unknown() -> None:
    """Jobs written before build tracking carry no identity and cannot be trusted."""
    assert _should_rebuild(record(build=None), build()) is True


def test_failed_job_is_never_served_from_cache() -> None:
    """A transient worker error must not pin a failure to this request forever."""
    assert _should_rebuild(record(state=JobState.FAILED), build()) is True


def test_blender_compiler_version_follows_its_script(tmp_path: Path) -> None:
    """Editing the compile script must invalidate builds without a manual bump."""
    script = tmp_path / "blender_compile.py"
    script.write_text("print('v1')", encoding="utf-8")
    compiler = BlenderCompiler(
        blender_path="blender",
        base_models={Archetype.BALANCED: tmp_path / "base.glb"},
        script_path=script,
    )
    first = compiler.version
    script.write_text("print('v2')", encoding="utf-8")
    assert compiler.version != first


@pytest.mark.asyncio
async def test_mock_pipeline_completes(tmp_path: Path) -> None:
    store = JobStore(tmp_path / "jobs")
    sketch = tmp_path / "sketch.png"
    sketch.write_bytes(b"not-a-real-png-needed-in-mock-mode")
    request = CharacterRequest(description="fast monkey with a red aviator helmet", seed=7)
    record = JobRecord(job_id="test-job", request=request, sketch_path=sketch)
    store.create(record)

    pipeline = CharacterPipeline(
        store=store,
        spec_provider=RuleBasedSpecProvider(),
        geometry_provider=MockGeometryProvider(),
        compiler=PassthroughCompiler(),
    )
    await pipeline.run("test-job")

    result = store.get("test-job")
    assert result is not None
    assert result.state is JobState.COMPLETE
    assert result.output_path is not None
    assert result.output_path.exists()
    assert (store.job_dir("test-job") / "output" / "manifest.json").exists()


@pytest.mark.asyncio
async def test_manifest_records_the_build_that_produced_it(tmp_path: Path) -> None:
    """The lockfile half: a finished job must say which toolchain built it."""
    store = JobStore(tmp_path / "jobs")
    sketch = tmp_path / "sketch.png"
    sketch.write_bytes(b"not-a-real-png-needed-in-mock-mode")
    request = CharacterRequest(description="fast monkey with a red aviator helmet", seed=7)
    store.create(JobRecord(job_id="test-job", request=request, sketch_path=sketch))

    identity = build()
    pipeline = CharacterPipeline(
        store=store,
        spec_provider=RuleBasedSpecProvider(),
        geometry_provider=MockGeometryProvider(),
        compiler=PassthroughCompiler(),
        build=identity,
    )
    await pipeline.run("test-job")

    manifest = json.loads(
        (store.job_dir("test-job") / "output" / "manifest.json").read_text(encoding="utf-8")
    )
    assert manifest["build"]["digest"] == identity.digest()
    assert manifest["build"]["provider"] == "mock"
    # Ground truth from the generator, independent of what the orchestrator believed.
    assert manifest["build"]["observed_provider_version"] == "mock-1"

    result = store.get("test-job")
    assert result is not None
    assert result.build == identity
    assert result.observed_provider_version == "mock-1"

